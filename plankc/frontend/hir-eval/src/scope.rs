use plank_core::{DenseIndexMap, IndexVec};
use plank_hir::{self as hir, ExprKind, InstructionKind};
use plank_mir as mir;
use plank_session::{
    MaybePoisoned, Poisoned, SourceId, SourceSpan, SrcLoc, poison::MaybePoisonedResult,
};
use plank_values::{TypeId, Value, ValueId};

use crate::Evaluator;

pub(crate) enum EvalError {
    Poisoned,
    Diverge(BlockDiverge),
}

impl EvalError {
    pub const NEVER: Self = EvalError::Diverge(BlockDiverge::Never);
}

trait EvalResultAsValue<T> {
    fn value(self) -> Result<MaybePoisoned<T>, BlockDiverge>;
}

impl<T> EvalResultAsValue<T> for Result<T, EvalError> {
    fn value(self) -> Result<MaybePoisoned<T>, BlockDiverge> {
        match self {
            Ok(value) => Ok(Ok(value)),
            Err(EvalError::Poisoned) => Ok(Err(Poisoned)),
            Err(EvalError::Diverge(diverge)) => Err(diverge),
        }
    }
}

impl From<Poisoned> for EvalError {
    fn from(_: Poisoned) -> Self {
        Self::Poisoned
    }
}

impl From<BlockDiverge> for EvalError {
    fn from(value: BlockDiverge) -> Self {
        Self::Diverge(value)
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum EvalValue {
    Comptime(ValueId),
    Runtime { expr: mir::Expr, result_type: TypeId },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LocalState {
    Runtime(mir::LocalId),
    Comptime(ValueId),
}

pub(crate) enum BlockDiverge {
    Never,
    Return(ValueId),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Local {
    pub state: MaybePoisoned<LocalState>,
    pub span: SourceSpan,
}

impl Local {
    pub fn poisoned(self) -> MaybePoisoned<(LocalState, SourceSpan)> {
        let state = self.state?;
        Ok((state, self.span))
    }
}

pub(crate) struct Function {
    pub ret_type: TypeId,
    pub ret_type_span: Option<SourceSpan>,
}

pub(crate) struct Scope<'a, 'ctx> {
    pub eval: &'a mut Evaluator<'ctx>,

    pub source: SourceId,
    pub func: Option<Function>,
    pub comptime: bool,

    pub bindings: DenseIndexMap<hir::LocalId, Local>,
    pub mir_types: IndexVec<mir::LocalId, TypeId>,
}

impl<'a, 'ctx> Scope<'a, 'ctx> {
    pub fn eval_fn_body(&mut self, hir_block: hir::BlockId) -> mir::BlockId {
        let (mir_block, eval_res) = self.eval_block(hir_block);
        match eval_res {
            Err(BlockDiverge::Return(_)) | Ok(()) => {
                let span = self.hir.block_spans[hir_block].expect("hir: fn body without span");
                self.eval.diag_ctx.emit_entry_point_missing_terminator(self.loc(span));
            }
            Err(BlockDiverge::Never) => {
                // Desired termination
            }
        }
        mir_block
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

    pub fn binding_type(&self, local: hir::LocalId) -> MaybePoisoned<TypeId> {
        Ok(self.state_type(self.bindings[local].state?))
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
        let (state, span) = self.bindings[type_local].poisoned()?;
        let type_loc = self.loc(span);
        let LocalState::Comptime(vid) = state else {
            self.diag_ctx.emit_type_not_comptime(type_loc);
            return Err(Poisoned);
        };
        let Value::Type(ty) = self.values.lookup(vid) else {
            let actual_ty = self.values.type_of_value(vid);
            self.eval.diag_ctx.emit_type_constraint_not_type(&self.eval.types, actual_ty, type_loc);
            return Err(Poisoned);
        };
        Ok(ty)
    }

    fn to_runtime_value(&mut self, value: EvalValue) -> mir::Expr {
        match value {
            EvalValue::Comptime(vid) => mir::Expr::Const(vid),
            EvalValue::Runtime { result_type: _, expr } => expr,
        }
    }

    fn expect_comptime_value(
        &mut self,
        value: EvalValue,
        expr_span: SourceSpan,
    ) -> MaybePoisoned<ValueId> {
        match value {
            EvalValue::Comptime(vid) => Ok(vid),
            EvalValue::Runtime { result_type: _, expr: _ } => {
                self.eval.diag_ctx.emit_runtime_eval_in_comptime(self.loc(expr_span));
                Err(Poisoned)
            }
        }
    }

    fn expect_comptime_state(
        &mut self,
        value: LocalState,
        local_span: SourceSpan,
        expr_span: SourceSpan,
    ) -> MaybePoisoned<ValueId> {
        match value {
            LocalState::Comptime(vid) => Ok(vid),
            LocalState::Runtime(_) => {
                self.eval.diag_ctx.emit_runtime_assign_from_comptime(
                    self.source,
                    local_span,
                    expr_span,
                );
                Err(Poisoned)
            }
        }
    }

    fn type_check(
        &mut self,
        value: EvalValue,
        expected_ty: TypeId,
        expected_span: SourceSpan,
        actual_span: SourceSpan,
    ) -> MaybePoisoned<()> {
        let actual_ty = self.value_type(value);
        if actual_ty.is_assignable_to(expected_ty) {
            Ok(())
        } else {
            self.eval.diag_ctx.emit_type_mismatch_error(
                &self.eval.types,
                expected_ty,
                self.loc(expected_span),
                actual_ty,
                self.loc(actual_span),
            );
            Err(Poisoned)
        }
    }

    pub fn eval_set(
        &mut self,
        local: hir::LocalId,
        r#type: Option<hir::LocalId>,
        expr: hir::Expr,
    ) -> Result<(), BlockDiverge> {
        let value = self.eval_expr(expr)?;
        let value = value.and_then(|value| {
            let Some(type_local) = r#type else {
                return Ok(value);
            };
            let expected_ty = self.expect_type(type_local)?;
            self.type_check(value, expected_ty, self.bindings[type_local].span, expr.span)?;
            Ok(value)
        });
        let state = value.and_then(|value| {
            if self.is_comptime() {
                self.expect_comptime_value(value, expr.span).map(LocalState::Comptime)
            } else {
                match value {
                    EvalValue::Comptime(vid) => Ok(LocalState::Comptime(vid)),
                    EvalValue::Runtime { expr, result_type } => {
                        let target = self.alloc_mir(result_type);
                        self.emit(mir::Instruction::Set { target, expr });
                        Ok(LocalState::Runtime(target))
                    }
                }
            }
        });
        self.bindings.insert_no_prev(local, Local { state, span: expr.span });
        Ok(())
    }

    pub fn eval_set_mut(
        &mut self,
        local: hir::LocalId,
        r#type: Option<hir::LocalId>,
        expr: hir::Expr,
    ) -> Result<(), BlockDiverge> {
        let value = self.eval_expr(expr)?;
        let value = value.and_then(|value| {
            let Some(type_local) = r#type else {
                return Ok(value);
            };
            let expected_ty = self.expect_type(type_local)?;
            self.type_check(value, expected_ty, self.bindings[type_local].span, expr.span)?;
            Ok(value)
        });

        let state = value.and_then(|value| {
            if self.is_comptime() {
                self.expect_comptime_value(value, expr.span).map(LocalState::Comptime)
            } else {
                let target = self.alloc_mir(self.value_type(value));
                let expr = self.to_runtime_value(value);
                self.emit(mir::Instruction::Set { target, expr });
                Ok(LocalState::Runtime(target))
            }
        });

        self.bindings.insert_no_prev(local, Local { state, span: expr.span });
        Ok(())
    }

    pub fn eval_block(&mut self, block: hir::BlockId) -> (mir::BlockId, Result<(), BlockDiverge>) {
        self.with_instructions(|this| {
            for &instr in &this.hir.block_instrs[block] {
                this.eval_instr(instr)?;
            }
            Ok(())
        })
    }

    fn eval_instr(&mut self, instr: hir::Instruction) -> Result<(), BlockDiverge> {
        match instr.kind {
            InstructionKind::Set { local, r#type, expr } => self.eval_set(local, r#type, expr)?,
            InstructionKind::SetMut { local, r#type, expr } => {
                self.eval_set_mut(local, r#type, expr)?
            }
            InstructionKind::Assign { target, expr } => {
                let value = self.eval_expr(expr)?;
                let local = self.bindings[target];
                let new_state = local.state.zip(value).and_then(|(state, value)| {
                    let expected_ty = self.state_type(state);
                    let type_check = self.type_check(value, expected_ty, local.span, expr.span);
                    if self.is_comptime() {
                        let state = self.expect_comptime_state(state, local.span, expr.span);
                        let value = self.expect_comptime_value(value, expr.span);
                        type_check.and(state).and(value).map(LocalState::Comptime)
                    } else {
                        let LocalState::Runtime(target) = state else {
                            unreachable!("invariant: runtime assign to comptime state")
                        };
                        let expr = self.to_runtime_value(value);
                        self.emit(mir::Instruction::Set { target, expr });
                        Ok(LocalState::Runtime(target))
                    }
                });
                self.bindings[target].state = new_state;
            }
            InstructionKind::Eval(expr) => {
                let value = self.eval_expr(expr)?;
                if self.is_comptime() {
                    if let Ok(value) = value {
                        let _ = self.expect_comptime_value(value, expr.span);
                    }
                } else {
                    if let Ok(EvalValue::Runtime { expr, result_type }) = value {
                        // Lower incase the expression has side effect.
                        let target = self.alloc_mir(result_type);
                        self.emit(mir::Instruction::Set { target, expr });
                    } else {
                        // In a runtime context don't have to lower comptime or poison as they have
                        // no side effects.
                    }
                }
            }
            instr => todo!("instr: {instr:?}"),
        };
        Ok(())
    }

    pub fn ensure_materialized(&mut self, state: LocalState) -> mir::LocalId {
        match state {
            LocalState::Runtime(mir) => mir,
            LocalState::Comptime(vid) => self.materialize(vid),
        }
    }

    pub fn alloc_mir(&mut self, ty: TypeId) -> mir::LocalId {
        self.mir_types.push(ty)
    }

    pub fn materialize(&mut self, vid: ValueId) -> mir::LocalId {
        let ty = self.values.type_of_value(vid);
        let local = self.alloc_mir(ty);
        self.instr_stack_buf
            .push(mir::Instruction::Set { target: local, expr: mir::Expr::Const(vid) });
        local
    }

    pub fn loc(&self, span: SourceSpan) -> SrcLoc {
        SrcLoc::new(self.source, span)
    }

    pub fn eval_expr(&mut self, expr: hir::Expr) -> Result<MaybePoisoned<EvalValue>, BlockDiverge> {
        let expr_loc = self.loc(expr.span);
        match expr.kind {
            ExprKind::Value(maybe_vid) => Ok(maybe_vid.map(EvalValue::Comptime)),
            ExprKind::EvmBuiltinCall { builtin, args } => {
                Ok(self.eval_builtin(builtin, args, expr_loc).value()?)
            }
            expr_kind => todo!("expr_kind: {expr_kind:?}"),
        }
    }

    pub fn with_values_buf<R>(&mut self, inner: impl FnOnce(&mut Self, usize) -> R) -> R {
        let buf_offset = self.values_buf.len();
        let res = inner(self, buf_offset);
        self.values_buf.truncate(buf_offset);
        res
    }

    pub fn with_types_buf<R>(&mut self, inner: impl FnOnce(&mut Self, usize) -> R) -> R {
        let buf_offset = self.types_buf.len();
        let res = inner(self, buf_offset);
        self.types_buf.truncate(buf_offset);
        res
    }

    pub fn with_locals_buf<R>(&mut self, inner: impl FnOnce(&mut Self, usize) -> R) -> R {
        let buf_offset = self.locals_buf.len();
        let res = inner(self, buf_offset);
        self.locals_buf.truncate(buf_offset);
        res
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
        &self.eval
    }
}

impl<'a, 'ctx> std::ops::DerefMut for Scope<'a, 'ctx> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.eval
    }
}
