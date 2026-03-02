use hashbrown::HashMap;
use sensei_core::{IndexVec, index_vec, list_of_lists::ListOfLists};
use sensei_hir::{ConstId, Hir};
use sensei_mir::{self as mir, Mir};
use sensei_values::{TypeId, TypeInterner, ValueId};

use comptime::ComptimeInterpreter;

mod comptime;
mod lower;
mod value;

use value::ValueInterner;

#[derive(Clone)]
enum ConstState {
    NotEvaluated,
    InProgress,
    Evaluated(ValueId),
}

pub(crate) struct Evaluator<'hir> {
    pub hir: &'hir Hir,
    pub values: ValueInterner,
    pub types: TypeInterner,
    const_states: IndexVec<ConstId, ConstState>,
    pub mir_blocks: ListOfLists<mir::BlockId, mir::Instruction>,
    pub mir_fns: IndexVec<mir::FnId, mir::FnDef>,
    pub mir_fn_locals: ListOfLists<mir::FnId, TypeId>,
    pub mir_args: ListOfLists<mir::ArgsId, mir::LocalId>,
    pub fn_cache: HashMap<ValueId, mir::FnId>,
}

impl<'hir> Evaluator<'hir> {
    fn new(hir: &'hir Hir) -> Self {
        let const_count = hir.consts.const_defs.len();
        Self {
            hir,
            values: ValueInterner::new(),
            types: TypeInterner::new(),
            const_states: index_vec![ConstState::NotEvaluated; const_count],
            mir_blocks: ListOfLists::new(),
            mir_fns: IndexVec::new(),
            mir_fn_locals: ListOfLists::new(),
            mir_args: ListOfLists::new(),
            fn_cache: HashMap::new(),
        }
    }

    pub fn ensure_const_evaluated(&mut self, const_id: ConstId) -> ValueId {
        match self.const_states[const_id] {
            ConstState::Evaluated(value_id) => value_id,
            ConstState::InProgress => todo!("diagnostic: cyclical const dependency"),
            ConstState::NotEvaluated => {
                self.const_states[const_id] = ConstState::InProgress;
                let const_def = self.hir.consts.const_defs[const_id];
                let value_id = ComptimeInterpreter::eval_const(self, const_def);
                self.const_states[const_id] = ConstState::Evaluated(value_id);
                value_id
            }
        }
    }
}

pub fn evaluate(hir: &Hir) -> Mir {
    let mut eval = Evaluator::new(hir);

    for const_id in hir.consts.const_defs.iter_idx() {
        eval.ensure_const_evaluated(const_id);
    }

    let init = lower::lower_block_as_fn(&mut eval, hir.init);
    let run = hir.run.map(|block| lower::lower_block_as_fn(&mut eval, block));

    Mir {
        blocks: eval.mir_blocks,
        args: eval.mir_args,
        fns: eval.mir_fns,
        fn_locals: eval.mir_fn_locals,
        types: eval.types,
        init,
        run,
    }
}
