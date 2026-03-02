use sensei_core::{Idx, IndexVec, vec_buf::VecBuf};
use sensei_hir::{self as hir, ConstDef};
use sensei_parser::StrId;
use sensei_values::{TypeId, ValueId};

use crate::{Evaluator, value::Value};

#[derive(Debug)]
struct ReturnValue(ValueId);

#[derive(Default)]
pub(crate) struct Bindings(IndexVec<hir::LocalId, Option<ValueId>>);

impl Bindings {
    pub(crate) fn set(&mut self, local: hir::LocalId, value: ValueId) -> Option<ValueId> {
        if local.get() as usize >= self.0.len() {
            self.0.raw.resize(local.idx() + 1, None);
        }
        self.0[local].replace(value)
    }

    pub(crate) fn get(&self, local: hir::LocalId) -> ValueId {
        self.0[local].expect("hir: unbound local")
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = (hir::LocalId, ValueId)> + '_ {
        self.0.enumerate_idx().filter_map(|(id, opt)| opt.map(|vid| (id, vid)))
    }
}

pub(crate) struct ComptimeInterpreter<'e, 'hir> {
    eval: &'e mut Evaluator<'hir>,
    bindings: Bindings,

    value_buf: VecBuf<ValueId>,
    type_buf: VecBuf<TypeId>,
    name_buf: VecBuf<StrId>,
}

impl<'e, 'hir> ComptimeInterpreter<'e, 'hir> {
    fn new(eval: &'e mut Evaluator<'hir>) -> Self {
        const EST_MAX_FIELD_COUNT: usize = 64;
        Self {
            eval,
            bindings: Bindings::default(),
            value_buf: VecBuf::default(),
            type_buf: VecBuf::with_capacity(EST_MAX_FIELD_COUNT),
            name_buf: VecBuf::with_capacity(EST_MAX_FIELD_COUNT),
        }
    }

    pub fn eval_const(eval: &mut Evaluator<'hir>, const_def: ConstDef) -> ValueId {
        let mut comptime = ComptimeInterpreter::new(eval);
        comptime.interpret_block(const_def.body).expect("hir: const expr shouldn't have `return`");
        comptime.bindings.get(const_def.result)
    }

    pub fn eval_preamble_block(
        eval: &'e mut Evaluator<'hir>,
        bindings: Bindings,
        block_id: hir::BlockId,
    ) -> Bindings {
        let mut comptime = ComptimeInterpreter::new(eval);
        comptime.bindings = bindings;
        comptime.interpret_block(block_id).expect("hir: preamble shouldn't have `return`");
        comptime.bindings
    }

    fn interpret_block(&mut self, block_id: hir::BlockId) -> Result<(), ReturnValue> {
        for &instr in &self.eval.hir.blocks[block_id] {
            self.interpret_instruction(instr)?;
        }
        Ok(())
    }

    fn interpret_instruction(&mut self, instr: hir::Instruction) -> Result<(), ReturnValue> {
        match instr {
            hir::Instruction::Set { local, expr } => {
                let value = self.eval_expr(expr)?;
                if self.bindings.set(local, value).is_some() {
                    unreachable!("hir: overwriting with set");
                }
            }
            hir::Instruction::Eval(expr) => {
                self.eval_expr(expr)?;
            }
            hir::Instruction::Return(expr) => {
                let value = self.eval_expr(expr)?;
                return Err(ReturnValue(value));
            }
            hir::Instruction::AssertType { value, of_type } => {
                let type_vid = self.bindings.get(of_type);
                let Value::Type(expected_type) = self.eval.values.lookup(type_vid) else {
                    todo!("diagnostic: type error, value not type")
                };
                let value_vid = self.bindings.get(value);
                let actual_type = self.eval.values.type_of_value(value_vid);
                if actual_type != expected_type {
                    todo!("diagnostic: hir-ty-assert type mismatch");
                }
            }
            hir::Instruction::Assign { target, value } => {
                let new_value = self.eval_expr(value)?;
                let Some(prev_value) = self.bindings.set(target, new_value) else {
                    unreachable!("hir: init with assign")
                };
                if self.eval.values.type_of_value(new_value)
                    != self.eval.values.type_of_value(prev_value)
                {
                    todo!("diagnostic: assign type mismatch");
                }
            }
            hir::Instruction::If { condition, then_block, else_block } => {
                let cond_vid = self.bindings.get(condition);
                match self.eval.values.lookup(cond_vid) {
                    Value::Bool(true) => self.interpret_block(then_block)?,
                    Value::Bool(false) => self.interpret_block(else_block)?,
                    _ => todo!("diagnostic: type err, condition not bool"),
                }
            }
            hir::Instruction::While { .. } => {
                todo!("comptime while loops not yet implemented")
            }
        }
        Ok(())
    }

    fn eval_expr(&mut self, expr: hir::Expr) -> Result<ValueId, ReturnValue> {
        let value = match expr {
            hir::Expr::Void => ValueId::VOID,
            hir::Expr::Bool(false) => ValueId::FALSE,
            hir::Expr::Bool(true) => ValueId::TRUE,
            hir::Expr::BigNum(id) => self.eval.values.intern_num(id),
            hir::Expr::Type(type_id) => self.eval.values.intern_type(type_id),
            hir::Expr::ConstRef(const_id) => self.eval.ensure_const_evaluated(const_id),
            hir::Expr::LocalRef(local_id) => self.bindings.get(local_id),
            hir::Expr::FnDef(fn_def_id) => self.eval_fn_def(fn_def_id)?,
            hir::Expr::Call { callee, args } => self.eval_call(callee, args)?,
            hir::Expr::StructDef(struct_def_id) => self.eval_struct_def(struct_def_id)?,
            hir::Expr::StructLit { ty, fields } => self.eval_struct_lit(ty, fields)?,
            hir::Expr::Member { object, member } => self.eval_member(object, member)?,
        };
        Ok(value)
    }

    fn eval_fn_def(&mut self, fn_def: hir::FnDefId) -> Result<ValueId, ReturnValue> {
        let value_id = self.value_buf.use_as(|captures| {
            for capture in &self.eval.hir.fn_captures[fn_def] {
                captures.push(self.bindings.get(capture.outer_local));
            }
            let closure = Value::Closure { fn_def, captures };
            self.eval.values.intern(closure)
        });

        Ok(value_id)
    }

    fn eval_struct_def(&mut self, struct_def_id: hir::StructDefId) -> Result<ValueId, ReturnValue> {
        let struct_def = self.eval.hir.struct_defs[struct_def_id];
        let type_index_vid = self.bindings.get(struct_def.type_index);
        let fields_info = &self.eval.hir.fields[struct_def.fields];

        let struct_type_id = self.type_buf.use_as(|types| {
            self.name_buf.use_as(|names| {
                for field in fields_info {
                    let field_vid = self.bindings.get(field.value);
                    match self.eval.values.lookup(field_vid) {
                        Value::Type(tid) => {
                            types.push(tid);
                            names.push(field.name);
                        }
                        _ => todo!("diagnostic: struct field type must be Type"),
                    }
                }

                self.eval.types.intern(sensei_values::Type::Struct(sensei_values::StructInfo {
                    source: struct_def.source,
                    type_index: type_index_vid,
                    field_types: types,
                    field_names: names,
                }))
            })
        });

        Ok(self.eval.values.intern_type(struct_type_id))
    }

    fn eval_struct_lit(
        &mut self,
        ty: hir::LocalId,
        fields_id: hir::FieldsId,
    ) -> Result<ValueId, ReturnValue> {
        let type_vid = self.bindings.get(ty);
        let Value::Type(struct_type_id) = self.eval.values.lookup(type_vid) else {
            todo!("diagnostic: struct literal type must be Type")
        };

        let fields_info = &self.eval.hir.fields[fields_id];

        self.value_buf.use_as(|fields| {
            for field in fields_info {
                fields.push(self.bindings.get(field.value));
            }
            Ok(self.eval.values.intern(Value::StructVal { ty: struct_type_id, fields }))
        })
    }

    fn eval_member(
        &mut self,
        object: hir::LocalId,
        member: sensei_parser::StrId,
    ) -> Result<ValueId, ReturnValue> {
        let obj_vid = self.bindings.get(object);
        match self.eval.values.lookup(obj_vid) {
            Value::StructVal { ty, fields } => {
                let Some(field_index) = self.eval.types.field_index_by_name(ty, member) else {
                    todo!("diagnostic: unknown struct field");
                };
                Ok(fields[field_index as usize])
            }
            _ => todo!("diagnostic: member access on non-struct"),
        }
    }

    fn eval_call(
        &mut self,
        callee: hir::LocalId,
        args: hir::CallArgsId,
    ) -> Result<ValueId, ReturnValue> {
        let closure_vid = self.bindings.get(callee);
        let Value::Closure { fn_def: fn_def_id, captures } = self.eval.values.lookup(closure_vid)
        else {
            todo!("diagnostic: comptime call on non-function")
        };

        let fn_def = self.eval.hir.fns[fn_def_id];
        let params = &self.eval.hir.fn_params[fn_def_id];
        let hir_captures = &self.eval.hir.fn_captures[fn_def_id];

        let arg_locals = &self.eval.hir.call_args[args];

        if params.len() != arg_locals.len() {
            todo!("diagnostic: function argument count mismatch");
        }

        let saved_bindings = self.value_buf.use_as(|args| {
            for &local in arg_locals {
                args.push(self.bindings.get(local));
            }

            let saved_bindings = std::mem::take(&mut self.bindings);

            for (capture_info, capture) in hir_captures.iter().zip(captures) {
                self.bindings.set(capture_info.inner_local, *capture);
            }

            for (param, arg) in params.iter().zip(args) {
                self.bindings.set(param.value, *arg);
            }

            saved_bindings
        });

        self.interpret_block(fn_def.type_preamble).expect("hir: preamble with return?");

        let Err(ReturnValue(result)) = self.interpret_block(fn_def.body) else {
            unreachable!("function body must end with Return instruction")
        };

        self.bindings = saved_bindings;
        Ok(result)
    }
}
