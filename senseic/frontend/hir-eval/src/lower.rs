use sensei_core::{Idx, IndexVec, list_of_lists::ListOfLists};
use sensei_hir::{self as hir};
use sensei_mir::{self as mir};
use sensei_parser::StrId;
use sensei_values::{TypeId, ValueId};

use crate::{
    Evaluator,
    comptime::{Bindings, ComptimeInterpreter},
    value::{Value, ValueInterner},
};

#[derive(Clone, Copy)]
pub(crate) enum LocalValue {
    Comptime(ValueId),
    Runtime { mir_local: mir::LocalId, ty: TypeId },
}

#[derive(Default)]
struct PartialBindings(IndexVec<hir::LocalId, Option<LocalValue>>);

impl PartialBindings {
    fn set(&mut self, local: hir::LocalId, value: LocalValue) -> Option<LocalValue> {
        if local >= self.0.len_idx() {
            self.0.raw.resize(local.idx() + 1, None);
        }
        self.0[local].replace(value)
    }

    fn get(&self, local: hir::LocalId) -> LocalValue {
        self.0[local].expect("hir: undefined local")
    }
}

struct BodyLowerer<'a, 'hir> {
    eval: &'a mut Evaluator<'hir>,
    bindings: PartialBindings,
    local_types: IndexVec<mir::LocalId, TypeId>,
    return_type: Option<TypeId>,

    arg_buf: Vec<mir::LocalId>,
    instructions_buf: Vec<mir::Instruction>,
    values_buf: Vec<ValueId>,
    types_buf: Vec<TypeId>,
    names_buf: Vec<StrId>,
}

impl<'a, 'hir> BodyLowerer<'a, 'hir> {
    fn new(eval: &'a mut Evaluator<'hir>) -> Self {
        Self {
            eval,
            bindings: PartialBindings::default(),
            local_types: IndexVec::new(),
            return_type: None,

            arg_buf: Vec::new(),
            instructions_buf: Vec::new(),
            values_buf: Vec::new(),
            types_buf: Vec::new(),
            names_buf: Vec::new(),
        }
    }

    fn alloc_mir_local(&mut self, ty: TypeId) -> mir::LocalId {
        self.local_types.push(ty)
    }

    fn import_comptime_bindings(&mut self, bindings: &Bindings) {
        for (local, vid) in bindings.iter() {
            self.bindings.set(local, LocalValue::Comptime(vid));
        }
    }

    fn materialize(&mut self, value_id: ValueId) -> mir::LocalId {
        Self::materialize_inner(
            &self.eval.values,
            &mut self.local_types,
            &mut self.instructions_buf,
            &mut self.arg_buf,
            &mut self.eval.mir_args,
            value_id,
        )
    }

    fn materialize_inner(
        values: &ValueInterner,
        local_types: &mut IndexVec<mir::LocalId, TypeId>,
        instructions_buf: &mut Vec<mir::Instruction>,
        arg_buf: &mut Vec<mir::LocalId>,
        mir_args: &mut ListOfLists<mir::ArgsId, mir::LocalId>,
        value_id: ValueId,
    ) -> mir::LocalId {
        match values.lookup(value_id) {
            Value::Void => {
                let local = local_types.push(TypeId::VOID);
                instructions_buf.push(mir::Instruction::Set { local, expr: mir::Expr::Void });
                local
            }
            Value::Bool(b) => {
                let local = local_types.push(TypeId::BOOL);
                instructions_buf.push(mir::Instruction::Set { local, expr: mir::Expr::Bool(b) });
                local
            }
            Value::BigNum(id) => {
                let local = local_types.push(TypeId::U256);
                instructions_buf.push(mir::Instruction::Set { local, expr: mir::Expr::BigNum(id) });
                local
            }
            Value::StructVal { ty, fields } => {
                let buf_start = arg_buf.len();
                for &field in fields {
                    let mir_local = Self::materialize_inner(
                        values,
                        local_types,
                        instructions_buf,
                        arg_buf,
                        mir_args,
                        field,
                    );
                    arg_buf.push(mir_local);
                }
                let fields = mir_args.push_iter(arg_buf.drain(buf_start..));
                let local = local_types.push(ty);
                instructions_buf.push(mir::Instruction::Set {
                    local,
                    expr: mir::Expr::StructLit { ty, fields },
                });
                local
            }
            Value::Type(_) => todo!("cannot materialize type"),
            Value::Closure { .. } => todo!("cannot materialize closure"),
        }
    }

    fn ensure_runtime(&mut self, local: LocalValue) -> mir::LocalId {
        match local {
            LocalValue::Runtime { mir_local, .. } => mir_local,
            LocalValue::Comptime(value_id) => self.materialize(value_id),
        }
    }

    fn translate_expr(&mut self, expr: hir::Expr) -> LocalValue {
        match expr {
            hir::Expr::Void => LocalValue::Comptime(ValueId::VOID),
            hir::Expr::Bool(b) => {
                LocalValue::Comptime(if b { ValueId::TRUE } else { ValueId::FALSE })
            }
            hir::Expr::BigNum(id) => LocalValue::Comptime(self.eval.values.intern_num(id)),
            hir::Expr::Type(type_id) => LocalValue::Comptime(self.eval.values.intern_type(type_id)),
            hir::Expr::ConstRef(const_id) => {
                let value_id = self.eval.ensure_const_evaluated(const_id);
                LocalValue::Comptime(value_id)
            }
            hir::Expr::LocalRef(local_id) => self.bindings.get(local_id),
            hir::Expr::FnDef(fn_def) => {
                let captured_values_start = self.values_buf.len();

                let captures = &self.eval.hir.fn_captures[fn_def];
                for capture in captures {
                    match self.bindings.get(capture.outer_local) {
                        LocalValue::Comptime(vid) => self.values_buf.push(vid),
                        LocalValue::Runtime { .. } => {
                            todo!("diagnostic: runtime capture not supported")
                        }
                    }
                }
                let value_id = self.eval.values.intern(Value::Closure {
                    fn_def,
                    captures: &self.values_buf[captured_values_start..],
                });

                self.values_buf.truncate(captured_values_start);
                LocalValue::Comptime(value_id)
            }
            hir::Expr::Call { callee, args: call_args_id } => {
                let callee_local = self.bindings.get(callee);
                let LocalValue::Comptime(closure_value_id) = callee_local else {
                    todo!("diagnostic: dynamically dispatching functions not supported")
                };

                let mir_fn_id = if let Some(&cached) = self.eval.fn_cache.get(&closure_value_id) {
                    cached
                } else {
                    let fn_id = lower_fn_body(self.eval, closure_value_id);
                    self.eval.fn_cache.insert(closure_value_id, fn_id);
                    fn_id
                };

                let arg_count = self.eval.hir.call_args[call_args_id].len();
                let buf_start = self.arg_buf.len();
                for i in 0..arg_count {
                    let arg_local = self.eval.hir.call_args[call_args_id][i];
                    let arg = self.bindings.get(arg_local);
                    let mir_local = self.ensure_runtime(arg);
                    self.arg_buf.push(mir_local);
                }
                let args = self.eval.mir_args.push_iter(self.arg_buf.drain(buf_start..));

                let return_type = self.eval.mir_fns[mir_fn_id].return_type;
                let mir_local = self.alloc_mir_local(return_type);
                self.instructions_buf.push(mir::Instruction::Set {
                    local: mir_local,
                    expr: mir::Expr::Call { callee: mir_fn_id, args },
                });
                LocalValue::Runtime { mir_local, ty: return_type }
            }
            hir::Expr::StructDef(struct_def_id) => {
                let struct_def = self.eval.hir.struct_defs[struct_def_id];
                let type_index_value = match self.bindings.get(struct_def.type_index) {
                    LocalValue::Comptime(vid) => vid,
                    LocalValue::Runtime { .. } => {
                        unreachable!("hir invariant: struct type_index must be comptime")
                    }
                };

                let field_count = self.eval.hir.fields[struct_def.fields].len();
                let types_start = self.types_buf.len();
                let names_start = self.names_buf.len();

                for i in 0..field_count {
                    let field = self.eval.hir.fields[struct_def.fields][i];
                    let value = self.bindings.get(field.value);
                    match value {
                        LocalValue::Comptime(vid) => match self.eval.values.lookup(vid) {
                            Value::Type(tid) => {
                                self.types_buf.push(tid);
                                self.names_buf.push(field.name);
                            }
                            _ => todo!("diagnostic: struct field type must be Type"),
                        },
                        LocalValue::Runtime { .. } => {
                            unreachable!("hir invariant: struct field types must be comptime")
                        }
                    }
                }

                let struct_type_id = self.eval.types.intern(sensei_values::Type::Struct(
                    sensei_values::StructInfo {
                        source: struct_def.source,
                        type_index: type_index_value,
                        field_types: &self.types_buf[types_start..],
                        field_names: &self.names_buf[names_start..],
                    },
                ));
                self.types_buf.truncate(types_start);
                self.names_buf.truncate(names_start);

                LocalValue::Comptime(self.eval.values.intern_type(struct_type_id))
            }
            hir::Expr::StructLit { ty, fields: fields_id } => {
                let struct_type_id = match self.bindings.get(ty) {
                    LocalValue::Comptime(vid) => match self.eval.values.lookup(vid) {
                        Value::Type(tid) => tid,
                        _ => todo!("diagnostic: struct lit type must be Type"),
                    },
                    LocalValue::Runtime { .. } => {
                        unreachable!("hir invariant: struct lit type must be comptime")
                    }
                };

                let field_count = self.eval.hir.fields[fields_id].len();

                // Pass 1: try comptime — push ValueIds into values_buf
                let buf_start = self.values_buf.len();
                let mut all_comptime = true;
                for i in 0..field_count {
                    let field_value = self.eval.hir.fields[fields_id][i].value;
                    match self.bindings.get(field_value) {
                        LocalValue::Comptime(vid) => self.values_buf.push(vid),
                        _ => {
                            all_comptime = false;
                            break;
                        }
                    }
                }

                if all_comptime {
                    let value_id = self.eval.values.intern(Value::StructVal {
                        ty: struct_type_id,
                        fields: &self.values_buf[buf_start..],
                    });
                    self.values_buf.truncate(buf_start);
                    LocalValue::Comptime(value_id)
                } else {
                    self.values_buf.truncate(buf_start);
                    // Pass 2: runtime path — re-iterate, ensure_runtime each field
                    let buf_start = self.arg_buf.len();
                    for i in 0..field_count {
                        let field_value = self.eval.hir.fields[fields_id][i].value;
                        let local = self.bindings.get(field_value);
                        let mir_local = self.ensure_runtime(local);
                        self.arg_buf.push(mir_local);
                    }
                    let args = self.eval.mir_args.push_iter(self.arg_buf.drain(buf_start..));
                    let mir_local = self.alloc_mir_local(struct_type_id);
                    self.instructions_buf.push(mir::Instruction::Set {
                        local: mir_local,
                        expr: mir::Expr::StructLit { ty: struct_type_id, fields: args },
                    });
                    LocalValue::Runtime { mir_local, ty: struct_type_id }
                }
            }
            hir::Expr::Member { object, member } => {
                let obj_local = self.bindings.get(object);

                match obj_local {
                    LocalValue::Comptime(vid) => match self.eval.values.lookup(vid) {
                        Value::StructVal { ty, fields } => {
                            let Some(field_index) = self.eval.types.field_index_by_name(ty, member)
                            else {
                                todo!("diagnostic: unknown struct field");
                            };
                            let field_value_id = fields[field_index as usize];
                            LocalValue::Comptime(field_value_id)
                        }
                        _ => todo!("diagnostic: member access on non-struct comptime value"),
                    },
                    LocalValue::Runtime { mir_local, ty } => {
                        let Some(field_index) = self.eval.types.field_index_by_name(ty, member)
                        else {
                            todo!("diagnostic: unknown struct field");
                        };

                        let field_ty = match self.eval.types.lookup(ty) {
                            sensei_values::Type::Struct(info) => {
                                info.field_types[field_index as usize]
                            }
                            _ => unreachable!("hir invariant: member access type must be struct"),
                        };

                        let result = self.alloc_mir_local(field_ty);
                        self.instructions_buf.push(mir::Instruction::Set {
                            local: result,
                            expr: mir::Expr::FieldAccess { object: mir_local, field_index },
                        });
                        LocalValue::Runtime { mir_local: result, ty: field_ty }
                    }
                }
            }
        }
    }

    fn walk_sub_block(&mut self, block_id: hir::BlockId) -> mir::BlockId {
        let saved = std::mem::take(&mut self.instructions_buf);
        self.walk_block(block_id);
        let mir_block = self.eval.mir_blocks.push_iter(self.instructions_buf.drain(..));
        self.instructions_buf = saved;
        mir_block
    }

    fn walk_block(&mut self, block_id: hir::BlockId) {
        for &instr in &self.eval.hir.blocks[block_id] {
            self.walk_instruction(instr);
        }
    }

    fn walk_instruction(&mut self, instr: hir::Instruction) {
        match instr {
            hir::Instruction::Set { local, expr } => {
                let value = self.translate_expr(expr);
                match value {
                    LocalValue::Comptime(value) => {
                        self.bindings.set(local, LocalValue::Comptime(value));
                    }
                    LocalValue::Runtime { mir_local: src, ty } => {
                        let dst = self.alloc_mir_local(ty);
                        self.instructions_buf.push(mir::Instruction::Set {
                            local: dst,
                            expr: mir::Expr::LocalRef(src),
                        });
                        self.bindings.set(local, LocalValue::Runtime { mir_local: dst, ty });
                    }
                }
            }
            hir::Instruction::Eval(expr) => {
                self.translate_expr(expr);
            }
            hir::Instruction::AssertType { value, of_type } => {
                let type_local = self.bindings.get(of_type);
                let expected = match type_local {
                    LocalValue::Comptime(vid) => match self.eval.values.lookup(vid) {
                        Value::Type(tid) => tid,
                        _ => todo!("diagnostic: AssertType of_type must be Type"),
                    },
                    _ => unreachable!("hir: AssertType of_type must be comptime"),
                };

                let ty = match self.bindings.get(value) {
                    LocalValue::Runtime { ty, .. } => ty,
                    LocalValue::Comptime(value) => self.eval.values.type_of_value(value),
                };

                if ty != expected {
                    todo!("diagnostic: type mismatch")
                }
            }
            hir::Instruction::Return(expr) => {
                let value = self.translate_expr(expr);
                if let LocalValue::Runtime { ty, .. } = value {
                    self.return_type = Some(ty);
                }
                let mir_local = self.ensure_runtime(value);
                self.instructions_buf
                    .push(mir::Instruction::Return(mir::Expr::LocalRef(mir_local)));
            }
            hir::Instruction::If { condition, then_block, else_block } => {
                let cond_local = self.bindings.get(condition);
                let mir_condition = self.ensure_runtime(cond_local);
                let mir_then = self.walk_sub_block(then_block);
                let mir_else = self.walk_sub_block(else_block);
                self.instructions_buf.push(mir::Instruction::If {
                    condition: mir_condition,
                    then_block: mir_then,
                    else_block: mir_else,
                });
            }
            hir::Instruction::While { condition_block, condition, body } => {
                let mir_condition_block = self.walk_sub_block(condition_block);
                let mir_condition = match self.bindings.get(condition) {
                    LocalValue::Runtime { mir_local, .. } => mir_local,
                    LocalValue::Comptime(_) => todo!("comptime while condition"),
                };
                let mir_body = self.walk_sub_block(body);
                self.instructions_buf.push(mir::Instruction::While {
                    condition_block: mir_condition_block,
                    condition: mir_condition,
                    body: mir_body,
                });
            }
            hir::Instruction::Assign { target, value } => {
                let target_local = match self.bindings.get(target) {
                    LocalValue::Runtime { mir_local, .. } => mir_local,
                    LocalValue::Comptime(vid) => {
                        let ty = self.eval.values.type_of_value(vid);
                        let mir_local = self.materialize(vid);
                        self.bindings.set(target, LocalValue::Runtime { mir_local, ty });
                        mir_local
                    }
                };
                let rhs = self.translate_expr(value);
                let rhs_mir = self.ensure_runtime(rhs);
                self.instructions_buf.push(mir::Instruction::Assign {
                    target: target_local,
                    value: mir::Expr::LocalRef(rhs_mir),
                });
            }
        }
    }

    fn flush_as_fn(self, param_count: u32, return_type: TypeId) -> mir::FnId {
        let body = self.eval.mir_blocks.push_iter(self.instructions_buf.into_iter());
        let fn_id = self.eval.mir_fns.push(mir::FnDef { body, param_count, return_type });
        let locals_id = self.eval.mir_fn_locals.push_iter(self.local_types.raw.into_iter());
        assert_eq!(fn_id, locals_id);
        fn_id
    }
}

fn lower_fn_body(eval: &mut Evaluator<'_>, closure_value_id: ValueId) -> mir::FnId {
    let Value::Closure { fn_def: fn_def_id, captures } = eval.values.lookup(closure_value_id)
    else {
        todo!("diagnostic: callee is not a function")
    };
    let fn_def = eval.hir.fns[fn_def_id];
    let params = &eval.hir.fn_params[fn_def_id];
    let hir_captures = &eval.hir.fn_captures[fn_def_id];

    // Phase 1: Bind captures into preamble bindings, evaluate type preamble.
    let mut preamble_bindings = Bindings::default();
    for (capture_info, &value_id) in hir_captures.iter().zip(captures) {
        preamble_bindings.set(capture_info.inner_local, value_id);
    }
    let preamble_bindings =
        ComptimeInterpreter::eval_preamble_block(eval, preamble_bindings, fn_def.type_preamble);

    // Phase 2: Extract param types and return type from evaluated preamble.
    let param_types: Vec<TypeId> = params
        .iter()
        .map(|param| {
            let type_vid = preamble_bindings.get(param.r#type);
            let Value::Type(tid) = eval.values.lookup(type_vid) else {
                todo!("diagnostic: param type must be Type")
            };
            tid
        })
        .collect();

    let return_type_vid = preamble_bindings.get(fn_def.return_type);
    let Value::Type(return_type) = eval.values.lookup(return_type_vid) else {
        todo!("diagnostic: return type must be Type")
    };

    // Phase 3: Create lowerer, import preamble bindings, allocate typed params, walk body.
    let mut lowerer = BodyLowerer::new(eval);
    lowerer.import_comptime_bindings(&preamble_bindings);

    let param_count = params.len() as u32;
    for (param, &ty) in params.iter().zip(&param_types) {
        let mir_local = lowerer.alloc_mir_local(ty);
        lowerer.bindings.set(param.value, LocalValue::Runtime { mir_local, ty });
    }

    lowerer.walk_block(fn_def.body);
    lowerer.flush_as_fn(param_count, return_type)
}

pub(crate) fn lower_block_as_fn(eval: &mut Evaluator<'_>, hir_block: hir::BlockId) -> mir::FnId {
    let mut lowerer = BodyLowerer::new(eval);
    lowerer.walk_block(hir_block);
    lowerer.flush_as_fn(0, TypeId::VOID)
}
