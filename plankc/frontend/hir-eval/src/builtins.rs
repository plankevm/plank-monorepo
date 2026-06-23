use crate::scope::{Diverge, EvalValue, Local, LocalState, Scope};
use alloy_primitives::U256;
use plank_hir as hir;
use plank_mir as mir;
use plank_session::{
    Builtin, BytesId, CBytes, MaybePoisoned, Poisoned, RuntimeBuiltin, SourceSpan, SrcLoc,
    builtins::BuiltinKind,
};
use plank_values::{
    Compound, DefOrigin, PrimitiveType, StructView, TupleKey, Type, TypeFlags, TypeId,
    TypeInterner, TypeName, Value, ValueId, ValueInterner, builtins as builtin_sigs,
};
use sha2::{Digest, Sha256};

struct ClosureArg {
    value: ValueId,
    fn_def_id: hir::FnDefId,
    captures: Vec<(ValueId, DefOrigin)>,
}

impl<'a, 'ctx> Scope<'a, 'ctx> {
    pub(crate) fn eval_builtin_call(
        &mut self,
        builtin: Builtin,
        args: hir::ArgsId,
        expr_span: SourceSpan,
    ) -> MaybePoisoned<Result<EvalValue, Diverge>> {
        let args = &self.eval.hir.args[args];
        match builtin {
            Builtin::Runtime(runtime) => {
                if runtime.foldable() {
                    self.eval_runtime_foldable_builtin(runtime, args, expr_span)
                } else {
                    self.eval_runtime_only_builtin(runtime, args, expr_span)
                }
            }
            builtin => match builtin.kind() {
                BuiltinKind::Comptime => self.eval_comptime_builtin(builtin, args, expr_span),
                BuiltinKind::ComptimeDynamic { .. } => {
                    self.eval_comptime_dynamic_builtin(builtin, args, expr_span)
                }
                BuiltinKind::RuntimeFoldable | BuiltinKind::RuntimeOnly => {
                    unreachable!("already matched")
                }
            },
        }
    }

    pub fn eval_runtime_foldable_builtin(
        &mut self,
        builtin: RuntimeBuiltin,
        args: &[hir::LocalId],
        expr_span: SourceSpan,
    ) -> MaybePoisoned<Result<EvalValue, Diverge>> {
        let result_type = self.resolve_runtime_builtin_result_type(builtin, args, expr_span)?;

        let folded = self.with_values_buf(|this, values_buf_offset| {
            for &arg in args {
                let (state, _arg_use_span, arg_origin) =
                    this.bindings[arg].poisoned().expect("invariant: arg type check checks poison");
                match state {
                    LocalState::Comptime(vid) => this.values_buf.push(vid),
                    LocalState::Runtime(_) if this.is_comptime() => {
                        this.diag_ctx.emit_runtime_ref_in_comptime(
                            this.loc(expr_span),
                            this.origin_loc(arg_origin),
                        );
                        return Err(Poisoned);
                    }
                    LocalState::Runtime(_) => return Ok(None),
                }
            }
            let result = fold_runtime_builtin(
                builtin,
                &this.eval.values_buf[values_buf_offset..],
                this.eval.values,
            );
            Ok(Some(match result_type {
                TypeId::U256 => this.eval.values.intern_num(result),
                TypeId::BOOL => match result {
                    U256::ZERO => ValueId::FALSE,
                    U256::ONE => ValueId::TRUE,
                    x => unreachable!("{x} can't be turned into `bool`"),
                },
                ty => unreachable!(
                    "unsupported result type `{}`",
                    this.eval.types.format(this.diag_ctx.session, this.eval.values, ty)
                ),
            }))
        })?;
        if let Some(value) = folded {
            return Ok(Ok(EvalValue::Comptime(value)));
        }

        Ok(self.emit_runtime_builtin_mir(builtin, args, result_type))
    }

    fn eval_runtime_only_builtin(
        &mut self,
        builtin: RuntimeBuiltin,
        args: &[hir::LocalId],
        expr_span: SourceSpan,
    ) -> MaybePoisoned<Result<EvalValue, Diverge>> {
        let result_type = self.resolve_runtime_builtin_result_type(builtin, args, expr_span);
        let poisoned_never =
            result_type.is_err() && builtin_sigs::builtin_returns_never(builtin.into());

        if self.is_comptime() {
            self.diag_ctx.emit_unsupported_eval_of_runtime_builtin(builtin, self.loc(expr_span));
            if result_type == Ok(TypeId::NEVER) || poisoned_never {
                return Ok(Err(Diverge::ControlFlowPoisoned));
            } else {
                return Err(Poisoned);
            }
        }

        match result_type {
            Ok(result_type) => Ok(self.emit_runtime_builtin_mir(builtin, args, result_type)),
            Err(Poisoned) if poisoned_never => Ok(Err(Diverge::END)),
            Err(Poisoned) => Err(Poisoned),
        }
    }

    fn resolve_runtime_builtin_result_type(
        &mut self,
        builtin: RuntimeBuiltin,
        args: &[hir::LocalId],
        expr_span: SourceSpan,
    ) -> MaybePoisoned<TypeId> {
        let expr_loc = self.loc(expr_span);
        self.with_types_buf(|this, types_buf_offset| {
            for &arg in args {
                let ty = this.state_type(this.bindings[arg].state?);
                this.eval.types_buf.push(ty);
            }

            let arg_types = &this.eval.types_buf[types_buf_offset..];
            builtin_sigs::resolve_result_type(builtin.into(), arg_types).ok_or_else(|| {
                this.diag_ctx.emit_no_matching_builtin_signature(
                    this.eval.values,
                    builtin.into(),
                    &this.eval.types_buf[types_buf_offset..],
                    expr_loc,
                );
                Poisoned
            })
        })
    }

    fn emit_runtime_builtin_mir(
        &mut self,
        builtin: RuntimeBuiltin,
        args: &[hir::LocalId],
        result_type: TypeId,
    ) -> Result<EvalValue, Diverge> {
        let mir_args = self.with_locals_buf(|this, locals_buf_offset| {
            for &arg in args {
                let state =
                    this.bindings[arg].state.expect("invariant: arg type check checks poison");
                if let LocalState::Comptime(vid) = state {
                    assert!(
                        !this.is_comptime_only(vid),
                        "runtime builtin typechecks for comptime only value"
                    );
                }
                let ty = this.state_type(state);
                let local = this.materialize_as_local(state, ty);
                this.locals_buf.push(local);
            }
            this.eval.mir_args.push_copy_slice(&this.eval.locals_buf[locals_buf_offset..])
        });

        let expr = mir::Expr::RuntimeBuiltinCall { builtin, args: mir_args };
        if result_type == TypeId::NEVER {
            // We diverge after this so we need to make sure the call is actually included.
            let target = self.mir_types.push(result_type);
            self.emit(mir::Instruction::Set { target, expr });
            return Err(Diverge::BlockEnd(None));
        }

        Ok(EvalValue::Runtime { expr, result_type })
    }

    fn eval_comptime_builtin(
        &mut self,
        builtin: Builtin,
        args: &[hir::LocalId],
        expr_span: SourceSpan,
    ) -> MaybePoisoned<Result<EvalValue, Diverge>> {
        let expr_loc = self.loc(expr_span);

        if builtin_sigs::arg_count(builtin) != args.len() {
            self.diag_ctx.emit_wrong_arg_count(self.eval.values, builtin, args.len(), expr_loc);
            return Err(Poisoned);
        }

        match builtin {
            Builtin::IsStruct => {
                let &[ty_local] = args else { unreachable!("arg count checked") };
                let ty = self.expect_type_arg(ty_local, builtin, expr_span)?;
                let is_struct = ty.is_struct();
                Ok(Ok(EvalValue::Comptime(is_struct.into())))
            }
            Builtin::IsTuple => {
                let &[ty_local] = args else { unreachable!("arg count checked") };
                let ty = self.expect_type_arg(ty_local, builtin, expr_span)?;
                let is_tuple = ty.is_tuple();
                Ok(Ok(EvalValue::Comptime(is_tuple.into())))
            }
            Builtin::HasPlainName | Builtin::HasParameterizedName => {
                let &[ty_local] = args else { unreachable!("arg count checked") };
                let ty = self.expect_type_arg(ty_local, builtin, expr_span)?;
                let r#struct = self.expect_struct(ty, builtin, expr_span)?;
                let matches_name_kind = match builtin {
                    Builtin::HasPlainName => {
                        matches!(r#struct.name.get(), Some(TypeName::Plain(_)))
                    }
                    Builtin::HasParameterizedName => {
                        matches!(r#struct.name.get(), Some(TypeName::Parameterized { .. }))
                    }
                    _ => unreachable!("matched above"),
                };
                Ok(Ok(EvalValue::Comptime(matches_name_kind.into())))
            }
            Builtin::TypeName => {
                let &[ty_local] = args else { unreachable!("arg count checked") };
                let ty = self.expect_type_arg(ty_local, builtin, expr_span)?;
                self.expect_struct(ty, builtin, expr_span)?;
                let name =
                    self.types.format(self.diag_ctx.session, self.eval.values, ty).to_string();
                let cbytes = self.diag_ctx.session.intern_cbytes(name.as_bytes());
                Ok(Ok(EvalValue::Comptime(self.eval.values.intern_bytes(
                    cbytes.contents,
                    cbytes.start,
                    cbytes.end,
                ))))
            }
            Builtin::FieldName => {
                let &[ty_local, index_local] = args else { unreachable!("arg count checked") };
                let ty = self.expect_type_arg(ty_local, builtin, expr_span)?;
                let r#struct = self.expect_struct(ty, builtin, expr_span)?;
                let index = self.expect_field_index_arg(
                    index_local,
                    builtin,
                    expr_span,
                    r#struct.fields.len(),
                )?;
                let field = r#struct.fields[index];
                let contents = BytesId::from(field.name);
                let len = self.diag_ctx.session.bytes_len(contents);
                Ok(Ok(EvalValue::Comptime(self.eval.values.intern_bytes(contents, 0, len))))
            }
            Builtin::FieldIndex => {
                let &[ty_local, name_local] = args else { unreachable!("arg count checked") };
                let ty = self.expect_type_arg(ty_local, builtin, expr_span)?;
                let r#struct = self.expect_struct(ty, builtin, expr_span)?;
                let name = self.expect_bytes_arg(name_local, builtin, expr_span)?;
                let index =
                    self.find_struct_field_by_name(r#struct, name).unwrap_or(r#struct.fields.len());
                let index = U256::from(index);
                Ok(Ok(EvalValue::Comptime(self.eval.values.intern_num(index))))
            }
            Builtin::FieldCount => {
                let &[r#struct] = args else { unreachable!("arg count checked") };
                let ty = self.expect_type_arg(r#struct, builtin, expr_span)?;
                let field_count = self.expect_compound(ty, builtin, expr_span)?.field_count();
                let count = U256::from(field_count);
                Ok(Ok(EvalValue::Comptime(self.eval.values.intern_num(count))))
            }
            Builtin::FnName => {
                let &[closure] = args else { unreachable!("arg count checked") };
                let closure = self.expect_closure_arg(closure, builtin)?;
                let value = match self.values.get_closure_name(closure.value) {
                    Some(name) => {
                        let contents = BytesId::from(name);
                        let len = self.diag_ctx.session.bytes_len(contents);
                        self.eval.values.intern_bytes(contents, 0, len)
                    }
                    None => ValueId::BYTES_EMPTY,
                };
                Ok(Ok(EvalValue::Comptime(value)))
            }
            Builtin::GetComptimeParamCount => {
                let &[closure] = args else { unreachable!("arg count checked") };
                let closure = self.expect_closure_arg(closure, builtin)?;
                let count = self.comptime_param_count(closure.fn_def_id);
                Ok(Ok(EvalValue::Comptime(self.eval.values.intern_num(U256::from(count)))))
            }
            Builtin::InComptime => Ok(Ok(EvalValue::Comptime(self.comptime.into()))),
            Builtin::SetEvalBranchQuota => {
                let &[quota_arg] = args else { unreachable!("arg count checked") };
                let binding = self.bindings[quota_arg];
                let (state, arg_use_span, arg_origin) = binding.poisoned()?;
                let LocalState::Comptime(quota_value) = state else {
                    self.diag_ctx.emit_runtime_ref_in_comptime(
                        self.loc(expr_span),
                        self.origin_loc(arg_origin),
                    );
                    return Err(Poisoned);
                };
                let requested_quota = match self.values.lookup(quota_value) {
                    Value::BigNum(requested_quota) => requested_quota,
                    other => {
                        self.diag_ctx.emit_no_matching_builtin_signature(
                            self.eval.values,
                            builtin,
                            &[other.get_type()],
                            expr_loc,
                        );
                        return Err(Poisoned);
                    }
                };
                let Ok(requested_quota) = u32::try_from(requested_quota) else {
                    self.diag_ctx.emit_eval_branch_quota_too_large(self.loc(arg_use_span));
                    return Err(Poisoned);
                };
                self.comptime_quota.raise_limit(requested_quota);
                self.max_eval_branch_quota_seen =
                    self.max_eval_branch_quota_seen.max(requested_quota);
                Ok(Ok(EvalValue::Comptime(ValueId::VOID)))
            }
            Builtin::CompileError => {
                let &[message] = args else { unreachable!("arg count checked") };
                let message = self.expect_bytes_arg(message, builtin, expr_span)?;
                let message = self.diag_ctx.session.lookup_bytes_lossy(message);
                self.diag_ctx.emit_custom_comptime_error(message, self.loc(expr_span));
                Ok(Err(Diverge::ControlFlowPoisoned))
            }
            Builtin::SliceCBytes => {
                let &[bytes, start, end] = args else { unreachable!("arg count checked") };
                let bytes = self.expect_bytes_arg(bytes, builtin, expr_span)?;
                let start = self.expect_comptime_u256(start, builtin, "slice start", expr_span)?;
                let end = self.expect_comptime_u256(end, builtin, "slice end", expr_span)?;
                let len = bytes.len();
                if start > end || end > U256::from(len) {
                    self.diag_ctx.emit_bytes_slice_out_of_bounds(start, end, len, expr_loc);
                    return Err(Poisoned);
                }
                let start = u32::try_from(start).expect("start <= end <= len which fits u32");
                let end = u32::try_from(end).expect("end <= len which fits u32");
                Ok(Ok(EvalValue::Comptime(self.eval.values.intern_bytes(
                    bytes.contents,
                    bytes.start + start,
                    bytes.start + end,
                ))))
            }
            Builtin::PaddedReadCBytes => {
                let &[bytes, offset] = args else { unreachable!("arg count checked") };
                let bytes = self.expect_bytes_arg(bytes, builtin, expr_span)?;
                let offset =
                    self.expect_comptime_u256(offset, builtin, "cbytes offset", expr_span)?;

                let slice = self.diag_ctx.session.lookup_bytes_slice(bytes);
                let offset = match usize::try_from(offset) {
                    Ok(offset) if offset <= slice.len() => offset,
                    _ => {
                        self.diag_ctx.emit_cbytes_read_offset_out_of_bounds(
                            offset,
                            slice.len(),
                            expr_loc,
                        );
                        return Err(Poisoned);
                    }
                };

                let slice = &slice[offset..(offset + 32).min(slice.len())];
                let mut word = [0; 32];
                word[..slice.len()].copy_from_slice(slice);

                let value = U256::from_be_bytes(word);
                Ok(Ok(EvalValue::Comptime(self.eval.values.intern_num(value))))
            }
            Builtin::Keccak256CBytes => {
                let &[bytes] = args else { unreachable!("arg count checked") };
                let bytes = self.expect_bytes_arg(bytes, builtin, expr_span)?;
                let slice = self.diag_ctx.session.lookup_bytes_slice(bytes);
                let hash = U256::from_be_bytes(alloy_primitives::keccak256(slice).0);
                Ok(Ok(EvalValue::Comptime(self.eval.values.intern_num(hash))))
            }
            Builtin::Sha256CBytes => {
                let &[bytes] = args else { unreachable!("arg count checked") };
                let bytes = self.expect_bytes_arg(bytes, builtin, expr_span)?;
                let slice = self.diag_ctx.session.lookup_bytes_slice(bytes);
                let hash = U256::from_be_bytes(Sha256::digest(slice).into());
                Ok(Ok(EvalValue::Comptime(self.eval.values.intern_num(hash))))
            }
            Builtin::DataOffset => {
                let &[bytes] = args else { unreachable!("arg count checked") };
                let bytes = self.expect_bytes_arg(bytes, builtin, expr_span)?;
                if self.is_comptime() {
                    self.diag_ctx.emit_data_offset_in_comptime(expr_loc);
                    return Err(Poisoned);
                }
                Ok(Ok(EvalValue::Runtime {
                    expr: mir::Expr::DataOffset { contents: bytes.contents, start: bytes.start },
                    result_type: TypeId::U256,
                }))
            }
            Builtin::ActiveEvmVersion => {
                let evm_version: U256 = self.eval.evm_version.into();
                Ok(Ok(EvalValue::Comptime(self.eval.values.intern_num(evm_version))))
            }
            _ => unreachable!("not a comptime builtin: {builtin}"),
        }
    }

    fn eval_comptime_dynamic_builtin(
        &mut self,
        builtin: Builtin,
        args: &[hir::LocalId],
        expr: SourceSpan,
    ) -> MaybePoisoned<Result<EvalValue, Diverge>> {
        if builtin_sigs::arg_count(builtin) != args.len() {
            self.diag_ctx.emit_wrong_arg_count(
                self.eval.values,
                builtin,
                args.len(),
                self.loc(expr),
            );
            return Err(Poisoned);
        }

        match builtin {
            Builtin::FieldType => self.eval_field_type(args, builtin, expr),
            Builtin::TypeIndex => self.eval_type_index(args, builtin, expr),
            Builtin::GetField => self.eval_get_field(args, builtin, expr),
            Builtin::SetField => self.eval_set_field(args, builtin, expr),
            Builtin::Uninit => self.eval_uninit(args, builtin, expr),
            Builtin::ConcatCBytes => self.eval_concat_cbytes(args, expr),
            Builtin::CompileLog => self.eval_compile_log(args, expr),
            Builtin::GetRuntimeSignature => self.eval_get_runtime_signature(args, expr),
            Builtin::GetReturnType => self.eval_get_return_type(args, expr),
            Builtin::Call => self.eval_call_builtin(args, expr),
            _ => unreachable!("not a comptime dynamic builtin: {builtin}"),
        }
    }

    fn eval_field_type(
        &mut self,
        args: &[hir::LocalId],
        builtin: Builtin,
        expr_span: SourceSpan,
    ) -> MaybePoisoned<Result<EvalValue, Diverge>> {
        let &[ty, field_index] = args else { unreachable!("arg count checked") };
        let ty = self.expect_type_arg(ty, builtin, expr_span)?;
        let compound = self.expect_compound(ty, builtin, expr_span)?;
        let index =
            self.expect_field_index_arg(field_index, builtin, expr_span, compound.field_count())?;
        let field_ty = compound.field_type(index);
        Ok(Ok(EvalValue::Comptime(self.eval.values.intern_type(field_ty))))
    }

    fn eval_type_index(
        &mut self,
        args: &[hir::LocalId],
        builtin: Builtin,
        expr_span: SourceSpan,
    ) -> MaybePoisoned<Result<EvalValue, Diverge>> {
        let &[ty] = args else { unreachable!("arg count checked") };
        let ty = self.expect_type_arg(ty, builtin, expr_span)?;
        let r#struct = self.expect_struct(ty, builtin, expr_span)?;
        Ok(Ok(EvalValue::Comptime(r#struct.type_index)))
    }

    fn eval_get_field(
        &mut self,
        args: &[hir::LocalId],
        builtin: Builtin,
        expr_span: SourceSpan,
    ) -> MaybePoisoned<Result<EvalValue, Diverge>> {
        let &[r#struct, field_index] = args else { unreachable!("arg count checked") };
        let instance_state = self.bindings[r#struct].state?;
        let ty = self.state_type(instance_state);
        let (compound, field_index) =
            self.resolve_field_selector(ty, field_index, builtin, expr_span)?;
        let field_ty = compound.field_type(field_index);

        match instance_state {
            LocalState::Comptime(vid) => match self.values.lookup(vid) {
                Value::Compound { fields, .. } => Ok(Ok(EvalValue::Comptime(fields[field_index]))),
                _ => unreachable!("invariant: type checked as compound"),
            },
            LocalState::Runtime(local) => Ok(Ok(EvalValue::Runtime {
                expr: mir::Expr::FieldAccess {
                    object: local,
                    field_index: field_index.try_into().expect("field index fits u32"),
                },
                result_type: field_ty,
            })),
        }
    }

    fn eval_set_field(
        &mut self,
        args: &[hir::LocalId],
        builtin: Builtin,
        expr_span: SourceSpan,
    ) -> MaybePoisoned<Result<EvalValue, Diverge>> {
        let &[instance, field_index, field_value] = args else { unreachable!("arg count checked") };
        let instance_state = self.bindings[instance].state?;
        let instance_ty = self.state_type(instance_state);
        let (compound, field_index) =
            self.resolve_field_selector(instance_ty, field_index, builtin, expr_span)?;
        let field_ty = compound.field_type(field_index);

        let (new_value_state, new_value_span, _) = self.bindings[field_value].poisoned()?;
        let actual_ty = self.state_type(new_value_state);
        if !actual_ty.is_assignable_to(field_ty) {
            match compound {
                Compound::Struct(r#struct) => {
                    let field = r#struct.fields[field_index];
                    self.diag_ctx.emit_type_mismatch(
                        self.eval.values,
                        field_ty,
                        SrcLoc::new(r#struct.def_loc.source, field.def_span),
                        actual_ty,
                        self.loc(new_value_span),
                        false,
                    );
                }
                Compound::Tuple(_) => {
                    self.diag_ctx.emit_type_mismatch_simple(
                        self.eval.values,
                        field_ty,
                        actual_ty,
                        self.loc(expr_span),
                    );
                }
            }
            return Err(Poisoned);
        }

        // Both comptime: pure comptime fold.
        if let (LocalState::Comptime(instance_vid), LocalState::Comptime(new_value_vid)) =
            (instance_state, new_value_state)
        {
            return Ok(self.with_values_buf(|this, values_buf_offset| {
                match this.eval.values.lookup(instance_vid) {
                    Value::Compound { fields: old_fields, .. } => {
                        this.eval.values_buf.extend_from_slice(old_fields);
                    }
                    _ => unreachable!("invariant: type checked as compound"),
                }
                let fields = &mut this.eval.values_buf[values_buf_offset..];
                fields[field_index] = new_value_vid;
                Ok(EvalValue::Comptime(
                    this.eval.values.intern(Value::Compound { ty: instance_ty, fields }),
                ))
            }));
        }

        // At least one side is runtime: emit MIR.

        if self.eval.types.is_comptime_only(instance_ty) {
            self.diag_ctx.emit_set_field_on_comptime_only(
                self.eval.values,
                instance_ty,
                self.loc(self.bindings[field_value].use_span),
                compound,
            );
            return Err(Poisoned);
        }

        let instance_local = self.materialize_as_local(instance_state, instance_ty);

        let mut lower_field = |idx: usize, ty| {
            if idx == field_index {
                return self.materialize_as_local(new_value_state, ty);
            }
            let target = self.mir_types.push(ty);
            self.emit(mir::Instruction::Set {
                target,
                expr: mir::Expr::FieldAccess {
                    object: instance_local,
                    field_index: idx.try_into().expect("field index fits u32"),
                },
            });
            target
        };

        let fields: Vec<_> = match compound {
            Compound::Struct(r#struct) => r#struct
                .fields
                .iter()
                .enumerate()
                .map(|(idx, field)| lower_field(idx, field.ty))
                .collect(),
            Compound::Tuple(tuple) => {
                tuple.fields.iter().enumerate().map(|(idx, &ty)| lower_field(idx, ty)).collect()
            }
        };

        let fields = self.eval.mir_args.push_copy_slice(&fields);

        Ok(Ok(EvalValue::Runtime {
            expr: mir::Expr::CompoundLit { ty: instance_ty, fields },
            result_type: instance_ty,
        }))
    }

    fn eval_uninit(
        &mut self,
        args: &[hir::LocalId],
        builtin: Builtin,
        expr_span: SourceSpan,
    ) -> MaybePoisoned<Result<EvalValue, Diverge>> {
        let &[ty_local] = args else { unreachable!("arg count checked") };
        let ty = self.expect_type_arg(ty_local, builtin, expr_span)?;
        let flags = self.types.lookup(ty).flags();
        if flags.contains(TypeFlags::UNINIT_INCOMPATIBLE) {
            let expr = self.loc(expr_span);
            self.diag_ctx.emit_uninit_incompatible_type(ty, expr, self.eval.values);
            return Err(Poisoned);
        }

        if flags.contains(TypeFlags::RUNTIME_ONLY) {
            if self.is_comptime() {
                self.diag_ctx.emit_uninit_memptr_in_comptime(self.loc(expr_span));
                return Err(Poisoned);
            }
            return Ok(Ok(self.emit_uninit_runtime(ty)));
        }

        Ok(Ok(EvalValue::Comptime(build_uninit_comptime(
            ty,
            self.eval.types,
            self.eval.values,
            &mut self.eval.values_buf,
        ))))
    }

    fn eval_concat_cbytes(
        &mut self,
        args: &[hir::LocalId],
        expr_span: SourceSpan,
    ) -> MaybePoisoned<Result<EvalValue, Diverge>> {
        let &[tuple] = args else { unreachable!("arg count checked") };
        let (state, _, origin) = self.bindings[tuple].poisoned()?;
        let LocalState::Comptime(tuple_vid) = state else {
            self.diag_ctx
                .emit_runtime_ref_in_comptime(self.loc(expr_span), self.origin_loc(origin));
            return Err(Poisoned);
        };

        let fields = match self.eval.values.lookup(tuple_vid) {
            Value::Compound { ty, fields } if ty.is_tuple() => fields,
            _ => {
                let actual_ty = self.values.type_of_value(tuple_vid);
                self.diag_ctx.emit_concat_cbytes_expected_tuple(
                    self.eval.values,
                    actual_ty,
                    self.loc(expr_span),
                );
                return Err(Poisoned);
            }
        };

        let mut buf = Vec::new();
        let mut contains_invalid = false;
        for &field in fields {
            match self.values.lookup(field) {
                Value::BigNum(n) => {
                    buf.extend_from_slice(&n.to_be_bytes::<32>());
                }
                Value::Bytes(bytes) => {
                    let slice = self.diag_ctx.session.lookup_bytes_slice(bytes);
                    buf.extend_from_slice(slice);
                }
                other => {
                    self.diag_ctx.emit_concat_cbytes_invalid_element(
                        self.eval.values,
                        other.get_type(),
                        self.loc(expr_span),
                    );
                    contains_invalid = true;
                }
            }
        }
        if contains_invalid {
            return Err(Poisoned);
        }
        let cbytes = self.diag_ctx.session.intern_cbytes(&buf);
        let value = self.eval.values.intern_bytes(cbytes.contents, cbytes.start, cbytes.end);
        Ok(Ok(EvalValue::Comptime(value)))
    }

    fn eval_compile_log(
        &mut self,
        args: &[hir::LocalId],
        expr_span: SourceSpan,
    ) -> MaybePoisoned<Result<EvalValue, Diverge>> {
        let &[obj] = args else { unreachable!("arg count checked") };
        let (state, _, origin) = self.bindings[obj].poisoned()?;
        let LocalState::Comptime(obj_vid) = state else {
            self.diag_ctx
                .emit_runtime_ref_in_comptime(self.loc(expr_span), self.origin_loc(origin));
            return Err(Poisoned);
        };

        self.diag_ctx.record_compile_log(self.eval.values, obj_vid, self.loc(expr_span));
        Ok(Ok(EvalValue::Comptime(ValueId::VOID)))
    }

    fn eval_get_runtime_signature(
        &mut self,
        args: &[hir::LocalId],
        _expr_span: SourceSpan,
    ) -> MaybePoisoned<Result<EvalValue, Diverge>> {
        let &[closure, comptime_args] = args else { unreachable!("arg count checked") };
        let closure = self.expect_closure_arg(closure, Builtin::GetRuntimeSignature)?;
        let comptime_args_values =
            self.expect_comptime_tuple_arg(comptime_args, Builtin::GetRuntimeSignature)?;
        let expected_comptime_args = self.comptime_param_count(closure.fn_def_id);
        if expected_comptime_args != comptime_args_values.len() {
            self.diag_ctx.emit_function_introspection_args_mismatch(
                Builtin::GetRuntimeSignature,
                self.values.get_closure_name(closure.value),
                expected_comptime_args,
                comptime_args_values.len(),
                self.loc(self.bindings[comptime_args].use_span),
            );
            return Err(Poisoned);
        }
        match self.eval_function_signature_introspection(
            closure.fn_def_id,
            &closure.captures,
            &comptime_args_values,
            self.bindings[comptime_args].use_span,
        )? {
            Ok((runtime_signature, _return_type)) => {
                Ok(Ok(EvalValue::Comptime(self.eval.values.intern_type(runtime_signature))))
            }
            Err(diverge) => Ok(Err(diverge)),
        }
    }

    fn eval_get_return_type(
        &mut self,
        args: &[hir::LocalId],
        _expr_span: SourceSpan,
    ) -> MaybePoisoned<Result<EvalValue, Diverge>> {
        let &[closure, comptime_args] = args else { unreachable!("arg count checked") };
        let closure = self.expect_closure_arg(closure, Builtin::GetReturnType)?;
        let comptime_args_values =
            self.expect_comptime_tuple_arg(comptime_args, Builtin::GetReturnType)?;
        let expected_comptime_args = self.comptime_param_count(closure.fn_def_id);
        if expected_comptime_args != comptime_args_values.len() {
            self.diag_ctx.emit_function_introspection_args_mismatch(
                Builtin::GetReturnType,
                self.values.get_closure_name(closure.value),
                expected_comptime_args,
                comptime_args_values.len(),
                self.loc(self.bindings[comptime_args].use_span),
            );
            return Err(Poisoned);
        }
        match self.eval_function_signature_introspection(
            closure.fn_def_id,
            &closure.captures,
            &comptime_args_values,
            self.bindings[comptime_args].use_span,
        )? {
            Ok((_runtime_signature, return_type)) => {
                Ok(Ok(EvalValue::Comptime(self.eval.values.intern_type(return_type))))
            }
            Err(diverge) => Ok(Err(diverge)),
        }
    }

    fn eval_call_builtin(
        &mut self,
        args: &[hir::LocalId],
        expr_span: SourceSpan,
    ) -> MaybePoisoned<Result<EvalValue, Diverge>> {
        let &[closure, comptime_args, runtime_args] = args else {
            unreachable!("arg count checked")
        };
        let closure = self.expect_closure_arg(closure, Builtin::Call)?;
        let comptime_args_values = self.expect_comptime_tuple_arg(comptime_args, Builtin::Call)?;
        let expected_comptime_args = self.comptime_param_count(closure.fn_def_id);
        if expected_comptime_args != comptime_args_values.len() {
            self.diag_ctx.emit_function_introspection_args_mismatch(
                Builtin::Call,
                self.values.get_closure_name(closure.value),
                expected_comptime_args,
                comptime_args_values.len(),
                self.loc(self.bindings[comptime_args].use_span),
            );
            return Err(Poisoned);
        }
        let (runtime_signature, _return_type) = match self.eval_function_signature_introspection(
            closure.fn_def_id,
            &closure.captures,
            &comptime_args_values,
            self.bindings[comptime_args].use_span,
        )? {
            Ok(function_types) => function_types,
            Err(diverge) => return Ok(Err(diverge)),
        };
        let runtime_arg_locals =
            self.prepare_call_runtime_arg_locals(runtime_args, runtime_signature)?;
        let (call_args, call_arg_spans) = self.assemble_call_args(
            closure.fn_def_id,
            &comptime_args_values,
            &runtime_arg_locals,
            self.bindings[comptime_args].use_span,
            self.bindings[runtime_args].use_span,
        );

        let type_name = self.values.get_closure_name(closure.value);
        self.with_captures_buf(|this, capture_buf_offset| {
            for &capture in &closure.captures {
                this.eval.captures_buf.push(capture);
            }
            this.with_maybe_values_buf(|this, values_buf_offset| {
                let call_args = this.eval.call_args.push_copy_slice(&call_args);
                let call_arg_spans = this.eval.call_arg_spans.push_copy_slice(&call_arg_spans);
                let result = this.eval_call_inner(
                    closure.value,
                    closure.fn_def_id,
                    call_args,
                    call_arg_spans,
                    expr_span,
                    type_name,
                    capture_buf_offset,
                    values_buf_offset,
                );
                this.eval.call_arg_spans.pop();
                this.eval.call_args.pop();
                result
            })
        })
    }

    fn prepare_call_runtime_arg_locals(
        &mut self,
        runtime_args: hir::LocalId,
        runtime_signature: TypeId,
    ) -> MaybePoisoned<Vec<Local>> {
        let runtime_args_binding = self.bindings[runtime_args];
        let runtime_args_state = runtime_args_binding.state?;
        let actual_ty = self.state_type(runtime_args_state);
        if !actual_ty.is_assignable_to(runtime_signature) {
            self.diag_ctx.emit_type_mismatch_simple(
                self.eval.values,
                runtime_signature,
                actual_ty,
                self.loc(runtime_args_binding.use_span),
            );
            return Err(Poisoned);
        }

        let Type::Compound(Compound::Tuple(tuple)) = self.types.lookup(runtime_signature) else {
            unreachable!("runtime signature introspection always returns a tuple type")
        };

        match runtime_args_state {
            LocalState::Comptime(value) => {
                let Value::Compound { fields, .. } = self.values.lookup(value) else {
                    unreachable!("runtime args type checked as tuple")
                };
                Ok(fields
                    .iter()
                    .map(|&field| {
                        Local::comptime(
                            field,
                            runtime_args_binding.use_span,
                            runtime_args_binding.origin,
                        )
                    })
                    .collect())
            }
            LocalState::Runtime(local) => Ok(tuple
                .fields
                .iter()
                .enumerate()
                .map(|(field_index, &field_ty)| {
                    let target = self.mir_types.push(field_ty);
                    self.emit(mir::Instruction::Set {
                        target,
                        expr: mir::Expr::FieldAccess {
                            object: local,
                            field_index: field_index.try_into().expect("field index fits u32"),
                        },
                    });
                    Local {
                        state: Ok(LocalState::Runtime(target)),
                        use_span: runtime_args_binding.use_span,
                        origin: runtime_args_binding.origin,
                    }
                })
                .collect()),
        }
    }

    fn assemble_call_args(
        &mut self,
        fn_def_id: hir::FnDefId,
        comptime_args: &[ValueId],
        runtime_args: &[Local],
        comptime_args_span: SourceSpan,
        runtime_args_span: SourceSpan,
    ) -> (Vec<Local>, Vec<SourceSpan>) {
        let mut call_args = Vec::with_capacity(self.hir.fn_params[fn_def_id].len());
        let mut call_arg_spans = Vec::with_capacity(self.hir.fn_params[fn_def_id].len());
        let mut next_comptime_arg = 0;
        let mut next_runtime_arg = 0;

        for &param in &self.hir.fn_params[fn_def_id] {
            let (arg, span) = if param.is_comptime {
                let value = comptime_args[next_comptime_arg];
                next_comptime_arg += 1;
                (
                    Local::comptime(
                        value,
                        comptime_args_span,
                        DefOrigin::Local(comptime_args_span),
                    ),
                    comptime_args_span,
                )
            } else {
                match param.r#type {
                    hir::ParamType::Any { .. } => {
                        next_comptime_arg += 1;
                    }
                    hir::ParamType::Explicit(_) => {}
                    hir::ParamType::Poisoned => {
                        call_args.push(Local {
                            state: Err(Poisoned),
                            use_span: runtime_args_span,
                            origin: DefOrigin::Local(runtime_args_span),
                        });
                        call_arg_spans.push(runtime_args_span);
                        continue;
                    }
                }
                let arg = runtime_args[next_runtime_arg];
                next_runtime_arg += 1;
                (arg, runtime_args_span)
            };
            call_args.push(arg);
            call_arg_spans.push(span);
        }

        assert_eq!(next_comptime_arg, comptime_args.len());
        assert_eq!(next_runtime_arg, runtime_args.len());
        (call_args, call_arg_spans)
    }

    fn eval_function_signature_introspection(
        &mut self,
        fn_def_id: hir::FnDefId,
        captures: &[(ValueId, DefOrigin)],
        comptime_args: &[ValueId],
        comptime_args_span: SourceSpan,
    ) -> MaybePoisoned<Result<(TypeId, TypeId), Diverge>> {
        let comptime_args_loc = self.loc(comptime_args_span);
        let (function_types, comptime_quota, max_eval_branch_quota_seen) = {
            let mut fn_scope =
                self.prepare_introspection_preamble_scope(fn_def_id, captures, comptime_args_loc);
            let function_types = match fn_scope.eval_function_introspection_type_preamble(
                fn_def_id,
                comptime_args,
                comptime_args_loc,
            ) {
                Ok(Ok(())) => fn_scope.with_types_buf(|this, types_buf_offset| {
                    let fn_def = this.hir.fns[fn_def_id];
                    let mut poisoned = false;
                    for &param in &this.hir.fn_params[fn_def_id] {
                        match this.bindings[param.value].state {
                            Ok(LocalState::Runtime(local)) => {
                                this.eval.types_buf.push(this.mir_types[local]);
                            }
                            Ok(LocalState::Comptime(_)) => {}
                            Err(Poisoned) => poisoned = true,
                        }
                    }

                    let return_type = this.expect_type(fn_def.return_type);
                    if poisoned {
                        return Err(Poisoned);
                    }
                    let return_type = return_type?;

                    let (tuple, ok) = this.eval.types.intern_tuple(TupleKey {
                        fields: &this.eval.types_buf[types_buf_offset..],
                    });
                    if ok.is_err() {
                        this.diag_ctx.emit_mixed_tuple_type(
                            comptime_args_loc,
                            tuple,
                            this.eval.values,
                        );
                        return Err(Poisoned);
                    }

                    Ok(Ok((TypeId::from_tuple(tuple), return_type)))
                }),
                Ok(Err(diverge)) => Ok(Err(diverge)),
                Err(Poisoned) => Err(Poisoned),
            };
            (function_types, fn_scope.comptime_quota, fn_scope.max_eval_branch_quota_seen)
        };
        self.comptime_quota = comptime_quota;
        self.max_eval_branch_quota_seen =
            self.max_eval_branch_quota_seen.max(max_eval_branch_quota_seen);
        function_types
    }

    fn eval_function_introspection_type_preamble(
        &mut self,
        fn_def_id: hir::FnDefId,
        comptime_args: &[ValueId],
        comptime_args_loc: SrcLoc,
    ) -> MaybePoisoned<Result<(), Diverge>> {
        let fn_def = self.hir.fns[fn_def_id];
        let mut next_comptime_arg = 0;
        for &instr in &self.hir.block_instrs[fn_def.type_preamble] {
            let hir::InstructionKind::Param { comptime, arg, r#type, idx } = instr.kind else {
                if let Err(diverge) = self.eval_instr(instr) {
                    return Ok(Err(diverge));
                }
                continue;
            };

            self.bind_function_introspection_param_from_comptime_args(
                fn_def_id,
                comptime,
                arg,
                r#type,
                idx,
                comptime_args,
                &mut next_comptime_arg,
                comptime_args_loc,
            )?;
            self.eval_param(comptime, arg, r#type, idx);
        }
        Ok(Ok(()))
    }

    #[allow(clippy::too_many_arguments)]
    fn bind_function_introspection_param_from_comptime_args(
        &mut self,
        fn_def_id: hir::FnDefId,
        comptime: bool,
        arg: hir::LocalId,
        param_kind: hir::ParamType,
        idx: u32,
        comptime_args: &[ValueId],
        next_comptime_arg: &mut usize,
        comptime_args_loc: SrcLoc,
    ) -> MaybePoisoned<()> {
        let param = self.hir.fn_params[fn_def_id][idx as usize];
        let state = match (comptime, param_kind) {
            (_, hir::ParamType::Poisoned) => Err(Poisoned),
            (true, _) => {
                let value = comptime_args[*next_comptime_arg];
                *next_comptime_arg += 1;
                Ok(LocalState::Comptime(value))
            }
            (false, hir::ParamType::Any { .. }) => {
                let value = comptime_args[*next_comptime_arg];
                *next_comptime_arg += 1;
                match self.values.lookup(value) {
                    Value::Type(ty) => {
                        let local = self.mir_types.push(ty);
                        Ok(LocalState::Runtime(local))
                    }
                    other => {
                        self.diag_ctx.emit_type_mismatch_simple(
                            self.eval.values,
                            TypeId::TYPE,
                            other.get_type(),
                            comptime_args_loc,
                        );
                        Err(Poisoned)
                    }
                }
            }
            (false, hir::ParamType::Explicit(type_local)) => match self.expect_type(type_local) {
                Ok(ty) => {
                    let local = self.mir_types.push(ty);
                    Ok(LocalState::Runtime(local))
                }
                Err(Poisoned) => Err(Poisoned),
            },
        };

        self.bind_param_local(param, arg, state);
        Ok(())
    }

    /// Emits MIR instructions for a runtime uninit value (memptr or struct containing memptr).
    fn emit_uninit_runtime(&mut self, ty: TypeId) -> EvalValue {
        let local = self.emit_uninit_runtime_local(ty);
        EvalValue::Runtime { expr: mir::Expr::LocalRef(local), result_type: ty }
    }

    fn emit_uninit_runtime_local(&mut self, ty: TypeId) -> mir::LocalId {
        match self.eval.types.lookup(ty) {
            Type::Primitive(PrimitiveType::U256) => {
                let target = self.mir_types.push(TypeId::U256);
                self.emit(mir::Instruction::Set {
                    target,
                    expr: mir::Expr::Const(ValueId::ZERO_NUM),
                });
                target
            }
            Type::Primitive(PrimitiveType::Bool) => {
                let target = self.mir_types.push(TypeId::BOOL);
                self.emit(mir::Instruction::Set { target, expr: mir::Expr::Const(ValueId::FALSE) });
                target
            }
            Type::Primitive(PrimitiveType::MemoryPointer) => {
                let size_local = self.mir_types.push(TypeId::U256);
                self.emit(mir::Instruction::Set {
                    target: size_local,
                    expr: mir::Expr::Const(ValueId::ZERO_NUM),
                });
                let args = self.eval.mir_args.push_copy_slice(&[size_local]);
                let target = self.mir_types.push(TypeId::MEMORY_POINTER);
                self.emit(mir::Instruction::Set {
                    target,
                    expr: mir::Expr::RuntimeBuiltinCall {
                        builtin: RuntimeBuiltin::DynamicAllocAnyBytes,
                        args,
                    },
                });
                target
            }
            Type::Primitive(
                PrimitiveType::Type
                | PrimitiveType::Function
                | PrimitiveType::CBytes
                | PrimitiveType::Never,
            ) => {
                unreachable!("comptime-only/never types do not produce runtime locals")
            }
            Type::Compound(compound) => {
                let fields: Vec<_> = match compound {
                    Compound::Struct(r#struct) => r#struct
                        .fields
                        .iter()
                        .map(|field| self.emit_uninit_runtime_local(field.ty))
                        .collect(),
                    Compound::Tuple(tuple) => {
                        tuple.fields.iter().map(|&ty| self.emit_uninit_runtime_local(ty)).collect()
                    }
                };
                let fields = self.eval.mir_args.push_copy_slice(&fields);
                let target = self.mir_types.push(ty);
                self.emit(mir::Instruction::Set {
                    target,
                    expr: mir::Expr::CompoundLit { ty, fields },
                });
                target
            }
        }
    }

    fn expect_field_index_arg(
        &mut self,
        index_arg: hir::LocalId,
        builtin: Builtin,
        expr_span: SourceSpan,
        field_count: usize,
    ) -> MaybePoisoned<usize> {
        let index = self.expect_comptime_u256(index_arg, builtin, "field index", expr_span)?;
        self.expect_field_index_in_bounds(index, index_arg, builtin, field_count)
    }

    fn find_struct_field_by_name(&self, r#struct: StructView<'a>, name: CBytes) -> Option<usize> {
        let name = self.diag_ctx.session.lookup_bytes_slice(name);
        r#struct
            .fields
            .iter()
            .position(|field| self.diag_ctx.session.lookup_bytes(BytesId::from(field.name)) == name)
    }

    fn resolve_field_selector(
        &mut self,
        ty: TypeId,
        selector_arg: hir::LocalId,
        builtin: Builtin,
        expr_span: SourceSpan,
    ) -> MaybePoisoned<(Compound<'a>, usize)> {
        let compound = self.expect_compound(ty, builtin, expr_span)?;
        let selector_binding = self.bindings[selector_arg];
        let state = selector_binding.state?;
        let LocalState::Comptime(selector) = state else {
            self.diag_ctx.emit_expected_comptime_arg(
                builtin,
                "field selector",
                self.loc(expr_span),
            );
            return Err(Poisoned);
        };

        match self.values.lookup(selector) {
            Value::BigNum(index) => {
                let index = self.expect_field_index_in_bounds(
                    index,
                    selector_arg,
                    builtin,
                    compound.field_count(),
                )?;
                Ok((compound, index))
            }
            Value::Bytes(name) => {
                let Compound::Struct(r#struct) = compound else {
                    self.diag_ctx.emit_invalid_field_selector_type(
                        self.eval.values,
                        builtin,
                        ty,
                        TypeId::CBYTES,
                        self.loc(selector_binding.use_span),
                    );
                    return Err(Poisoned);
                };
                let Some(field_index) = self.find_struct_field_by_name(r#struct, name) else {
                    self.diag_ctx.emit_unknown_field_name_selector(
                        self.eval.values,
                        builtin,
                        ty,
                        name,
                        self.loc(selector_binding.use_span),
                    );
                    return Err(Poisoned);
                };
                Ok((Compound::Struct(r#struct), field_index))
            }
            other => {
                self.diag_ctx.emit_invalid_field_selector_type(
                    self.eval.values,
                    builtin,
                    ty,
                    other.get_type(),
                    self.loc(selector_binding.use_span),
                );
                Err(Poisoned)
            }
        }
    }

    fn expect_field_index_in_bounds(
        &mut self,
        index: U256,
        index_arg: hir::LocalId,
        builtin: Builtin,
        field_count: usize,
    ) -> MaybePoisoned<usize> {
        match usize::try_from(index) {
            Ok(index) if index < field_count => Ok(index),
            _ => {
                self.diag_ctx.emit_field_index_out_of_bounds(
                    builtin,
                    index,
                    field_count,
                    self.loc(self.bindings[index_arg].use_span),
                );
                Err(Poisoned)
            }
        }
    }

    fn expect_type_arg(
        &mut self,
        arg_local: hir::LocalId,
        builtin: Builtin,
        span: SourceSpan,
    ) -> MaybePoisoned<TypeId> {
        let state = self.bindings[arg_local].state?;
        if let LocalState::Comptime(vid) = state
            && let Value::Type(ty) = self.values.lookup(vid)
        {
            return Ok(ty);
        }
        let actual_ty = self.state_type(state);
        self.diag_ctx.emit_expected_type_arg(self.eval.values, builtin, actual_ty, self.loc(span));
        Err(Poisoned)
    }

    fn expect_bytes_arg(
        &mut self,
        arg_local: hir::LocalId,
        builtin: Builtin,
        span: SourceSpan,
    ) -> MaybePoisoned<CBytes> {
        let state = self.bindings[arg_local].state?;
        if let LocalState::Comptime(vid) = state
            && let Value::Bytes(bytes) = self.values.lookup(vid)
        {
            return Ok(bytes);
        }
        let actual_ty = self.state_type(state);
        self.diag_ctx.emit_no_matching_builtin_signature(
            self.eval.values,
            builtin,
            &[actual_ty],
            self.loc(span),
        );
        Err(Poisoned)
    }

    fn expect_closure_arg(
        &mut self,
        arg_local: hir::LocalId,
        builtin: Builtin,
    ) -> MaybePoisoned<ClosureArg> {
        let arg_binding = self.bindings[arg_local];
        let state = arg_binding.state?;
        if let LocalState::Comptime(vid) = state
            && let Value::Closure { fn_def, captures, .. } = self.values.lookup(vid)
        {
            return Ok(ClosureArg { value: vid, fn_def_id: fn_def, captures: captures.to_vec() });
        }
        let actual_ty = self.state_type(state);
        self.diag_ctx.emit_expected_closure_arg(
            self.eval.values,
            builtin,
            actual_ty,
            self.loc(arg_binding.use_span),
        );
        Err(Poisoned)
    }

    fn comptime_param_count(&self, fn_def_id: hir::FnDefId) -> usize {
        self.hir.fn_params[fn_def_id]
            .iter()
            .filter(|param| param.is_comptime || matches!(param.r#type, hir::ParamType::Any { .. }))
            .count()
    }

    fn expect_comptime_tuple_arg(
        &mut self,
        arg_local: hir::LocalId,
        builtin: Builtin,
    ) -> MaybePoisoned<Vec<ValueId>> {
        let arg_binding = self.bindings[arg_local];
        let state = arg_binding.state?;
        let LocalState::Comptime(vid) = state else {
            self.diag_ctx.emit_expected_comptime_arg(
                builtin,
                "comptime_args",
                self.loc(arg_binding.use_span),
            );
            return Err(Poisoned);
        };
        match self.values.lookup(vid) {
            Value::Compound { ty, fields } if ty.is_tuple() => Ok(fields.to_vec()),
            other => {
                self.diag_ctx.emit_expected_tuple_arg(
                    self.eval.values,
                    builtin,
                    "comptime_args",
                    other.get_type(),
                    self.loc(arg_binding.use_span),
                );
                Err(Poisoned)
            }
        }
    }

    fn expect_comptime_u256(
        &mut self,
        arg_local: hir::LocalId,
        builtin: Builtin,
        arg_name: &str,
        span: SourceSpan,
    ) -> MaybePoisoned<U256> {
        let arg_binding = self.bindings[arg_local];
        let state = arg_binding.state?;
        let LocalState::Comptime(vid) = state else {
            self.diag_ctx.emit_expected_comptime_arg(builtin, arg_name, self.loc(span));
            return Err(Poisoned);
        };
        let Value::BigNum(n) = self.values.lookup(vid) else {
            self.diag_ctx.emit_type_mismatch_simple(
                self.eval.values,
                TypeId::U256,
                self.eval.values.type_of_value(vid),
                self.loc(arg_binding.use_span),
            );
            return Err(Poisoned);
        };
        Ok(n)
    }

    fn expect_compound(
        &mut self,
        ty: TypeId,
        builtin: Builtin,
        span: SourceSpan,
    ) -> MaybePoisoned<Compound<'a>> {
        match self.types.lookup(ty) {
            Type::Compound(compound) => Ok(compound),
            _ => {
                self.diag_ctx.emit_expected_compound_type_arg(
                    self.eval.values,
                    builtin,
                    ty,
                    self.loc(span),
                );
                Err(Poisoned)
            }
        }
    }

    fn expect_struct(
        &mut self,
        ty: TypeId,
        builtin: Builtin,
        span: SourceSpan,
    ) -> MaybePoisoned<StructView<'a>> {
        match self.types.lookup(ty) {
            Type::Compound(Compound::Struct(r#struct)) => Ok(r#struct),
            _ => {
                self.diag_ctx.emit_expected_struct_type_arg(
                    self.eval.values,
                    builtin,
                    ty,
                    self.loc(span),
                );
                Err(Poisoned)
            }
        }
    }

    pub(crate) fn materialize_as_local(&mut self, state: LocalState, ty: TypeId) -> mir::LocalId {
        match state {
            LocalState::Runtime(local) => local,
            LocalState::Comptime(vid) => {
                let target = self.mir_types.push(ty);
                self.emit(mir::Instruction::Set { target, expr: mir::Expr::Const(vid) });
                target
            }
        }
    }
}

pub(crate) fn fold_runtime_builtin(
    builtin: RuntimeBuiltin,
    args: &[ValueId],
    values: &mut ValueInterner,
) -> U256 {
    use plank_evm as evm;
    match *args {
        [a] => {
            let a = as_u256(values, a);
            match builtin {
                RuntimeBuiltin::IsZero => U256::from(plank_evm::iszero(a)),
                RuntimeBuiltin::Not => plank_evm::not(a),
                _ => unreachable!("not a unary foldable builtin: {builtin}"),
            }
        }
        [a, b] => {
            let a = as_u256(values, a);
            let b = as_u256(values, b);
            match builtin {
                RuntimeBuiltin::Add => evm::add(a, b),
                RuntimeBuiltin::Mul => evm::mul(a, b),
                RuntimeBuiltin::Sub => evm::sub(a, b),
                RuntimeBuiltin::Div => evm::div(a, b),
                RuntimeBuiltin::SDiv => evm::sdiv(a, b),
                RuntimeBuiltin::Mod => evm::r#mod(a, b),
                RuntimeBuiltin::SMod => evm::smod(a, b),
                RuntimeBuiltin::Exp => evm::exp(a, b),
                RuntimeBuiltin::SignExtend => evm::signextend(a, b),
                RuntimeBuiltin::Lt => U256::from(evm::lt(a, b)),
                RuntimeBuiltin::Gt => U256::from(evm::gt(a, b)),
                RuntimeBuiltin::SLt => U256::from(evm::slt(a, b)),
                RuntimeBuiltin::SGt => U256::from(evm::sgt(a, b)),
                RuntimeBuiltin::Eq => U256::from(evm::eq(a, b)),
                RuntimeBuiltin::And => evm::and(a, b),
                RuntimeBuiltin::Or => evm::or(a, b),
                RuntimeBuiltin::Xor => evm::xor(a, b),
                RuntimeBuiltin::Byte => evm::byte(a, b),
                RuntimeBuiltin::Shl => evm::shl(a, b),
                RuntimeBuiltin::Shr => evm::shr(a, b),
                RuntimeBuiltin::Sar => evm::sar(a, b),
                _ => unreachable!("not a binary foldable builtin: {builtin}"),
            }
        }
        [a, b, c] => {
            let a = as_u256(values, a);
            let b = as_u256(values, b);
            let c = as_u256(values, c);
            match builtin {
                RuntimeBuiltin::AddMod => plank_evm::addmod(a, b, c),
                RuntimeBuiltin::MulMod => plank_evm::mulmod(a, b, c),
                _ => unreachable!("not a ternary foldable builtin: {builtin}"),
            }
        }
        _ => unreachable!("non-foldable builtin cannot be evaluated: {builtin}"),
    }
}

fn build_uninit_comptime(
    ty: TypeId,
    types: &TypeInterner,
    values: &mut ValueInterner,
    buf: &mut Vec<ValueId>,
) -> ValueId {
    match types.lookup(ty) {
        Type::Primitive(PrimitiveType::U256) => ValueId::ZERO_NUM,
        Type::Primitive(PrimitiveType::Bool) => ValueId::FALSE,
        Type::Primitive(PrimitiveType::Type) => values.intern_type(TypeId::VOID),
        Type::Primitive(PrimitiveType::CBytes) => ValueId::BYTES_EMPTY,
        Type::Primitive(
            PrimitiveType::MemoryPointer | PrimitiveType::Function | PrimitiveType::Never,
        ) => {
            unreachable!("memptr/function/never cannot appear in comptime uninit compound")
        }
        Type::Compound(compound) => {
            let buf_offset = buf.len();
            match compound {
                Compound::Struct(r#struct) => {
                    for field in r#struct.fields {
                        let vid = build_uninit_comptime(field.ty, types, values, buf);
                        buf.push(vid);
                    }
                }
                Compound::Tuple(tuple) => {
                    for &field in tuple.fields {
                        let vid = build_uninit_comptime(field, types, values, buf);
                        buf.push(vid);
                    }
                }
            }
            let result = values.intern(Value::Compound { ty, fields: &buf[buf_offset..] });
            buf.truncate(buf_offset);
            result
        }
    }
}

pub(crate) fn as_u256(values: &ValueInterner, vid: ValueId) -> U256 {
    match values.lookup(vid) {
        Value::BigNum(n) => n,
        Value::Bool(b) => {
            if b {
                U256::ONE
            } else {
                U256::ZERO
            }
        }
        other => unreachable!("invariant: type checked as u256, got {other:?}"),
    }
}
