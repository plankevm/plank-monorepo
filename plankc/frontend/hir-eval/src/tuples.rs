use crate::scope::{EvalValue, LocalState, Scope};
use plank_hir as hir;
use plank_mir as mir;
use plank_session::{MaybePoisoned, Poisoned, SourceSpan};
use plank_values::{MixedComptimeAndRuntime, TupleKey, TypeId, Value};

impl<'eval, 'ctx> Scope<'eval, 'ctx> {
    pub(crate) fn eval_tuple_type(
        &mut self,
        elements: hir::ElementsId,
        expr_span: SourceSpan,
    ) -> MaybePoisoned<TypeId> {
        self.with_types_buf(|this, types_buf_offset| {
            let mut poisoned = false;
            for &element in &this.hir.elements[elements] {
                let Ok(ty) = this.expect_type(element) else {
                    poisoned = true;
                    continue;
                };
                this.eval.types_buf.push(ty);
            }

            if poisoned {
                return Err(Poisoned);
            }

            let (tuple, ok) = this
                .eval
                .types
                .intern_tuple(TupleKey { elements: &this.eval.types_buf[types_buf_offset..] });

            if let Err(MixedComptimeAndRuntime) = ok {
                this.diag_ctx.emit_mixed_tuple_type(this.loc(expr_span), tuple, this.eval.values);
                return Err(Poisoned);
            }

            Ok(TypeId::from_tuple(tuple))
        })
    }

    pub(crate) fn eval_tuple_lit(
        &mut self,
        elements: hir::ElementsId,
        lit_span: SourceSpan,
    ) -> MaybePoisoned<EvalValue> {
        self.with_types_buf(|this, types_buf_offset| {
            this.with_values_buf(|this, values_buf_offset| {
                let mut validity = Ok(());
                let mut first_runtime_span = None;
                for &element in &this.hir.elements[elements] {
                    let Ok((state, use_span, origin)) = this.bindings[element].poisoned() else {
                        validity = Err(Poisoned);
                        continue;
                    };
                    let ty = this.state_type(state);
                    this.eval.types_buf.push(ty);

                    match state {
                        LocalState::Runtime(_) if this.is_comptime() => {
                            this.diag_ctx.emit_runtime_ref_in_comptime(
                                this.loc(lit_span),
                                this.origin_loc(origin),
                            );
                            validity = Err(Poisoned);
                        }
                        LocalState::Runtime(_) => {
                            first_runtime_span.get_or_insert(use_span);
                        }
                        LocalState::Comptime(value) => this.eval.values_buf.push(value),
                    }
                }

                validity?;

                // Mixed comptime/runtime checked independently
                let (tuple, _ok) = this
                    .eval
                    .types
                    .intern_tuple(TupleKey { elements: &this.eval.types_buf[types_buf_offset..] });
                let ty = TypeId::from_tuple(tuple);

                if let Some(runtime_span) = first_runtime_span {
                    this.eval_runtime_tuple_lit(ty, elements, lit_span, runtime_span)
                } else {
                    let elements = &this.eval.values_buf[values_buf_offset..];
                    let tuple = this.eval.values.intern(Value::TupleVal { ty, elements });
                    Ok(EvalValue::Comptime(tuple))
                }
            })
        })
    }

    fn eval_runtime_tuple_lit(
        &mut self,
        ty: TypeId,
        elements: hir::ElementsId,
        lit_span: SourceSpan,
        runtime_span: SourceSpan,
    ) -> MaybePoisoned<EvalValue> {
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
