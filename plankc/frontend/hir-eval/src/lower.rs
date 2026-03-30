use plank_core::list_of_lists::ListOfLists;
use plank_hir::{self as hir};
use plank_mir::{self as mir};
use plank_session::{SrcLoc, StrId};
use plank_values::{StructInfo, Type, TypeId, TypeInterner, ValueId};

use crate::{
    Evaluator,
    comptime::ComptimeInterpreter,
    local_state::*,
    value::{Value, ValueInterner},
};

const INSTRUCTION_BUF_CAPACITY: usize = 1024;
const VALUES_BUF_CAPACITY: usize = 32;
const MIR_LOCALS_BUF_CAPACITY: usize = 32;
const FIELDS_BUF_CAPACITY: usize = 128;

struct FunctionLowerScope {
    expected_return_type: TypeId,
    expected_return_type_loc: Option<SrcLoc>,
    locals: Locals,
    interpreter: ComptimeInterpreter,

    instr_buf_stack: Vec<mir::Instruction>,
    mir_buf_stack: Vec<mir::LocalId>,
    values_buf: Vec<ValueId>,
    captures_buf: Vec<(ValueId, SrcLoc)>,
    field_types_buf: Vec<TypeId>,
    field_names_buf: Vec<StrId>,
}

#[derive(Debug, Clone, Copy)]
enum ExprResult {
    Runtime { expr: mir::Expr, ty: TypeId, comptime: Option<ValueId> },
    ComptimeOnly(ValueId),
}

struct BlockControlFlowDiverges;

impl FunctionLowerScope {
    fn materialize(
        &mut self,
        values: &ValueInterner,
        types: &TypeInterner,
        mir_args: &mut ListOfLists<mir::ArgsId, mir::LocalId>,
        value: ValueId,
    ) -> Option<(mir::Expr, TypeId)> {
        let (ty, fields) = match values.lookup(value) {
            Value::Error => return Some((mir::Expr::Error, TypeId::ERROR)),
            Value::Void => return Some((mir::Expr::Void, TypeId::VOID)),
            Value::Bool(b) => return Some((mir::Expr::Bool(b), TypeId::BOOL)),
            Value::BigNum(x) => return Some((mir::Expr::BigNum(x), TypeId::U256)),
            Value::Type(_) | Value::Closure { .. } => return None,
            Value::StructVal { ty, fields } => {
                if types.comptime_only(ty) {
                    return None;
                }
                (ty, fields)
            }
        };
        let mir_buf_start = self.mir_buf_stack.len();
        for &field in fields {
            let (expr, ty) = self
                .materialize(values, types, mir_args, field)
                .expect("struct has comptime-only fields");
            let target = match expr {
                mir::Expr::LocalRef(local) => local,
                expr => {
                    let target = self.locals.alloc_anonymous_mir(ty);
                    self.instr_buf_stack.push(mir::Instruction::Set { target, expr });
                    target
                }
            };
            self.mir_buf_stack.push(target);
        }
        let fields = mir_args.push_iter(self.mir_buf_stack.drain(mir_buf_start..));
        Some((mir::Expr::StructLit { ty, fields }, ty))
    }

    fn translate_struct_literal(
        &mut self,
        eval: &mut Evaluator<'_>,
        ty: hir::LocalId,
        fields: hir::FieldsId,
    ) -> ExprResult {
        let ty_loc = self.locals.def_loc(ty);
        let Some(ty) = self.locals.comptime(ty) else {
            todo!("diagnostic: struct type not comptime known");
        };
        let Value::Type(ty) = eval.values.lookup(ty) else {
            eval.emit_type_constraint_not_type(eval.values.type_of_value(ty), ty_loc);
            return ExprResult::ComptimeOnly(ValueId::ERROR);
        };
        if !matches!(eval.types.lookup(ty), Type::Struct(_)) {
            eval.emit_not_a_struct_type(ty, ty_loc);
            return ExprResult::ComptimeOnly(ValueId::ERROR);
        }

        for (i, field) in eval.hir.fields[fields].iter().enumerate() {
            let r#struct = match eval.types.lookup(ty) {
                Type::Struct(s) => s,
                _ => unreachable!(),
            };
            let Some(field_pos) = r#struct.field_names.iter().position(|&name| name == field.name)
            else {
                todo!("diagnostic: struct _ has no field named _");
            };
            if eval.hir.fields[fields][..i].iter().any(|f| f.name == field.name) {
                todo!("diagnostic: duplicate struct field assignment");
            }
            let expected_field_ty = r#struct.field_types[field_pos];
            let field_value_ty = self.locals.get_type(field.value, &eval.values);
            if !field_value_ty.is_assignable_to(expected_field_ty) {
                eval.emit_type_mismatch_simple(
                    expected_field_ty,
                    field_value_ty,
                    self.locals.def_loc(field.value),
                );
            }
        }

        let Type::Struct(r#struct) = eval.types.lookup(ty) else { unreachable!() };

        assert!(self.values_buf.is_empty());

        if eval.types.comptime_only(ty) {
            for &field_name in r#struct.field_names {
                let Some(&field) =
                    eval.hir.fields[fields].iter().find(|field| field.name == field_name)
                else {
                    todo!("diagnostic: literal missing struct field");
                };
                let Some(value) = self.locals.comptime(field.value) else {
                    todo!("diagnostic: non-comptime field in struct with comptime-only fields");
                };
                self.values_buf.push(value);
            }
            let struct_value =
                eval.values.intern(Value::StructVal { ty, fields: &self.values_buf });
            self.values_buf.clear();
            return ExprResult::ComptimeOnly(struct_value);
        }

        let mir_start = self.mir_buf_stack.len();
        let mut comptime_known = true;
        for &field_name in r#struct.field_names {
            let Some(&field) =
                eval.hir.fields[fields].iter().find(|field| field.name == field_name)
            else {
                todo!("diagnostic: literal missing struct field");
            };
            if comptime_known {
                if let Some(value) = self.locals.comptime(field.value) {
                    self.values_buf.push(value);
                } else {
                    comptime_known = false;
                }
            }
            // Only comptime only values may have value but no hir local.
            self.mir_buf_stack.push(self.locals.hir_to_mir(field.value));
        }
        let fields = eval.mir_args.push_iter(self.mir_buf_stack.drain(mir_start..));
        let comptime = comptime_known.then(|| {
            assert_eq!(self.values_buf.len(), r#struct.field_types.len());
            eval.values.intern(Value::StructVal { ty, fields: &self.values_buf })
        });
        self.values_buf.clear();
        ExprResult::Runtime { expr: mir::Expr::StructLit { ty, fields }, ty, comptime }
    }

    fn translate_expr(&mut self, eval: &mut Evaluator<'_>, expr: hir::Expr) -> ExprResult {
        match expr.kind {
            hir::ExprKind::Void => ExprResult::Runtime {
                expr: mir::Expr::Void,
                ty: TypeId::VOID,
                comptime: Some(ValueId::VOID),
            },
            hir::ExprKind::Bool(b) => ExprResult::Runtime {
                expr: mir::Expr::Bool(b),
                ty: TypeId::BOOL,
                comptime: Some(if b { ValueId::TRUE } else { ValueId::FALSE }),
            },
            hir::ExprKind::BigNum(big_num_id) => ExprResult::Runtime {
                expr: mir::Expr::BigNum(big_num_id),
                ty: TypeId::U256,
                comptime: Some(eval.values.intern_num(big_num_id)),
            },
            hir::ExprKind::BuiltinCall { builtin, args } => {
                let args = &eval.hir.call_args[args];
                'sig: for &(input_types, result_type) in builtin.signatures() {
                    if input_types.len() != args.len() {
                        todo!("diagnostic: builtin argument count mismatch");
                    }

                    for (&input, &arg) in input_types.iter().zip(args) {
                        if !input.is_assignable_to(self.locals.get_type(arg, &eval.values)) {
                            continue 'sig;
                        }
                    }

                    let args = eval
                        .mir_args
                        .push_iter(args.iter().map(|&arg| self.locals.hir_to_mir(arg)));
                    return ExprResult::Runtime {
                        expr: mir::Expr::BuiltinCall { builtin, args },
                        ty: result_type,
                        comptime: None,
                    };
                }
                todo!("diagnostic: no matching builtin type signature")
            }
            hir::ExprKind::LocalRef(hir) => {
                let value = self.locals.comptime(hir);
                let mir = self.locals.get_mir(hir);
                match (mir, value) {
                    (Some(mir), comptime) => ExprResult::Runtime {
                        expr: mir::Expr::LocalRef(mir),
                        ty: self.locals.mir_type(mir),
                        comptime,
                    },
                    (None, Some(value)) => ExprResult::ComptimeOnly(value),
                    (None, None) => unreachable!("undefined hir {hir:?}"),
                }
            }
            hir::ExprKind::ConstRef(id) => {
                let value = eval.ensure_const_evaluated(&mut self.interpreter, id);
                match self.materialize(&eval.values, &eval.types, &mut eval.mir_args, value) {
                    None => ExprResult::ComptimeOnly(value),
                    Some((expr, ty)) => ExprResult::Runtime { expr, ty, comptime: Some(value) },
                }
            }
            hir::ExprKind::Type(ty) => ExprResult::ComptimeOnly(eval.values.intern_type(ty)),
            hir::ExprKind::FnDef(fn_def) => {
                let captures = &eval.hir.fn_captures[fn_def];
                assert!(self.captures_buf.is_empty());
                for capture in captures {
                    let vid = self
                        .locals
                        .comptime(capture.outer_local)
                        .expect("todo-diagnostic: closure capture must be comptime");
                    let loc = self.locals.def_loc(capture.outer_local);
                    self.captures_buf.push((vid, loc));
                }
                let value_id =
                    eval.values.intern(Value::Closure { fn_def, captures: &self.captures_buf });
                self.captures_buf.clear();
                ExprResult::ComptimeOnly(value_id)
            }
            hir::ExprKind::Call { callee, args } => {
                let callee_loc = self.locals.def_loc(callee);
                let closure = self
                    .locals
                    .comptime(callee)
                    .expect("todo-diagnostic: call target must be comptime-known");
                let callee = eval.fn_cache.get(&closure).copied().unwrap_or_else(|| {
                    let id = self.lower_closure(eval, closure, callee_loc);
                    eval.fn_cache.insert(closure, id);
                    id
                });

                let fn_def = eval.mir_fns[callee];
                let arg_locals = &eval.hir.call_args[args];
                if arg_locals.len() != fn_def.param_count as usize {
                    todo!("diagnostic: function call argument count mismatch");
                }

                for (arg_i, &arg_local) in arg_locals.iter().enumerate() {
                    let expected_ty = eval.mir_fn_locals[callee][arg_i];
                    let actual_ty = self.locals.get_type(arg_local, &eval.values);
                    if !actual_ty.is_assignable_to(expected_ty) {
                        eval.emit_type_mismatch_simple(
                            expected_ty,
                            actual_ty,
                            self.locals.def_loc(arg_local),
                        );
                    }
                }

                let args =
                    eval.mir_args.push_iter(arg_locals.iter().map(|&hir| {
                        self.locals.get_mir(hir).expect("todo: non-runtime arg handling")
                    }));

                ExprResult::Runtime {
                    expr: mir::Expr::Call { callee, args },
                    ty: fn_def.return_type,
                    comptime: None,
                }
            }
            hir::ExprKind::StructDef(struct_def_id) => {
                let struct_def = eval.hir.struct_defs[struct_def_id];
                let Some(type_index) = self.locals.comptime(struct_def.type_index) else {
                    todo!("diagnostic: `type_index` not comptime known");
                };
                let fields = &eval.hir.fields[struct_def.fields];
                assert!(self.field_types_buf.is_empty());
                assert!(self.field_names_buf.is_empty());
                for field in fields {
                    let Some(value) = self.locals.comptime(field.value) else {
                        todo!("diagnostic: field type not comptime known");
                    };
                    let Value::Type(r#type) = eval.values.lookup(value) else {
                        eval.emit_type_constraint_not_type(
                            eval.values.type_of_value(value),
                            self.locals.def_loc(field.value),
                        );
                        self.field_types_buf.push(TypeId::ERROR);
                        self.field_names_buf.push(field.name);
                        continue;
                    };
                    self.field_types_buf.push(r#type);
                    self.field_names_buf.push(field.name);
                }
                let ty = eval.types.intern(Type::Struct(StructInfo {
                    source_id: struct_def.source_id,
                    source_span: struct_def.source_span,
                    type_index,
                    field_names: &self.field_names_buf,
                    field_types: &self.field_types_buf,
                }));
                self.field_names_buf.clear();
                self.field_types_buf.clear();
                ExprResult::ComptimeOnly(eval.values.intern(Value::Type(ty)))
            }
            hir::ExprKind::StructLit { ty, fields } => {
                self.translate_struct_literal(eval, ty, fields)
            }
            hir::ExprKind::Member { object, member } => {
                let ty = self.locals.get_type(object, &eval.values);
                let Type::Struct(r#struct) = eval.types.lookup(ty) else {
                    eval.emit_member_on_non_struct(ty, self.locals.def_loc(object));
                    return ExprResult::ComptimeOnly(ValueId::ERROR);
                };
                let Some(field_index) =
                    r#struct.field_names.iter().position(|&name| name == member)
                else {
                    todo!("diagnostic: access undefined attribute");
                };
                let value = self.locals.comptime(object).map(|object| {
                    let Value::StructVal { ty: _, fields } = eval.values.lookup(object) else {
                        unreachable!("invalid hir: type soundness");
                    };
                    fields[field_index]
                });
                let mir = self.locals.get_mir(object);
                match (mir, value) {
                    (Some(object), comptime) => ExprResult::Runtime {
                        expr: mir::Expr::FieldAccess { object, field_index: field_index as u32 },
                        ty: r#struct.field_types[field_index],
                        comptime,
                    },
                    (None, Some(value)) => ExprResult::ComptimeOnly(value),
                    (None, None) => unreachable!("invalid hir"),
                }
            }
            hir::ExprKind::Error => unreachable!("error expression reached hir-eval"),
        }
    }

    fn lower_closure(
        &mut self,
        eval: &mut Evaluator<'_>,
        closure: ValueId,
        callee_loc: SrcLoc,
    ) -> mir::FnId {
        let Value::Closure { fn_def, captures } = eval.values.lookup(closure) else {
            eval.emit_not_callable(eval.values.type_of_value(closure), callee_loc);
            todo!("diagnostic: callee is not a function — error recovery")
        };
        let func = eval.hir.fns[fn_def];
        let params = &eval.hir.fn_params[fn_def];
        let hir_captures = &eval.hir.fn_captures[fn_def];

        // TODO: Optimize to use same allocation across scopes.
        let saved_locals = std::mem::take(&mut self.locals);

        self.interpreter.reset();
        // Insert captures.
        for (capture_info, &(value, loc)) in hir_captures.iter().zip(captures) {
            let prev = self.interpreter.bindings.insert(capture_info.inner_local, (value, loc));
            assert!(prev.is_none(), "invalid hir");
            self.locals.set_comptime_only(capture_info.inner_local, value, loc);
        }
        // Interpret type premable to determine types.
        self.interpreter
            .interpret_block(eval, func.type_preamble)
            .expect("invalid hir: premable with `return`");
        let (return_type, return_type_loc) = self.interpreter.bindings[func.return_type];
        let Value::Type(return_type) = eval.values.lookup(return_type) else {
            eval.emit_type_constraint_not_type(
                eval.values.type_of_value(return_type),
                return_type_loc,
            );
            todo!("diagnostic: return type not type — error recovery")
        };
        let saved_return_type = std::mem::replace(&mut self.expected_return_type, return_type);
        let saved_return_type_loc = self.expected_return_type_loc.replace(return_type_loc);

        for param in params {
            let (ty, _) = self.interpreter.bindings[param.r#type];
            let param_src_loc = SrcLoc::new(func.source, param.span);
            let ty = match eval.values.lookup(ty) {
                Value::Type(ty) => ty,
                non_type_value => {
                    eval.emit_type_constraint_not_type(non_type_value.get_type(), param_src_loc);
                    TypeId::ERROR
                }
            };
            self.locals.associate_hir_to_new_mir(param.value, ty, param_src_loc);
        }

        let (body, _) = self.translate_block(eval, func.body);

        let fn_id1 = eval.mir_fn_locals.push_iter(self.locals.mir_types());
        let fn_id2 =
            eval.mir_fns.push(mir::FnDef { body, param_count: params.len() as u32, return_type });
        assert_eq!(fn_id1, fn_id2);

        self.locals = saved_locals;
        self.expected_return_type = saved_return_type;
        self.expected_return_type_loc = saved_return_type_loc;

        fn_id1
    }

    fn expect_type(&mut self, eval: &mut Evaluator<'_>, local: hir::LocalId) -> TypeId {
        let Some(type_value) = self.locals.comptime(local) else {
            todo!("diagnostic: AssertType of_type must be comptime")
        };
        let Value::Type(expected) = eval.values.lookup(type_value) else {
            eval.emit_type_constraint_not_type(
                self.locals.get_type(local, &eval.values),
                self.locals.def_loc(local),
            );
            return TypeId::ERROR;
        };
        expected
    }

    fn translate_block_inner(
        &mut self,
        eval: &mut Evaluator<'_>,
        block: hir::BlockId,
    ) -> Result<(), BlockControlFlowDiverges> {
        for &instr in &eval.hir.blocks[block] {
            match instr.kind {
                hir::InstructionKind::Set { local, r#type, expr } => {
                    let src_loc = expr.src_loc();
                    let ty = match self.translate_expr(eval, expr) {
                        ExprResult::Runtime { expr, ty, comptime } => {
                            match self.locals.set(local, ty, src_loc, comptime) {
                                Ok(target) => {
                                    self.instr_buf_stack
                                        .push(mir::Instruction::Set { target, expr });
                                }
                                Err(TypeMismatchError { expected_ty, received_ty }) => {
                                    eval.emit_type_mismatch_error(
                                        expected_ty,
                                        self.locals.def_loc(local),
                                        received_ty,
                                        src_loc,
                                    );
                                }
                            }
                            ty
                        }
                        ExprResult::ComptimeOnly(value) => {
                            self.locals.set_comptime_only(local, value, src_loc);
                            eval.values.type_of_value(value)
                        }
                    };

                    if let Some(r#type) = r#type {
                        let expected = self.expect_type(eval, r#type);
                        if !ty.is_assignable_to(expected) {
                            eval.emit_type_mismatch_error(
                                expected,
                                self.locals.def_loc(r#type),
                                ty,
                                src_loc,
                            );
                        }
                    }

                    if ty == TypeId::NEVER {
                        return Err(BlockControlFlowDiverges);
                    }
                }
                hir::InstructionKind::BranchSet { local, expr } => {
                    let src_loc = expr.src_loc();
                    match self.translate_expr(eval, expr) {
                        ExprResult::Runtime { expr, ty, comptime: _ } => {
                            match self.locals.set_from_branch(local, ty, src_loc) {
                                Ok(target) => {
                                    self.instr_buf_stack
                                        .push(mir::Instruction::Set { target, expr });
                                }
                                Err(TypeUnificationError { existing_def, existing_ty, new_ty }) => {
                                    eval.emit_incompatible_branch_types(
                                        existing_ty,
                                        existing_def,
                                        new_ty,
                                        src_loc,
                                    );
                                }
                            }
                            if ty == TypeId::NEVER {
                                return Err(BlockControlFlowDiverges);
                            }
                        }
                        ExprResult::ComptimeOnly(value) => {
                            self.locals.set_comptime_only(local, value, src_loc)
                        }
                    }
                }
                hir::InstructionKind::Assign { target, value } => {
                    match self.translate_expr(eval, value) {
                        ExprResult::Runtime { expr, ty, comptime: _ } => {
                            match self.locals.handle_assign(target, ty) {
                                Ok(mir_target) => {
                                    self.instr_buf_stack
                                        .push(mir::Instruction::Set { target: mir_target, expr });
                                }
                                Err(TypeMismatchError { expected_ty, received_ty }) => {
                                    eval.emit_type_mismatch_error(
                                        expected_ty,
                                        self.locals.def_loc(target),
                                        received_ty,
                                        value.src_loc(),
                                    );
                                }
                            }
                        }
                        ExprResult::ComptimeOnly(_) => {
                            todo!("diagnostic: assigning comptime only value in runtime ctx")
                        }
                    }
                }
                hir::InstructionKind::Eval(expr) => match self.translate_expr(eval, expr) {
                    ExprResult::ComptimeOnly(_) => { /* No MIR equivalent, do nothing */ }
                    ExprResult::Runtime { expr, ty, comptime: _ } => {
                        // MIR doesn't have `Eval` so we use `Set`.
                        let target = self.locals.alloc_anonymous_mir(ty);
                        self.instr_buf_stack.push(mir::Instruction::Set { target, expr });
                        if ty == TypeId::NEVER {
                            return Err(BlockControlFlowDiverges);
                        }
                    }
                },
                hir::InstructionKind::Return(expr) => {
                    let return_src_loc = expr.src_loc();
                    match self.translate_expr(eval, expr) {
                        ExprResult::ComptimeOnly(_) => {
                            todo!("diagnostic: returning comptime-only in runtime ctx")
                        }
                        ExprResult::Runtime { expr, ty, comptime: _ } => {
                            let temp_store = self.locals.alloc_anonymous_mir(ty);
                            self.instr_buf_stack
                                .push(mir::Instruction::Set { target: temp_store, expr });
                            if !ty.is_assignable_to(self.expected_return_type) {
                                if let Some(expected_loc) = self.expected_return_type_loc {
                                    eval.emit_type_mismatch_error(
                                        self.expected_return_type,
                                        expected_loc,
                                        ty,
                                        return_src_loc,
                                    );
                                } else {
                                    eval.emit_type_mismatch_simple(
                                        self.expected_return_type,
                                        ty,
                                        return_src_loc,
                                    );
                                }
                            }
                            if ty == TypeId::NEVER {
                                return Err(BlockControlFlowDiverges);
                            }
                            self.instr_buf_stack.push(mir::Instruction::Return(temp_store));
                            return Err(BlockControlFlowDiverges);
                        }
                    }
                }
                hir::InstructionKind::If { condition, then_block, else_block } => {
                    match self.locals.comptime(condition) {
                        Some(ValueId::TRUE) => self.translate_block_inner(eval, then_block)?,
                        Some(ValueId::FALSE) => self.translate_block_inner(eval, else_block)?,
                        Some(_) => {
                            let cond_ty = self.locals.get_type(condition, &eval.values);
                            eval.emit_type_mismatch_simple(
                                TypeId::BOOL,
                                cond_ty,
                                self.locals.def_loc(condition),
                            );
                            self.translate_block_inner(eval, else_block)?
                        }
                        None => {
                            let ty = self.locals.get_type(condition, &eval.values);
                            if !ty.is_assignable_to(TypeId::BOOL) {
                                eval.emit_type_mismatch_simple(
                                    TypeId::BOOL,
                                    ty,
                                    self.locals.def_loc(condition),
                                );
                            }
                            let (then_block, then_control) = self.translate_block(eval, then_block);
                            let (else_block, else_control) = self.translate_block(eval, else_block);
                            let condition = self.locals.hir_to_mir(condition);
                            self.instr_buf_stack.push(mir::Instruction::If {
                                condition,
                                then_block,
                                else_block,
                            });
                            if then_control.is_err() && else_control.is_err() {
                                return Err(BlockControlFlowDiverges);
                            }
                        }
                    }
                }
                hir::InstructionKind::While { condition_block, condition, body } => {
                    let (condition_block, cond_control) =
                        self.translate_block(eval, condition_block);
                    let () = cond_control?;

                    let ty = self.locals.get_type(condition, &eval.values);
                    if !ty.is_assignable_to(TypeId::BOOL) {
                        eval.emit_type_mismatch_simple(
                            TypeId::BOOL,
                            ty,
                            self.locals.def_loc(condition),
                        );
                    }
                    let condition = self.locals.hir_to_mir(condition);
                    let (body, _) = self.translate_block(eval, body);
                    self.instr_buf_stack.push(mir::Instruction::While {
                        condition_block,
                        condition,
                        body,
                    })
                }
            }
        }
        Ok(())
    }

    fn translate_block(
        &mut self,
        eval: &mut Evaluator<'_>,
        block: hir::BlockId,
    ) -> (mir::BlockId, Result<(), BlockControlFlowDiverges>) {
        let instr_start = self.instr_buf_stack.len();
        let control_flow = self.translate_block_inner(eval, block);
        let id = eval.mir_blocks.push_iter(self.instr_buf_stack.drain(instr_start..));
        (id, control_flow)
    }
}

pub(crate) fn lower_entry_point_as_fn(
    eval: &mut Evaluator<'_>,
    hir_block: hir::BlockId,
) -> mir::FnId {
    let mut scope = FunctionLowerScope {
        expected_return_type: TypeId::NEVER,
        expected_return_type_loc: None,
        locals: Locals::default(),
        interpreter: ComptimeInterpreter::new(),

        instr_buf_stack: Vec::with_capacity(INSTRUCTION_BUF_CAPACITY),
        values_buf: Vec::with_capacity(VALUES_BUF_CAPACITY),
        captures_buf: Vec::with_capacity(VALUES_BUF_CAPACITY),
        mir_buf_stack: Vec::with_capacity(MIR_LOCALS_BUF_CAPACITY),
        field_types_buf: Vec::with_capacity(FIELDS_BUF_CAPACITY),
        field_names_buf: Vec::with_capacity(FIELDS_BUF_CAPACITY),
    };

    let (body, control_flow) = scope.translate_block(eval, hir_block);
    if !matches!(control_flow, Err(BlockControlFlowDiverges)) {
        todo!("diagnostic: entry point must have an explicit terminator");
    }

    let fn_id1 = eval.mir_fn_locals.push_iter(scope.locals.mir_types());
    let fn_id2 = eval.mir_fns.push(mir::FnDef { body, param_count: 0, return_type: TypeId::NEVER });
    assert_eq!(fn_id1, fn_id2);
    fn_id1
}
