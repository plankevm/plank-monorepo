use crate::scope::{EvalValue, LocalState, Scope};
use plank_hir as hir;
use plank_mir as mir;
use plank_session::{MaybePoisoned, Poisoned, SourceSpan};
use plank_values::{TupleKey, TypeId, Value};

impl<'eval, 'ctx> Scope<'eval, 'ctx> {
    pub(crate) fn eval_tuple_type(&mut self, elements: hir::ElementsId) -> MaybePoisoned<TypeId> {
        self.with_types_buf(|this, types_buf_offset| {
            let mut validity = Ok(());
            for &element in &this.hir.elements[elements] {
                let Ok(ty) = this.expect_type(element) else {
                    validity = Err(Poisoned);
                    continue;
                };
                if ty == TypeId::NEVER {
                    let element_loc = this.loc(this.bindings[element].use_span);
                    this.diag_ctx.emit_never_as_tuple_element(element_loc);
                    validity = Err(Poisoned);
                    continue;
                }
                this.eval.types_buf.push(ty);
            }

            validity?;

            let tuple = this
                .eval
                .types
                .intern_tuple(TupleKey { elements: &this.eval.types_buf[types_buf_offset..] });
            Ok(tuple.into())
        })
    }

    pub(crate) fn eval_tuple_lit(
        &mut self,
        elements: hir::ElementsId,
        lit_span: SourceSpan,
    ) -> MaybePoisoned<EvalValue> {
        self.with_types_buf(|this, types_buf_offset| {
            let mut validity = Ok(());
            let mut first_runtime_span = None;
            for &element in &this.hir.elements[elements] {
                let Ok(state) = this.bindings[element].state else {
                    validity = Err(Poisoned);
                    continue;
                };
                let ty = this.state_type(state);
                if ty == TypeId::NEVER {
                    let element_loc = this.loc(this.bindings[element].use_span);
                    this.diag_ctx.emit_never_as_tuple_element(element_loc);
                    validity = Err(Poisoned);
                    continue;
                }
                this.eval.types_buf.push(ty);

                if let LocalState::Runtime(_) = state {
                    first_runtime_span.get_or_insert(this.bindings[element].use_span);
                }
            }

            validity?;

            let tuple = this
                .eval
                .types
                .intern_tuple(TupleKey { elements: &this.eval.types_buf[types_buf_offset..] });
            let ty = tuple.into();
            if let Some(runtime_span) = first_runtime_span {
                this.eval_runtime_tuple_lit(ty, elements, lit_span, runtime_span)
            } else {
                Ok(this.eval_comptime_tuple_lit(ty, elements))
            }
        })
    }

    fn eval_comptime_tuple_lit(&mut self, ty: TypeId, elements: hir::ElementsId) -> EvalValue {
        self.with_values_buf(|this, values_buf_offset| {
            for &element in &this.hir.elements[elements] {
                let Ok(LocalState::Comptime(value)) = this.bindings[element].state else {
                    unreachable!("tuple literal selected comptime path with non-comptime element")
                };
                this.eval.values_buf.push(value);
            }

            let elements = &this.eval.values_buf[values_buf_offset..];
            EvalValue::Comptime(this.eval.values.intern(Value::TupleVal { ty, elements }))
        })
    }

    fn eval_runtime_tuple_lit(
        &mut self,
        ty: TypeId,
        elements: hir::ElementsId,
        lit_span: SourceSpan,
        runtime_span: SourceSpan,
    ) -> MaybePoisoned<EvalValue> {
        if self.is_comptime() {
            for &element in &self.hir.elements[elements] {
                let local = self.bindings[element];
                let Ok(LocalState::Runtime(_)) = local.state else { continue };
                self.diag_ctx.emit_runtime_ref_in_comptime(
                    self.loc(lit_span),
                    self.origin_loc(local.origin),
                );
            }
            return Err(Poisoned);
        }

        self.with_locals_buf(|this, locals_buf_offset| {
            let tuple_elements = &this.hir.elements[elements];
            let mut validity = Ok(());

            for &element in tuple_elements {
                let local = this.bindings[element];
                let Ok(state) = local.state else {
                    unreachable!("tuple literal selected runtime path with poisoned element")
                };

                match state {
                    LocalState::Runtime(mir_local) => {
                        this.eval.locals_buf.push(mir_local);
                    }
                    LocalState::Comptime(value) => {
                        let value_ty = this.values.type_of_value(value);
                        if this.types.is_comptime_only(value_ty) {
                            this.diag_ctx.emit_mixed_comptime_runtime_tuple(
                                this.source,
                                lit_span,
                                local.use_span,
                                runtime_span,
                            );
                            validity = Err(Poisoned);
                            continue;
                        }

                        let tmp_local = this.mir_types.push(value_ty);
                        this.eval.instr_stack_buf.push(mir::Instruction::Set {
                            target: tmp_local,
                            expr: mir::Expr::Const(value),
                        });
                        this.eval.locals_buf.push(tmp_local);
                    }
                }
            }

            validity?;

            let locals = &this.eval.locals_buf[locals_buf_offset..];
            assert_eq!(locals.len(), tuple_elements.len());
            let elements = this.eval.mir_args.push_copy_slice(locals);
            Ok(EvalValue::Runtime { expr: mir::Expr::TupleLit { ty, elements }, result_type: ty })
        })
    }
}
