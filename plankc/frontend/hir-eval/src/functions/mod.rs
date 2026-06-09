use plank_core::{DenseIndexMap, IndexVec};
use plank_hir::{self as hir, ValueId};
use plank_mir as mir;
use plank_session::{MaybePoisoned, Poisoned, SourceId, SourceSpan, SrcLoc, poison};
use plank_values::{DefOrigin, TypeId, Value};

mod cache;

use cache::*;
pub(crate) use cache::{EvaluatedFunctionCache, LoweredFunctionsCache};

use crate::{
    evaluator::CallArgSpansIdx,
    quota::{ComptimeQuota, QuotaExhaustedError},
    scope::{Diverge, EvalContext, EvalValue, Local, LocalState, Scope},
};

/// Empty marker to track the invariant that arg/param comptimeness matching was already checked.
#[derive(Clone, Copy)]
struct ArgParamComptimenessMatch;

#[derive(Debug)]
struct PreambleResult {
    return_type: MaybePoisoned<TypeId>,
    is_comptime_only: bool,
}

impl PreambleResult {
    fn suppress_poison_iff_diverging_return_type<T>(&self) -> MaybePoisoned<Result<T, Diverge>> {
        if self.return_type == Ok(TypeId::NEVER) { Ok(Err(Diverge::END)) } else { Err(Poisoned) }
    }
}

struct Call<'a> {
    source: SourceId,
    caller_comptime: bool,
    caller_bindings: &'a DenseIndexMap<hir::LocalId, Local>,
    caller_mir_types: &'a mut IndexVec<mir::LocalId, TypeId>,
    span: SourceSpan,

    closure: ValueId,
    func: hir::FnDef,
    args: &'a [hir::LocalId],
    params: &'a [hir::ParamInfo],

    validated: ArgParamComptimenessMatch,
}

impl Call<'_> {
    fn loc(&self) -> SrcLoc {
        SrcLoc::new(self.source, self.span)
    }
}

impl<'a, 'ctx> Scope<'a, 'ctx> {
    #[allow(clippy::too_many_arguments)]
    fn prepare_new_fn_scope_for_preamble_eval<'s>(
        &'s mut self,
        closure: ValueId,
        fn_def_id: hir::FnDefId,
        args: &'s [hir::LocalId],
        arg_spans: CallArgSpansIdx,
        call_span: SourceSpan,
        capture_buf_offset: usize,
        validated: ArgParamComptimenessMatch,
    ) -> (Scope<'s, 'ctx>, Call<'s>, &'s mut ComptimeQuota, &'s mut u32) {
        let fn_def = self.eval.hir.fns[fn_def_id];
        let params = &self.eval.hir.fn_params[fn_def_id];
        let caller_comptime = self.is_comptime();
        let eval_branch_quota_start_loc = self.eval_branch_quota_start_loc;
        let call_source = self.source;
        let comptime_quota = self.comptime_quota;
        let caller_bindings = &mut self.bindings;
        let caller_mir_types = &mut self.mir_types;
        let parent_comptime_quota = &mut self.comptime_quota;
        let parent_max_eval_branch_quota_seen = &mut self.max_eval_branch_quota_seen;

        let mut fn_scope = Scope::new(
            self.eval,
            self.diag_ctx,
            fn_def.source,
            false,
            comptime_quota,
            eval_branch_quota_start_loc,
            EvalContext::FunctionPreamble { arg_spans, call_source },
        );

        let captured_values = &fn_scope.eval.captures_buf[capture_buf_offset..];
        let capture_defs = &fn_scope.eval.hir.fn_captures[fn_def_id];
        for (&(value, _origin), &def) in captured_values.iter().zip(capture_defs) {
            fn_scope.bindings.insert_no_prev(
                def.inner_local,
                Local::comptime(value, def.use_span, DefOrigin::Local(def.use_span)),
            );
        }

        for (&param, &arg) in params.iter().zip(args) {
            let binding = caller_bindings[arg];
            let state = match binding.state {
                Ok(state) => state,
                Err(Poisoned) => {
                    fn_scope.bindings.insert_no_prev(
                        param.value,
                        Local {
                            state: Err(Poisoned),
                            use_span: param.span,
                            origin: DefOrigin::Local(param.span),
                        },
                    );
                    continue;
                }
            };

            let state = 'state: {
                if param.is_comptime || caller_comptime {
                    let LocalState::Comptime(value) = state else {
                        let ArgParamComptimenessMatch = validated;
                        unreachable!("invariant: already validated");
                    };
                    break 'state Ok(LocalState::Comptime(value));
                }
                let ty = match state {
                    LocalState::Runtime(outer_mir) => caller_mir_types[outer_mir],
                    LocalState::Comptime(value) => {
                        let ty = fn_scope.eval.values.type_of_value(value);
                        // If value is comptime-only, even for a runtime call we treat it as a
                        // comptime argument.
                        if fn_scope.eval.types.is_comptime_only(ty) {
                            break 'state Ok(LocalState::Comptime(value));
                        }
                        ty
                    }
                };
                let inner_mir = fn_scope.mir_types.push(ty);
                Ok(LocalState::Runtime(inner_mir))
            };
            fn_scope.bindings.insert_no_prev(
                param.value,
                Local { state, use_span: param.span, origin: DefOrigin::Local(param.span) },
            );
        }

        let call = Call {
            source: call_source,
            caller_comptime,
            caller_bindings,
            caller_mir_types,
            span: call_span,
            closure,
            func: fn_def,
            args,
            params,
            validated,
        };

        (fn_scope, call, parent_comptime_quota, parent_max_eval_branch_quota_seen)
    }

    fn eval_preamble(
        &mut self,
        fn_def_id: hir::FnDefId,
    ) -> Result<MaybePoisoned<PreambleResult>, Diverge> {
        let fn_def = self.hir.fns[fn_def_id];
        match self.eval_comptime(fn_def.type_preamble) {
            Ok(()) => {}
            Err(Diverge::ComptimeQuotaExhausted) => {
                return Err(Diverge::ComptimeQuotaExhausted);
            }
            Err(Diverge::ControlFlowPoisoned | Diverge::BlockEnd(_)) => {
                return Ok(Err(Poisoned));
            }
        }
        let return_type = self.expect_type(fn_def.return_type);
        let ret_type_loc = self.origin_loc(self.bindings[fn_def.return_type].origin);
        self.ctx = EvalContext::FunctionBody { ret_type: return_type, ret_type_loc };
        let is_comptime_only = return_type.is_ok_and(|ty| self.types.is_comptime_only(ty));
        Ok(Ok(PreambleResult { return_type, is_comptime_only }))
    }

    pub(crate) fn eval_fn_def(&mut self, id: hir::FnDefId) -> MaybePoisoned<EvalValue> {
        let def_captures = &self.hir.fn_captures[id];
        self.with_captures_buf(|this, captures_buf_offset| {
            let mut poisoned = false;
            for &capture in def_captures {
                let binding = this.bindings[capture.outer_local];
                let Ok(state) = binding.state else {
                    poisoned = true;
                    continue;
                };
                let value = match state {
                    LocalState::Comptime(value) => value,
                    LocalState::Runtime(_) => {
                        this.diag_ctx.emit_closure_capture_not_comptime(
                            this.loc(capture.use_span),
                            this.origin_loc(binding.origin),
                        );
                        poisoned = true;
                        continue;
                    }
                };
                this.captures_buf.push((value, binding.origin));
            }
            if poisoned {
                return Err(Poisoned);
            }
            let capture_values = &this.eval.captures_buf[captures_buf_offset..];
            assert_eq!(capture_values.len(), def_captures.len());
            let closure_value =
                this.eval.values.intern(Value::Closure { fn_def: id, captures: capture_values });
            Ok(EvalValue::Comptime(closure_value))
        })
    }

    pub(crate) fn eval_call(
        &mut self,
        callee: hir::LocalId,
        args_id: hir::CallArgsId,
        call_span: SourceSpan,
    ) -> MaybePoisoned<Result<EvalValue, Diverge>> {
        self.with_captures_buf(|this, capture_buf_offset: usize| {
            this.with_maybe_values_buf(|this, values_buf_offset: usize| {
                let (state, callee_use_span, callee_origin) = this.bindings[callee].poisoned()?;
                let closure_vid = match state {
                    LocalState::Comptime(value) => value,
                    LocalState::Runtime(_) => {
                        this.diag_ctx.emit_call_target_not_comptime(this.loc(callee_use_span));
                        return Err(Poisoned);
                    }
                };
                let Value::Closure { fn_def: fn_def_id, captures } =
                    this.eval.values.lookup(closure_vid)
                else {
                    let ty = this.values.type_of_value(closure_vid);
                    this.diag_ctx
                        .emit_not_callable(ty, this.binding_loc(callee_use_span, callee_origin));
                    return Err(Poisoned);
                };
                for &capture in captures {
                    this.eval.captures_buf.push(capture);
                }

                let args = &this.hir.call_args[args_id];
                let arg_spans = this
                    .eval
                    .call_arg_spans
                    .push_iter(args.iter().map(|&arg| this.bindings[arg].use_span));
                let eval_res = this.eval_call_inner(
                    closure_vid,
                    fn_def_id,
                    args,
                    arg_spans,
                    call_span,
                    capture_buf_offset,
                    values_buf_offset,
                );
                this.eval.call_arg_spans.pop();

                eval_res
            })
        })
    }

    fn validate_args_param_comptimeness_match(
        &mut self,
        func: hir::FnDef,
        params: &[hir::ParamInfo],
        args: &[hir::LocalId],
    ) -> MaybePoisoned<ArgParamComptimenessMatch> {
        let mut comptime_args_poisoned = false;
        for (param, &arg) in params.iter().zip(args) {
            let arg = self.bindings[arg];
            if (param.is_comptime || self.is_comptime())
                && let Ok(LocalState::Runtime(_)) = arg.state
            {
                self.diag_ctx
                    .emit_comptime_param_got_runtime(self.loc(arg.use_span), func.loc(param.span));
                comptime_args_poisoned = true;
                continue;
            };
        }
        if comptime_args_poisoned { Err(Poisoned) } else { Ok(ArgParamComptimenessMatch) }
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn eval_call_inner(
        &mut self,
        closure: ValueId,
        fn_def_id: hir::FnDefId,
        args: &[hir::LocalId],
        arg_spans: CallArgSpansIdx,
        call_span: SourceSpan,
        capture_buf_offset: usize,
        values_buf_offset: usize,
    ) -> MaybePoisoned<Result<EvalValue, Diverge>> {
        let func = self.hir.fns[fn_def_id];
        let params = &self.hir.fn_params[fn_def_id];
        let call_loc = self.loc(call_span);

        if params.len() != args.len() {
            self.diag_ctx.emit_arg_count_mismatch(
                params.len(),
                args.len(),
                self.loc(call_span),
                func.loc(func.param_list_span),
            );
            return Err(Poisoned);
        }

        let validated = self.validate_args_param_comptimeness_match(func, params, args)?;

        let (mut scope, call, parent_comptime_quota, parent_max_eval_branch_quota_seen) = self
            .prepare_new_fn_scope_for_preamble_eval(
                closure,
                fn_def_id,
                args,
                arg_spans,
                call_span,
                capture_buf_offset,
                validated,
            );
        let result = scope.eval_callee_scope(fn_def_id, call, values_buf_offset, call_loc);

        *parent_comptime_quota = scope.comptime_quota;
        *parent_max_eval_branch_quota_seen =
            (*parent_max_eval_branch_quota_seen).max(scope.max_eval_branch_quota_seen);

        result
    }

    fn eval_callee_scope(
        &mut self,
        fn_def_id: hir::FnDefId,
        mut call: Call<'_>,
        values_buf_offset: usize,
        call_loc: SrcLoc,
    ) -> MaybePoisoned<Result<EvalValue, Diverge>> {
        let preamble = {
            let restore = self.diag_ctx.set_preamble_call_site(call.loc());
            let preamble = self.eval_preamble(fn_def_id);
            self.diag_ctx.restore_preamble_call_site(restore);
            match preamble {
                Ok(Ok(preamble)) => preamble,
                Ok(Err(Poisoned)) => return Err(Poisoned),
                Err(diverge) => return Ok(Err(diverge)),
            }
        };

        let mut runtime_param_count = 0;

        // Assemble comptime parameters for the function key.
        for (&param, &arg) in call.params.iter().zip(call.args) {
            let param_key_value = match self.bindings[param.value].state {
                Ok(LocalState::Comptime(value)) => Some(Ok(value)),
                Err(Poisoned) => Some(Err(Poisoned)),
                Ok(LocalState::Runtime(_)) => {
                    runtime_param_count += 1;
                    // `create_fn_scope_frame` optimistically makes params runtime in runtime
                    // contexts, if we find out we need to evaluate as comptime
                    // we need to make sure all arguments are added to the key.
                    match call.caller_bindings[arg].state {
                        Ok(LocalState::Comptime(value)) if preamble.is_comptime_only => {
                            Some(Ok(value))
                        }
                        _ => {
                            let ArgParamComptimenessMatch = call.validated;
                            None
                        }
                    }
                }
            };

            if let Some(value) = param_key_value {
                self.eval.maybe_values_buf.push(value);
            } else if let hir::ParamType::Any { capture } = param.r#type {
                let capture_key_value = match self.bindings[capture].state {
                    Ok(LocalState::Comptime(value)) => Ok(value),
                    Err(Poisoned) => Err(Poisoned),
                    Ok(LocalState::Runtime(_)) => {
                        unreachable!("any-type capture should be comptime")
                    }
                };
                self.eval.maybe_values_buf.push(capture_key_value);
            }
        }

        if call.caller_comptime || preamble.is_comptime_only {
            let call_result = self.fold_comptime_call(&call, preamble, values_buf_offset);
            return match call_result {
                Ok(Ok(result)) => match result.outcome {
                    ComptimeCallOutcome::Value(value) => Ok(Ok(EvalValue::Comptime(value))),
                    ComptimeCallOutcome::DivergedEnd => Ok(Err(Diverge::END)),
                },
                Ok(Err(diverged)) => Ok(Err(diverged)),
                Err(Poisoned) => Err(Poisoned),
            };
        }

        // Non-comptime params are already bound as Runtime in `create_fn_scope_frame`.
        let function =
            FunctionKey::new(call.closure, &self.eval.maybe_values_buf[values_buf_offset..]);

        let lowered = match self.eval.lowered_fns_cache.retrieve_or_create_entry(function) {
            Ok(&mut LoweredFnState::Done(fn_id)) => fn_id,
            Ok(state @ LoweredFnState::InProgress) => {
                self.diag_ctx.emit_runtime_call_with_recursion(call_loc);
                *state = LoweredFnState::Done(Err(Poisoned));
                return Ok(Err(Diverge::ControlFlowPoisoned));
            }
            Ok(LoweredFnState::Empty) => unreachable!("empty lowered entry should be retried"),
            Err(new_entry_id) => {
                let fn_id = (|| {
                    let (body, body_eval_res) = self.eval_block_to_mir(call.func.body);
                    match body_eval_res {
                        Ok(()) => unreachable!("lowerer should guarantee return in function body"),
                        Err(Diverge::BlockEnd(_)) => {}
                        Err(Diverge::ControlFlowPoisoned) => {
                            return Err(Poisoned);
                        }
                        Err(Diverge::ComptimeQuotaExhausted) => {
                            return Ok(Err(Diverge::ComptimeQuotaExhausted));
                        }
                    }
                    let return_type = preamble.return_type?;
                    let fn_id1 = self.eval.mir_fn_locals.push_copy_slice(&self.mir_types);
                    let fn_id2 = self.eval.mir_fns.push(mir::FnDef {
                        body,
                        param_count: runtime_param_count,
                        return_type,
                    });
                    assert_eq!(fn_id1, fn_id2);
                    Ok(Ok(fn_id1))
                })();
                match fn_id {
                    Ok(Ok(fn_id)) => {
                        self.eval.lowered_fns_cache.try_set_lowered(new_entry_id, Ok(fn_id))
                    }
                    Ok(Err(Diverge::ComptimeQuotaExhausted)) => {
                        self.eval.lowered_fns_cache.mark_retryable(new_entry_id);
                        return Ok(Err(Diverge::ComptimeQuotaExhausted));
                    }
                    Ok(Err(Diverge::ControlFlowPoisoned | Diverge::BlockEnd(_))) => {
                        unreachable!(
                            "invariant: only comptime quota exhaustion is retryable during runtime lowering"
                        )
                    }
                    Err(Poisoned) => {
                        self.eval.lowered_fns_cache.try_set_lowered(new_entry_id, Err(Poisoned))
                    }
                }
            }
        };
        let lowered = match lowered {
            Ok(lowered) => lowered,
            Err(Poisoned) => {
                return preamble.suppress_poison_iff_diverging_return_type();
            }
        };

        self.lower_runtime_call_at_site(&mut call, lowered, preamble)
    }

    fn lower_runtime_call_at_site(
        &mut self,
        call: &mut Call<'_>,
        lowered: mir::FnId,
        preamble: PreambleResult,
    ) -> MaybePoisoned<Result<EvalValue, Diverge>> {
        let (mir_args, validity) = self.eval.mir_args.push_with_res(|mut pusher| {
            for (&param, &arg) in call.params.iter().zip(call.args) {
                let state = call.caller_bindings[arg].state?;
                let local = match state {
                    LocalState::Runtime(local) => local,
                    LocalState::Comptime(value) => {
                        if param.is_comptime {
                            continue;
                        }
                        let ty = self.eval.values.type_of_value(value);
                        if self.eval.types.is_comptime_only(ty) {
                            continue;
                        }
                        let target = call.caller_mir_types.push(ty);
                        self.eval
                            .instr_stack_buf
                            .push(mir::Instruction::Set { target, expr: mir::Expr::Const(value) });
                        target
                    }
                };
                pusher.push(local);
            }
            Ok(())
        });
        if let Err(Poisoned) = validity {
            return preamble.suppress_poison_iff_diverging_return_type();
        }

        let expr = mir::Expr::Call { callee: lowered, args: mir_args };
        let result_type = self.eval.mir_fns[lowered].return_type;
        if result_type == TypeId::NEVER {
            let target = call.caller_mir_types.push(result_type);
            self.eval.instr_stack_buf.push(mir::Instruction::Set { target, expr });
            return Ok(Err(Diverge::END));
        }

        Ok(Ok(EvalValue::Runtime { expr, result_type }))
    }

    fn fold_comptime_call(
        &mut self,
        call: &Call<'_>,
        preamble: PreambleResult,
        values_buf_offset: usize,
    ) -> MaybePoisoned<Result<ComptimeCallResult, Diverge>> {
        preamble.return_type?;

        if let Err(QuotaExhaustedError) = self.comptime_quota.spend_branch() {
            self.diag_ctx.emit_comptime_call_branch_quota_exhausted(
                call.loc(),
                self.comptime_quota.limit(),
                self.eval_branch_quota_start_loc,
            );
            return Ok(Err(Diverge::ComptimeQuotaExhausted));
        }

        let function =
            FunctionKey::new(call.closure, &self.eval.maybe_values_buf[values_buf_offset..]);
        let mut existing_cached_value = None;
        let cache_state = match self.eval.evaluated_fns_cache.lookup(function) {
            Err(new_entry) => new_entry.result,
            Ok(state) => match state.get() {
                EvaluatedFnState::Empty => state,
                EvaluatedFnState::InProgress => {
                    self.diag_ctx.emit_infinite_comptime_recursion(call.loc());
                    state.set(EvaluatedFnState::Done(Err(Poisoned)));
                    return Err(Poisoned);
                }
                EvaluatedFnState::Done(value) => match value {
                    Ok(cached)
                        if self.comptime_quota.replay_cached_call(
                            cached.branches_consumed,
                            cached.max_eval_branch_quota_seen,
                        ) =>
                    {
                        self.max_eval_branch_quota_seen =
                            self.max_eval_branch_quota_seen.max(cached.max_eval_branch_quota_seen);
                        return Ok(Ok(cached));
                    }
                    Ok(cached) => {
                        existing_cached_value = Some(cached);
                        state
                    }
                    Err(Poisoned) => {
                        return preamble.suppress_poison_iff_diverging_return_type();
                    }
                },
            },
        };

        // Pessimistically set result incase we short-circuit before evaluating the body.
        cache_state.set(EvaluatedFnState::Done(Err(Poisoned)));

        let mut poisoned = false;
        for (&param, &arg) in call.params.iter().zip(call.args) {
            if param.is_comptime {
                let ArgParamComptimenessMatch = call.validated;
                continue;
            }
            let Ok((state, _arg_use_span, arg_origin)) = call.caller_bindings[arg].poisoned()
            else {
                poisoned = true;
                continue;
            };
            match state {
                LocalState::Runtime(_) => {
                    if call.caller_comptime {
                        let binding_loc = match arg_origin {
                            DefOrigin::Local(span) => SrcLoc::new(call.source, span),
                            DefOrigin::Const(id) => self.eval.hir.consts[id].loc(),
                        };
                        self.diag_ctx.emit_runtime_ref_in_comptime(
                            SrcLoc::new(call.source, call.span),
                            binding_loc,
                        );
                    } else {
                        let arg_loc = match arg_origin {
                            DefOrigin::Local(span) => SrcLoc::new(call.source, span),
                            DefOrigin::Const(id) => self.eval.hir.consts[id].loc(),
                        };
                        self.diag_ctx
                            .emit_comptime_only_return_with_runtime_arg(arg_loc, call.loc());
                    }
                    poisoned = true;
                }
                LocalState::Comptime(value) => {
                    // If the calling context was runtime we need to un-materialize any comptime
                    // values it turned into runtime in `create_fn_scope_frame`.
                    if let Ok(state) = self.bindings[param.value].state.as_mut() {
                        *state = LocalState::Comptime(value);
                    }
                }
            }
        }
        if poisoned {
            return Err(Poisoned);
        }

        // Undo pessimistic result poison (allows recursion detection).
        cache_state.set(EvaluatedFnState::InProgress);

        let spent_before_body = self.comptime_quota.spent();
        let eval_res = match self.eval_comptime(call.func.body) {
            Ok(()) => unreachable!("lowerer should guarantee return in function body"),
            Err(Diverge::ControlFlowPoisoned) if preamble.return_type == Ok(TypeId::NEVER) => {
                Ok(Err(Diverge::END))
            }
            Err(Diverge::ControlFlowPoisoned) => Err(Poisoned),
            Err(Diverge::ComptimeQuotaExhausted) => {
                cache_state.set(match existing_cached_value {
                    Some(cached) => EvaluatedFnState::Done(Ok(cached)),
                    // Since this was the first attempt at evaluation and it failed due to quota
                    // exhaustion we set the empty state to ensure the call can be retried.
                    None => EvaluatedFnState::Empty,
                });
                return Ok(Err(Diverge::ComptimeQuotaExhausted));
            }
            Err(Diverge::BlockEnd(None)) => Ok(Err(Diverge::END)),
            Err(Diverge::BlockEnd(Some(ret_value))) => Ok(Ok(ret_value)),
        };
        let outcome = match eval_res {
            Ok(Ok(value)) => ComptimeCallOutcome::Value(value),
            Err(Poisoned) => {
                assert!(
                    existing_cached_value.is_none(),
                    "cached function re-evaluation should not poison"
                );
                cache_state.set(EvaluatedFnState::Done(Err(Poisoned)));
                return Err(Poisoned);
            }
            Ok(Err(diverge)) => {
                assert_eq!(diverge, Diverge::END, "only end divergence is cacheable");
                ComptimeCallOutcome::DivergedEnd
            }
        };
        assert!(
            existing_cached_value.is_none_or(|cached| cached.outcome == outcome),
            "re-evaluated function produced different cached outcome"
        );
        let result = ComptimeCallResult {
            outcome,
            branches_consumed: self.comptime_quota.spent() - spent_before_body,
            max_eval_branch_quota_seen: self.max_eval_branch_quota_seen,
        };
        cache_state.set(EvaluatedFnState::Done(Ok(result)));
        Ok(Ok(result))
    }

    pub fn eval_param(
        &mut self,
        comptime: bool,
        arg: hir::LocalId,
        param_kind: hir::ParamType,
        idx: u32,
    ) {
        let EvalContext::FunctionPreamble { arg_spans, call_source } = self.ctx else {
            unreachable!("invariant: param instr outside of fn preamable")
        };

        let arg_span = self.eval.call_arg_spans[arg_spans][idx as usize];
        let arg_loc = SrcLoc::new(call_source, arg_span);

        match param_kind {
            hir::ParamType::Explicit(local_id) => {
                let Ok(param_ty) = self.expect_type(local_id) else {
                    self.bindings[arg].state = Err(Poisoned);
                    return;
                };
                let arg_binding = self.bindings[arg];
                let Ok(state) = arg_binding.state else { return };
                assert!(
                    !comptime || matches!(state, LocalState::Comptime(_)),
                    "invariant: comptime param not comptime in eval"
                );
                let arg_ty = self.state_type(state);
                if !arg_ty.is_assignable_to(param_ty) {
                    self.diag_ctx.emit_type_mismatch(
                        param_ty,
                        self.origin_loc(self.bindings[local_id].origin),
                        arg_ty,
                        arg_loc,
                        false,
                    );
                    self.bindings[arg].state = Err(Poisoned);
                }
            }
            hir::ParamType::Any { capture } => {
                let arg_binding = self.bindings[arg];
                let Ok(state) = arg_binding.state else {
                    self.bindings.insert_no_prev(
                        capture,
                        Local {
                            state: Err(Poisoned),
                            use_span: arg_binding.use_span,
                            origin: DefOrigin::Local(arg_binding.use_span),
                        },
                    );
                    return;
                };
                assert!(
                    !comptime || matches!(state, LocalState::Comptime(_)),
                    "invariant: comptime param not comptime in eval"
                );
                let arg_ty = self.state_type(state);
                let type_value = self.values.intern_type(arg_ty);
                self.bindings.insert_no_prev(
                    capture,
                    Local::comptime(
                        type_value,
                        arg_binding.use_span,
                        DefOrigin::Local(arg_binding.use_span),
                    ),
                );
            }
            hir::ParamType::Poisoned => {
                self.bindings[arg].state = Err(Poisoned);
            }
        }
    }

    pub fn eval_return(&mut self, expr: hir::Expr) -> Result<(), Diverge> {
        let EvalContext::FunctionBody { ret_type, ret_type_loc } = self.ctx else {
            unreachable!("return outside of function body not filtered out by hir-lowerer")
        };
        let value = self.eval_expr(expr)?;

        if let Ok((return_type, value)) = poison::zip(ret_type, value) {
            let ty = self.value_type(value);
            if !ty.is_assignable_to(return_type) {
                self.diag_ctx.emit_type_mismatch(
                    return_type,
                    ret_type_loc,
                    ty,
                    self.loc(expr.span),
                    true,
                );
                return Err(Diverge::ControlFlowPoisoned);
            }
        }

        if self.is_comptime() {
            let Ok(value) = value.and_then(|value| self.expect_comptime_value(value, expr.span))
            else {
                return Err(Diverge::ControlFlowPoisoned);
            };
            return Err(Diverge::BlockEnd(Some(value)));
        }

        let Ok(value) = value else {
            return Err(Diverge::ControlFlowPoisoned);
        };
        let local = match value {
            EvalValue::Runtime { expr, result_type } => {
                let target = self.mir_types.push(result_type);
                self.emit(mir::Instruction::Set { target, expr });
                target
            }
            EvalValue::Comptime(value) => {
                if self.is_comptime_only(value) {
                    self.diag_ctx.emit_comptime_only_value_at_runtime(self.loc(expr.span));
                    return Err(Diverge::ControlFlowPoisoned);
                }
                let ty = self.values.type_of_value(value);
                let target = self.mir_types.push(ty);
                self.emit(mir::Instruction::Set { target, expr: mir::Expr::Const(value) });
                target
            }
        };
        self.emit(mir::Instruction::Return(local));
        Err(Diverge::END)
    }
}
