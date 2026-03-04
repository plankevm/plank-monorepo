use sensei_core::{Idx, IndexVec};
use sensei_hir::{self as hir};
use sensei_mir::{self as mir};
use sensei_parser::StrId;
use sensei_values::{TypeId, ValueId};

use crate::{
    Evaluator,
    comptime::{self, ComptimeInterpreter},
    value::{Value, ValueInterner},
};

/// Result of evaluating an HIR expression.
enum ExprResult {
    /// No MIR representation (types, closures).
    ComptimeOnly(ValueId),
    /// Eagerly materialized into MIR.
    Runtime { mir_local: mir::LocalId, ty: TypeId, comptime_value: Option<ValueId> },
}

/// Unified tracking of HIR local -> MIR local mapping and comptime values.
#[derive(Default)]
struct LocalState {
    /// Every HIR local optionally maps to a MIR local.
    /// Once set, never changes (the MIR local ID is stable).
    hir_to_mir: IndexVec<hir::LocalId, Option<mir::LocalId>>,

    /// Every HIR local optionally has a comptime-known value.
    /// Can be set and later cleared (e.g., after runtime if/else).
    comptime_value: IndexVec<hir::LocalId, Option<ValueId>>,

    /// The concrete type of each MIR local. Starts as None for
    /// pre-allocated locals (if/else results), filled in on first Set.
    mir_type: IndexVec<mir::LocalId, Option<TypeId>>,
}

impl LocalState {
    fn alloc_untyped_if_result(&mut self, local: hir::LocalId) {
        if self.get_mir(local).is_none() {
            self.ensure_hir_entries(local);
            let mir = self.mir_type.push(None);
            self.hir_to_mir[local] = Some(mir);
        }
    }

    /// Allocates a fresh MIR local with a known type.
    fn alloc_mir_typed(&mut self, ty: TypeId) -> mir::LocalId {
        self.mir_type.push(Some(ty))
    }

    fn ensure_hir_entries(&mut self, local: hir::LocalId) {
        let needed = local.idx() + 1;
        if self.hir_to_mir.len() < needed {
            self.hir_to_mir.raw.resize(needed, None);
        }
        if self.comptime_value.len() < needed {
            self.comptime_value.raw.resize(needed, None);
        }
    }

    /// Returns `Err(TypeId)` if reassigning with different type.
    fn assign_or_define(&mut self, hir: hir::LocalId, ty: TypeId) -> Result<mir::LocalId, TypeId> {
        self.ensure_hir_entries(hir);
        if let Some(mir) = self.get_mir(hir) {
            self.set_mir_type(mir, ty)?;
            return Ok(mir);
        }
        let mir = self.alloc_mir_typed(ty);
        self.hir_to_mir[hir] = Some(mir);
        Ok(mir)
    }

    /// Records a comptime-known value for an HIR local.
    fn set_comptime(&mut self, hir_local: hir::LocalId, value: ValueId) {
        self.ensure_hir_entries(hir_local);
        self.comptime_value[hir_local].replace(value);
    }

    /// Removes comptime knowledge for an HIR local.
    fn clear_comptime(&mut self, hir_local: hir::LocalId) {
        if let Some(value) = self.comptime_value.get_mut(hir_local) {
            *value = None;
        }
    }

    fn set_mir_type(&mut self, mir_local: mir::LocalId, ty: TypeId) -> Result<(), TypeId> {
        let prev = self.mir_type[mir_local].replace(ty);
        match prev {
            None => Ok(()),
            Some(prev_ty) if prev_ty == ty => Ok(()),
            Some(prev_ty) => Err(prev_ty),
        }
    }

    fn get_mir(&self, hir_local: hir::LocalId) -> Option<mir::LocalId> {
        self.hir_to_mir.get(hir_local).copied().flatten()
    }

    fn get_comptime(&self, hir_local: hir::LocalId) -> Option<ValueId> {
        self.comptime_value.get(hir_local).copied().flatten()
    }

    fn get_mir_type(&self, mir_local: mir::LocalId) -> Option<TypeId> {
        self.mir_type[mir_local]
    }

    /// Gets the type of an HIR local, either from its comptime value or its MIR local.
    fn get_type(&self, hir_local: hir::LocalId, values: &ValueInterner) -> TypeId {
        if let Some(vid) = self.get_comptime(hir_local) {
            values.type_of_value(vid)
        } else {
            self.get_mir(hir_local).and_then(|mir| self.get_mir_type(mir)).expect("missing type")
        }
    }
}

struct BodyLowerer<'a, 'hir> {
    eval: &'a mut Evaluator<'hir>,
    locals: LocalState,
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
            locals: LocalState::default(),
            return_type: None,

            arg_buf: Vec::new(),
            instructions_buf: Vec::new(),
            values_buf: Vec::new(),
            types_buf: Vec::new(),
            names_buf: Vec::new(),
        }
    }

    fn import_comptime_bindings(&mut self, bindings: &comptime::Bindings) {
        for (local, vid) in bindings.iter() {
            self.locals.set_comptime(local, vid);
        }
    }

    /// Given an HIR local, get its mir::LocalId, panicking if it's comptime-only.
    fn ensure_runtime(&self, hir_local: hir::LocalId) -> mir::LocalId {
        self.locals.get_mir(hir_local).expect("comptime-only value used in runtime context")
    }

    /// Emit a mir::Set instruction and return the allocated local.
    fn emit_set(&mut self, ty: TypeId, value: mir::Expr) -> mir::LocalId {
        let target = self.locals.alloc_mir_typed(ty);
        self.instructions_buf.push(mir::Instruction::Set { target, value });
        target
    }

    fn translate_call_args(&mut self, call_args_id: hir::CallArgsId) -> mir::ArgsId {
        let arg_count = self.eval.hir.call_args[call_args_id].len();
        let buf_start = self.arg_buf.len();
        for i in 0..arg_count {
            let arg_local = self.eval.hir.call_args[call_args_id][i];
            let mir_local = self.ensure_runtime(arg_local);
            self.arg_buf.push(mir_local);
        }
        self.eval.mir_args.push_iter(self.arg_buf.drain(buf_start..))
    }

    fn translate_expr(&mut self, expr: hir::Expr) -> ExprResult {
        match expr {
            hir::Expr::Void => {
                let mir_local = self.emit_set(TypeId::VOID, mir::Expr::Void);
                ExprResult::Runtime {
                    mir_local,
                    ty: TypeId::VOID,
                    comptime_value: Some(ValueId::VOID),
                }
            }
            hir::Expr::Bool(b) => {
                let mir_local = self.emit_set(TypeId::BOOL, mir::Expr::Bool(b));
                let value_id = if b { ValueId::TRUE } else { ValueId::FALSE };
                ExprResult::Runtime { mir_local, ty: TypeId::BOOL, comptime_value: Some(value_id) }
            }
            hir::Expr::BigNum(id) => {
                let mir_local = self.emit_set(TypeId::U256, mir::Expr::BigNum(id));
                let value_id = self.eval.values.intern_num(id);
                ExprResult::Runtime { mir_local, ty: TypeId::U256, comptime_value: Some(value_id) }
            }
            hir::Expr::Type(type_id) => {
                let value_id = self.eval.values.intern_type(type_id);
                ExprResult::ComptimeOnly(value_id)
            }
            hir::Expr::ConstRef(const_id) => {
                let value_id = self.eval.ensure_const_evaluated(const_id);
                self.value_to_expr_result(value_id)
            }
            hir::Expr::LocalRef(local_id) => {
                let comptime_value = self.locals.get_comptime(local_id);
                if let Some(mir_local) = self.locals.get_mir(local_id) {
                    let ty = self.locals.get_mir_type(mir_local).expect("MIR local has no type");
                    ExprResult::Runtime { mir_local, ty, comptime_value }
                } else {
                    ExprResult::ComptimeOnly(
                        comptime_value.expect("local has neither MIR nor comptime value"),
                    )
                }
            }
            hir::Expr::FnDef(fn_def) => {
                let captured_values_start = self.values_buf.len();

                let captures = &self.eval.hir.fn_captures[fn_def];
                for capture in captures {
                    let vid = self
                        .locals
                        .get_comptime(capture.outer_local)
                        .expect("closure capture must be comptime");
                    self.values_buf.push(vid);
                }
                let value_id = self.eval.values.intern(Value::Closure {
                    fn_def,
                    captures: &self.values_buf[captured_values_start..],
                });

                self.values_buf.truncate(captured_values_start);
                ExprResult::ComptimeOnly(value_id)
            }
            hir::Expr::Call { callee, args: call_args_id } => {
                let closure_value_id = self
                    .locals
                    .get_comptime(callee)
                    .expect("dynamically dispatching functions not supported");

                let mir_fn_id = if let Some(&cached) = self.eval.fn_cache.get(&closure_value_id) {
                    cached
                } else {
                    let fn_id = lower_fn_body(self.eval, closure_value_id);
                    self.eval.fn_cache.insert(closure_value_id, fn_id);
                    fn_id
                };

                let fn_def = self.eval.mir_fns[mir_fn_id];
                let arg_locals = &self.eval.hir.call_args[call_args_id];

                if arg_locals.len() != fn_def.param_count as usize {
                    todo!("diagnostic: function call argument count mismatch");
                }

                let param_types =
                    &self.eval.mir_fn_locals[mir_fn_id][..fn_def.param_count as usize];
                for (&arg_local, &expected_ty) in arg_locals.iter().zip(param_types) {
                    let actual_ty = self.locals.get_type(arg_local, &self.eval.values);
                    if actual_ty != expected_ty {
                        todo!("diagnostic: function call argument type mismatch");
                    }
                }

                let args = self.translate_call_args(call_args_id);
                let mir_local =
                    self.emit_set(fn_def.return_type, mir::Expr::Call { callee: mir_fn_id, args });
                ExprResult::Runtime { mir_local, ty: fn_def.return_type, comptime_value: None }
            }
            hir::Expr::BuiltinCall { builtin, args: call_args_id } => {
                let arg_locals = &self.eval.hir.call_args[call_args_id];
                let signatures = builtin.signatures();

                let types_start = self.types_buf.len();
                for &arg_local in arg_locals {
                    let arg_ty = self.locals.get_type(arg_local, &self.eval.values);
                    self.types_buf.push(arg_ty);
                }
                let actual_types = &self.types_buf[types_start..];

                let return_type = signatures
                    .iter()
                    .find_map(|(inputs, output)| (*inputs == actual_types).then_some(*output))
                    .unwrap_or_else(|| {
                        todo!("diagnostic: no matching builtin signature for {builtin} (sigs: {signatures:?}, actual: {actual_types:?})")
                    });

                self.types_buf.truncate(types_start);

                let args = self.translate_call_args(call_args_id);
                let mir_local =
                    self.emit_set(return_type, mir::Expr::BuiltinCall { builtin, args });
                ExprResult::Runtime { mir_local, ty: return_type, comptime_value: None }
            }
            hir::Expr::StructDef(struct_def_id) => {
                let struct_def = self.eval.hir.struct_defs[struct_def_id];
                let type_index_value = self
                    .locals
                    .get_comptime(struct_def.type_index)
                    .expect("struct type_index must be comptime");

                let field_count = self.eval.hir.fields[struct_def.fields].len();
                let types_start = self.types_buf.len();
                let names_start = self.names_buf.len();

                for i in 0..field_count {
                    let field = self.eval.hir.fields[struct_def.fields][i];
                    let value = self
                        .locals
                        .get_comptime(field.value)
                        .expect("struct field must be comptime");
                    match self.eval.values.lookup(value) {
                        Value::Type(tid) => {
                            self.types_buf.push(tid);
                            self.names_buf.push(field.name);
                        }
                        _ => todo!("diagnostic: struct field type must be Type"),
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

                ExprResult::ComptimeOnly(self.eval.values.intern_type(struct_type_id))
            }
            hir::Expr::StructLit { ty, fields: fields_id } => {
                let type_vid =
                    self.locals.get_comptime(ty).expect("struct lit type must be comptime");
                let Value::Type(struct_type_id) = self.eval.values.lookup(type_vid) else {
                    todo!("diagnostic: struct lit type must be Type");
                };

                let field_count = self.eval.hir.fields[fields_id].len();

                let buf_start = self.values_buf.len();
                let mut all_comptime = true;
                for i in 0..field_count {
                    let field_value = self.eval.hir.fields[fields_id][i].value;
                    if let Some(vid) = self.locals.get_comptime(field_value) {
                        self.values_buf.push(vid);
                    } else {
                        all_comptime = false;
                        break;
                    }
                }

                if all_comptime {
                    let value_id = self.eval.values.intern(Value::StructVal {
                        ty: struct_type_id,
                        fields: &self.values_buf[buf_start..],
                    });
                    self.values_buf.truncate(buf_start);

                    // Still emit MIR for the struct lit
                    let arg_buf_start = self.arg_buf.len();
                    for i in 0..field_count {
                        let field_value = self.eval.hir.fields[fields_id][i].value;
                        let mir_local = self.ensure_runtime(field_value);
                        self.arg_buf.push(mir_local);
                    }
                    let args = self.eval.mir_args.push_iter(self.arg_buf.drain(arg_buf_start..));
                    let mir_local = self.emit_set(
                        struct_type_id,
                        mir::Expr::StructLit { ty: struct_type_id, fields: args },
                    );

                    ExprResult::Runtime {
                        mir_local,
                        ty: struct_type_id,
                        comptime_value: Some(value_id),
                    }
                } else {
                    self.values_buf.truncate(buf_start);
                    let arg_buf_start = self.arg_buf.len();
                    for i in 0..field_count {
                        let field_value = self.eval.hir.fields[fields_id][i].value;
                        let mir_local = self.ensure_runtime(field_value);
                        self.arg_buf.push(mir_local);
                    }
                    let args = self.eval.mir_args.push_iter(self.arg_buf.drain(arg_buf_start..));
                    let mir_local = self.emit_set(
                        struct_type_id,
                        mir::Expr::StructLit { ty: struct_type_id, fields: args },
                    );

                    ExprResult::Runtime { mir_local, ty: struct_type_id, comptime_value: None }
                }
            }
            hir::Expr::Member { object, member } => {
                let obj_comptime = self.locals.get_comptime(object);
                let obj_mir = self.locals.get_mir(object);

                match (obj_comptime, obj_mir) {
                    (Some(vid), Some(mir_local)) => {
                        // Runtime with known value - do field access at runtime
                        let obj_ty = self.eval.values.type_of_value(vid);
                        let Some(field_index) = self.eval.types.field_index_by_name(obj_ty, member)
                        else {
                            todo!("diagnostic: unknown struct field");
                        };
                        let field_ty = match self.eval.types.lookup(obj_ty) {
                            sensei_values::Type::Struct(info) => {
                                info.field_types[field_index as usize]
                            }
                            _ => unreachable!("member access type must be struct"),
                        };

                        // Get the comptime field value
                        let field_value = match self.eval.values.lookup(vid) {
                            Value::StructVal { fields, .. } => Some(fields[field_index as usize]),
                            _ => None,
                        };

                        let result = self.emit_set(
                            field_ty,
                            mir::Expr::FieldAccess { object: mir_local, field_index },
                        );
                        ExprResult::Runtime {
                            mir_local: result,
                            ty: field_ty,
                            comptime_value: field_value,
                        }
                    }
                    (Some(vid), None) => {
                        // Comptime-only struct
                        match self.eval.values.lookup(vid) {
                            Value::StructVal { ty, fields } => {
                                let Some(field_index) =
                                    self.eval.types.field_index_by_name(ty, member)
                                else {
                                    todo!("diagnostic: unknown struct field");
                                };
                                let field_value_id = fields[field_index as usize];
                                self.value_to_expr_result(field_value_id)
                            }
                            _ => todo!("diagnostic: member access on non-struct comptime value"),
                        }
                    }
                    (None, Some(mir_local)) => {
                        // Runtime unknown
                        let ty =
                            self.locals.get_mir_type(mir_local).expect("MIR local has no type");
                        let Some(field_index) = self.eval.types.field_index_by_name(ty, member)
                        else {
                            todo!("diagnostic: unknown struct field");
                        };
                        let field_ty = match self.eval.types.lookup(ty) {
                            sensei_values::Type::Struct(info) => {
                                info.field_types[field_index as usize]
                            }
                            _ => unreachable!("member access type must be struct"),
                        };

                        let result = self.emit_set(
                            field_ty,
                            mir::Expr::FieldAccess { object: mir_local, field_index },
                        );
                        ExprResult::Runtime {
                            mir_local: result,
                            ty: field_ty,
                            comptime_value: None,
                        }
                    }
                    (None, None) => {
                        panic!("local has neither MIR nor comptime value")
                    }
                }
            }
        }
    }

    /// Convert a ValueId to an ExprResult, emitting MIR if the value is materializable.
    fn value_to_expr_result(&mut self, value_id: ValueId) -> ExprResult {
        match self.eval.values.lookup(value_id) {
            Value::Void => {
                let mir_local = self.emit_set(TypeId::VOID, mir::Expr::Void);
                ExprResult::Runtime { mir_local, ty: TypeId::VOID, comptime_value: Some(value_id) }
            }
            Value::Bool(b) => {
                let mir_local = self.emit_set(TypeId::BOOL, mir::Expr::Bool(b));
                ExprResult::Runtime { mir_local, ty: TypeId::BOOL, comptime_value: Some(value_id) }
            }
            Value::BigNum(id) => {
                let mir_local = self.emit_set(TypeId::U256, mir::Expr::BigNum(id));
                ExprResult::Runtime { mir_local, ty: TypeId::U256, comptime_value: Some(value_id) }
            }
            Value::StructVal { ty, fields } => {
                let values_buf_start = self.values_buf.len();
                for &field_vid in fields {
                    self.values_buf.push(field_vid);
                }

                let arg_buf_start = self.arg_buf.len();
                for i in values_buf_start..self.values_buf.len() {
                    let field_vid = self.values_buf[i];
                    match self.value_to_expr_result(field_vid) {
                        ExprResult::Runtime { mir_local, .. } => self.arg_buf.push(mir_local),
                        ExprResult::ComptimeOnly(_) => {
                            panic!("struct field should be materializable")
                        }
                    }
                }
                self.values_buf.truncate(values_buf_start);

                let args = self.eval.mir_args.push_iter(self.arg_buf.drain(arg_buf_start..));
                let mir_local = self.emit_set(ty, mir::Expr::StructLit { ty, fields: args });
                ExprResult::Runtime { mir_local, ty, comptime_value: Some(value_id) }
            }
            Value::Type(_) => ExprResult::ComptimeOnly(value_id),
            Value::Closure { .. } => ExprResult::ComptimeOnly(value_id),
        }
    }

    fn walk_sub_block(&mut self, block_id: hir::BlockId) -> mir::BlockId {
        let instructions_start = self.instructions_buf.len();
        self.walk_block(block_id);
        let instructions = self.instructions_buf.drain(instructions_start..);
        self.eval.mir_blocks.push_iter(instructions)
    }

    fn walk_block(&mut self, block_id: hir::BlockId) {
        for &instr in &self.eval.hir.blocks[block_id] {
            self.walk_instruction(instr);
        }
    }

    fn walk_instruction(&mut self, instr: hir::Instruction) {
        match instr {
            hir::Instruction::Set { local, expr } => {
                match self.translate_expr(expr) {
                    ExprResult::ComptimeOnly(value_id) => self.locals.set_comptime(local, value_id),
                    ExprResult::Runtime { mir_local: src_mir, ty, comptime_value } => {
                        let Ok(dst_mir) = self.locals.assign_or_define(local, ty) else {
                            todo!("diagnostic: type mismatch on set")
                        };

                        self.instructions_buf.push(mir::Instruction::Set {
                            target: dst_mir,
                            value: mir::Expr::LocalRef(src_mir),
                        });

                        // Record comptime value if known
                        if let Some(vid) = comptime_value {
                            self.locals.set_comptime(local, vid);
                        }
                    }
                }
            }
            hir::Instruction::Eval(expr) => {
                self.translate_expr(expr);
            }
            hir::Instruction::AssertType { value, of_type } => {
                let Some(type_value) = self.locals.get_comptime(of_type) else {
                    todo!("diagnostic: AssertType of_type must be comptime")
                };
                let Value::Type(expected) = self.eval.values.lookup(type_value) else {
                    todo!("diagnostic: AssertType of_type must be Type");
                };
                let actual = self.locals.get_type(value, &self.eval.values);
                if actual != expected {
                    todo!("diagnostic: type mismatch in AssertType")
                }
            }
            hir::Instruction::Return(expr) => {
                let result = self.translate_expr(expr);
                let mir_local = match result {
                    ExprResult::Runtime { mir_local, ty, .. } => {
                        self.return_type = Some(ty);
                        mir_local
                    }
                    ExprResult::ComptimeOnly(vid) => {
                        todo!("diagnostic: cannot return comptime-only value {vid:?}")
                    }
                };
                self.instructions_buf.push(mir::Instruction::Return(mir_local));
            }
            hir::Instruction::If { condition, then_block, else_block, result } => {
                let cond_comptime = self.locals.get_comptime(condition);

                match cond_comptime {
                    Some(ValueId::TRUE) => self.walk_block(then_block),
                    Some(ValueId::FALSE) => self.walk_block(else_block),
                    Some(_) => todo!("diagnostic: comptime condition type mismatch"),
                    None => {
                        // Runtime condition - lower both branches
                        let condition = self
                            .locals
                            .get_mir(condition)
                            .expect("runtime condition must have MIR local");

                        // Verify condition is bool
                        let cond_ty = self.locals.get_mir_type(condition);
                        if cond_ty != Some(TypeId::BOOL) {
                            todo!("diagnostic: if condition must be bool, got {cond_ty:?}");
                        }

                        self.locals.alloc_untyped_if_result(result);
                        let then_block = self.walk_sub_block(then_block);
                        let else_block = self.walk_sub_block(else_block);
                        // Clear comptime value for result (we don't know which branch ran)
                        self.locals.clear_comptime(result);

                        self.instructions_buf.push(mir::Instruction::If {
                            condition,
                            then_block,
                            else_block,
                        });
                    }
                }
            }
            hir::Instruction::While { condition_block, condition, body } => {
                let mir_condition_block = self.walk_sub_block(condition_block);
                let mir_condition =
                    self.locals.get_mir(condition).expect("while condition must have MIR local");
                let mir_body = self.walk_sub_block(body);
                self.instructions_buf.push(mir::Instruction::While {
                    condition_block: mir_condition_block,
                    condition: mir_condition,
                    body: mir_body,
                });
            }
            hir::Instruction::Assign { target, value } => {
                let target_mir =
                    self.locals.get_mir(target).expect("assign target must have MIR local");
                let target_ty =
                    self.locals.get_mir_type(target_mir).expect("assign target must have type");

                let result = self.translate_expr(value);
                let (rhs_mir, rhs_ty) = match result {
                    ExprResult::Runtime { mir_local, ty, .. } => (mir_local, ty),
                    ExprResult::ComptimeOnly(_) => {
                        todo!("diagnostic: cannot assign comptime-only value")
                    }
                };

                if target_ty != rhs_ty {
                    todo!(
                        "diagnostic: assign type mismatch: expected {target_ty:?}, got {rhs_ty:?}"
                    );
                }

                // Clear comptime value for target
                self.locals.clear_comptime(target);

                self.instructions_buf.push(mir::Instruction::Assign {
                    target: target_mir,
                    value: mir::Expr::LocalRef(rhs_mir),
                });
            }
        }
    }

    fn flush_as_fn(self, param_count: u32, return_type: TypeId) -> mir::FnId {
        let body = self.eval.mir_blocks.push_iter(self.instructions_buf.into_iter());
        let fn_id = self.eval.mir_fns.push(mir::FnDef { body, param_count, return_type });

        // Convert Option<TypeId> to TypeId, asserting all locals have types
        let types_iter =
            self.locals.mir_type.raw.into_iter().map(|opt| opt.expect("MIR local has no type"));
        let locals_id = self.eval.mir_fn_locals.push_iter(types_iter);
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
    let mut preamble_bindings = comptime::Bindings::default();
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
        lowerer.locals.assign_or_define(param.value, ty).expect("overwriting via params?");
    }

    lowerer.walk_block(fn_def.body);
    lowerer.flush_as_fn(param_count, return_type)
}

pub(crate) fn lower_block_as_fn(eval: &mut Evaluator<'_>, hir_block: hir::BlockId) -> mir::FnId {
    let mut lowerer = BodyLowerer::new(eval);
    lowerer.walk_block(hir_block);
    lowerer.flush_as_fn(0, TypeId::VOID)
}
