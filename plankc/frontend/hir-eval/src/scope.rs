use plank_core::{DenseIndexMap, IndexVec};
use plank_hir::{self as hir, ExprKind, InstructionKind};
use plank_mir as mir;
use plank_session::{SourceId, SourceSpan, SrcLoc};
use plank_values::{TypeId, Value, ValueId};

use crate::Evaluator;

pub(crate) type EvalResult = Result<Ref, BlockDiverge>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Ref {
    Runtime(mir::LocalId),
    Comptime(ValueId),
}

impl Ref {
    pub const ERROR: Self = Self::Comptime(ValueId::ERROR);
}

pub(crate) enum BlockDiverge {
    Never,
    Return(ValueId),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Local {
    pub state: Ref,
    pub span: SourceSpan,
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
                let loc = self.loc(
                    self.hir.block_spans[hir_block].expect("fn body block should have a span"),
                );
                self.diag_ctx.emit_entry_point_missing_terminator(loc);
            }
            Err(BlockDiverge::Never) => {
                // Desired guaranteed termination
            }
        }
        mir_block
    }

    pub fn ref_type(&self, r#ref: Ref) -> TypeId {
        match r#ref {
            Ref::Runtime(mir) => self.mir_types[mir],
            Ref::Comptime(vid) => self.values.type_of_value(vid),
        }
    }

    pub fn binding_type(&self, local: hir::LocalId) -> TypeId {
        self.ref_type(self.bindings[local].state)
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

    pub fn expect_type(&mut self, type_local: hir::LocalId) -> TypeId {
        let binding = &self.bindings[type_local];
        let type_loc = self.loc(binding.span);
        match &binding.state {
            Ref::Runtime(_) => {
                self.diag_ctx.emit_type_not_comptime(type_loc);
                TypeId::ERROR
            }
            &Ref::Comptime(vid) => match self.values.lookup(vid) {
                Value::Type(ty) => ty,
                Value::Error => TypeId::ERROR,
                _ => {
                    let actual_ty = self.values.type_of_value(vid);
                    let Evaluator { ref mut diag_ctx, ref types, .. } = *self.eval;
                    diag_ctx.emit_type_constraint_not_type(types, actual_ty, type_loc);
                    TypeId::ERROR
                }
            },
        }
    }

    pub fn check_type_of(
        &mut self,
        state: Ref,
        expr_span: SourceSpan,
        type_local: hir::LocalId,
    ) -> Ref {
        let expected_ty = self.expect_type(type_local);
        let actual_ty = self.ref_type(state);
        if !actual_ty.is_assignable_to(expected_ty) {
            self.eval.diag_ctx.emit_type_mismatch_error(
                &self.eval.types,
                expected_ty,
                self.loc(self.bindings[type_local].span),
                actual_ty,
                self.loc(expr_span),
            );
            return Ref::ERROR;
        } else if expected_ty == TypeId::ERROR {
            return Ref::ERROR;
        }
        state
    }

    pub fn eval_block(&mut self, block: hir::BlockId) -> (mir::BlockId, Result<(), BlockDiverge>) {
        self.with_instructions(|this| {
            for instr in &this.hir.block_instrs[block] {
                match instr.kind {
                    InstructionKind::Set { local, r#type, expr } => {
                        let result = this.eval_expr(expr)?;
                        let state = r#type.map_or(result, |type_local| {
                            this.check_type_of(result, expr.span, type_local)
                        });
                        this.bindings.insert_no_prev(local, Local { state, span: expr.span });
                    }
                    InstructionKind::SetMut { local, r#type, expr } => {
                        let result = this.eval_expr(expr)?;
                        let state = r#type.map_or(result, |type_local| {
                            this.check_type_of(result, expr.span, type_local)
                        });
                        this.bindings.insert_no_prev(local, Local { state, span: expr.span });
                    }
                    // InstructionKind::Assign { target, expr } => {
                    //     let new_result = this.eval_expr(expr)?;
                    //     let prev = this.bindings[target];
                    //     match prev.state {
                    //         Ref::Comptime(ValueId::ERROR) => {}
                    //         Ref::Runtime(mir) => {
                    //             let new_ty =
                    //
                    //         }
                    //     }
                    //     // let new_state
                    // }
                    InstructionKind::Eval(expr) => {
                        this.eval_expr(expr)?;
                    }
                    instr => todo!("instr: {instr:?}"),
                };
            }
            Ok(())
        })
    }

    pub fn ensure_materialized(&mut self, state: Ref) -> mir::LocalId {
        match state {
            Ref::Runtime(mir) => mir,
            Ref::Comptime(vid) => self.materialize(vid),
        }
    }

    pub fn alloc_anon_mir(&mut self, ty: TypeId) -> mir::LocalId {
        self.mir_types.push(ty)
    }

    pub fn materialize(&mut self, vid: ValueId) -> mir::LocalId {
        let ty = self.values.type_of_value(vid);
        let local = self.alloc_anon_mir(ty);
        self.instr_stack_buf
            .push(mir::Instruction::Set { target: local, expr: mir::Expr::Const(vid) });
        local
    }

    pub fn loc(&self, span: SourceSpan) -> SrcLoc {
        SrcLoc::new(self.source, span)
    }

    pub fn eval_expr(&mut self, expr: hir::Expr) -> Result<Ref, BlockDiverge> {
        let expr_loc = self.loc(expr.span);
        let r#ref = match expr.kind {
            ExprKind::Value(vid) => Ref::Comptime(vid),
            ExprKind::EvmBuiltinCall { builtin, args } => {
                self.eval_builtin(builtin, args, expr_loc)?
            }
            expr_kind => todo!("expr_kind: {expr_kind:?}"),
        };
        Ok(r#ref)
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
