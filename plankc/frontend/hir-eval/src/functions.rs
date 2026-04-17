use hashbrown::{DefaultHashBuilder, HashTable, hash_table::Entry};
use plank_core::{DenseIndexMap, IndexVec, list_of_lists::ListOfLists, newtype_index};
use plank_hir::{self as hir, ValueId};
use plank_mir as mir;
use plank_session::{MaybePoisoned, Poisoned, SourceSpan, SrcLoc, poison};
use plank_values::{DefOrigin, TypeId, Value};

use crate::{
    evaluator::State,
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

/// Empty marker to track the invariant that arg/param comptimeness matching was already checked.
#[derive(Clone, Copy)]
struct ArgParamComptimenessMatch;

#[derive(Clone, Copy)]
enum RuntimeLowerError {
    PoisonResult,
    PoisonedNever,
}

impl From<Poisoned> for RuntimeLowerError {
    fn from(_value: Poisoned) -> Self {
        Self::PoisonResult
    }
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
        lowered_res: MaybePoisoned<mir::FnId>,
    ) -> MaybePoisoned<mir::FnId> {
        match &mut self.functions[id].state {
            State::Done(Err(Poisoned)) => return Err(Poisoned),
            State::Done(Ok(_)) => unreachable!("invariant: state corrupted while lowering"),
            state @ State::InProgress => {
                *state = State::Done(lowered_res);
                lowered_res
            }
        }
    }

    fn retrieve_or_create_entry<'a>(
        &mut self,
        func: UniqueFunction<'a>,
    ) -> Result<&mut State<MaybePoisoned<mir::FnId>>, LoweredFnIdx> {
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
                let id = *occupied.get();
                Ok(&mut self.functions[id].state)
            }
            Entry::Vacant(vacant) => {
                let new_entry_id = self
                    .functions
                    .push(LoweredFn { state: State::InProgress, closure: func.closure });
                let id2 = self.comptime_params.push_copy_slice(func.comptime_params);
                assert_eq!(new_entry_id, id2);
                vacant.insert(new_entry_id);
                Err(new_entry_id)
            }
        }
    }
}

#[derive(Debug)]
struct PreambleResult {
    return_type: MaybePoisoned<TypeId>,
    is_comptime_only: bool,
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
        let parent_bindings = &mut self.bindings;
        let parent_mir_types = &mut self.mir_types;

        let arg_spans =
            self.eval.call_arg_spans.push_iter(args.iter().map(|&arg| parent_bindings[arg].span));

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
            let binding = parent_bindings[arg];
            let state = binding.state.and_then(|state| {
                if param.is_comptime {
                    let ArgParamComptimenessMatch = validated;
                    let LocalState::Comptime(value) = state else {
                        unreachable!("invariant: comptime param validated before this point");
                    };
                    Ok(LocalState::Comptime(value))
                } else if is_comptime {
                    // In comptime context, runtime non-comptime params are caught and
                    // diagnosed by the validation loop in eval_call_inner (which has
                    // access to call_span for a better diagnostic). Just poison here.
                    match state {
                        LocalState::Runtime(_) => Err(Poisoned),
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

        (fn_scope, parent_bindings)
    }

    fn eval_preamble(&mut self, fn_def_id: hir::FnDefId) -> MaybePoisoned<PreambleResult> {
        let fn_def = self.hir.fns[fn_def_id];
        match self.eval_comptime(fn_def.type_preamble) {
            Ok(()) => {}
            Err(Diverge::PoisonedControlFlow | Diverge::PoisonedNever) => return Err(Poisoned),
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
                    this.diag_ctx.emit_not_callable(ty, this.loc(callee_def_span));
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

    fn validate_args_param_comptimeness_match(
        &mut self,
        func: hir::FnDef,
        params: &[hir::ParamInfo],
        args: &[hir::LocalId],
    ) -> MaybePoisoned<ArgParamComptimenessMatch> {
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
                    self.diag_ctx
                        .emit_comptime_param_got_runtime(self.loc(arg_span), func.loc(param.span));
                    comptime_args_poisoned = true;
                    continue;
                }
            };
            self.values_buf.push(arg_value);
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

        let is_parent_comptime = self.is_comptime();
        let parent_source = self.source;
        let (mut fn_scope, parent_bindings) =
            self.create_fn_scope(fn_def_id, args_id, capture_buf_offset, validated);

        let restore =
            fn_scope.diag_ctx.set_preamble_call_site(SrcLoc::new(parent_source, call_span));
        let preamble = fn_scope.eval_preamble(fn_def_id);
        fn_scope.diag_ctx.restore_preamble_call_site(restore);
        let preamble = preamble?;

        if is_parent_comptime || preamble.is_comptime_only {
            preamble.return_type?;

            let mut poisoned = false;
            for (&param, &arg) in params.iter().zip(args) {
                if param.is_comptime {
                    let ArgParamComptimenessMatch = validated;
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
                    LocalState::Comptime(value) if !is_parent_comptime => {
                        // We optimistically "materialized" all bindings in `create_fn_scope`, so
                        // now we have to undo that for the comptime ones.
                        fn_scope.bindings[param.value].state = Ok(LocalState::Comptime(value));
                    }
                    LocalState::Comptime(_) => { /* already bound in `create_fn_scope` */ }
                }
            }
            if poisoned {
                return Err(Poisoned);
            }

            return match fn_scope.eval_comptime(func.body) {
                Ok(()) => unreachable!("lowerer should guarantee return in function body"),
                Err(Diverge::PoisonedNever) => return Ok(Err(Diverge::PoisonedNever)),
                Err(Diverge::PoisonedControlFlow | Diverge::BlockEnd(None)) => Err(Poisoned),
                Err(Diverge::BlockEnd(Some(ret_value))) => Ok(Ok(EvalValue::Comptime(ret_value))),
            };
        }

        // --- Runtime path ---
        // Non-comptime params are already bound as Runtime in `create_fn_scope`.

        let function = UniqueFunction {
            closure,
            comptime_params: &fn_scope.eval.values_buf[values_buf_offset..],
        };

        let call_loc = fn_scope.loc(call_span);
        let lowered = match fn_scope.eval.lowered_fns_cache.retrieve_or_create_entry(function) {
            Ok(&mut State::Done(lowered)) => lowered?,
            Ok(state @ State::InProgress) => {
                fn_scope.diag_ctx.emit_runtime_call_with_recursion(call_loc);
                *state = State::Done(Err(Poisoned));
                return if preamble.return_type == Ok(TypeId::NEVER) {
                    Ok(Err(Diverge::PoisonedNever))
                } else {
                    // If the returned type was poisoned we can't know if the user intended to
                    // have a terminating function or not, but they are rare & usually simple
                    // so we default to a poisoned value instead of control flow.
                    Err(Poisoned)
                };
            }
            Err(new_entry_id) => {
                let fn_id = (|| {
                    let (body, body_eval_res) = fn_scope.eval_block_to_mir(func.body);
                    match body_eval_res {
                        Ok(()) => unreachable!("lowerer should guarantee return in function body"),
                        Err(Diverge::PoisonedNever) => {
                            return Err(RuntimeLowerError::PoisonedNever);
                        }
                        Err(Diverge::PoisonedControlFlow) => {
                            return Err(RuntimeLowerError::PoisonResult);
                        }
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
                    Ok(fn_id1)
                })();
                let set_res = fn_scope
                    .eval
                    .lowered_fns_cache
                    .set_lowered(new_entry_id, fn_id.map_err(|_| Poisoned));
                match fn_id {
                    Err(RuntimeLowerError::PoisonResult) => return Err(Poisoned),
                    Err(RuntimeLowerError::PoisonedNever) => {
                        return Ok(Err(Diverge::PoisonedNever));
                    }
                    Ok(_) => set_res?,
                }
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
