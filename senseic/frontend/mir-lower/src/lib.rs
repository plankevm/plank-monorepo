mod builtins;

#[cfg(test)]
mod tests;

use sensei_core::{DenseIndexMap, DenseIndexSet, Idx};
use sensei_hir::BigNumInterner;
use sensei_mir::{self as mir, Expr, Instruction, Mir};
use sensei_values::{Type, TypeId};
use sir_data::{
    self as sir, Control, EthIRProgram, Operation, Span,
    builder::{EthIRBuilder, FunctionBuilder},
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
        self.0[&local].as_slice()
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

    fn verify_or_create_many(
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

struct LowerCtx<'mir> {
    mir: &'mir Mir,
    big_nums: &'mir BigNumInterner,

    mir_to_sir_functions: DenseIndexMap<mir::FnId, sir::FunctionId>,
    entered_functions: DenseIndexSet<mir::FnId>,
    locals_map: LocalMap,

    locals_buf: Vec<sir::LocalId>,
}

impl LowerCtx<'_> {
    fn size_in_locals(&self, ty: TypeId) -> u32 {
        match self.mir.types.lookup(ty) {
            Type::Void => 0,
            Type::Bool | Type::Int | Type::MemoryPointer => 1,
            Type::Function => panic!("function unsizeable in SIR"),
            Type::Type => panic!("type unsizeable in SIR"),
            Type::Struct(r#struct) => {
                r#struct.field_types.iter().map(|&ty| self.size_in_locals(ty)).sum()
            }
        }
    }
}

pub fn lower(mir: &Mir, big_nums: &BigNumInterner) -> EthIRProgram {
    let mut builder = EthIRBuilder::new();

    let mut ctx = LowerCtx {
        mir,
        big_nums,

        mir_to_sir_functions: DenseIndexMap::with_capacity(mir.fns.len()),
        entered_functions: DenseIndexSet::with_capacity_in_bits(mir.fns.len()),
        locals_map: LocalMap::new(),

        locals_buf: Vec::new(),
    };

    let init = lower_function(&mut ctx, &mut builder, mir.init);
    let run = mir.run.as_ref().map(|&run| lower_function(&mut ctx, &mut builder, run));

    builder.build(init, run)
}

fn lower_function(
    ctx: &mut LowerCtx<'_>,
    builder: &mut EthIRBuilder,
    mir_func: mir::FnId,
) -> sir::FunctionId {
    if let Some(&sir_func) = ctx.mir_to_sir_functions.get(mir_func) {
        return sir_func;
    }

    if !ctx.entered_functions.add(mir_func) {
        todo!("diagnostic: cyclic call graph");
    }
    let fn_def = ctx.mir.fns[mir_func];
    ensure_block_func_deps_lowered(ctx, builder, fn_def.body);
    ctx.entered_functions.remove(mir_func);

    let mut new_func = builder.begin_function();
    ctx.locals_map.reset();

    for param in fn_def.iter_params() {
        let ty = ctx.mir.fn_locals[mir_func][param.idx()];
        let size = ctx.size_in_locals(ty);
        ctx.locals_map.verify_or_create_many(param, || new_func.new_local(), size as usize);
    }

    let entry_bb_id =
        lower_basic_block(ctx, &mut new_func, mir_func, ctx.mir.fns[mir_func].body, true);
    let fn_id = new_func.finish(entry_bb_id);
    ctx.mir_to_sir_functions.insert(mir_func, fn_id);
    return fn_id;
}

fn lower_basic_block(
    ctx: &mut LowerCtx<'_>,
    fn_builder: &mut FunctionBuilder<'_>,
    mir_func: mir::FnId,
    block: mir::BlockId,
    is_entry: bool,
) -> sir::BasicBlockId {
    let mut current_bb = fn_builder.begin_basic_block();
    if is_entry {
        ctx.locals_buf.clear();
        for param in ctx.mir.fns[mir_func].iter_params() {
            ctx.locals_buf.extend(ctx.locals_map.get(param));
        }

        current_bb.set_inputs(&ctx.locals_buf);
    }

    for &instr in &ctx.mir.blocks[block] {
        match instr {
            Instruction::Set { target, value } | Instruction::Assign { target, value } => {
                match value {
                    Expr::Void => {}
                    Expr::Bool(b) => {
                        let value = if b { 1u32 } else { 0u32 };
                        let sets =
                            ctx.locals_map.get_or_create_single(target, || current_bb.new_local());
                        current_bb.add_operation(Operation::SetSmallConst(SetSmallConstData {
                            sets,
                            value,
                        }));
                    }
                    Expr::BigNum(id) => {
                        let sets =
                            ctx.locals_map.get_or_create_single(target, || current_bb.new_local());
                        let value = ctx.big_nums.lookup(id);
                        current_bb.add_set_const_op(sets, value);
                    }
                    Expr::LocalRef(mir_src) => {
                        println!("mir_src: {:?}", mir_src);
                        println!("ctx.locals_map: {:?}", ctx.locals_map);
                        let src_sir_locals = ctx.locals_map.get(mir_src).len();
                        ctx.locals_map.verify_or_create_many(
                            target,
                            || current_bb.new_local(),
                            src_sir_locals,
                        );
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
                        let output = (ty != TypeId::VOID).then(|| {
                            ctx.locals_map.get_or_create_single(target, || current_bb.new_local())
                        });

                        ctx.locals_buf.clear();
                        for &arg in &ctx.mir.args[args] {
                            let inputs = ctx.locals_map.get(arg);
                            ctx.locals_buf.extend(inputs);
                        }

                        let operation = builtins::builtin_to_operation(
                            builtin,
                            &ctx.locals_buf,
                            output,
                            current_bb.fn_builder.ir_builder,
                        )
                        .expect("mistyped MIR");
                        current_bb.add_operation(operation);

                        if operation.kind().is_terminating() {
                            return current_bb.finish(Control::LastOpTerminates).unwrap();
                        }
                    }
                    Expr::Call { callee, args } => {
                        let ret_type = ctx.mir.fns[callee].return_type;
                        ctx.locals_map.verify_or_create_many(
                            target,
                            || current_bb.new_local(),
                            ctx.size_in_locals(ret_type) as usize,
                        );
                        ctx.locals_buf.clear();
                        for &arg in &ctx.mir.args[args] {
                            let inputs = ctx.locals_map.get(arg);
                            ctx.locals_buf.extend(inputs);
                        }
                        let icall = Operation::try_build(
                            OperationKind::InternalCall,
                            &ctx.locals_buf,
                            ctx.locals_map.get(target),
                            OpExtraData::FuncId(ctx.mir_to_sir_functions[callee]),
                            current_bb.fn_builder.ir_builder,
                        )
                        .expect("MIR structure should guarantee valid construction");
                        current_bb.add_operation(icall);
                    }
                    other => todo!("set: {other:#?}"),
                }
            }
            Instruction::Return(local) => {
                current_bb.set_outputs(ctx.locals_map.get(local));
                return current_bb.finish(Control::InternalReturn).unwrap();
            }
            other => todo!("instr: {other:#?}"),
        }
    }

    unreachable!("malformed MIR, missing explicit terminator");
}

fn ensure_block_func_deps_lowered(
    ctx: &mut LowerCtx<'_>,
    builder: &mut EthIRBuilder,
    block: mir::BlockId,
) {
    for &instr in &ctx.mir.blocks[block] {
        match instr {
            Instruction::Set { target: _, value: expr } => {
                ensure_expr_func_deps_lowered(ctx, builder, expr);
            }
            Instruction::Assign { target: _, value } => {
                ensure_expr_func_deps_lowered(ctx, builder, value);
            }
            Instruction::Return(_) => {}
            Instruction::If { condition: _, then_block, else_block } => {
                ensure_block_func_deps_lowered(ctx, builder, then_block);
                ensure_block_func_deps_lowered(ctx, builder, else_block);
            }
            Instruction::While { condition_block, condition: _, body } => {
                ensure_block_func_deps_lowered(ctx, builder, condition_block);
                ensure_block_func_deps_lowered(ctx, builder, body);
            }
        }
    }
}

fn ensure_expr_func_deps_lowered(
    ctx: &mut LowerCtx<'_>,
    builder: &mut EthIRBuilder,
    expr: mir::Expr,
) {
    if let Expr::Call { callee, args: _ } = expr {
        lower_function(ctx, builder, callee);
    }
}
