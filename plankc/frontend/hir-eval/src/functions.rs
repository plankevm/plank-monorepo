use hashbrown::hash_map::Entry;
use plank_hir as hir;
use plank_mir as mir;
use plank_session::{MaybePoisoned, Poisoned, SourceSpan, SrcLoc};
use plank_values::{DefOrigin, Value};

use crate::{
    evaluator::State,
    scope::{EvalContext, EvalValue, Local, LocalState, Scope},
};

impl Scope<'_, '_> {
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
    ) -> MaybePoisoned<EvalValue> {
        let (state, callee_def_span) = self.bindings[callee].poisoned()?;
        let closure_vid = match state {
            LocalState::Comptime(value) => value,
            LocalState::Runtime(_) => {
                self.diag_ctx.emit_call_target_not_comptime(self.loc(callee_def_span));
                return Err(Poisoned);
            }
        };
        let Value::Closure { fn_def: fn_def_id, captures } = self.values.lookup(closure_vid) else {
            let ty = self.values.type_of_value(closure_vid);
            self.diag_ctx.emit_not_callable(&self.eval.types, ty, self.loc(callee_def_span));
            return Err(Poisoned);
        };
        let func = self.hir.fns[fn_def_id];
        let params = &self.hir.fn_params[fn_def_id];
        let args = &self.hir.call_args[args_id];
        if params.len() != args.len() {
            self.diag_ctx.emit_arg_count_mismatch(
                params.len(),
                args.len(),
                self.loc(call_span),
                func.loc(func.param_list_span),
            );
            return Err(Poisoned);
        }

        if self.is_comptime() {
            // TODO
            self.diag_ctx.emit_not_yet_implemented("comptime function calls", self.loc(call_span));
            return Err(Poisoned);
        }

        // Comptime:
        // - eval preamble
        // - eval body with comptime context
        // - return ends evaluation, bubbles up result as `EvalValue::Comptime`
        //
        // Runtime:
        // - check lowered cache
        // - no cache:
        //     - comptime eval preamble
        //     - runtime eval body
        //     - `return` statements lowered to mir returns
        // - result is `EvalValue::Runtime` with [`mir::Expr::Call`]

        let lowered = match self.lowered_fns_cache.entry(closure_vid) {
            Entry::Occupied(mut occupied) => match *occupied.get() {
                State::Done(lowered) => lowered?,
                State::InProgress => {
                    occupied.insert(State::Done(Err(Poisoned)));
                    self.diag_ctx.emit_runtime_call_with_recursion(self.loc(call_span));
                    return Err(Poisoned);
                }
            },
            Entry::Vacant(vacant) => {
                vacant.insert(State::InProgress);
                let lowered = self.lower_runtime_function(fn_def_id, args_id);
                match self.lowered_fns_cache.get_mut(&closure_vid) {
                    Some(state @ State::InProgress) => *state = State::Done(lowered),
                    Some(State::Done(Err(Poisoned))) => return Err(Poisoned),
                    Some(State::Done(Ok(_))) | None => {
                        unreachable!("invariant: state corruped while lowering")
                    }
                };
                lowered?
            }
        };

        todo!()
    }

    fn lower_runtime_function(
        &mut self,
        fn_def_id: hir::FnDefId,
        args_id: hir::CallArgsId,
    ) -> MaybePoisoned<mir::FnId> {
        let fn_def = self.hir.fns[fn_def_id];
        let params = &self.hir.fn_params[fn_def_id];
        let args = &self.hir.call_args[args_id];

        let parent_source = self.source;
        let parent_bindings = &mut self.bindings;
        let parent_mir_types = &mut self.mir_types;

        let arg_spans = self.eval.call_arg_spans.push_with(|mut pusher| {
            for &arg in args {
                let span = self.bindings[arg].span;
                pusher.push(span);
            }
        });

        let mut fn_scope = Scope::new(
            self.eval,
            self.diag_ctx,
            fn_def.source,
            true,
            EvalContext::FunctionPreamble { call_scope_source: self.source, arg_spans },
        );

        fn_scope.eval_comptime(fn_def.type_preamble).expect("todo: diverge handling");

        let ret_type = fn_scope.expect_type(fn_def.return_type);
        fn_scope.ctx =
            Some(Function { ret_type, ret_type_span: fn_scope.bindings[fn_def.return_type].span });
        fn_scope.comptime = false;

        for (&param, &arg) in params.iter().zip(args) {
            let param_ty = fn_scope.expect_type(param.r#type).and_then(|param_ty| {
                let arg_binding = parent_bindings[arg];
                let arg_ty = match arg_binding.state? {
                    LocalState::Runtime(mir_local) => parent_mir_types[mir_local],
                    LocalState::Comptime(value) => fn_scope.values.type_of_value(value),
                };
                if !arg_ty.is_assignable_to(param_ty) {
                    fn_scope.diag_ctx.emit_type_mismatch(
                        &fn_scope.eval.types,
                        param_ty,
                        fn_def.loc(param.span),
                        arg_ty,
                        SrcLoc::new(parent_source, arg_binding.span),
                    );
                    return Err(Poisoned);
                }
                Ok(param_ty)
            });
            fn_scope.bindings.insert_no_prev(
                param.value,
                Local {
                    state: param_ty.map(|ty| {
                        let mir_local = fn_scope.mir_types.push(ty);
                        LocalState::Runtime(mir_local)
                    }),
                    span: param.span,
                },
            );
        }

        let res = fn_scope.eval_block_to_mir(fn_def.body);
        // Ensures we don't use `self` before we're done with `fn_scope`.
        drop(fn_scope);

        todo!()
    }
}
