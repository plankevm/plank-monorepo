use crate::{
    Evaluator,
    diagnostics::{BindingLoc, DiagCtx},
    evaluator::CallArgSpansIdx,
};
use plank_core::{DenseIndexMap, IndexVec};
use plank_hir::{self as hir, ExprKind, InstructionKind};
use plank_mir as mir;
use plank_session::{MaybePoisoned, Poisoned, SourceId, SourceSpan, SrcLoc, poison};
use plank_values::{DefOrigin, TypeId, Value, ValueId};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Local {
    pub state: MaybePoisoned<LocalState>,
    pub use_span: SourceSpan,
    pub origin: DefOrigin,
}

impl Local {
    pub fn comptime(value: ValueId, use_span: SourceSpan, origin: DefOrigin) -> Self {
        Self { state: Ok(LocalState::Comptime(value)), use_span, origin }
    }

    pub fn poisoned(self) -> MaybePoisoned<(LocalState, SourceSpan, DefOrigin)> {
        let state = self.state?;
        Ok((state, self.use_span, self.origin))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LocalState {
    Runtime(mir::LocalId),
    Comptime(ValueId),
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum EvalValue {
    Comptime(ValueId),
    Runtime { expr: mir::Expr, result_type: TypeId },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Diverge {
    ControlFlowPoisoned,
    BlockEnd(Option<ValueId>),
}

impl Diverge {
    pub const END: Self = Self::BlockEnd(None);
}

pub(crate) enum EvalContext {
    FunctionBody { ret_type: MaybePoisoned<TypeId>, ret_type_loc: SrcLoc },
    FunctionPreamble { arg_spans: CallArgSpansIdx, call_source: SourceId },
    Other,
}

pub(crate) struct Scope<'a, 'ctx> {
    pub eval: &'a mut Evaluator<'ctx>,
    pub diag_ctx: &'a mut DiagCtx<'ctx>,

    pub source: SourceId,
    pub ctx: EvalContext,
    pub comptime: bool,

    pub bindings: DenseIndexMap<hir::LocalId, Local>,
    pub mir_types: IndexVec<mir::LocalId, TypeId>,
}

impl<'a, 'ctx> Scope<'a, 'ctx> {
    pub fn new(
        eval: &'a mut Evaluator<'ctx>,
        diag_ctx: &'a mut DiagCtx<'ctx>,
        source: SourceId,
        comptime: bool,
        ctx: EvalContext,
    ) -> Self {
        Self {
            eval,
            diag_ctx,

            source,
            ctx,
            comptime,

            bindings: DenseIndexMap::new(),
            mir_types: IndexVec::new(),
        }
    }

    pub fn eval_entry_point_body(&mut self, hir_block: hir::BlockId) -> mir::BlockId {
        let (mir_block, eval_res) = self.eval_block_to_mir(hir_block);
        match eval_res {
            Ok(()) => {
                if let Ok(span) = self.hir.block_spans[hir_block] {
                    self.diag_ctx.emit_entry_point_missing_terminator(self.loc(span));
                }
            }
            Err(Diverge::ControlFlowPoisoned | Diverge::BlockEnd(_)) => {}
        }
        mir_block
    }

    pub fn eval_comptime(&mut self, block: hir::BlockId) -> Result<(), Diverge> {
        let parent_comptime = std::mem::replace(&mut self.comptime, true);
        let res = self.eval_block_inline(block);
        self.comptime = parent_comptime;
        res
    }

    pub fn emit(&mut self, instr: mir::Instruction) {
        assert!(!self.is_comptime());
        self.eval.instr_stack_buf.push(instr);
    }

    pub fn try_comptime(
        &mut self,
        binding: Local,
        expr: SourceSpan,
    ) -> MaybePoisoned<Option<ValueId>> {
        match binding.state? {
            LocalState::Comptime(value) => Ok(Some(value)),
            LocalState::Runtime(_) if !self.is_comptime() => Ok(None),
            LocalState::Runtime(_) => {
                self.diag_ctx
                    .emit_runtime_ref_in_comptime(self.loc(expr), self.origin_loc(binding.origin));
                Err(Poisoned)
            }
        }
    }

    pub fn state_type(&self, state: LocalState) -> TypeId {
        match state {
            LocalState::Runtime(mir) => self.mir_types[mir],
            LocalState::Comptime(vid) => self.values.type_of_value(vid),
        }
    }

    pub fn value_type(&self, value: EvalValue) -> TypeId {
        match value {
            EvalValue::Comptime(vid) => self.values.type_of_value(vid),
            EvalValue::Runtime { expr: _, result_type } => result_type,
        }
    }

    pub fn with_instructions<R>(
        &mut self,
        inner: impl FnOnce(&mut Self) -> R,
    ) -> (mir::BlockId, R) {
        let instr_offset = self.instr_stack_buf.len();
        let res = inner(self);
        let block = self.eval.mir_blocks.push_iter(self.eval.instr_stack_buf.drain(instr_offset..));
        (block, res)
    }

    pub fn expect_type(&mut self, type_local: hir::LocalId) -> MaybePoisoned<TypeId> {
        let (state, use_span, origin) = self.bindings[type_local].poisoned()?;
        let type_loc = self.loc(use_span);
        let LocalState::Comptime(vid) = state else {
            self.diag_ctx.emit_type_not_comptime(type_loc);
            return Err(Poisoned);
        };
        let Value::Type(ty) = self.values.lookup(vid) else {
            let actual_ty = self.values.type_of_value(vid);
            self.diag_ctx.emit_type_not_type(actual_ty, self.binding_loc(use_span, origin));
            return Err(Poisoned);
        };
        Ok(ty)
    }

    pub fn value_to_runtime_expr(
        &mut self,
        value: EvalValue,
        use_span: SourceSpan,
    ) -> MaybePoisoned<(mir::Expr, TypeId)> {
        match value {
            EvalValue::Comptime(vid) => {
                if self.is_comptime_only(vid) {
                    self.diag_ctx.emit_comptime_only_value_at_runtime(self.loc(use_span));
                    Err(Poisoned)
                } else {
                    Ok((mir::Expr::Const(vid), self.values.type_of_value(vid)))
                }
            }
            EvalValue::Runtime { result_type, expr } => Ok((expr, result_type)),
        }
    }

    pub fn expect_comptime_value(
        &mut self,
        value: EvalValue,
        expr_span: SourceSpan,
    ) -> MaybePoisoned<ValueId> {
        match value {
            EvalValue::Comptime(vid) => Ok(vid),
            EvalValue::Runtime { result_type: _, expr: _ } => {
                self.diag_ctx.emit_runtime_eval_in_comptime(self.loc(expr_span));
                Err(Poisoned)
            }
        }
    }

    pub fn type_check(
        &mut self,
        value: EvalValue,
        expected_ty: TypeId,
        expected_loc: SrcLoc,
        actual_span: SourceSpan,
    ) -> MaybePoisoned<()> {
        let actual_ty = self.value_type(value);
        if actual_ty.is_assignable_to(expected_ty) {
            Ok(())
        } else {
            self.diag_ctx.emit_type_mismatch(
                expected_ty,
                expected_loc,
                actual_ty,
                self.loc(actual_span),
                true,
            );
            Err(Poisoned)
        }
    }

    fn eval_set(
        &mut self,
        local: hir::LocalId,
        r#type: Option<hir::LocalId>,
        expr: hir::Expr,
    ) -> Result<(), Diverge> {
        let value = self.eval_expr(expr)?;
        let value = value.and_then(|value| {
            let Some(type_local) = r#type else {
                return Ok(value);
            };
            let expected_ty = self.expect_type(type_local)?;
            let type_loc = self.loc(self.bindings[type_local].use_span);
            self.type_check(value, expected_ty, type_loc, expr.span)?;
            Ok(value)
        });
        let state = value.and_then(|value| {
            if self.is_comptime() {
                return self.expect_comptime_value(value, expr.span).map(LocalState::Comptime);
            }
            match value {
                EvalValue::Comptime(vid) => Ok(LocalState::Comptime(vid)),
                EvalValue::Runtime { expr, result_type } => {
                    let target = self.mir_types.push(result_type);
                    self.emit(mir::Instruction::Set { target, expr });
                    Ok(LocalState::Runtime(target))
                }
            }
        });
        self.bindings.insert_no_prev(
            local,
            Local { state, use_span: expr.span, origin: self.expr_origin(expr) },
        );
        Ok(())
    }

    fn eval_set_mut(
        &mut self,
        local: hir::LocalId,
        r#type: Option<hir::LocalId>,
        expr: hir::Expr,
    ) -> Result<(), Diverge> {
        let value = self.eval_expr(expr)?;
        let value = value.and_then(|value| {
            let Some(type_local) = r#type else {
                return Ok(value);
            };
            let expected_ty = self.expect_type(type_local)?;
            let type_loc = self.loc(self.bindings[type_local].use_span);
            self.type_check(value, expected_ty, type_loc, expr.span)?;
            Ok(value)
        });

        let new_state = value.and_then(|value| {
            if self.is_comptime() {
                self.expect_comptime_value(value, expr.span).map(LocalState::Comptime)
            } else {
                self.value_to_runtime_expr(value, expr.span).map(|(expr, _ty)| {
                    let target = self.mir_types.push(self.value_type(value));
                    self.emit(mir::Instruction::Set { target, expr });
                    LocalState::Runtime(target)
                })
            }
        });

        self.bindings.insert_no_prev(
            local,
            Local { state: new_state, use_span: expr.span, origin: self.expr_origin(expr) },
        );
        Ok(())
    }

    fn eval_branch_set(&mut self, local: hir::LocalId, expr: hir::Expr) -> Result<(), Diverge> {
        let value = self.eval_expr(expr)?;
        if self.is_comptime() {
            let state = value
                .and_then(|value| self.expect_comptime_value(value, expr.span))
                .map(LocalState::Comptime);
            let _ = self.bindings.insert(
                local,
                Local { state, use_span: expr.span, origin: self.expr_origin(expr) },
            );
            return Ok(());
        }

        let mir_expr = value.and_then(|value| self.value_to_runtime_expr(value, expr.span));
        match self.bindings.get(local).copied() {
            None => {
                let state = mir_expr.map(|(expr, ty)| {
                    let target = self.mir_types.push(ty);
                    self.emit(mir::Instruction::Set { target, expr });
                    LocalState::Runtime(target)
                });
                self.bindings.insert_no_prev(
                    local,
                    Local { state, use_span: expr.span, origin: self.expr_origin(expr) },
                );
            }
            Some(binding) => {
                let new_state = poison::zip(binding.state, mir_expr).and_then(
                    |(prev_state, (mir_expr, ty))| {
                        let LocalState::Runtime(target) = prev_state else {
                            unreachable!(
                                "invariant: runtime branch set overwriting comptime state"
                            );
                        };
                        if let Err(existing_ty) = self.mir_types[target].unify(ty) {
                            self.diag_ctx.emit_incompatible_branch_types(
                                existing_ty,
                                self.origin_loc(binding.origin),
                                ty,
                                self.loc(expr.span),
                            );
                            return Err(Poisoned);
                        }

                        self.emit(mir::Instruction::Set { target, expr: mir_expr });

                        Ok(LocalState::Runtime(target))
                    },
                );
                self.bindings[local] =
                    Local { state: new_state, use_span: expr.span, origin: self.expr_origin(expr) };
            }
        }

        Ok(())
    }

    pub fn eval_block_to_mir(
        &mut self,
        block: hir::BlockId,
    ) -> (mir::BlockId, Result<(), Diverge>) {
        self.with_instructions(|this| this.eval_block_inline(block))
    }

    pub fn eval_block_inline(&mut self, block: hir::BlockId) -> Result<(), Diverge> {
        for &instr in &self.hir.block_instrs[block] {
            self.eval_instr(instr)?;
        }
        Ok(())
    }

    fn eval_assign(&mut self, target: hir::LocalId, expr: hir::Expr) -> Result<(), Diverge> {
        let value = self.eval_expr(expr)?;
        let local = self.bindings[target];
        let new_state = poison::zip(local.state, value).and_then(|(state, value)| {
            let expected_ty = self.state_type(state);
            let type_check =
                self.type_check(value, expected_ty, self.origin_loc(local.origin), expr.span);
            if self.is_comptime() {
                let state = match state {
                    LocalState::Comptime(vid) => Ok(vid),
                    LocalState::Runtime(_) => {
                        self.diag_ctx.emit_runtime_ref_in_comptime(
                            self.origin_loc(local.origin),
                            self.loc(expr.span),
                        );
                        Err(Poisoned)
                    }
                };
                let value = self.expect_comptime_value(value, expr.span);
                type_check.and(state).and(value).map(LocalState::Comptime)
            } else {
                let LocalState::Runtime(target) = state else {
                    unreachable!("invariant: runtime assign to comptime state")
                };
                self.value_to_runtime_expr(value, expr.span).map(|(expr, _ty)| {
                    self.emit(mir::Instruction::Set { target, expr });
                    LocalState::Runtime(target)
                })
            }
        });
        self.bindings[target].state = new_state;
        Ok(())
    }

    pub fn eval_instr(&mut self, instr: hir::Instruction) -> Result<(), Diverge> {
        match instr.kind {
            InstructionKind::Set { local, r#type, expr } => self.eval_set(local, r#type, expr)?,
            InstructionKind::SetMut { local, r#type, expr } => {
                self.eval_set_mut(local, r#type, expr)?
            }
            InstructionKind::BranchSet { local, expr } => self.eval_branch_set(local, expr)?,
            InstructionKind::ComptimeBlock { body } => self.eval_comptime(body)?,
            InstructionKind::Assign { target, expr } => self.eval_assign(target, expr)?,
            InstructionKind::Eval(expr) => {
                let value = self.eval_expr(expr)?;
                if self.is_comptime() {
                    if let Ok(value) = value {
                        let _ = self.expect_comptime_value(value, expr.span);
                    }
                } else {
                    if let Ok(EvalValue::Runtime { expr, result_type }) = value {
                        // Lower incase the expression has side effect.
                        let target = self.mir_types.push(result_type);
                        self.emit(mir::Instruction::Set { target, expr });
                    } else {
                        // In a runtime context don't have to lower comptime or poison as they have
                        // no side effects.
                    }
                }
            }
            InstructionKind::If { condition, then_block, else_block } => {
                self.eval_if(condition, then_block, else_block)?
            }
            InstructionKind::While { condition_block, condition, body } => {
                self.eval_while(condition_block, condition, body)?
            }
            InstructionKind::Return(expr) => self.eval_return(expr)?,
            InstructionKind::Param { comptime, arg, r#type, idx } => {
                self.eval_param(comptime, arg, r#type, idx)
            }
        };
        Ok(())
    }

    fn eval_if(
        &mut self,
        condition: hir::LocalId,
        then: hir::BlockId,
        r#else: hir::BlockId,
    ) -> Result<(), Diverge> {
        let binding = self.bindings[condition];
        match binding.state {
            Ok(LocalState::Runtime(mir_local))
                if self.mir_types[mir_local].is_assignable_to(TypeId::BOOL) =>
            {
                if self.is_comptime() {
                    self.diag_ctx.emit_runtime_eval_in_comptime(self.loc(binding.use_span));
                    return Err(Diverge::ControlFlowPoisoned);
                }
                let (then, then_res) = self.eval_block_to_mir(then);
                let (r#else, else_res) = self.eval_block_to_mir(r#else);
                self.emit(mir::Instruction::If {
                    condition: mir_local,
                    then_block: then,
                    else_block: r#else,
                });
                match (then_res, else_res) {
                    // Control flow was poisoned in either branch so we have to assume everything
                    // was poisoned and bubble up
                    (Err(Diverge::ControlFlowPoisoned), _)
                    | (_, Err(Diverge::ControlFlowPoisoned)) => Err(Diverge::ControlFlowPoisoned),
                    (Err(Diverge::BlockEnd(_)), Err(Diverge::BlockEnd(_))) => {
                        Err(Diverge::BlockEnd(None))
                    }
                    (Ok(()), Ok(()))
                    | (Err(Diverge::BlockEnd(_)), Ok(()))
                    | (Ok(()), Err(Diverge::BlockEnd(_))) => Ok(()),
                }
            }
            Ok(LocalState::Comptime(ValueId::TRUE)) => self.eval_block_inline(then),
            Ok(LocalState::Comptime(ValueId::FALSE)) => self.eval_block_inline(r#else),
            Ok(state) => {
                let state_ty = self.state_type(state);
                self.diag_ctx.emit_type_mismatch_simple(
                    TypeId::BOOL,
                    state_ty,
                    self.loc(binding.use_span),
                );
                Err(Diverge::ControlFlowPoisoned)
            }
            Err(Poisoned) => Err(Diverge::ControlFlowPoisoned),
        }
    }

    fn eval_while(
        &mut self,
        condition_block: hir::BlockId,
        condition: hir::LocalId,
        body: hir::BlockId,
    ) -> Result<(), Diverge> {
        if self.is_comptime() {
            if let Ok(span) = self.hir.block_spans[condition_block] {
                self.diag_ctx.emit_not_yet_implemented("comptime while", self.loc(span));
            }
            return Err(Diverge::ControlFlowPoisoned);
        }

        let (condition_block, mir_condition_local) = self.with_instructions(|this| {
            this.eval_block_inline(condition_block)?;
            let binding = this.bindings[condition];
            let state = match binding.state {
                Err(Poisoned) => return Err(Diverge::ControlFlowPoisoned),
                Ok(state) => state,
            };
            let state_ty = this.state_type(state);
            if !state_ty.is_assignable_to(TypeId::BOOL) {
                this.diag_ctx.emit_type_mismatch_simple(
                    TypeId::BOOL,
                    state_ty,
                    this.loc(binding.use_span),
                );
                return Err(Diverge::ControlFlowPoisoned);
            }
            match state {
                LocalState::Runtime(local) => Ok(local),
                LocalState::Comptime(value) => {
                    if this.is_comptime_only(value) {
                        this.diag_ctx
                            .emit_comptime_only_value_at_runtime(this.loc(binding.use_span));
                        return Err(Diverge::ControlFlowPoisoned);
                    }
                    let condition = this.mir_types.push(this.values.type_of_value(value));
                    this.emit(mir::Instruction::Set {
                        target: condition,
                        expr: mir::Expr::Const(value),
                    });
                    Ok(condition)
                }
            }
        });
        let condition = mir_condition_local?;
        let (body, body_res) = self.eval_block_to_mir(body);
        match body_res {
            Err(Diverge::ControlFlowPoisoned) => {
                return Err(Diverge::ControlFlowPoisoned);
            }
            Err(Diverge::BlockEnd(_)) | Ok(()) => {}
        }
        self.emit(mir::Instruction::While { condition_block, condition, body });

        Ok(())
    }

    pub fn loc(&self, span: SourceSpan) -> SrcLoc {
        SrcLoc::new(self.source, span)
    }

    pub fn expr_origin(&self, expr: hir::Expr) -> DefOrigin {
        if let ExprKind::ConstRef(id) = expr.kind {
            DefOrigin::Const(id)
        } else {
            DefOrigin::Local(expr.span)
        }
    }

    pub fn origin_loc(&self, origin: DefOrigin) -> SrcLoc {
        match origin {
            DefOrigin::Local(span) => self.loc(span),
            DefOrigin::Const(id) => self.hir.consts[id].loc(),
        }
    }

    pub fn binding_loc(&self, use_span: SourceSpan, origin: DefOrigin) -> BindingLoc {
        let r#use = self.loc(use_span);
        match origin {
            // Omit definition location for local defs (besides constants), to avoid having
            // overlapping "cause" & "defined here" diagnostic notes.
            DefOrigin::Local(_) => BindingLoc::inline(r#use),
            DefOrigin::Const(id) => BindingLoc::with_def(r#use, self.hir.consts[id].loc()),
        }
    }

    pub fn eval_expr(&mut self, expr: hir::Expr) -> Result<MaybePoisoned<EvalValue>, Diverge> {
        let value = match expr.kind {
            ExprKind::Value(maybe_vid) => maybe_vid.map(EvalValue::Comptime),
            ExprKind::BuiltinCall { builtin, args } => {
                poison::transpose(self.eval_builtin_call(builtin, args, expr.span))?
            }
            ExprKind::LocalRef(local) => self.bindings[local].state.map(|state| match state {
                LocalState::Comptime(vid) => EvalValue::Comptime(vid),
                LocalState::Runtime(local) => EvalValue::Runtime {
                    expr: mir::Expr::LocalRef(local),
                    result_type: self.mir_types[local],
                },
            }),
            ExprKind::LogicalNot { input } => self.eval_logical_not(input),
            ExprKind::ConstRef(const_id) => {
                self.eval.evaluate_const(const_id, self.diag_ctx).map(EvalValue::Comptime)
            }
            ExprKind::StructDef(struct_def_id) => self
                .eval_struct_def(struct_def_id, expr.span)
                .map(|ty| EvalValue::Comptime(self.values.intern_type(ty))),
            ExprKind::BinaryOpCall { op, lhs, rhs } => {
                poison::transpose(self.eval_binary_op(op, lhs, rhs, expr.span))?
            }
            ExprKind::UnaryOpCall { op, input } => {
                poison::transpose(self.eval_unary_op(op, input, expr.span))?
            }
            ExprKind::StructLit { ty, fields } => self.eval_struct_lit(ty, fields, expr.span),
            ExprKind::Member { object, member } => {
                self.eval_struct_member_access(object, member, expr.span)
            }
            ExprKind::FnDef(fn_def_id) => self.eval_fn_def(fn_def_id),
            ExprKind::Call { callee, args } => {
                poison::transpose(self.eval_call(callee, args, expr.span))?
            }
        };
        Ok(value)
    }

    pub fn is_comptime(&self) -> bool {
        self.comptime
    }
}

// Deref traits defined for convenient access of `eval` members via `self`, however to resolve
// borrow checker conflicts you'll often still need to access via `self.eval`.
impl<'a, 'ctx> std::ops::Deref for Scope<'a, 'ctx> {
    type Target = Evaluator<'ctx>;

    fn deref(&self) -> &Self::Target {
        self.eval
    }
}

impl<'a, 'ctx> std::ops::DerefMut for Scope<'a, 'ctx> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.eval
    }
}
