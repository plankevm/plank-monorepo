use plank_core::DenseIndexMap;
use plank_hir::{self as hir, ValueId};
use plank_mir as mir;
use plank_session::{MaybePoisoned, Poisoned, SourceId, SourceSpan, SrcLoc, poison};
use plank_values::{DefOrigin, TypeId, Value};

mod cache;

use cache::*;
pub(crate) use cache::{EvaluatedFunctionCache, LoweredFunctionsCache};

use crate::{
    evaluator::State,
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

struct Call<'a> {
    source: SourceId,
    caller_comptime: bool,
    caller_bindings: &'a DenseIndexMap<hir::LocalId, Local>,
    span: SourceSpan,

    closure: ValueId,
    func: hir::FnDef,
    args: &'a [hir::LocalId],
    params: &'a [hir::ParamInfo],

    validated: ArgParamComptimenessMatch,
}

fn comptime_args(
    is_comptime: bool,
    params: &[hir::ParamInfo],
    args: &[hir::LocalId],
) -> impl Iterator<Item = (hir::ParamInfo, hir::LocalId)> {
    params.iter().zip(args).filter_map(move |(&param, &arg)| {
        (param.is_comptime || is_comptime).then_some((param, arg))
    })
}

impl Call<'_> {
    fn loc(&self) -> SrcLoc {
        SrcLoc::new(self.source, self.span)
    }
}

impl<'a, 'ctx> Scope<'a, 'ctx> {
    fn create_fn_scope<'s>(
        &'s mut self,
        fn_def_id: hir::FnDefId,
        args_id: hir::CallArgsId,
        capture_buf_offset: usize,
        validated: ArgParamComptimenessMatch,
    ) -> (Scope<'s, 'ctx>, &'s DenseIndexMap<hir::LocalId, Local>) {
        let fn_def = self.eval.hir.fns[fn_def_id];
        let params = &self.eval.hir.fn_params[fn_def_id];
        let args = &self.eval.hir.call_args[args_id];
        let is_comptime = self.is_comptime();
        let caller_bindings = &mut self.bindings;
        let caller_mir_types = &mut self.mir_types;

        let arg_spans =
            self.eval.call_arg_spans.push_iter(args.iter().map(|&arg| caller_bindings[arg].span));

        let mut fn_scope = Scope::new(
            self.eval,
            self.diag_ctx,
            fn_def.source,
            false,
            EvalContext::FunctionPreamble { call_scope_source: self.source, arg_spans },
        );

        let captured_values = &fn_scope.eval.captures_buf[capture_buf_offset..];
        let capture_defs = &fn_scope.eval.hir.fn_captures[fn_def_id];
        for (&(value, _origin), &def) in captured_values.iter().zip(capture_defs) {
            fn_scope.bindings.insert_no_prev(def.inner_local, Local::comptime(value, def.use_span));
        }

        for (&param, &arg) in params.iter().zip(args) {
            let binding = caller_bindings[arg];
            let state = match binding.state {
                Ok(state) => state,
                Err(Poisoned) => {
                    fn_scope.bindings.insert_no_prev(
                        param.value,
                        Local { state: Err(Poisoned), span: param.span },
                    );
                    continue;
                }
            };

            let state = if param.is_comptime {
                let LocalState::Comptime(value) = state else {
                    let ArgParamComptimenessMatch = validated;
                    unreachable!("invariant: already validated");
                };
                Ok(LocalState::Comptime(value))
            } else if is_comptime {
                match state {
                    LocalState::Runtime(_) => {
                        let ArgParamComptimenessMatch = validated;
                        Err(Poisoned)
                    }
                    LocalState::Comptime(value) => Ok(LocalState::Comptime(value)),
                }
            } else {
                'state: {
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
                }
            };
            fn_scope.bindings.insert_no_prev(param.value, Local { state, span: param.span });
        }

        (fn_scope, caller_bindings)
    }

    fn eval_preamble(&mut self, fn_def_id: hir::FnDefId) -> MaybePoisoned<PreambleResult> {
        let fn_def = self.hir.fns[fn_def_id];
        match self.eval_comptime(fn_def.type_preamble) {
            Ok(()) => {}
            Err(Diverge::ControlFlowPoisoned | Diverge::BlockEnd(_)) => return Err(Poisoned),
        }
        let return_type = self.expect_type(fn_def.return_type);
        let ret_type_span = self.bindings[fn_def.return_type].span;
        self.ctx = EvalContext::FunctionBody { ret_type: return_type, ret_type_span };
        let is_comptime_only = return_type.is_ok_and(|ty| self.types.is_comptime_only(ty));
        Ok(PreambleResult { return_type, is_comptime_only })
    }

    pub(crate) fn eval_fn_def(&mut self, id: hir::FnDefId) -> MaybePoisoned<EvalValue> {
        let def_captures = &self.hir.fn_captures[id];
        self.with_captures_buf(|this, captures_buf_offset| {
            let mut poisoned = false;
            for &capture in def_captures {
                let Local { state, span: def_span } = this.bindings[capture.outer_local];
                let Ok(state) = state else {
                    poisoned = true;
                    continue;
                };
                let value = match state {
                    LocalState::Comptime(value) => value,
                    LocalState::Runtime(_) => {
                        this.diag_ctx.emit_closure_capture_not_comptime(
                            this.loc(capture.use_span),
                            this.loc(def_span),
                        );
                        poisoned = true;
                        continue;
                    }
                };
                this.captures_buf.push((value, DefOrigin::Local(def_span)));
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
                let (state, callee_def_span) = this.bindings[callee].poisoned()?;
                let closure_vid = match state {
                    LocalState::Comptime(value) => value,
                    LocalState::Runtime(_) => {
                        this.diag_ctx.emit_call_target_not_comptime(this.loc(callee_def_span));
                        return Err(Poisoned);
                    }
                };
                let Value::Closure { fn_def: fn_def_id, captures } =
                    this.eval.values.lookup(closure_vid)
                else {
                    let ty = this.values.type_of_value(closure_vid);
                    this.diag_ctx.emit_not_callable(ty, this.loc(callee_def_span));
                    return Err(Poisoned);
                };
                for &capture in captures {
                    this.eval.captures_buf.push(capture);
                }

                let arg_span_groups_before = this.eval.call_arg_spans.len();
                let eval_res = this.eval_call_inner(
                    closure_vid,
                    fn_def_id,
                    args_id,
                    call_span,
                    capture_buf_offset,
                    values_buf_offset,
                );
                let arg_span_groups_after = this.eval.call_arg_spans.len();

                let diff = arg_span_groups_after
                    .checked_sub(arg_span_groups_before)
                    .expect("inconsistent arg spans cleanup");
                assert!(diff <= 1);
                if diff == 1 {
                    this.eval.call_arg_spans.pop();
                }

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
        for (param, arg) in comptime_args(self.is_comptime(), params, args) {
            let arg = self.bindings[arg];
            if let Ok(LocalState::Runtime(_)) = arg.state {
                self.diag_ctx
                    .emit_comptime_param_got_runtime(self.loc(arg.span), func.loc(param.span));
                comptime_args_poisoned = true;
                continue;
            };
        }
        if comptime_args_poisoned { Err(Poisoned) } else { Ok(ArgParamComptimenessMatch) }
    }

    pub(crate) fn eval_call_inner(
        &mut self,
        closure: ValueId,
        fn_def_id: hir::FnDefId,
        args_id: hir::CallArgsId,
        call_span: SourceSpan,
        capture_buf_offset: usize,
        values_buf_offset: usize,
    ) -> MaybePoisoned<Result<EvalValue, Diverge>> {
        let func = self.hir.fns[fn_def_id];
        let params = &self.hir.fn_params[fn_def_id];
        let args = &self.hir.call_args[args_id];
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

        let (mut fn_scope, call) = {
            let caller_comptime = self.is_comptime();
            let call_source = self.source;
            let (fn_scope, caller_bindings) =
                self.create_fn_scope(fn_def_id, args_id, capture_buf_offset, validated);
            let call = Call {
                source: call_source,
                caller_comptime,
                caller_bindings,
                span: call_span,
                closure,
                func,
                args,
                params,
                validated,
            };
            (fn_scope, call)
        };

        let preamble = {
            let restore = fn_scope.diag_ctx.set_preamble_call_site(call.loc());
            let preamble = fn_scope.eval_preamble(fn_def_id);
            fn_scope.diag_ctx.restore_preamble_call_site(restore);
            preamble?
        };

        // Assemble comptime parameters for the function key.
        for (&param, &arg) in params.iter().zip(args) {
            let value = match fn_scope.bindings[param.value].state {
                Ok(LocalState::Comptime(value)) => Ok(value),
                Err(Poisoned) => Err(Poisoned),
                Ok(LocalState::Runtime(_)) => match call.caller_bindings[arg].state {
                    // `create_fn_scope` optimistically makes params runtime in runtime contexts,
                    // if we find out we need to evaluate as comptime we need to make sure all
                    // arguments are added to the key.
                    Ok(LocalState::Comptime(value)) if preamble.is_comptime_only => Ok(value),
                    _ => {
                        let ArgParamComptimenessMatch = validated;
                        continue;
                    }
                },
            };
            fn_scope.eval.maybe_values_buf.push(value);
        }

        if call.caller_comptime || preamble.is_comptime_only {
            return fn_scope.fold_comptime_call(&call, preamble, values_buf_offset);
        }

        // --- Runtime path ---
        // Non-comptime params are already bound as Runtime in `create_fn_scope`.
        let function =
            FunctionKey::new(closure, &fn_scope.eval.maybe_values_buf[values_buf_offset..]);
        let param_count = (params.len() - function.params.len()) as u32;

        let lowered = match fn_scope.eval.lowered_fns_cache.retrieve_or_create_entry(function) {
            Ok(&mut State::Done(fn_id)) => fn_id,
            Ok(state @ State::InProgress) => {
                fn_scope.diag_ctx.emit_runtime_call_with_recursion(call_loc);
                *state = State::Done(Err(Poisoned));
                Err(Poisoned)
            }
            Err(new_entry_id) => {
                let fn_id = (|| {
                    let (body, body_eval_res) = fn_scope.eval_block_to_mir(func.body);
                    match body_eval_res {
                        Ok(()) => unreachable!("lowerer should guarantee return in function body"),
                        Err(Diverge::BlockEnd(_)) => {}
                        Err(Diverge::ControlFlowPoisoned) => return Err(Poisoned),
                    }
                    let return_type = preamble.return_type?;
                    let fn_id1 = fn_scope.eval.mir_fn_locals.push_copy_slice(&fn_scope.mir_types);
                    let fn_id2 =
                        fn_scope.eval.mir_fns.push(mir::FnDef { body, param_count, return_type });
                    assert_eq!(fn_id1, fn_id2);
                    Ok(fn_id1)
                })();
                fn_scope.eval.lowered_fns_cache.try_set_lowered(new_entry_id, fn_id)
            }
        };
        let lowered = match lowered {
            Ok(lowered) => lowered,
            Err(Poisoned) => {
                return if preamble.return_type == Ok(TypeId::NEVER) {
                    Ok(Err(Diverge::END))
                } else {
                    Err(Poisoned)
                };
            }
        };

        let (mir_args, validity) = self.eval.mir_args.push_with_res(|mut pusher| {
            for (&param, &arg) in params.iter().zip(args) {
                let state = self.bindings[arg].state?;
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
                        let target = self.mir_types.push(ty);
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
            return Err(Poisoned);
        }

        let expr = mir::Expr::Call { callee: lowered, args: mir_args };
        let result_type = self.eval.mir_fns[lowered].return_type;
        if result_type == TypeId::NEVER {
            let target = self.mir_types.push(result_type);
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
    ) -> MaybePoisoned<Result<EvalValue, Diverge>> {
        let function =
            FunctionKey::new(call.closure, &self.eval.maybe_values_buf[values_buf_offset..]);
        preamble.return_type?;

        let new_fn_eval_cache_entry = match self.eval.evaluated_fns_cache.lookup(function) {
            Err(new_entry) => new_entry,
            Ok(state) => match state.get() {
                State::InProgress => {
                    self.diag_ctx.emit_infinite_comptime_recursion(call.loc());
                    state.set(State::Done(Err(Poisoned)));
                    return Err(Poisoned);
                }
                State::Done(value) => {
                    return match value {
                        Ok(value) => Ok(Ok(EvalValue::Comptime(value))),
                        Err(Poisoned) => {
                            // Cache collapses `Diverge` into Err(Poisoned);
                            // reconstruct the diverge when the return type was never.
                            if preamble.return_type == Ok(TypeId::NEVER) {
                                Ok(Err(Diverge::END))
                            } else {
                                Err(Poisoned)
                            }
                        }
                    };
                }
            },
        };

        // Pessimistically set result incase we short-circuit before evaluating the body.
        new_fn_eval_cache_entry.result.set(State::Done(Err(Poisoned)));

        let mut poisoned = false;
        for (&param, &arg) in call.params.iter().zip(call.args) {
            if param.is_comptime {
                let ArgParamComptimenessMatch = call.validated;
                continue;
            }
            let Ok((state, arg_span)) = call.caller_bindings[arg].poisoned() else {
                poisoned = true;
                continue;
            };
            match state {
                LocalState::Runtime(_) => {
                    if call.caller_comptime {
                        self.diag_ctx.emit_runtime_ref_in_comptime(
                            call.source,
                            call.span,
                            arg_span,
                        );
                    } else {
                        self.diag_ctx.emit_comptime_only_return_with_runtime_arg(
                            SrcLoc::new(call.source, arg_span),
                            call.loc(),
                        );
                    }
                    poisoned = true;
                }
                LocalState::Comptime(value) => {
                    // If the calling context was runtime we need to un-materialize any comptime
                    // values it turned into runtime in `create_fn_scope`.
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
        new_fn_eval_cache_entry.result.set(State::InProgress);

        let eval_res = match self.eval_comptime(call.func.body) {
            Ok(()) => unreachable!("lowerer should guarantee return in function body"),
            Err(Diverge::ControlFlowPoisoned) => Err(Poisoned),
            Err(Diverge::BlockEnd(None)) => Ok(Err(Diverge::END)),
            Err(Diverge::BlockEnd(Some(ret_value))) => Ok(Ok(ret_value)),
        };
        new_fn_eval_cache_entry.result.set(State::Done(match eval_res {
            Ok(Ok(value)) => Ok(value),
            Err(Poisoned) | Ok(Err(_)) => Err(Poisoned),
        }));
        eval_res.map(|value_or_diverge| value_or_diverge.map(EvalValue::Comptime))
    }

    pub fn eval_param(
        &mut self,
        comptime: bool,
        arg: hir::LocalId,
        r#type: hir::LocalId,
        idx: u32,
    ) {
        let EvalContext::FunctionPreamble { call_scope_source, arg_spans } = self.ctx else {
            unreachable!("invariant: param instr outside of fn preamable")
        };

        let Ok(param_ty) = self.expect_type(r#type) else {
            self.bindings[arg].state = Err(Poisoned);
            return;
        };
        let arg_binding = self.bindings[arg];
        let Ok(state) = arg_binding.state else { return };
        if comptime {
            assert!(
                matches!(state, LocalState::Comptime(_)),
                "invariant: comptime param not comptime in eval"
            );
        }
        let arg_ty = self.state_type(state);
        if !arg_ty.is_assignable_to(param_ty) {
            let arg_span = self.eval.call_arg_spans[arg_spans][idx as usize];
            self.diag_ctx.emit_type_mismatch(
                param_ty,
                self.loc(self.bindings[r#type].span),
                arg_ty,
                SrcLoc::new(call_scope_source, arg_span),
                false,
            );
            self.bindings[arg].state = Err(Poisoned);
        }
    }

    pub fn eval_return(&mut self, expr: hir::Expr) -> Result<(), Diverge> {
        let EvalContext::FunctionBody { ret_type, ret_type_span } = self.ctx else {
            unreachable!("return outside of function body not filtered out by hir-lowerer")
        };
        let value = self.eval_expr(expr)?;

        if let Ok((return_type, value)) = poison::zip(ret_type, value) {
            let ty = self.value_type(value);
            if !ty.is_assignable_to(return_type) {
                self.diag_ctx.emit_type_mismatch(
                    return_type,
                    self.loc(ret_type_span),
                    ty,
                    self.loc(expr.span),
                    true,
                );
                return Err(Diverge::END);
            }
        }

        if self.is_comptime() {
            let Ok(value) = value.and_then(|value| self.expect_comptime_value(value, expr.span))
            else {
                return Err(Diverge::END);
            };
            return Err(Diverge::BlockEnd(Some(value)));
        }

        let Ok(value) = value else {
            return Err(Diverge::END);
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
                    return Err(Diverge::END);
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
