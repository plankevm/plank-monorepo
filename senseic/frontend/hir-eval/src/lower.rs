use hashbrown::HashMap;
use sensei_core::{DenseIndexMap, Idx, IndexVec};
use sensei_hir::{self as hir};
use sensei_mir::{self as mir};
use sensei_parser::StrId;
use sensei_values::{TypeId, ValueId};

use crate::{
    Evaluator,
    comptime::{self, ComptimeInterpreter},
    value::{Value, ValueInterner},
};

const INSTRUCTION_BUF_CAPACITY: usize = 1024;
const VALUES_BUF_CAPACITY: usize = 32;

#[derive(Default)]
struct LocalState {
    /// Every HIR local optionally maps to a MIR local.
    /// Once set, never changes (the MIR local ID is stable).
    hir_to_mir: DenseIndexMap<hir::LocalId, mir::LocalId>,

    /// Every HIR local optionally has a comptime-known value.
    /// Can be set and later cleared (e.g., after runtime if/else).
    comptime: DenseIndexMap<hir::LocalId, ValueId>,

    /// The concrete type of each MIR local. Stored separately so it can
    /// pre-allocated for if/else results, filled in on first Set.
    mir_type: IndexVec<mir::LocalId, Option<TypeId>>,
}

impl LocalState {
    fn mir_type(&self, mir: mir::LocalId) -> TypeId {
        self.mir_type[mir].expect("mapped mir local without type")
    }

    fn set_hir_to_mir(
        &mut self,
        hir: hir::LocalId,
        ty: TypeId,
        comptime: Option<ValueId>,
    ) -> mir::LocalId {
        if let Some(&mir_local) = self.hir_to_mir.get(hir) {
            // Not first set, value not guaranteed to be comptime known.
            self.comptime.remove(hir);
            let existing_ty = self.mir_type(mir_local);
            if existing_ty == TypeId::NEVER {
                // Value was set to `never` in another branch, save more concrete type.
                self.mir_type[mir_local] = Some(ty);
            } else if !ty.is_assignable_to(existing_ty) {
                todo!("diagnostic: set type mismatch");
            } else {
                // `existing_ty` is not `never` and is compatible with `ty`, do nothing.
            }
            mir_local
        } else {
            // We only save the comptime value if this is the first decl (certain comptime).
            if let Some(value) = comptime {
                self.comptime.insert(hir, value);
            }
            let mir = self.mir_type.push(Some(ty));
            self.hir_to_mir.insert(hir, mir);
            mir
        }
    }

    fn get_type(&self, hir_local: hir::LocalId, values: &ValueInterner) -> TypeId {
        if let Some(&vid) = self.comptime.get(hir_local) {
            values.type_of_value(vid)
        } else {
            let mir = self.hir_to_mir[hir_local];
            self.mir_type(mir)
        }
    }
}

struct FunctionLowerScope {
    expected_return_type: TypeId,
    locals: LocalState,
    interpreter: ComptimeInterpreter,

    instructions_buf: Vec<mir::Instruction>,
    values_buf: Vec<ValueId>,
}

#[derive(Debug, Clone, Copy)]
enum ExprResult {
    Runtime { expr: mir::Expr, ty: TypeId, comptime: Option<ValueId> },
    ComptimeOnly(ValueId),
}

enum BlockControlFlow {
    /// Signals that control flow diverges, either via a `return` or a halting builtin or
    /// function.
    Diverges,
    /// Normal control flow that continues through.
    Continues,
}

impl FunctionLowerScope {
    fn translate_expr(&mut self, eval: &mut Evaluator<'_>, expr: hir::Expr) -> ExprResult {
        match expr {
            hir::Expr::Bool(b) => {
                let value = if b { ValueId::TRUE } else { ValueId::FALSE };
                ExprResult::Runtime {
                    expr: mir::Expr::Bool(b),
                    ty: TypeId::BOOL,
                    comptime: Some(value),
                }
            }
            hir::Expr::BigNum(big_num_id) => ExprResult::Runtime {
                expr: mir::Expr::BigNum(big_num_id),
                ty: TypeId::U256,
                comptime: Some(eval.values.intern_num(big_num_id)),
            },
            hir::Expr::BuiltinCall { builtin, args } => {
                let args = &eval.hir.call_args[args];
                'sig: for &(input_types, result_type) in builtin.signatures() {
                    if input_types.len() != args.len() {
                        todo!("diagnostic: builtin argument count mismatch");
                    }

                    for (&input, &arg) in input_types.iter().zip(args) {
                        if input != self.locals.get_type(arg, &eval.values) {
                            continue 'sig;
                        }
                    }

                    let args = eval
                        .mir_args
                        .push_iter(args.iter().map(|&arg| self.locals.hir_to_mir[arg]));
                    return ExprResult::Runtime {
                        expr: mir::Expr::BuiltinCall { builtin, args },
                        ty: result_type,
                        comptime: None,
                    };
                }
                todo!("diagnostic: no matching builtin type signature")
            }
            hir::Expr::LocalRef(hir) => {
                let value = self.locals.comptime.get(hir).copied();
                let mir = self.locals.hir_to_mir.get(hir).copied();
                match (mir, value) {
                    (Some(mir), comptime) => ExprResult::Runtime {
                        expr: mir::Expr::LocalRef(mir),
                        ty: self.locals.mir_type(mir),
                        comptime,
                    },
                    (None, Some(value)) => ExprResult::ComptimeOnly(value),
                    (None, None) => unreachable!("invalid hir"),
                }
            }
            hir::Expr::Type(ty) => ExprResult::ComptimeOnly(eval.values.intern_type(ty)),
            hir::Expr::FnDef(fn_def) => {
                let captures = &eval.hir.fn_captures[fn_def];
                for capture in captures {
                    let vid = self
                        .locals
                        .comptime
                        .get(capture.outer_local)
                        .expect("closure capture must be comptime");
                    self.values_buf.push(*vid);
                }
                let value_id =
                    eval.values.intern(Value::Closure { fn_def, captures: &self.values_buf });
                ExprResult::ComptimeOnly(value_id)
            }
            hir::Expr::Call { callee, args } => {
                let &closure = self
                    .locals
                    .comptime
                    .get(callee)
                    .expect("todo-diagnostic: call target must be comptime-known");
                let callee = eval.fn_cache.get(&closure).copied().unwrap_or_else(|| {
                    let id = self.lower_closure(eval, closure);
                    eval.fn_cache.insert(closure, id);
                    id
                });

                let fn_def = eval.mir_fns[callee];
                let arg_locals = &eval.hir.call_args[args];
                if arg_locals.len() != fn_def.param_count as usize {
                    todo!("diagnostic: function call argument count mismatch");
                }

                let param_types = &eval.mir_fn_locals[callee][..fn_def.param_count as usize];
                for (&arg_local, &expected_ty) in arg_locals.iter().zip(param_types) {
                    let actual_ty = self.locals.get_type(arg_local, &eval.values);
                    if !actual_ty.is_assignable_to(expected_ty) {
                        todo!("diagnostic: function call argument type mismatch");
                    }
                }

                let args = eval.mir_args.push_iter(arg_locals.iter().map(|&hir| {
                    *self.locals.hir_to_mir.get(hir).expect("todo: non-runtime arg handling")
                }));

                ExprResult::Runtime {
                    expr: mir::Expr::Call { callee, args },
                    ty: fn_def.return_type,
                    comptime: None,
                }
            }
            other => todo!("expr: {other:?}"),
        }
    }

    fn lower_closure(&mut self, eval: &mut Evaluator<'_>, closure: ValueId) -> mir::FnId {
        let Value::Closure { fn_def, captures } = eval.values.lookup(closure) else {
            todo!("diagnostic: callee is not a function")
        };
        let func = eval.hir.fns[fn_def];
        let params = &eval.hir.fn_params[fn_def];
        let hir_captures = &eval.hir.fn_captures[fn_def];

        // TODO: Optimize to use same allocation across scopes.
        let saved_locals = std::mem::take(&mut self.locals);

        self.interpreter.reset();
        // Insert captures.
        for (capture_info, &value) in hir_captures.iter().zip(captures) {
            let prev = self.interpreter.bindings.insert(capture_info.inner_local, value);
            assert!(prev.is_none(), "invalid hir");
        }
        // Interpret type premable to determine types.
        self.interpreter
            .interpret_block(eval, func.type_preamble)
            .expect("invalid hir: premable with `return`");
        let return_type = self.interpreter.bindings[func.return_type];
        let Value::Type(return_type) = eval.values.lookup(return_type) else {
            todo!("diagnostic: return type not type")
        };
        let saved_return_type = std::mem::replace(&mut self.expected_return_type, return_type);

        for param in params {
            let ty = self.interpreter.bindings[param.r#type];
            let Value::Type(ty) = eval.values.lookup(ty) else {
                todo!("diagnostic: param type must be Type")
            };
            self.locals.set_hir_to_mir(param.value, ty, None);
        }

        todo!()
    }

    fn translate_block_inner(
        &mut self,
        eval: &mut Evaluator<'_>,
        block: hir::BlockId,
    ) -> BlockControlFlow {
        for &instr in &eval.hir.blocks[block] {
            match instr {
                hir::Instruction::Set { local, expr } => match self.translate_expr(eval, expr) {
                    ExprResult::Runtime { expr, ty, comptime } => {
                        let target = self.locals.set_hir_to_mir(local, ty, comptime);
                        self.instructions_buf.push(mir::Instruction::Set { target, expr });
                        if ty == TypeId::NEVER {
                            return BlockControlFlow::Diverges;
                        }
                    }
                    ExprResult::ComptimeOnly(value) => {
                        self.locals.comptime.insert(local, value);
                    }
                },
                hir::Instruction::AssertType { value, of_type } => {
                    let Some(&type_value) = self.locals.comptime.get(of_type) else {
                        todo!("diagnostic: AssertType of_type must be comptime")
                    };
                    let Value::Type(expected) = eval.values.lookup(type_value) else {
                        todo!("diagnostic: AssertType of_type must be Type");
                    };
                    let actual = self.locals.get_type(value, &eval.values);
                    if !actual.is_assignable_to(expected) {
                        todo!("diagnostic: type mismatch in AssertType")
                    }
                }
                hir::Instruction::Eval(expr) => match self.translate_expr(eval, expr) {
                    ExprResult::ComptimeOnly(_) => { /* No MIR equivalent, do nothing */ }
                    ExprResult::Runtime { expr, ty, comptime: _ } => {
                        // MIR doesn't have `Eval` so we use `Set`.
                        let target = self.locals.mir_type.push(Some(ty));
                        self.instructions_buf.push(mir::Instruction::Set { target, expr });
                        if ty == TypeId::NEVER {
                            return BlockControlFlow::Diverges;
                        }
                    }
                },
                other => todo!("{other:?}"),
            }
        }
        BlockControlFlow::Continues
    }

    fn translate_block(
        &mut self,
        eval: &mut Evaluator<'_>,
        block: hir::BlockId,
    ) -> (mir::BlockId, BlockControlFlow) {
        let instr_start = self.instructions_buf.len();
        let control_flow = self.translate_block_inner(eval, block);
        let id = eval.mir_blocks.push_iter(self.instructions_buf.drain(instr_start..));
        (id, control_flow)
    }
}

pub(crate) fn lower_entry_point_as_fn(
    eval: &mut Evaluator<'_>,
    hir_block: hir::BlockId,
) -> mir::FnId {
    let mut scope = FunctionLowerScope {
        expected_return_type: TypeId::NEVER,
        locals: LocalState::default(),
        interpreter: ComptimeInterpreter::new(),

        instructions_buf: Vec::with_capacity(INSTRUCTION_BUF_CAPACITY),
        values_buf: Vec::with_capacity(VALUES_BUF_CAPACITY),
    };

    let (body, _) = scope.translate_block(eval, hir_block);

    let fn_id1 = eval
        .mir_fn_locals
        .push_iter(scope.locals.mir_type.iter().map(|&ty| ty.expect("local left unset")));
    let fn_id2 = eval.mir_fns.push(mir::FnDef { body, param_count: 0, return_type: TypeId::NEVER });
    assert_eq!(fn_id1, fn_id2);
    fn_id1
}
