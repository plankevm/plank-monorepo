mod builtins;

#[cfg(test)]
mod tests;

use plank_core::{DenseIndexMap, Idx};
use plank_mir::{self as mir, Expr, Instruction, Mir};
use plank_values::{PrimitiveType, Type, TypeId, Value, ValueId, ValueInterner};
use sir_data::{
    self as sir, Branch, Control, EthIRProgram, Operation,
    builder::{BasicBlockBuilder, EthIRBuilder, FunctionBuilder},
    operation::{InlineOperands, OpExtraData, OperationKind, SetSmallConstData},
};
use std::collections::{HashMap, hash_map::Entry};

#[derive(Debug)]
struct LocalMap(HashMap<mir::LocalId, Vec<sir::LocalId>>);

impl LocalMap {
    fn new() -> Self {
        Self(HashMap::new())
    }

    fn reset(&mut self) {
        self.0.clear();
    }

    fn get(&self, local: mir::LocalId) -> &[sir::LocalId] {
        self.0.get(&local).map_or(&[] as &[_], Vec::as_slice)
    }

    fn get_or_create_single(
        &mut self,
        local: mir::LocalId,
        create: impl FnOnce() -> sir::LocalId,
    ) -> sir::LocalId {
        let &[single] = self.0.entry(local).or_insert_with(|| vec![create()]).as_slice() else {
            unreachable!("mistyped MIR: expected single local")
        };
        single
    }

    fn ensure_many(
        &mut self,
        local: mir::LocalId,
        mut create: impl FnMut() -> sir::LocalId,
        count: usize,
    ) {
        match self.0.entry(local) {
            Entry::Occupied(mapped) => assert_eq!(mapped.get().len(), count),
            Entry::Vacant(vacant) => {
                vacant.insert((0..count).map(|_| create()).collect());
            }
        }
    }
}

struct LowerCtx<'a> {
    mir: &'a Mir,

    mir_to_sir_functions: DenseIndexMap<mir::FnId, sir::FunctionId>,
    locals_map: LocalMap,

    locals_buf: Vec<sir::LocalId>,
}

impl LowerCtx<'_> {
    fn size_in_locals(&self, ty: TypeId) -> u32 {
        match self.mir.types.lookup(ty) {
            Type::Primitive(prim) => match prim {
                PrimitiveType::Void | PrimitiveType::Never => 0,
                PrimitiveType::Bool | PrimitiveType::U256 | PrimitiveType::MemoryPointer => 1,
                PrimitiveType::Function => unreachable!("function unsizeable in SIR"),
                PrimitiveType::Type => unreachable!("PrimitiveType unsizeable in SIR"),
            },
            Type::Struct(r#struct) => {
                r#struct.fields.iter().map(|&field| self.size_in_locals(field.ty)).sum()
            }
        }
    }
}

pub fn lower(mir: &Mir, values: &ValueInterner) -> EthIRProgram {
    let mut builder = EthIRBuilder::new();

    let mut ctx = LowerCtx {
        mir,

        mir_to_sir_functions: DenseIndexMap::with_capacity(mir.fns.len()),
        locals_map: LocalMap::new(),

        locals_buf: Vec::new(),
    };

    let init = lower_function(&mut ctx, values, &mut builder, mir.init);
    let run = mir.run.as_ref().map(|&run| lower_function(&mut ctx, values, &mut builder, run));

    builder.build(init, run)
}

fn lower_function(
    ctx: &mut LowerCtx<'_>,
    values: &ValueInterner,
    builder: &mut EthIRBuilder,
    mir_func: mir::FnId,
) -> sir::FunctionId {
    if let Some(&sir_func) = ctx.mir_to_sir_functions.get(mir_func) {
        return sir_func;
    }

    let fn_def = ctx.mir.fns[mir_func];
    ensure_block_func_deps_lowered(ctx, values, builder, fn_def.body);

    let mut new_func = builder.begin_function();
    ctx.locals_map.reset();

    for param in fn_def.iter_params() {
        let ty = ctx.mir.fn_locals[mir_func][param.idx()];
        let size = ctx.size_in_locals(ty);
        ctx.locals_map.ensure_many(param, || new_func.new_local(), size as usize);
    }

    let CFGSegment { bb_in: entry_bb_id, .. } =
        lower_basic_block(ctx, values, &mut new_func, mir_func, ctx.mir.fns[mir_func].body, true);
    let fn_id = new_func.finish(entry_bb_id);
    ctx.mir_to_sir_functions.insert(mir_func, fn_id);
    fn_id
}

struct CFGSegment {
    bb_in: sir::BasicBlockId,
    bb_out: sir::BasicBlockId,
    end_loose: bool,
}

fn lower_basic_block(
    ctx: &mut LowerCtx<'_>,
    values: &ValueInterner,
    fn_builder: &mut FunctionBuilder<'_>,
    mir_func: mir::FnId,
    block: mir::BlockId,
    is_entry: bool,
) -> CFGSegment {
    let mut current_bb = fn_builder.begin_basic_block();
    if is_entry {
        ctx.locals_buf.clear();
        for param in ctx.mir.fns[mir_func].iter_params() {
            ctx.locals_buf.extend(ctx.locals_map.get(param));
        }

        current_bb.set_inputs(&ctx.locals_buf);
    }

    let mut bb_in = None;

    for &instr in &ctx.mir.blocks[block] {
        match instr {
            Instruction::Set { target, expr } => match expr {
                Expr::Const(vid) => match values.lookup(vid) {
                    Value::Void => {}
                    Value::Bool(b) => {
                        let value = if b { 1u32 } else { 0u32 };
                        let sets =
                            ctx.locals_map.get_or_create_single(target, || current_bb.new_local());
                        current_bb.add_operation(Operation::SetSmallConst(SetSmallConstData {
                            sets,
                            value,
                        }));
                    }
                    Value::BigNum(x) => {
                        let sets =
                            ctx.locals_map.get_or_create_single(target, || current_bb.new_local());
                        current_bb.add_set_const_op(sets, x);
                    }

                    Value::StructVal { fields, ty } => {
                        let size = ctx.size_in_locals(ty);
                        ctx.locals_map.ensure_many(
                            target,
                            || current_bb.new_local(),
                            size as usize,
                        );
                        let mut locals = ctx.locals_map.get(target).iter().copied();
                        materialize_constant_struct_literal(
                            values,
                            &mut current_bb,
                            &mut locals,
                            fields,
                        )
                    }
                    Value::Type(_) | Value::Closure { .. } => {
                        unreachable!("comptime-only value in MIR")
                    }
                },
                Expr::LocalRef(mir_src) => {
                    let ty = ctx.mir.fn_locals[mir_func][mir_src.idx()];
                    if ctx.size_in_locals(ty) == 0 {
                        continue;
                    }
                    let src_sir_locals = ctx.locals_map.get(mir_src).len();
                    ctx.locals_map.ensure_many(target, || current_bb.new_local(), src_sir_locals);
                    for (src, dst) in
                        ctx.locals_map.get(mir_src).iter().zip(ctx.locals_map.get(target))
                    {
                        current_bb.add_operation(Operation::SetCopy(InlineOperands {
                            outs: [*dst],
                            ins: [*src],
                        }));
                    }
                }
                Expr::BuiltinCall { builtin, args } => {
                    let ty = ctx.mir.fn_locals[mir_func][target.idx()];
                    let output = (ctx.size_in_locals(ty) > 0).then(|| {
                        ctx.locals_map.get_or_create_single(target, || current_bb.new_local())
                    });

                    ctx.locals_buf.clear();
                    for &arg in &ctx.mir.args[args] {
                        let inputs = ctx.locals_map.get(arg);
                        ctx.locals_buf.extend(inputs);
                    }

                    let operation =
                        builtins::add_as_op(builtin, &ctx.locals_buf, output, &mut current_bb)
                            .expect("mistyped MIR");

                    if operation.is_terminating() {
                        let end_id = current_bb
                            .finish_terminating()
                            .expect("error despite `is_terminating` check");
                        return CFGSegment {
                            bb_in: bb_in.unwrap_or(end_id),
                            bb_out: end_id,
                            end_loose: false,
                        };
                    }
                }
                Expr::Call { callee, args } => {
                    let ret_type = ctx.mir.fns[callee].return_type;
                    ctx.locals_map.ensure_many(
                        target,
                        || current_bb.new_local(),
                        ctx.size_in_locals(ret_type) as usize,
                    );
                    ctx.locals_buf.clear();
                    for &arg in &ctx.mir.args[args] {
                        let inputs = ctx.locals_map.get(arg);
                        ctx.locals_buf.extend(inputs);
                    }
                    current_bb
                        .try_add_op(
                            OperationKind::InternalCall,
                            &ctx.locals_buf,
                            ctx.locals_map.get(target),
                            OpExtraData::FuncId(ctx.mir_to_sir_functions[callee]),
                        )
                        .expect("mir should guarantee valid construction");
                    if ret_type == TypeId::NEVER {
                        current_bb.add_operation(Operation::Invalid(()));
                        let end_id =
                            current_bb.finish_terminating().expect("error dispite invalid");
                        return CFGSegment {
                            bb_in: bb_in.unwrap_or(end_id),
                            bb_out: end_id,
                            end_loose: false,
                        };
                    }
                }
                Expr::StructLit { ty, fields } => {
                    lower_struct_literal(ctx, &mut current_bb, target, ty, fields);
                }
                Expr::FieldAccess { object, field_index } => {
                    lower_field_access(ctx, &mut current_bb, target, mir_func, object, field_index);
                }
            },
            Instruction::Return(local) => {
                current_bb.set_outputs(ctx.locals_map.get(local));
                let end_id = current_bb.finish_with_internal_return().expect("invalid MIR");
                return CFGSegment {
                    bb_in: bb_in.unwrap_or(end_id),
                    bb_out: end_id,
                    end_loose: false,
                };
            }
            Instruction::If { condition, then_block, else_block } => {
                let &[condition] = ctx.locals_map.get(condition) else {
                    unreachable!("invalid mir")
                };

                let last_end_id = current_bb.finish_with_placeholder_control();

                bb_in = bb_in.or(Some(last_end_id));
                let then = lower_basic_block(ctx, values, fn_builder, mir_func, then_block, false);
                let r#else =
                    lower_basic_block(ctx, values, fn_builder, mir_func, else_block, false);

                let mut continue_bb = fn_builder.begin_basic_block();
                continue_bb
                    .set_fn_control(
                        last_end_id,
                        Control::Branches(Branch {
                            condition,
                            non_zero_target: then.bb_in,
                            zero_target: r#else.bb_in,
                        }),
                    )
                    .unwrap();
                let merge_id = continue_bb.id();
                if then.end_loose {
                    continue_bb
                        .set_fn_control(then.bb_out, Control::ContinuesTo(merge_id))
                        .unwrap();
                }
                if r#else.end_loose {
                    continue_bb
                        .set_fn_control(r#else.bb_out, Control::ContinuesTo(merge_id))
                        .unwrap();
                }

                current_bb = continue_bb;
            }
            Instruction::While { condition_block, condition, body } => {
                // Purposefully invalid placeholder control.
                let loop_entry_id = current_bb.finish_with_placeholder_control();
                bb_in = bb_in.or(Some(loop_entry_id));

                let condition_segment =
                    lower_basic_block(ctx, values, fn_builder, mir_func, condition_block, false);
                let &[condition] = ctx.locals_map.get(condition) else {
                    unreachable!("invalid mir")
                };

                fn_builder
                    .set_control(loop_entry_id, Control::ContinuesTo(condition_segment.bb_in))
                    .unwrap();
                let body = lower_basic_block(ctx, values, fn_builder, mir_func, body, false);
                if body.end_loose {
                    fn_builder
                        .set_control(body.bb_out, Control::ContinuesTo(condition_segment.bb_in))
                        .unwrap();
                }

                let mut continue_bb = fn_builder.begin_basic_block();
                let continue_id = continue_bb.id();

                if condition_segment.end_loose {
                    continue_bb
                        .set_fn_control(
                            condition_segment.bb_out,
                            Control::Branches(Branch {
                                condition,
                                non_zero_target: body.bb_in,
                                zero_target: continue_id,
                            }),
                        )
                        .unwrap();
                }

                current_bb = continue_bb;
            }
        }
    }

    if is_entry {
        current_bb.add_operation(Operation::Invalid(()));
        let bb_out = current_bb.finish_terminating().expect("error despite invalid");
        return CFGSegment { bb_in: bb_in.unwrap_or(bb_out), bb_out, end_loose: false };
    }

    // For non entry segments the parent is responsible for hooking up control flow.
    let bb_out = current_bb.finish_with_placeholder_control();
    CFGSegment { bb_in: bb_in.unwrap_or(bb_out), bb_out, end_loose: true }
}

fn materialize_constant_struct_literal(
    values: &ValueInterner,
    bb: &mut BasicBlockBuilder<'_, '_>,
    targets: &mut impl Iterator<Item = sir::LocalId>,
    fields: &[ValueId],
) {
    for &field in fields {
        match values.lookup(field) {
            Value::Void => {}
            Value::Bool(b) => {
                let value = match b {
                    true => 1,
                    false => 0,
                };
                let sets = targets.next().expect("target count, size mismatch");
                bb.add_operation(Operation::SetSmallConst(SetSmallConstData { sets, value }));
            }
            Value::BigNum(x) => {
                let sets = targets.next().expect("target count, size mismatch");
                bb.add_set_const_op(sets, x);
            }
            Value::StructVal { ty: _, fields } => {
                materialize_constant_struct_literal(values, bb, targets, fields);
            }
            Value::Type(_) | Value::Closure { .. } => {
                unreachable!("MIR: comptime-only value")
            }
        }
    }
}

fn lower_struct_literal(
    ctx: &mut LowerCtx<'_>,
    bb: &mut BasicBlockBuilder<'_, '_>,
    target: mir::LocalId,
    struct_type: TypeId,
    fields: mir::ArgsId,
) {
    let size = ctx.size_in_locals(struct_type);
    if size == 0 {
        return;
    }
    ctx.locals_map.ensure_many(target, || bb.new_local(), size as usize);
    for (src, dst) in ctx.mir.args[fields]
        .iter()
        .flat_map(|src_local| ctx.locals_map.get(*src_local))
        .zip(ctx.locals_map.get(target))
    {
        bb.add_operation(Operation::SetCopy(InlineOperands { outs: [*dst], ins: [*src] }));
    }
}

fn lower_field_access(
    ctx: &mut LowerCtx<'_>,
    bb: &mut BasicBlockBuilder<'_, '_>,
    target: mir::LocalId,
    mir_func: mir::FnId,
    object: mir::LocalId,
    field_index: u32,
) {
    let object_type = ctx.mir.fn_locals[mir_func][object.idx()];
    let Type::Struct(r#struct) = ctx.mir.types.lookup(object_type) else {
        unreachable!("MIR invariant: field access on non-struct");
    };
    let target_field = r#struct.fields[field_index as usize];
    let size = ctx.size_in_locals(target_field.ty);
    if size == 0 {
        return;
    }

    let flattened_fields_offset = r#struct.fields[..field_index as usize]
        .iter()
        .map(|&field| ctx.size_in_locals(field.ty))
        .sum::<u32>() as usize;

    ctx.locals_map.ensure_many(target, || bb.new_local(), size as usize);

    for (src, dst) in ctx.locals_map.get(object)[flattened_fields_offset..][..size as usize]
        .iter()
        .zip(ctx.locals_map.get(target))
    {
        bb.add_operation(Operation::SetCopy(InlineOperands { outs: [*dst], ins: [*src] }));
    }
}

fn ensure_block_func_deps_lowered(
    ctx: &mut LowerCtx<'_>,
    values: &ValueInterner,
    builder: &mut EthIRBuilder,
    block: mir::BlockId,
) {
    for &instr in &ctx.mir.blocks[block] {
        match instr {
            Instruction::Set { target: _, expr } => {
                ensure_expr_func_deps_lowered(ctx, values, builder, expr);
            }
            Instruction::Return(_) => {}
            Instruction::If { condition: _, then_block, else_block } => {
                ensure_block_func_deps_lowered(ctx, values, builder, then_block);
                ensure_block_func_deps_lowered(ctx, values, builder, else_block);
            }
            Instruction::While { condition_block, condition: _, body } => {
                ensure_block_func_deps_lowered(ctx, values, builder, condition_block);
                ensure_block_func_deps_lowered(ctx, values, builder, body);
            }
        }
    }
}

fn ensure_expr_func_deps_lowered(
    ctx: &mut LowerCtx<'_>,
    values: &ValueInterner,
    builder: &mut EthIRBuilder,
    expr: mir::Expr,
) {
    if let Expr::Call { callee, args: _ } = expr {
        lower_function(ctx, values, builder, callee);
    }
}
