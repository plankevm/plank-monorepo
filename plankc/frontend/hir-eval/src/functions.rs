use hashbrown::hash_map::Entry;
use plank_hir::{self as hir, ValueId};
use plank_mir as mir;
use plank_session::{MaybePoisoned, Poisoned, SourceSpan, SrcLoc, poison};
use plank_values::{DefOrigin, TypeId, Value};

use crate::{
    evaluator::State,
    scope::{Diverge, EvalContext, EvalValue, Local, LocalState, Scope},
};

impl Scope<'_, '_> {
    pub(crate) fn eval_fn_def(&mut self, id: hir::FnDefId) -> MaybePoisoned<EvalValue> {
        let fn_def = self.hir.fns[id];
        let params = &self.hir.fn_params[id];
        if let Some(&param) = params.iter().find(|param| param.is_comptime) {
            self.diag_ctx.emit_not_yet_implemented("comptime parameters", fn_def.loc(param.span));
            return Err(Poisoned);
        }

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
                this.diag_ctx.emit_not_callable(&this.eval.types, ty, this.loc(callee_def_span));
                return Err(Poisoned);
            };
            for &capture in captures {
                this.eval.captures_buf.push(capture);
            }
            this.eval_call_inner(closure_vid, fn_def_id, args_id, call_span, capture_buf_offset)
        })
    }

    pub(crate) fn eval_call_inner(
        &mut self,
        closure: ValueId,
        fn_def_id: hir::FnDefId,
        args_id: hir::CallArgsId,
        call_span: SourceSpan,
        capture_buf_offset: usize,
    ) -> MaybePoisoned<Result<EvalValue, Diverge>> {
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
            return self.fold_comptime_call(fn_def_id, args_id, capture_buf_offset, call_span);
        }

        let lowered = match self.lowered_fns_cache.entry(closure) {
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
                let lowered = self.lower_runtime_function(fn_def_id, args_id, capture_buf_offset);
                match self.lowered_fns_cache.get_mut(&closure) {
                    Some(state @ State::InProgress) => *state = State::Done(lowered),
                    Some(State::Done(Err(Poisoned))) => return Err(Poisoned),
                    Some(State::Done(Ok(_))) | None => {
                        unreachable!("invariant: state corruped while lowering")
                    }
                };
                lowered?
            }
        };

        let (mir_args, validity) = self.eval.mir_args.push_with_res(|mut pusher| {
            for &arg in args {
                let state = self.bindings[arg].state?;
                let local = match state {
                    LocalState::Runtime(local) => local,
                    LocalState::Comptime(value) => {
                        let ty = self.eval.values.type_of_value(value);
                        assert!(!self.eval.types.comptime_only(ty), "todo comptime params");
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
            self.emit(mir::Instruction::Set { target, expr });
            return Ok(Err(Diverge::BlockEnd(None)));
        }

        Ok(Ok(EvalValue::Runtime { expr, result_type }))
    }

    fn lower_runtime_function(
        &mut self,
        fn_def_id: hir::FnDefId,
        args_id: hir::CallArgsId,
        capture_buf_offset: usize,
    ) -> MaybePoisoned<mir::FnId> {
        let fn_def = self.hir.fns[fn_def_id];
        let params = &self.hir.fn_params[fn_def_id];
        let args = &self.hir.call_args[args_id];

        let arg_spans =
            self.eval.call_arg_spans.push_iter(args.iter().map(|&arg| self.bindings[arg].span));

        let parent_bindings = &mut self.bindings;
        let parent_mir_types = &mut self.mir_types;

        let mut fn_scope = Scope::new(
            self.eval,
            self.diag_ctx,
            fn_def.source,
            true,
            EvalContext::FunctionPreamble { call_scope_source: self.source, arg_spans },
        );

        let captured_values = &fn_scope.eval.captures_buf[capture_buf_offset..];
        let capture_defs = &fn_scope.eval.hir.fn_captures[fn_def_id];
        for (&(value, _origin), &def) in captured_values.iter().zip(capture_defs) {
            fn_scope.bindings.insert_no_prev(def.inner_local, Local::comptime(value, def.use_span))
        }

        for (&param, &arg) in params.iter().zip(args) {
            assert!(!param.is_comptime, "todo: comptime parameters");
            let binding = parent_bindings[arg];
            let state = binding.state.map(|state| {
                let ty = match state {
                    LocalState::Runtime(outer_mir) => parent_mir_types[outer_mir],
                    LocalState::Comptime(value) => fn_scope.eval.values.type_of_value(value),
                };
                let inner_mir = fn_scope.mir_types.push(ty);
                LocalState::Runtime(inner_mir)
            });
            fn_scope.bindings.insert_no_prev(param.value, Local { state, span: param.span });
        }

        match fn_scope.eval_comptime(fn_def.type_preamble) {
            Ok(()) => {}
            Err(Diverge::PoisonedControlFlow) => return Err(Poisoned),
            Err(Diverge::BlockEnd(_)) => unreachable!("invariant: block end in premable?"),
        }

        let return_type = fn_scope.expect_type(fn_def.return_type);
        fn_scope.comptime = false;
        fn_scope.ctx = EvalContext::FunctionBody {
            ret_type: return_type,
            ret_type_span: fn_scope.bindings[fn_def.return_type].span,
        };

        let (body, body_eval_res) = fn_scope.eval_block_to_mir(fn_def.body);
        match body_eval_res {
            Ok(()) => unreachable!("lowerer should guarantee return in function body"),
            Err(Diverge::PoisonedControlFlow) => return Err(Poisoned),
            Err(Diverge::BlockEnd(_)) => {}
        }

        let return_type = return_type?;

        let fn_id1 = fn_scope.eval.mir_fn_locals.push_copy_slice(&fn_scope.mir_types);
        let fn_id2 = fn_scope.eval.mir_fns.push(mir::FnDef {
            body,
            param_count: params.len() as u32,
            return_type,
        });
        assert_eq!(fn_id1, fn_id2);

        // Ensures we don't use `self` before we're done with `fn_scope`.
        drop(fn_scope);

        Ok(fn_id1)
    }

    pub fn eval_param(
        &mut self,
        comptime: bool,
        arg: hir::LocalId,
        r#type: hir::LocalId,
        idx: u32,
    ) {
        assert!(!comptime, "todo: comptime parameters");
        let EvalContext::FunctionPreamble { call_scope_source, arg_spans } = self.ctx else {
            unreachable!("invariant: param instr outside of fn preamable")
        };

        let Ok(param_ty) = self.expect_type(r#type) else {
            self.bindings[arg].state = Err(Poisoned);
            return;
        };
        let arg_binding = self.bindings[arg];
        let Ok(state) = arg_binding.state else { return };
        let arg_ty = self.state_type(state);
        if !arg_ty.is_assignable_to(param_ty) {
            let arg_span = self.eval.call_arg_spans[arg_spans][idx as usize];
            self.diag_ctx.emit_type_mismatch(
                &self.eval.types,
                param_ty,
                self.loc(self.bindings[r#type].span),
                arg_ty,
                SrcLoc::new(call_scope_source, arg_span),
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
                    &self.eval.types,
                    return_type,
                    self.loc(ret_type_span),
                    ty,
                    self.loc(expr.span),
                );
                return Err(Diverge::BlockEnd(None));
            }
        }

        if self.is_comptime() {
            let Ok(value) = value.and_then(|value| self.expect_comptime_value(value, expr.span))
            else {
                return Err(Diverge::BlockEnd(None));
            };
            return Err(Diverge::BlockEnd(Some(value)));
        }

        let Ok(value) = value else {
            return Err(Diverge::BlockEnd(None));
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
                    return Err(Diverge::BlockEnd(None));
                }
                let ty = self.values.type_of_value(value);
                let target = self.mir_types.push(ty);
                self.emit(mir::Instruction::Set { target, expr: mir::Expr::Const(value) });
                target
            }
        };
        self.emit(mir::Instruction::Return(local));
        Err(Diverge::BlockEnd(None))
    }

    fn fold_comptime_call(
        &mut self,
        fn_def_id: hir::FnDefId,
        args_id: hir::CallArgsId,
        capture_buf_offset: usize,
        call_span: SourceSpan,
    ) -> MaybePoisoned<Result<EvalValue, Diverge>> {
        let fn_def = self.hir.fns[fn_def_id];
        let params = &self.hir.fn_params[fn_def_id];
        let args = &self.hir.call_args[args_id];

        let arg_spans =
            self.eval.call_arg_spans.push_iter(args.iter().map(|&arg| self.bindings[arg].span));

        let parent_source = self.source;
        let parent_bindings = &mut self.bindings;

        let mut fn_scope = Scope::new(
            self.eval,
            self.diag_ctx,
            fn_def.source,
            true,
            EvalContext::FunctionPreamble { call_scope_source: self.source, arg_spans },
        );

        let captured_values = &fn_scope.eval.captures_buf[capture_buf_offset..];
        let capture_defs = &fn_scope.eval.hir.fn_captures[fn_def_id];
        for (&(value, _origin), &def) in captured_values.iter().zip(capture_defs) {
            fn_scope.bindings.insert_no_prev(def.inner_local, Local::comptime(value, def.use_span))
        }

        for (&param, &arg) in params.iter().zip(args) {
            assert!(!param.is_comptime, "todo: comptime parameters");
            let binding = parent_bindings[arg];
            let state = binding.state.and_then(|state| match state {
                LocalState::Runtime(_) => {
                    fn_scope.diag_ctx.emit_runtime_ref_in_comptime(
                        parent_source,
                        call_span,
                        binding.span,
                    );
                    Err(Poisoned)
                }
                LocalState::Comptime(value) => Ok(LocalState::Comptime(value)),
            });
            fn_scope.bindings.insert_no_prev(param.value, Local { state, span: param.span });
        }

        match fn_scope.eval_comptime(fn_def.type_preamble) {
            Ok(()) => {}
            Err(Diverge::PoisonedControlFlow) => return Err(Poisoned),
            Err(Diverge::BlockEnd(_)) => unreachable!("invariant: block end in premable?"),
        }

        let return_type = fn_scope.expect_type(fn_def.return_type);
        let ret_type_span = fn_scope.bindings[fn_def.return_type].span;
        fn_scope.ctx = EvalContext::FunctionBody { ret_type: return_type, ret_type_span };
        fn_scope.comptime = true;

        let return_value = match fn_scope.eval_comptime(fn_def.body) {
            Ok(()) => unreachable!("lowerer should guarantee return in function body"),
            Err(Diverge::PoisonedControlFlow | Diverge::BlockEnd(None)) => return Err(Poisoned),
            Err(Diverge::BlockEnd(Some(ret_value))) => ret_value,
        };

        // Ensures we don't use `self` before we're done with `fn_scope`.
        drop(fn_scope);

        Ok(Ok(EvalValue::Comptime(return_value)))
    }
}
