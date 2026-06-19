use crate::scope::{Diverge, EvalValue, LocalState, Scope};
use alloy_primitives::U256;
use plank_hir as hir;
use plank_mir as mir;
use plank_session::{
    Builtin, MaybePoisoned, Poisoned, RuntimeBuiltin, SourceSpan, SrcLoc, builtins::BuiltinKind,
};
use plank_values::{
    CBytes, Compound, PrimitiveType, Type, TypeFlags, TypeId, TypeInterner, Value, ValueId,
    ValueInterner, builtins as builtin_sigs,
};

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
            Builtin::FieldCount => {
                let &[r#struct] = args else { unreachable!("arg count checked") };
                let ty = self.expect_type_arg(r#struct, builtin, expr_span)?;
                let field_count = self.expect_compound(ty, builtin, expr_span)?.field_count();
                let count = U256::from(field_count);
                Ok(Ok(EvalValue::Comptime(self.eval.values.intern_num(count))))
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
                let message = self.diag_ctx.session.lookup_bytes_lossy(
                    message.contents,
                    message.start,
                    message.end,
                );
                self.diag_ctx.emit_custom_comptime_error(message, self.loc(expr_span));
                Ok(Err(Diverge::ControlFlowPoisoned))
            }
            Builtin::SliceCBytes => {
                let &[bytes, start, end] = args else { unreachable!("arg count checked") };
                let bytes = self.expect_bytes_arg(bytes, builtin, expr_span)?;
                let start = self.expect_comptime_u256(start, builtin, "slice start", expr_span)?;
                let end = self.expect_comptime_u256(end, builtin, "slice end", expr_span)?;
                let len = bytes.end - bytes.start;
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
            Builtin::Keccak256CBytes => {
                let &[bytes] = args else { unreachable!("arg count checked") };
                let bytes = self.expect_bytes_arg(bytes, builtin, expr_span)?;
                let slice = self.diag_ctx.session.lookup_bytes_slice(
                    bytes.contents,
                    bytes.start,
                    bytes.end,
                );
                let hash = U256::from_be_bytes(alloy_primitives::keccak256(slice).0);
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
        let (compound, index) = self.resolve_field_index(ty, field_index, builtin, expr_span)?;
        let field_ty = compound.field_type(index as usize);
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
        let Type::Compound(Compound::Struct(r#struct)) = self.types.lookup(ty) else {
            self.diag_ctx.emit_expected_struct_type_arg(
                self.eval.values,
                builtin,
                ty,
                self.loc(expr_span),
            );
            return Err(Poisoned);
        };
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
            self.resolve_field_index(ty, field_index, builtin, expr_span)?;
        let field_ty = compound.field_type(field_index as usize);

        match instance_state {
            LocalState::Comptime(vid) => match self.values.lookup(vid) {
                Value::Compound { fields, .. } => {
                    Ok(Ok(EvalValue::Comptime(fields[field_index as usize])))
                }
                _ => unreachable!("invariant: type checked as compound"),
            },
            LocalState::Runtime(local) => Ok(Ok(EvalValue::Runtime {
                expr: mir::Expr::FieldAccess { object: local, field_index },
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
            self.resolve_field_index(instance_ty, field_index, builtin, expr_span)?;
        let field_ty = compound.field_type(field_index as usize);

        let (new_value_state, new_value_span, _) = self.bindings[field_value].poisoned()?;
        let actual_ty = self.state_type(new_value_state);
        if !actual_ty.is_assignable_to(field_ty) {
            match compound {
                Compound::Struct(r#struct) => {
                    let field = r#struct.fields[field_index as usize];
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
                fields[field_index as usize] = new_value_vid;
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

        let mut lower_field = |idx, ty| {
            if idx == field_index {
                return self.materialize_as_local(new_value_state, ty);
            }
            let target = self.mir_types.push(ty);
            self.emit(mir::Instruction::Set {
                target,
                expr: mir::Expr::FieldAccess { object: instance_local, field_index: idx },
            });
            target
        };

        let fields: Vec<_> = match compound {
            Compound::Struct(r#struct) => {
                (0..).zip(r#struct.fields).map(|(idx, field)| lower_field(idx, field.ty)).collect()
            }
            Compound::Tuple(tuple) => {
                (0..).zip(tuple.fields).map(|(idx, &ty)| lower_field(idx, ty)).collect()
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
            Type::Primitive(PrimitiveType::Void) => {
                let target = self.mir_types.push(TypeId::VOID);
                self.emit(mir::Instruction::Set { target, expr: mir::Expr::Const(ValueId::VOID) });
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

    fn resolve_field_index(
        &mut self,
        ty: TypeId,
        index_arg: hir::LocalId,
        builtin: Builtin,
        expr_span: SourceSpan,
    ) -> MaybePoisoned<(Compound<'a>, u32)> {
        let compound = self.expect_compound(ty, builtin, expr_span)?;
        let index = self.expect_comptime_u256(index_arg, builtin, "field index", expr_span)?;
        let field_count = compound.field_count();
        let Some(index) = u32::try_from(index).ok().filter(|&index| (index as usize) < field_count)
        else {
            self.diag_ctx.emit_field_index_out_of_bounds(
                builtin,
                index,
                field_count,
                self.loc(self.bindings[index_arg].use_span),
            );
            return Err(Poisoned);
        };
        Ok((compound, index))
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
        Type::Primitive(PrimitiveType::Void) => ValueId::VOID,
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
