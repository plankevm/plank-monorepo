use hashbrown::{DefaultHashBuilder, HashTable, hash_table::Entry};
use plank_core::{DenseIndexMap, IndexVec, list_of_lists::ListOfLists, newtype_index};
use plank_hir::{self as hir, ValueId};
use plank_mir as mir;
use plank_session::{MaybePoisoned, Poisoned, SourceId, SourceSpan, SrcLoc, poison};
use plank_values::{DefOrigin, TypeId, Value};

use crate::{
    diagnostics::DiagCtx,
    evaluator::{Evaluator, State},
    scope::{Diverge, EvalContext, EvalValue, Local, LocalState, Scope},
};

newtype_index! {
    pub(crate) struct LoweredFnIdx;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct UniqueFunction<'a> {
    closure: ValueId,
    comptime_params: &'a [ValueId],
}

struct LoweredFn {
    state: State<MaybePoisoned<mir::FnId>>,
    closure: ValueId,
}

pub(crate) struct LoweredFunctionsCache {
    functions: IndexVec<LoweredFnIdx, LoweredFn>,
    comptime_params: ListOfLists<LoweredFnIdx, ValueId>,
    dedup: HashTable<LoweredFnIdx>,
    hasher: DefaultHashBuilder,
}

impl LoweredFunctionsCache {
    pub fn new() -> Self {
        Self {
            functions: IndexVec::new(),
            comptime_params: ListOfLists::new(),
            dedup: HashTable::new(),
            hasher: DefaultHashBuilder::default(),
        }
    }

    fn set_lowered(
        &mut self,
        id: LoweredFnIdx,
        lowered: MaybePoisoned<mir::FnId>,
    ) -> MaybePoisoned<mir::FnId> {
        match &mut self.functions[id].state {
            State::Done(Err(Poisoned)) => return Err(Poisoned),
            State::Done(Ok(_)) => unreachable!("invariant: state corrupted while lowering"),
            state @ State::InProgress => *state = State::Done(lowered),
        }
        lowered
    }

    fn try_start_lower<'a>(
        &mut self,
        func: UniqueFunction<'a>,
    ) -> Result<State<MaybePoisoned<mir::FnId>>, LoweredFnIdx> {
        use std::hash::BuildHasher;
        let hash = self.hasher.hash_one(func);
        let entry = self.dedup.entry(
            hash,
            |&idx| {
                let closure = self.functions[idx].closure;
                closure == func.closure && func.comptime_params == &self.comptime_params[idx]
            },
            |&idx| {
                let closure = self.functions[idx].closure;
                let comptime_params = &self.comptime_params[idx];
                self.hasher.hash_one(UniqueFunction { closure, comptime_params })
            },
        );
        match entry {
            Entry::Occupied(occupied) => {
                let state = &mut self.functions[*occupied.get()].state;
                if let State::InProgress = state {
                    *state = State::Done(Err(Poisoned));
                }
                Ok(*state)
            }
            Entry::Vacant(vacant) => {
                let id = self
                    .functions
                    .push(LoweredFn { state: State::InProgress, closure: func.closure });
                let id2 = self.comptime_params.push_copy_slice(func.comptime_params);
                assert_eq!(id, id2);
                vacant.insert(id);
                Err(id)
            }
        }
    }
}

struct PreambleResult {
    return_type: MaybePoisoned<TypeId>,
    is_comptime_only: bool,
}

impl<'a, 'ctx> Scope<'a, 'ctx> {
    #[allow(clippy::too_many_arguments)]
    fn create_fn_scope(
        eval: &'a mut Evaluator<'ctx>,
        diag_ctx: &'a mut DiagCtx<'ctx>,
        parent_bindings: &DenseIndexMap<hir::LocalId, Local>,
        parent_mir_types: &IndexVec<mir::LocalId, TypeId>,
        parent_source: SourceId,
        fn_def_id: hir::FnDefId,
        args_id: hir::CallArgsId,
        capture_buf_offset: usize,
        is_comptime: bool,
    ) -> Scope<'a, 'ctx> {
        let fn_def = eval.hir.fns[fn_def_id];
        let params = &eval.hir.fn_params[fn_def_id];
        let args = &eval.hir.call_args[args_id];

        let arg_spans =
            eval.call_arg_spans.push_iter(args.iter().map(|&arg| parent_bindings[arg].span));

        let mut fn_scope = Scope::new(
            eval,
            diag_ctx,
            fn_def.source,
            false,
            EvalContext::FunctionPreamble { call_scope_source: parent_source, arg_spans },
        );

        let captured_values = &fn_scope.eval.captures_buf[capture_buf_offset..];
        let capture_defs = &fn_scope.eval.hir.fn_captures[fn_def_id];
        for (&(value, _origin), &def) in captured_values.iter().zip(capture_defs) {
            fn_scope.bindings.insert_no_prev(def.inner_local, Local::comptime(value, def.use_span));
        }

        for (&param, &arg) in params.iter().zip(args) {
            let binding = parent_bindings[arg];
            let state = binding.state.and_then(|state| {
                if param.is_comptime {
                    let LocalState::Comptime(value) = state else {
                        unreachable!("invariant: comptime param validated before this point");
                    };
                    Ok(LocalState::Comptime(value))
                } else if is_comptime {
                    match state {
                        LocalState::Runtime(_) => {
                            fn_scope.diag_ctx.emit_runtime_ref_in_comptime(
                                parent_source,
                                binding.span,
                                binding.span,
                            );
                            Err(Poisoned)
                        }
                        LocalState::Comptime(value) => Ok(LocalState::Comptime(value)),
                    }
                } else {
                    let ty = match state {
                        LocalState::Runtime(outer_mir) => parent_mir_types[outer_mir],
                        LocalState::Comptime(value) => fn_scope.eval.values.type_of_value(value),
                    };
                    let inner_mir = fn_scope.mir_types.push(ty);
                    Ok(LocalState::Runtime(inner_mir))
                }
            });
            fn_scope.bindings.insert_no_prev(param.value, Local { state, span: param.span });
        }

        fn_scope
    }

    fn eval_preamble(&mut self, fn_def_id: hir::FnDefId) -> MaybePoisoned<PreambleResult> {
        let fn_def = self.hir.fns[fn_def_id];
        match self.eval_comptime(fn_def.type_preamble) {
            Ok(()) => {}
            Err(Diverge::PoisonedControlFlow) => return Err(Poisoned),
            Err(Diverge::BlockEnd(_)) => unreachable!("invariant: block end in preamble?"),
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
            this.with_values_buf(|this, values_buf_offset: usize| {
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
                    this.diag_ctx.emit_not_callable(
                        &this.eval.types,
                        ty,
                        this.loc(callee_def_span),
                    );
                    return Err(Poisoned);
                };
                for &capture in captures {
                    this.eval.captures_buf.push(capture);
                }
                this.eval_call_inner(
                    closure_vid,
                    fn_def_id,
                    args_id,
                    call_span,
                    capture_buf_offset,
                    values_buf_offset,
                )
            })
        })
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

        if params.len() != args.len() {
            self.diag_ctx.emit_arg_count_mismatch(
                params.len(),
                args.len(),
                self.loc(call_span),
                func.loc(func.param_list_span),
            );
            return Err(Poisoned);
        }

        if !self.is_comptime() {
            // We can skip validating individual parameter comptimeness if caller context is
            // comptime as we'll force all values to be comptime anyway.
            let mut comptime_args_poisoned = false;
            for (&param, &arg) in params.iter().zip(args) {
                if !param.is_comptime {
                    continue;
                }
                let Ok((arg_state, arg_span)) = self.bindings[arg].poisoned() else {
                    comptime_args_poisoned = true;
                    continue;
                };
                let arg_value = match arg_state {
                    LocalState::Comptime(value) => value,
                    LocalState::Runtime(_) => {
                        self.diag_ctx.emit_comptime_param_got_runtime(
                            self.loc(arg_span),
                            func.loc(param.span),
                        );
                        comptime_args_poisoned = true;
                        continue;
                    }
                };
                self.values_buf.push(arg_value);
            }
            if comptime_args_poisoned {
                return Err(Poisoned);
            }
        }

        let is_parent_comptime = self.is_comptime();
        let parent_source = self.source;
        let parent_bindings = &self.bindings;

        let mut fn_scope = Scope::create_fn_scope(
            self.eval,
            self.diag_ctx,
            parent_bindings,
            &self.mir_types,
            parent_source,
            fn_def_id,
            args_id,
            capture_buf_offset,
            is_parent_comptime,
        );
        let parent_mir_types = &mut self.mir_types;

        let preamble = fn_scope.eval_preamble(fn_def_id)?;

        if is_parent_comptime || preamble.is_comptime_only {
            preamble.return_type?;

            let mut poisoned = false;
            for (&param, &arg) in params.iter().zip(args) {
                if param.is_comptime {
                    continue;
                }
                let Ok((state, span)) = parent_bindings[arg].poisoned() else {
                    poisoned = true;
                    continue;
                };
                match state {
                    LocalState::Runtime(_) => {
                        if is_parent_comptime {
                            fn_scope.diag_ctx.emit_runtime_ref_in_comptime(
                                parent_source,
                                call_span,
                                span,
                            );
                        } else {
                            fn_scope.diag_ctx.emit_comptime_only_return_with_runtime_arg(
                                SrcLoc::new(parent_source, span),
                                SrcLoc::new(parent_source, call_span),
                            );
                        }
                        poisoned = true;
                    }
                    LocalState::Comptime(value) => {
                        // Rebind Runtime → Comptime only when create_fn_scope materialized them as
                        // Runtime (i.e. runtime context with comptime-only return). In the
                        // comptime-parent path the binding is already Comptime and may have been
                        // poisoned by eval_param — overwriting would suppress the type error.
                        if !is_parent_comptime {
                            fn_scope.bindings[param.value].state = Ok(LocalState::Comptime(value));
                        }
                    }
                }
            }
            if poisoned {
                return Err(Poisoned);
            }

            fn_scope.comptime = true;
            return match fn_scope.eval_comptime(func.body) {
                Ok(()) => unreachable!("lowerer should guarantee return in function body"),
                Err(Diverge::PoisonedControlFlow | Diverge::BlockEnd(None)) => Err(Poisoned),
                Err(Diverge::BlockEnd(Some(ret_value))) => Ok(Ok(EvalValue::Comptime(ret_value))),
            };
        }

        // --- Runtime path ---
        // Non-comptime params are already bound as Runtime by create_fn_scope.

        let function = UniqueFunction {
            closure,
            comptime_params: &fn_scope.eval.values_buf[values_buf_offset..],
        };
        let lowered = match fn_scope.eval.lowered_fns_cache.try_start_lower(function) {
            Ok(State::Done(lowered)) => lowered?,
            Ok(State::InProgress) => {
                fn_scope.diag_ctx.emit_runtime_call_with_recursion(fn_scope.loc(call_span));
                return Err(Poisoned);
            }
            Err(lowered_id) => {
                let (body, body_eval_res) = fn_scope.eval_block_to_mir(func.body);
                match body_eval_res {
                    Ok(()) => unreachable!("lowerer should guarantee return in function body"),
                    Err(Diverge::PoisonedControlFlow) => return Err(Poisoned),
                    Err(Diverge::BlockEnd(_)) => {}
                }
                let return_type = preamble.return_type?;
                let fn_id1 = fn_scope.eval.mir_fn_locals.push_copy_slice(&fn_scope.mir_types);
                let fn_id2 = fn_scope.eval.mir_fns.push(mir::FnDef {
                    body,
                    param_count: params.iter().filter(|p| !p.is_comptime).count() as u32,
                    return_type,
                });
                assert_eq!(fn_id1, fn_id2);
                fn_scope.eval.lowered_fns_cache.set_lowered(lowered_id, Ok(fn_id1))?
            }
        };

        let (mir_args, validity) = fn_scope.eval.mir_args.push_with_res(|mut pusher| {
            for (&param, &arg) in params.iter().zip(args) {
                let state = parent_bindings[arg].state?;
                let local = match state {
                    LocalState::Runtime(local) => local,
                    LocalState::Comptime(value) => {
                        if param.is_comptime {
                            continue;
                        }
                        let ty = fn_scope.eval.values.type_of_value(value);
                        let target = parent_mir_types.push(ty);
                        fn_scope
                            .eval
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
        let result_type = fn_scope.eval.mir_fns[lowered].return_type;
        if result_type == TypeId::NEVER {
            let target = parent_mir_types.push(result_type);
            fn_scope.eval.instr_stack_buf.push(mir::Instruction::Set { target, expr });
            return Ok(Err(Diverge::BlockEnd(None)));
        }

        Ok(Ok(EvalValue::Runtime { expr, result_type }))
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
}
