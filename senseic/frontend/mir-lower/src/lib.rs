mod builtins;

#[cfg(test)]
mod tests;

use sensei_core::{DenseIndexMap, DenseIndexSet, Idx};
use sensei_hir::BigNumInterner;
use sensei_mir::{self as mir, Expr, Instruction, Mir};
use sensei_values::TypeId;
use sir_data::{
    self as sir, Control, EthIRProgram, Operation,
    builder::{EthIRBuilder, FunctionBuilder},
    operation::{InlineOperands, SetSmallConstData},
};
use std::collections::HashMap;

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
}

struct LowerCtx<'mir> {
    mir: &'mir Mir,
    big_nums: &'mir BigNumInterner,

    mir_to_sir_functions: DenseIndexMap<mir::FnId, sir::FunctionId>,
    entered_functions: DenseIndexSet<mir::FnId>,
    locals_map: LocalMap,

    locals_buf: Vec<sir::LocalId>,
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
    ensure_block_func_deps_lowered(ctx, builder, ctx.mir.fns[mir_func].body);
    ctx.entered_functions.remove(mir_func);

    let mut new_func = builder.begin_function();
    ctx.locals_map.reset();

    let entry_bb_id = lower_basic_block(ctx, &mut new_func, mir_func, ctx.mir.fns[mir_func].body);
    let fn_id = new_func.finish(entry_bb_id);
    ctx.mir_to_sir_functions.insert(mir_func, fn_id);
    return fn_id;
}

fn lower_basic_block(
    ctx: &mut LowerCtx<'_>,
    fn_builder: &mut FunctionBuilder<'_>,
    mir_func: mir::FnId,
    block: mir::BlockId,
) -> sir::BasicBlockId {
    let mut current_bb = fn_builder.begin_basic_block();

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
                        let src_sir_locals = ctx.locals_map.0[&mir_src].len();
                        let dst_sir_locals = ctx
                            .locals_map
                            .0
                            .entry(target)
                            .or_insert_with(|| {
                                (0..src_sir_locals).map(|_| current_bb.new_local()).collect()
                            })
                            .len();
                        assert_eq!(src_sir_locals, dst_sir_locals);
                        for (src, dst) in
                            ctx.locals_map.0[&mir_src].iter().zip(&ctx.locals_map.0[&target])
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

                        assert!(ctx.locals_buf.is_empty());
                        for &arg in &ctx.mir.args[args] {
                            let input =
                                ctx.locals_map.get_or_create_single(arg, || current_bb.new_local());
                            ctx.locals_buf.push(input);
                        }

                        let operation = builtins::builtin_to_operation(
                            builtin,
                            &ctx.locals_buf,
                            output,
                            current_bb.fn_builder.ir_builder,
                        )
                        .expect("mistyped MIR");
                        current_bb.add_operation(operation);
                        ctx.locals_buf.clear();

                        if operation.kind().is_terminating() {
                            return current_bb.finish(Control::LastOpTerminates).unwrap();
                        }
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
