use plank_core::{DenseIndexMap, vec_buf::VecBuf};
use plank_hir::{self as hir, ConstDef};
use plank_session::{SrcLoc, StrId};
use plank_values::{StructInfo, Type, TypeId, ValueId};

use crate::{Evaluator, value::Value};

#[derive(Debug)]
pub struct ReturnValue(ValueId);

pub(crate) struct ComptimeInterpreter {
    pub(crate) bindings: DenseIndexMap<hir::LocalId, (ValueId, SrcLoc)>,

    value_buf: VecBuf<ValueId>,
    capture_buf: VecBuf<(ValueId, SrcLoc)>,
    type_buf: VecBuf<TypeId>,
    name_buf: VecBuf<StrId>,
}

impl ComptimeInterpreter {
    pub fn new() -> Self {
        const EST_MAX_FIELD_COUNT: usize = 64;
        Self {
            bindings: DenseIndexMap::default(),
            value_buf: VecBuf::default(),
            capture_buf: VecBuf::new(),
            type_buf: VecBuf::with_capacity(EST_MAX_FIELD_COUNT),
            name_buf: VecBuf::with_capacity(EST_MAX_FIELD_COUNT),
        }
    }

    pub fn reset(&mut self) {
        self.bindings.clear();
    }

    pub fn eval_const(&mut self, eval: &mut Evaluator<'_>, const_def: ConstDef) -> ValueId {
        self.interpret_block(eval, const_def.body)
            .expect("hir: const expr shouldn't have `return`");
        self.bindings[const_def.result].0
    }

    pub fn interpret_block(
        &mut self,
        eval: &mut Evaluator<'_>,
        block_id: hir::BlockId,
    ) -> Result<(), ReturnValue> {
        for &instr in &eval.hir.blocks[block_id] {
            self.interpret_instruction(eval, instr)?;
        }
        Ok(())
    }

    fn interpret_instruction(
        &mut self,
        eval: &mut Evaluator<'_>,
        instr: hir::Instruction,
    ) -> Result<(), ReturnValue> {
        match instr.kind {
            hir::InstructionKind::Set { local, r#type, expr } => {
                let mut value = self.eval_expr(eval, expr.kind)?;
                if let Some(r#type) = r#type
                    && value != ValueId::ERROR
                {
                    let (type_value, type_loc) = self.bindings[r#type];
                    match eval.values.lookup(type_value) {
                        Value::Error => { /* already had error, supress cascade */ }
                        Value::Type(expected_ty) => {
                            let actual_ty = eval.values.type_of_value(value);
                            if !actual_ty.is_assignable_to(expected_ty) {
                                eval.emit_type_mismatch_error(
                                    expected_ty,
                                    type_loc,
                                    actual_ty,
                                    expr.src_loc(),
                                );
                            }
                        }
                        non_type_value => {
                            eval.emit_type_constraint_not_type(non_type_value.get_type(), type_loc);
                            value = ValueId::ERROR;
                        }
                    }
                }
                if self.bindings.insert(local, (value, expr.src_loc())).is_some() {
                    unreachable!("hir: overwriting with set");
                }
            }
            hir::InstructionKind::BranchSet { local, expr } => {
                let value = self.eval_expr(eval, expr.kind)?;
                if self.bindings.insert(local, (value, expr.src_loc())).is_some() {
                    unreachable!("hir: overwriting with set");
                }
            }
            hir::InstructionKind::Eval(expr) => {
                self.eval_expr(eval, expr.kind)?;
            }
            hir::InstructionKind::Return(expr) => {
                let value = self.eval_expr(eval, expr.kind)?;
                return Err(ReturnValue(value));
            }
            hir::InstructionKind::Assign { target, value } => {
                let new_value = self.eval_expr(eval, value.kind)?;
                let Some(prev_value) = self.bindings.insert(target, (new_value, value.src_loc()))
                else {
                    unreachable!("hir: init with assign")
                };
                let new_type = eval.values.type_of_value(new_value);
                let prev_type = eval.values.type_of_value(prev_value.0);
                if !new_type.is_assignable_to(prev_type) {
                    eval.emit_type_mismatch_error(
                        prev_type,
                        prev_value.1,
                        new_type,
                        value.src_loc(),
                    );
                }
            }
            hir::InstructionKind::If { condition, then_block, else_block } => {
                let (cond_vid, cond_loc) = self.bindings[condition];
                match eval.values.lookup(cond_vid) {
                    Value::Bool(true) => self.interpret_block(eval, then_block)?,
                    Value::Bool(false) => self.interpret_block(eval, else_block)?,
                    other => {
                        eval.emit_type_mismatch_simple(TypeId::BOOL, other.get_type(), cond_loc);
                        self.interpret_block(eval, else_block)?
                    }
                }
            }
            hir::InstructionKind::While { .. } => {
                todo!("comptime while loops not yet implemented")
            }
        }
        Ok(())
    }

    fn eval_expr(
        &mut self,
        eval: &mut Evaluator<'_>,
        expr: hir::ExprKind,
    ) -> Result<ValueId, ReturnValue> {
        let value = match expr {
            hir::ExprKind::Void => ValueId::VOID,
            hir::ExprKind::Bool(false) => ValueId::FALSE,
            hir::ExprKind::Bool(true) => ValueId::TRUE,
            hir::ExprKind::BigNum(id) => eval.values.intern_num(id),
            hir::ExprKind::Type(type_id) => eval.values.intern_type(type_id),
            hir::ExprKind::ConstRef(const_id) => eval.ensure_const_evaluated(self, const_id),
            hir::ExprKind::LocalRef(local_id) => self.bindings[local_id].0,
            hir::ExprKind::FnDef(fn_def_id) => self.eval_fn_def(eval, fn_def_id)?,
            hir::ExprKind::Call { callee, args } => self.eval_call(eval, callee, args)?,
            hir::ExprKind::StructDef(struct_def_id) => self.eval_struct_def(eval, struct_def_id)?,
            hir::ExprKind::StructLit { ty, fields } => self.eval_struct_lit(eval, ty, fields)?,
            hir::ExprKind::Member { object, member } => self.eval_member(eval, object, member)?,
            hir::ExprKind::BuiltinCall { .. } => todo!("comptime builtin eval not yet implemented"),
            hir::ExprKind::Error => unreachable!("error expression reached hir-eval"),
        };
        Ok(value)
    }

    fn eval_fn_def(
        &mut self,
        eval: &mut Evaluator<'_>,
        fn_def: hir::FnDefId,
    ) -> Result<ValueId, ReturnValue> {
        let value_id = self.capture_buf.use_as(|captures| {
            for capture in &eval.hir.fn_captures[fn_def] {
                captures.push(self.bindings[capture.outer_local]);
            }
            let closure = Value::Closure { fn_def, captures };
            eval.values.intern(closure)
        });

        Ok(value_id)
    }

    fn eval_struct_def(
        &mut self,
        eval: &mut Evaluator<'_>,
        struct_def_id: hir::StructDefId,
    ) -> Result<ValueId, ReturnValue> {
        let struct_def = eval.hir.struct_defs[struct_def_id];
        let (type_index_vid, _) = self.bindings[struct_def.type_index];
        let fields_info = &eval.hir.fields[struct_def.fields];

        let struct_type_id = self.type_buf.use_as(|types| {
            self.name_buf.use_as(|names| {
                for field in fields_info {
                    let (field_vid, field_loc) = self.bindings[field.value];
                    match eval.values.lookup(field_vid) {
                        Value::Type(tid) => {
                            types.push(tid);
                            names.push(field.name);
                        }
                        non_type => {
                            eval.emit_type_constraint_not_type(non_type.get_type(), field_loc);
                            types.push(TypeId::ERROR);
                            names.push(field.name);
                        }
                    }
                }

                eval.types.intern(Type::Struct(StructInfo {
                    source_id: struct_def.source_id,
                    source_span: struct_def.source_span,
                    type_index: type_index_vid,
                    field_types: types,
                    field_names: names,
                }))
            })
        });

        Ok(eval.values.intern_type(struct_type_id))
    }

    fn eval_struct_lit(
        &mut self,
        eval: &mut Evaluator<'_>,
        ty: hir::LocalId,
        fields_id: hir::FieldsId,
    ) -> Result<ValueId, ReturnValue> {
        let (type_vid, type_loc) = self.bindings[ty];
        let Value::Type(struct_type_id) = eval.values.lookup(type_vid) else {
            eval.emit_type_constraint_not_type(eval.values.type_of_value(type_vid), type_loc);
            return Ok(ValueId::ERROR);
        };
        if !matches!(eval.types.lookup(struct_type_id), Type::Struct(_)) {
            eval.emit_not_a_struct_type(struct_type_id, type_loc);
            return Ok(ValueId::ERROR);
        }

        let fields_info = &eval.hir.fields[fields_id];

        for (i, field) in fields_info.iter().enumerate() {
            let Type::Struct(r#struct) = eval.types.lookup(struct_type_id) else { unreachable!() };
            let Some(field_pos) = r#struct.field_names.iter().position(|&name| name == field.name)
            else {
                todo!("diagnostic: struct _ has no field named _");
            };
            if fields_info[..i].iter().any(|f| f.name == field.name) {
                todo!("diagnostic: duplicate struct field assignment");
            }
            let expected_field_ty = r#struct.field_types[field_pos];
            let (field_value_vid, field_value_loc) = self.bindings[field.value];
            let field_value_ty = eval.values.type_of_value(field_value_vid);
            if !field_value_ty.is_assignable_to(expected_field_ty) {
                eval.emit_type_mismatch_simple(expected_field_ty, field_value_ty, field_value_loc);
            }
        }

        self.value_buf.use_as(|fields| {
            let Type::Struct(r#struct) = eval.types.lookup(struct_type_id) else { unreachable!() };
            for &field_name in r#struct.field_names {
                let Some(&field) = fields_info.iter().find(|field| field.name == field_name) else {
                    todo!("diagnostic: literal missing struct field");
                };
                fields.push(self.bindings[field.value].0);
            }
            Ok(eval.values.intern(Value::StructVal { ty: struct_type_id, fields }))
        })
    }

    fn eval_member(
        &mut self,
        eval: &mut Evaluator<'_>,
        object: hir::LocalId,
        member: StrId,
    ) -> Result<ValueId, ReturnValue> {
        let (obj_vid, obj_loc) = self.bindings[object];
        match eval.values.lookup(obj_vid) {
            Value::StructVal { ty, fields } => {
                let Some(field_index) = eval.types.field_index_by_name(ty, member) else {
                    todo!("diagnostic: unknown struct field");
                };
                Ok(fields[field_index as usize])
            }
            other => {
                eval.emit_member_on_non_struct(other.get_type(), obj_loc);
                Ok(ValueId::ERROR)
            }
        }
    }

    fn eval_call(
        &mut self,
        eval: &mut Evaluator<'_>,
        callee: hir::LocalId,
        args: hir::CallArgsId,
    ) -> Result<ValueId, ReturnValue> {
        let (closure_vid, callee_loc) = self.bindings[callee];
        let Value::Closure { fn_def: fn_def_id, captures } = eval.values.lookup(closure_vid) else {
            eval.emit_not_callable(eval.values.type_of_value(closure_vid), callee_loc);
            return Ok(ValueId::ERROR);
        };

        let fn_def = eval.hir.fns[fn_def_id];
        let params = &eval.hir.fn_params[fn_def_id];
        let hir_captures = &eval.hir.fn_captures[fn_def_id];

        let arg_locals = &eval.hir.call_args[args];

        if params.len() != arg_locals.len() {
            todo!("diagnostic: function argument count mismatch");
        }

        let saved_bindings = self.capture_buf.use_as(|args| {
            for &local in arg_locals {
                args.push(self.bindings[local]);
            }

            let saved_bindings = std::mem::take(&mut self.bindings);

            for (capture_info, &capture) in hir_captures.iter().zip(captures) {
                self.bindings.insert(capture_info.inner_local, capture);
            }

            for (param, &(arg_value, arg_loc)) in params.iter().zip(args.iter()) {
                self.bindings.insert(param.value, (arg_value, arg_loc));
            }

            saved_bindings
        });

        self.interpret_block(eval, fn_def.type_preamble).expect("hir: preamble with return?");

        for param in params {
            let (expected_type_vid, expected_type_loc) = self.bindings[param.r#type];
            let Value::Type(expected_type) = eval.values.lookup(expected_type_vid) else {
                eval.emit_type_constraint_not_type(
                    eval.values.type_of_value(expected_type_vid),
                    expected_type_loc,
                );
                continue;
            };
            let (actual_arg_vid, actual_arg_loc) = self.bindings[param.value];
            let actual_type = eval.values.type_of_value(actual_arg_vid);
            if actual_type != expected_type {
                eval.emit_type_mismatch_error(
                    expected_type,
                    expected_type_loc,
                    actual_type,
                    actual_arg_loc,
                );
            }
        }

        let Err(ReturnValue(result)) = self.interpret_block(eval, fn_def.body) else {
            unreachable!("function body must end with Return instruction")
        };

        self.bindings = saved_bindings;
        Ok(result)
    }
}
