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

#[derive(Default)]
struct LocalState {
    /// Every HIR local optionally maps to a MIR local.
    /// Once set, never changes (the MIR local ID is stable).
    hir_to_mir: DenseIndexMap<hir::LocalId, mir::LocalId>,

    /// Every HIR local optionally has a comptime-known value.
    /// Can be set and later cleared (e.g., after runtime if/else).
    comptime_value: DenseIndexMap<hir::LocalId, ValueId>,

    /// The concrete type of each MIR local. Stored separately so it can
    /// pre-allocated for if/else results, filled in on first Set.
    mir_type: DenseIndexMap<mir::LocalId, TypeId>,
}

struct FunctionLowerScope {
    expected_return_type: TypeId,

    instructions_buf: Vec<mir::Instruction>,
}

enum ExprResult {
    Runtime(mir::Expr),
    Comptime { expr: mir::Expr, value: ValueId },
    ComptimeOnly(ValueId),
}

impl FunctionLowerScope {
    fn translate_expr(&mut self, expr: hir::Expr) -> ExprResult {
        match expr {
            hir::Expr::Bool(b) => {
                let value = if b { ValueId::TRUE } else { ValueId::FALSE };
                ExprResult::Comptime { expr: mir::Expr::Bool(b), value }
            }
            hir::Expr::Bool(false) => todo!(),
            other => todo!("{other:?}"),
        }
    }

    fn translate_block(&mut self, eval: &mut Evaluator<'_>, block: hir::BlockId) -> mir::BlockId {
        let instr_start = self.instructions_buf.len();

        for &instr in &eval.hir.blocks[block] {
            match instr {
                hir::Instruction::Set { local, expr } => {
                    let (expr, value) = self.translate_expr(expr);
                }
                other => todo!("{other:?}"),
            }
        }

        eval.mir_blocks.push_iter(self.instructions_buf.drain(instr_start..))
    }
}

pub(crate) fn lower_entry_point_as_fn(
    eval: &mut Evaluator<'_>,
    hir_block: hir::BlockId,
) -> mir::FnId {
    let mut scope = FunctionLowerScope {
        expected_return_type: TypeId::NEVER,
        instructions_buf: Vec::with_capacity(INSTRUCTION_BUF_CAPACITY),
    };

    let body = scope.translate_block(eval, hir_block);

    eval.mir_fns.push(mir::FnDef {
        body: mir::BlockId::ZERO,
        param_count: 0,
        return_type: TypeId::NEVER,
    })
}
