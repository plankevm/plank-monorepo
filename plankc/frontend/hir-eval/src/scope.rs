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

    pub fn eval_set(
        &mut self,
        local: hir::LocalId,
        r#type: Option<hir::LocalId>,
        expr: hir::Expr,
    ) -> Result<(), BlockDiverge> {
        let value = self.eval_expr(expr)?;
        let value = r#type.map_or(value, |type_local| {
            let expected_ty = self.expect_type(type_local)?;
            let actual_ty = self.value_type(value?);
            if !actual_ty.is_assignable_to(expected_ty) {
                self.eval.diag_ctx.emit_type_mismatch_error(
                    &self.eval.types,
                    expected_ty,
                    self.loc(self.bindings[type_local].span),
                    actual_ty,
                    self.loc(expr.span),
                );
                return Err(Poisoned);
            }
            value
        });
        let state = value.and_then(|value| match value {
            EvalValue::Comptime(vid) => Ok(LocalState::Comptime(vid)),
            EvalValue::Runtime { expr: _, result_type: _ } if self.is_comptime() => {
                self.eval.diag_ctx.emit_comptime_local_not_available(self.loc(expr.span));
                Err(Poisoned)
            }
            EvalValue::Runtime { expr, result_type } => {
                let target = self.alloc_mir(result_type);
                self.emit(mir::Instruction::Set { target, expr });
                Ok(LocalState::Runtime(target))
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
        let value = r#type.map_or(value, |type_local| {
            let expected_ty = self.expect_type(type_local)?;
            let actual_ty = self.value_type(value?);
            if !actual_ty.is_assignable_to(expected_ty) {
                self.eval.diag_ctx.emit_type_mismatch_error(
                    &self.eval.types,
                    expected_ty,
                    self.loc(self.bindings[type_local].span),
                    actual_ty,
                    self.loc(expr.span),
                );
                return Err(Poisoned);
            }
            value
        });

        let state = value.and_then(|value| {
            if self.is_comptime() {
                match value {
                    EvalValue::Comptime(vid) => Ok(LocalState::Comptime(vid)),
                    EvalValue::Runtime { expr: _, result_type: _ } => {
                        self.eval.diag_ctx.emit_comptime_local_not_available(self.loc(expr.span));
                        Err(Poisoned)
                    }
                }
            } else {
                let target = self.alloc_mir(self.value_type(value));
                let expr = match value {
                    EvalValue::Comptime(vid) => mir::Expr::Const(vid),
                    EvalValue::Runtime { expr, result_type: _ } => expr,
                };
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

                match (local.state.zip(value), self.is_comptime()) {
                    (Err(Poisoned), _) => {
                        // Already poisoned, so we don't even type check to supress
                        // potential error cascades.
                        self.bindings[target].state = Err(Poisoned);
                    }
                    (Ok((state, value)), true) => {
                        let (state, expected_ty) = match state {
                            LocalState::Comptime(vid) => (Ok(vid), self.values.type_of_value(vid)),
                            LocalState::Runtime(mir_local) => {
                                self.eval.diag_ctx.emit_runtime_assign_from_comptime(
                                    self.source,
                                    local.span,
                                    expr.span,
                                );
                                (Err(Poisoned), self.mir_types[mir_local])
                            }
                        };
                        let (value, actual_ty) = match value {
                            EvalValue::Comptime(vid) => (Ok(vid), self.values.type_of_value(vid)),
                            EvalValue::Runtime { result_type, expr: _ } => {
                                self.eval
                                    .diag_ctx
                                    .emit_runtime_eval_in_comptime(self.loc(expr.span));
                                (Err(Poisoned), result_type)
                            }
                        };
                        if !actual_ty.is_assignable_to(expected_ty) {
                            self.eval.diag_ctx.emit_type_mismatch_error(
                                &self.eval.types,
                                expected_ty,
                                self.loc(local.span),
                                actual_ty,
                                self.loc(expr.span),
                            );
                        }
                        self.bindings[target].state = match (state, value) {
                            (Ok(_), Ok(value)) => Ok(LocalState::Comptime(value)),
                            (Err(Poisoned), _) | (_, Err(Poisoned)) => Err(Poisoned),
                        };
                    }
                    (Ok((state, value)), false) => {
                        let LocalState::Runtime(mir_local) = state else {
                            unreachable!("runtime assign to value with existing mutable state")
                        };
                        let (mir_expr, actual_ty) = match value {
                            EvalValue::Comptime(vid) => {
                                (mir::Expr::Const(vid), self.values.type_of_value(vid))
                            }
                            EvalValue::Runtime { expr, result_type } => (expr, result_type),
                        };
                        let expected_ty = self.mir_types[mir_local];
                        if !actual_ty.is_assignable_to(expected_ty) {
                            self.eval.diag_ctx.emit_type_mismatch_error(
                                &self.eval.types,
                                expected_ty,
                                self.loc(local.span),
                                actual_ty,
                                self.loc(expr.span),
                            );
                            self.bindings[target].state = Err(Poisoned);
                        }
                        self.emit(mir::Instruction::Set { target: mir_local, expr: mir_expr });
                    }
                }
            }
            InstructionKind::Eval(expr) => match self.eval_expr(expr)? {
                Ok(EvalValue::Runtime { .. }) if self.is_comptime() => {
                    self.eval.diag_ctx.emit_runtime_eval_in_comptime(self.loc(expr.span));
                }
                Ok(EvalValue::Runtime { expr, result_type }) => {
                    // Lower incase the expression has side effect.
                    let target = self.alloc_mir(result_type);
                    self.emit(mir::Instruction::Set { target, expr });
                }
                Err(Poisoned) | Ok(EvalValue::Comptime(_)) => {
                    // Value with no side effect, do nothing.
                }
            },
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
