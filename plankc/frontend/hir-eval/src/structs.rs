use crate::scope::{EvalValue, LocalState, Scope};
use plank_hir as hir;
use plank_mir as mir;
use plank_session::{MaybePoisoned, Poisoned, SourceSpan, SrcLoc, StrId};
use plank_values::{StructInfo, Type, TypeId, Value};

impl<'eval, 'ctx> Scope<'eval, 'ctx> {
    pub(crate) fn eval_struct_def(
        &mut self,
        struct_def_id: hir::StructDefId,
        def_expr_span: SourceSpan,
    ) -> MaybePoisoned<TypeId> {
        self.with_fields_buf(|this, fields_buf_offset| {
            let struct_def = this.hir.struct_defs[struct_def_id];
            let type_index = this.bindings[struct_def.type_index].poisoned().and_then(
                |(state, span)| match state {
                    LocalState::Comptime(vid) => Ok(vid),
                    LocalState::Runtime(_) => {
                        this.diag_ctx.emit_struct_type_index_not_comptime(this.loc(span));
                        Err(Poisoned)
                    }
                },
            );

            // Poisoned flag instead of short-circuit to make sure we validate as many
            // fields as possible.
            let mut fields_poisoned = false;
            let fields = &this.hir.fields[struct_def.fields];
            for (i, &field) in fields.iter().enumerate() {
                let Ok(ty) = this.expect_type(field.value) else {
                    fields_poisoned = true;
                    continue;
                };
                if let Some(first_offset) = fields[..i].iter().find_map(|prev_field| {
                    (prev_field.name == field.name).then_some(prev_field.name_offset)
                }) {
                    this.diag_ctx.emit_struct_def_duplicate_field(
                        this.source,
                        field.name,
                        first_offset,
                        field.name_offset,
                    );
                    fields_poisoned = true;
                }
                this.fields_buf.push((field.name, ty));
            }

            if fields_poisoned {
                return Err(Poisoned);
            }

            let struct_ty = this.eval.types.intern(Type::Struct(StructInfo {
                def_loc: this.loc(def_expr_span),
                type_index: type_index?,
                fields: &this.eval.fields_buf[fields_buf_offset..],
            }));
            Ok(struct_ty)
        })
    }

    pub(crate) fn eval_struct_member_access(
        &mut self,
        object: hir::LocalId,
        member: StrId,
        expr_span: SourceSpan,
    ) -> MaybePoisoned<EvalValue> {
        let (state, value_def_span) = self.bindings[object].poisoned()?;
        let struct_ty = self.state_type(state);
        let Type::Struct(struct_type_info) = self.types.lookup(struct_ty) else {
            self.diag_ctx.emit_member_on_non_struct(
                &self.eval.types,
                struct_ty,
                self.loc(value_def_span),
            );
            return Err(Poisoned);
        };

        let Some((field_index, &(_name, field_type))) = (0u32..)
            .zip(struct_type_info.fields)
            .find(|&(_i, &(field_name, _ty))| field_name == member)
        else {
            self.diag_ctx.emit_struct_unknown_field_access(
                &self.eval.types,
                struct_ty,
                self.loc(expr_span),
                member,
            );
            return Err(Poisoned);
        };

        match state {
            LocalState::Comptime(vid) => {
                let Value::StructVal { ty: _, fields } = self.values.lookup(vid) else {
                    unreachable!("invariant: `state_type` != type of value")
                };
                Ok(EvalValue::Comptime(fields[field_index as usize]))
            }
            LocalState::Runtime(local) => Ok(EvalValue::Runtime {
                expr: mir::Expr::FieldAccess { object: local, field_index },
                result_type: field_type,
            }),
        }
    }

    pub(crate) fn eval_struct_lit(
        &mut self,
        struct_type_local: hir::LocalId,
        fields: hir::FieldsId,
        lit_span: SourceSpan,
    ) -> MaybePoisoned<EvalValue> {
        self.with_values_buf(|this, values_buf_offset| {
            this.with_locals_buf(|this, locals_buf_offset| {
                this.with_fields_buf(|this, fields_buf_offset| {
                    this.eval_struct_lit_inner(
                        struct_type_local,
                        fields,
                        lit_span,
                        values_buf_offset,
                        locals_buf_offset,
                        fields_buf_offset,
                    )
                })
            })
        })
    }

    fn struct_lit_diagnose_duplicate_fields(
        &mut self,
        lit_loc: SrcLoc,
        lit_fields: &[hir::FieldInfo],
    ) -> MaybePoisoned<()> {
        let mut validity = Ok(());
        for (i, cur_field) in lit_fields.iter().enumerate() {
            if let Some(prev) =
                lit_fields[..i].iter().find(|prev_field| prev_field.name == cur_field.name)
            {
                self.diag_ctx.emit_struct_duplicate_field(
                    cur_field.name,
                    lit_loc,
                    prev.name_offset,
                    cur_field.name_offset,
                );
                validity = Err(Poisoned);
            }
        }
        validity
    }

    fn force_fold_struct_lit(
        &mut self,
        mut validity: MaybePoisoned<()>,
        struct_ty: TypeId,
        def_fields_buf_offset: usize,
        lit_fields: &[hir::FieldInfo],
        lit_span: SourceSpan,
        values_buf_offset: usize,
    ) -> MaybePoisoned<EvalValue> {
        let def_fields = &self.eval.fields_buf[def_fields_buf_offset..];
        for &(name, _ty) in def_fields {
            let Some(lit_field) = lit_fields.iter().find(|lit_field| lit_field.name == name) else {
                // should've already been set above but just incase.
                validity = Err(Poisoned);
                continue;
            };
            let local = self.bindings[lit_field.value];
            let Ok(state) = local.state else {
                // should've already been set if state poisoned but just incase.
                validity = Err(Poisoned);
                continue;
            };
            let LocalState::Comptime(value) = state else {
                self.diag_ctx.emit_runtime_ref_in_comptime(self.source, lit_span, local.span);
                validity = Err(Poisoned);
                continue;
            };
            self.eval.values_buf.push(value);
        }

        validity.map(|()| {
            let field_values = &self.eval.values_buf[values_buf_offset..];
            assert_eq!(field_values.len(), def_fields.len());
            EvalValue::Comptime(
                self.eval.values.intern(Value::StructVal { ty: struct_ty, fields: field_values }),
            )
        })
    }

    // R*st wants me to *manually* split things to appease the borrow checker and the linter
    // complains that my function has "too many arguments", ok man, please kys.
    #[allow(clippy::too_many_arguments)]
    fn runtime_eval_struct_lit(
        &mut self,
        mut validity: MaybePoisoned<()>,
        struct_ty: TypeId,
        values_buf_offset: usize,
        def_fields_buf_offset: usize,
        locals_buf_offset: usize,
        lit_fields: &[hir::FieldInfo],
        lit_span: SourceSpan,
    ) -> MaybePoisoned<EvalValue> {
        let def_fields = &self.eval.fields_buf[def_fields_buf_offset..];
        let mut first_runtime_field = None;

        for &(name, _ty) in def_fields {
            let Some(&lit_field) = lit_fields.iter().find(|lit_field| lit_field.name == name)
            else {
                // should've already been set above but just incase.
                validity = Err(Poisoned);
                continue;
            };
            let local = self.bindings[lit_field.value];
            let Ok(state) = local.state else {
                // should've already been set if state poisoned but just incase.
                validity = Err(Poisoned);
                continue;
            };

            match state {
                LocalState::Runtime(mir_local) => {
                    if first_runtime_field.is_none() {
                        // One time conversion of already pushed values.
                        'materialize_comptime: for (&value, &(name, _ty)) in
                            self.eval.values_buf[values_buf_offset..].iter().zip(def_fields)
                        {
                            let value_ty = self.values.type_of_value(value);
                            if self.types.comptime_only(value_ty) {
                                let &comptime_lit_field = lit_fields
                                    .iter()
                                    .find(|lit_field| lit_field.name == name)
                                    .expect("pushed, but not skipped by lit_fields.find?");
                                self.diag_ctx.emit_mixed_comptime_runtime_struct(
                                    self.source,
                                    lit_span,
                                    comptime_lit_field,
                                    lit_field,
                                );

                                validity = Err(Poisoned);
                                continue 'materialize_comptime;
                            }

                            let tmp_local = self.mir_types.push(value_ty);
                            self.eval.instr_stack_buf.push(mir::Instruction::Set {
                                target: tmp_local,
                                expr: mir::Expr::Const(value),
                            });
                        }

                        first_runtime_field = Some(lit_field);
                    }
                    self.eval.locals_buf.push(mir_local);
                }
                LocalState::Comptime(value) => {
                    let Some(first_runtime_field) = first_runtime_field else {
                        self.eval.values_buf.push(value);
                        continue;
                    };

                    let value_ty = self.values.type_of_value(value);
                    if self.types.comptime_only(value_ty) {
                        self.diag_ctx.emit_mixed_comptime_runtime_struct(
                            self.source,
                            lit_span,
                            lit_field,
                            first_runtime_field,
                        );
                        validity = Err(Poisoned);
                        continue;
                    }
                    let tmp_local = self.mir_types.push(value_ty);
                    self.eval.instr_stack_buf.push(mir::Instruction::Set {
                        target: tmp_local,
                        expr: mir::Expr::Const(value),
                    });
                }
            }
        }

        validity.map(|()| match first_runtime_field {
            None => {
                let field_values = &self.eval.values_buf[values_buf_offset..];
                assert_eq!(field_values.len(), def_fields.len());
                EvalValue::Comptime(
                    self.eval
                        .values
                        .intern(Value::StructVal { ty: struct_ty, fields: field_values }),
                )
            }
            Some(_) => {
                let locals = &self.eval.locals_buf[locals_buf_offset..];
                assert_eq!(locals.len(), def_fields.len());
                let fields = self.eval.mir_args.push_copy_slice(locals);
                EvalValue::Runtime {
                    expr: mir::Expr::StructLit { ty: struct_ty, fields },
                    result_type: struct_ty,
                }
            }
        })
    }

    fn eval_struct_lit_inner(
        &mut self,
        struct_type_local: hir::LocalId,
        fields: hir::FieldsId,
        lit_span: SourceSpan,
        values_buf_offset: usize,
        locals_buf_offset: usize,
        fields_buf_offset: usize,
    ) -> MaybePoisoned<EvalValue> {
        let lit_fields = &self.eval.hir.fields[fields];
        let lit_loc = self.loc(lit_span);

        let mut validity = self.struct_lit_diagnose_duplicate_fields(lit_loc, lit_fields);

        // Retrieve struct type information.
        let ty_loc = self.loc(self.bindings[struct_type_local].span);
        let struct_ty = self.expect_type(struct_type_local)?;
        let Type::Struct(def) = self.eval.types.lookup(struct_ty) else {
            self.diag_ctx.emit_not_a_struct_type(&self.eval.types, struct_ty, ty_loc);
            return Err(Poisoned);
        };
        // Save to temporary buffer because Rust borrow checker is a f***ing ****
        self.eval.fields_buf.extend_from_slice(def.fields);

        // Diagnose field existence and type match.
        for &lit_field in lit_fields {
            let Some(&(_, expected_field_ty)) =
                def.fields.iter().find(|&&(name, _ty)| name == lit_field.name)
            else {
                self.diag_ctx.emit_struct_lit_unexpected_field(
                    &self.eval.types,
                    struct_ty,
                    lit_loc,
                    lit_field,
                );
                validity = Err(Poisoned);
                continue;
            };
            let Ok((field_value_state, field_value_span)) =
                self.bindings[lit_field.value].poisoned()
            else {
                validity = Err(Poisoned);
                continue;
            };
            let field_value_ty = self.state_type(field_value_state);
            if !field_value_ty.is_assignable_to(expected_field_ty) {
                self.diag_ctx.emit_struct_literal_field_type_mismatch(
                    &self.eval.types,
                    expected_field_ty,
                    field_value_ty,
                    self.loc(field_value_span),
                    lit_field.name,
                );
                validity = Err(Poisoned);
                continue;
            }
        }

        // Check for missing fields.
        for &(name, _ty) in def.fields {
            if !lit_fields.iter().any(|lit_field| lit_field.name == name) {
                self.diag_ctx.emit_struct_missing_field(&self.eval.types, struct_ty, name, lit_loc);
                validity = Err(Poisoned);
            };
        }

        // Attempt to build literal value.
        if self.is_comptime() {
            self.force_fold_struct_lit(
                validity,
                struct_ty,
                fields_buf_offset,
                lit_fields,
                lit_span,
                values_buf_offset,
            )
        } else {
            self.runtime_eval_struct_lit(
                validity,
                struct_ty,
                values_buf_offset,
                fields_buf_offset,
                locals_buf_offset,
                lit_fields,
                lit_span,
            )
        }
    }
}
