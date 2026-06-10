use crate::scope::{EvalValue, LocalState, Scope};
use plank_hir as hir;
use plank_mir as mir;
use plank_session::{MaybePoisoned, Poisoned, SourceSpan};
use plank_values::{TupleInfo, TypeId, Value};

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
                .intern_tuple(TupleInfo { elements: &this.eval.types_buf[types_buf_offset..] });
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
            let mut has_runtime = false;
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

                match state {
                    LocalState::Comptime(_) => {}
                    LocalState::Runtime(_) => {
                        has_runtime = true;
                    }
                }
            }

            validity?;

            let tuple = this
                .eval
                .types
                .intern_tuple(TupleInfo { elements: &this.eval.types_buf[types_buf_offset..] });
            let ty = tuple.into();
            if has_runtime {
                this.runtime_eval_tuple_lit(ty, elements, lit_span)
            } else {
                this.fold_tuple_lit(ty, elements)
            }
        })
    }

    fn fold_tuple_lit(
        &mut self,
        ty: TypeId,
        elements: hir::ElementsId,
    ) -> MaybePoisoned<EvalValue> {
        self.with_values_buf(|this, values_buf_offset| {
            let mut validity = Ok(());
            for &element in &this.hir.elements[elements] {
                match this.bindings[element].state {
                    Ok(LocalState::Comptime(value)) => this.eval.values_buf.push(value),
                    Ok(LocalState::Runtime(_)) => {
                        unreachable!("tuple literal selected comptime path with runtime element")
                    }
                    Err(Poisoned) => {
                        validity = Err(Poisoned);
                    }
                }
            }

            validity.map(|()| {
                let elements = &this.eval.values_buf[values_buf_offset..];
                EvalValue::Comptime(this.eval.values.intern(Value::TupleVal { ty, elements }))
            })
        })
    }

    fn runtime_eval_tuple_lit(
        &mut self,
        ty: TypeId,
        elements: hir::ElementsId,
        lit_span: SourceSpan,
    ) -> MaybePoisoned<EvalValue> {
        self.with_locals_buf(|this, locals_buf_offset| {
            let mut validity = Ok(());
            let tuple_elements = &this.hir.elements[elements];

            if this.is_comptime() {
                for &element in tuple_elements {
                    let local = this.bindings[element];
                    let Ok(LocalState::Runtime(_)) = local.state else { continue };
                    this.diag_ctx.emit_runtime_ref_in_comptime(
                        this.loc(lit_span),
                        this.origin_loc(local.origin),
                    );
                    validity = Err(Poisoned);
                }
                return validity
                    .map(|()| unreachable!("runtime tuple literal without runtime element"));
            }

            let runtime_element = tuple_elements
                .iter()
                .copied()
                .find(|&element| matches!(this.bindings[element].state, Ok(LocalState::Runtime(_))))
                .expect("runtime tuple literal without runtime element");
            let runtime_span = this.bindings[runtime_element].use_span;

            for &element in tuple_elements {
                let local = this.bindings[element];
                let Ok(state) = local.state else {
                    validity = Err(Poisoned);
                    continue;
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

            validity.map(|()| {
                let locals = &this.eval.locals_buf[locals_buf_offset..];
                assert_eq!(locals.len(), tuple_elements.len());
                let elements = this.eval.mir_args.push_copy_slice(locals);
                EvalValue::Runtime { expr: mir::Expr::TupleLit { ty, elements }, result_type: ty }
            })
        })
    }
}
