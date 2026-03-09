use plank_core::{DenseIndexMap, vec_buf::VecBuf};
use plank_hir::{self as hir, ConstDef};
use plank_parser::StrId;
use plank_values::{TypeId, ValueId};

use crate::{Evaluator, value::Value};

#[derive(Debug)]
pub struct ReturnValue(ValueId);

pub(crate) struct ComptimeInterpreter {
    pub(crate) bindings: DenseIndexMap<hir::LocalId, ValueId>,

    value_buf: VecBuf<ValueId>,
    type_buf: VecBuf<TypeId>,
    name_buf: VecBuf<StrId>,
}

impl ComptimeInterpreter {
    pub fn new() -> Self {
        const EST_MAX_FIELD_COUNT: usize = 64;
        Self {
            bindings: DenseIndexMap::default(),
            value_buf: VecBuf::default(),
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
        self.bindings[const_def.result]
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
        match instr {
            hir::Instruction::Set { local, expr } => {
                let value = self.eval_expr(eval, expr)?;
                if self.bindings.insert(local, value).is_some() {
                    unreachable!("hir: overwriting with set");
                }
            }
            hir::Instruction::Eval(expr) => {
                self.eval_expr(eval, expr)?;
            }
            hir::Instruction::Return(expr) => {
                let value = self.eval_expr(eval, expr)?;
                return Err(ReturnValue(value));
            }
            hir::Instruction::AssertType { value, of_type } => {
                let type_vid = self.bindings[of_type];
                let Value::Type(expected_type) = eval.values.lookup(type_vid) else {
                    todo!("diagnostic: type error, value not type")
                };
                let value_vid = self.bindings[value];
                let actual_type = eval.values.type_of_value(value_vid);
                if !actual_type.is_assignable_to(expected_type) {
                    todo!("diagnostic: hir-ty-assert type mismatch");
                }
            }
            hir::Instruction::Assign { target, value } => {
                let new_value = self.eval_expr(eval, value)?;
                let Some(prev_value) = self.bindings.insert(target, new_value) else {
                    unreachable!("hir: init with assign")
                };
                let new_type = eval.values.type_of_value(new_value);
                let prev_type = eval.values.type_of_value(prev_value);
                if !new_type.is_assignable_to(prev_type) {
                    todo!("diagnostic: assign type mismatch");
                }
            }
            hir::Instruction::If { condition, then_block, else_block } => {
                let cond_vid = self.bindings[condition];
                match eval.values.lookup(cond_vid) {
                    Value::Bool(true) => self.interpret_block(eval, then_block)?,
                    Value::Bool(false) => self.interpret_block(eval, else_block)?,
                    _ => todo!("diagnostic: type err, condition not bool"),
                }
            }
            hir::Instruction::While { .. } => {
                todo!("comptime while loops not yet implemented")
            }
        }
        Ok(())
    }

    fn eval_expr(
        &mut self,
        eval: &mut Evaluator<'_>,
        expr: hir::Expr,
    ) -> Result<ValueId, ReturnValue> {
        let value = match expr {
            hir::Expr::Void => ValueId::VOID,
            hir::Expr::Bool(false) => ValueId::FALSE,
            hir::Expr::Bool(true) => ValueId::TRUE,
            hir::Expr::BigNum(id) => eval.values.intern_num(id),
            hir::Expr::Type(type_id) => eval.values.intern_type(type_id),
            hir::Expr::ConstRef(const_id) => eval.ensure_const_evaluated(self, const_id),
            hir::Expr::LocalRef(local_id) => self.bindings[local_id],
            hir::Expr::FnDef(fn_def_id) => self.eval_fn_def(eval, fn_def_id)?,
            hir::Expr::Call { callee, args } => self.eval_call(eval, callee, args)?,
            hir::Expr::StructDef(struct_def_id) => self.eval_struct_def(eval, struct_def_id)?,
            hir::Expr::StructLit { ty, fields } => self.eval_struct_lit(eval, ty, fields)?,
            hir::Expr::Member { object, member } => self.eval_member(eval, object, member)?,
            hir::Expr::BuiltinCall { .. } => todo!("comptime builtin eval not yet implemented"),
        };
        Ok(value)
    }

    fn eval_fn_def(
        &mut self,
        eval: &mut Evaluator<'_>,
        fn_def: hir::FnDefId,
    ) -> Result<ValueId, ReturnValue> {
        let value_id = self.value_buf.use_as(|captures| {
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
        let type_index_vid = self.bindings[struct_def.type_index];
        let fields_info = &eval.hir.fields[struct_def.fields];

        let struct_type_id = self.type_buf.use_as(|types| {
            self.name_buf.use_as(|names| {
                for field in fields_info {
                    let field_vid = self.bindings[field.value];
                    match eval.values.lookup(field_vid) {
                        Value::Type(tid) => {
                            types.push(tid);
                            names.push(field.name);
                        }
                        _ => todo!("diagnostic: struct field type must be Type"),
                    }
                }

                eval.types.intern(plank_values::Type::Struct(plank_values::StructInfo {
                    source: struct_def.source,
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
        let type_vid = self.bindings[ty];
        let Value::Type(struct_type_id) = eval.values.lookup(type_vid) else {
            todo!("diagnostic: struct literal type must be Type")
        };
        let plank_values::Type::Struct(r#struct) = eval.types.lookup(struct_type_id) else {
            todo!("diagnostic: struct type not struct");
        };

        let fields_info = &eval.hir.fields[fields_id];

        for (i, field) in fields_info.iter().enumerate() {
            let Some(field_pos) = r#struct.field_names.iter().position(|&name| name == field.name)
            else {
                todo!("diagnostic: struct _ has no field named _");
            };
            if fields_info[..i].iter().any(|f| f.name == field.name) {
                todo!("diagnostic: duplicate struct field assignment");
            }
            let field_value_vid = self.bindings[field.value];
            let field_value_ty = eval.values.type_of_value(field_value_vid);
            if !field_value_ty.is_assignable_to(r#struct.field_types[field_pos]) {
                todo!("diagnostic: field type mismatch");
            }
        }

        self.value_buf.use_as(|fields| {
            for &field_name in r#struct.field_names {
                let Some(&field) = fields_info.iter().find(|field| field.name == field_name) else {
                    todo!("diagnostic: literal missing struct field");
                };
                fields.push(self.bindings[field.value]);
            }
            Ok(eval.values.intern(Value::StructVal { ty: struct_type_id, fields }))
        })
    }

    fn eval_member(
        &mut self,
        eval: &mut Evaluator<'_>,
        object: hir::LocalId,
        member: plank_parser::StrId,
    ) -> Result<ValueId, ReturnValue> {
        let obj_vid = self.bindings[object];
        match eval.values.lookup(obj_vid) {
            Value::StructVal { ty, fields } => {
                let Some(field_index) = eval.types.field_index_by_name(ty, member) else {
                    todo!("diagnostic: unknown struct field");
                };
                Ok(fields[field_index as usize])
            }
            _ => todo!("diagnostic: member access on non-struct"),
        }
    }

    fn eval_call(
        &mut self,
        eval: &mut Evaluator<'_>,
        callee: hir::LocalId,
        args: hir::CallArgsId,
    ) -> Result<ValueId, ReturnValue> {
        let closure_vid = self.bindings[callee];
        let Value::Closure { fn_def: fn_def_id, captures } = eval.values.lookup(closure_vid) else {
            todo!("diagnostic: comptime call on non-function")
        };

        let fn_def = eval.hir.fns[fn_def_id];
        let params = &eval.hir.fn_params[fn_def_id];
        let hir_captures = &eval.hir.fn_captures[fn_def_id];

        let arg_locals = &eval.hir.call_args[args];

        if params.len() != arg_locals.len() {
            todo!("diagnostic: function argument count mismatch");
        }

        let saved_bindings = self.value_buf.use_as(|args| {
            for &local in arg_locals {
                args.push(self.bindings[local]);
            }

            let saved_bindings = std::mem::take(&mut self.bindings);

            for (capture_info, capture) in hir_captures.iter().zip(captures) {
                self.bindings.insert(capture_info.inner_local, *capture);
            }

            for (param, arg) in params.iter().zip(args) {
                self.bindings.insert(param.value, *arg);
            }

            saved_bindings
        });

        self.interpret_block(eval, fn_def.type_preamble).expect("hir: preamble with return?");

        for param in params {
            let expected_type_vid = self.bindings[param.r#type];
            let Value::Type(expected_type) = eval.values.lookup(expected_type_vid) else {
                todo!("diagnostic: param type must be Type")
            };
            let actual_arg_vid = self.bindings[param.value];
            let actual_type = eval.values.type_of_value(actual_arg_vid);
            if actual_type != expected_type {
                todo!("diagnostic: comptime call argument type mismatch");
            }
        }

        let Err(ReturnValue(result)) = self.interpret_block(eval, fn_def.body) else {
            unreachable!("function body must end with Return instruction")
        };

        self.bindings = saved_bindings;
        Ok(result)
    }
}
