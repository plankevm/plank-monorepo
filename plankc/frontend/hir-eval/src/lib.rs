use std::cell::RefCell;

use hashbrown::HashMap;
use plank_core::{IndexVec, index_vec, list_of_lists::ListOfLists};
use plank_hir::{ConstId, Hir};
use plank_mir::{self as mir, Mir};
use plank_session::{Session, StrId};
use plank_values::{BigNumInterner, TypeId, TypeInterner, ValueId};

use comptime::ComptimeInterpreter;

mod builtins;
mod comptime;
mod diagnostics;
mod local_state;
mod lower;
mod value;

#[cfg(test)]
mod tests;

use value::{Value, ValueInterner};

#[derive(Clone)]
enum ConstState {
    NotEvaluated,
    InProgress,
    Evaluated(ValueId),
}

pub(crate) struct Evaluator<'a> {
    pub hir: &'a Hir,
    pub session: RefCell<&'a mut Session>,
    pub big_nums: &'a mut BigNumInterner,
    pub values: ValueInterner,
    pub types: TypeInterner,
    const_states: IndexVec<ConstId, ConstState>,
    pub mir_blocks: ListOfLists<mir::BlockId, mir::Instruction>,
    pub mir_fns: IndexVec<mir::FnId, mir::FnDef>,
    pub mir_fn_locals: ListOfLists<mir::FnId, TypeId>,
    pub mir_args: ListOfLists<mir::ArgsId, mir::LocalId>,
    pub fn_cache: HashMap<ValueId, mir::FnId>,
}

impl<'a> Evaluator<'a> {
    fn new(hir: &'a Hir, big_nums: &'a mut BigNumInterner, session: &'a mut Session) -> Self {
        let const_count = hir.consts.len();
        Self {
            hir,
            session: RefCell::new(session),
            big_nums,
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

    pub fn push_error_fn(&mut self) -> mir::FnId {
        let body = self.mir_blocks.push_iter(std::iter::empty());
        let fn_id1 = self.mir_fn_locals.push_iter(std::iter::empty());
        let fn_id2 =
            self.mir_fns.push(mir::FnDef { body, param_count: 0, return_type: TypeId::ERROR });
        assert_eq!(fn_id1, fn_id2);
        fn_id1
    }

    pub fn ensure_const_evaluated(
        &mut self,
        interpreter: &mut ComptimeInterpreter,
        const_id: ConstId,
    ) -> ValueId {
        match self.const_states[const_id] {
            ConstState::Evaluated(value_id) => value_id,
            ConstState::InProgress => todo!("diagnostic: cyclical const dependency"),
            ConstState::NotEvaluated => {
                self.const_states[const_id] = ConstState::InProgress;
                let const_def = self.hir.consts[const_id];
                let saved = std::mem::take(&mut interpreter.bindings);
                let value_id = interpreter.eval_const(self, const_def);
                interpreter.bindings = saved;
                self.const_states[const_id] = ConstState::Evaluated(value_id);
                self.try_name_type(value_id, const_def.name);
                value_id
            }
        }
    }

    pub fn get_const(&self, const_id: ConstId) -> ValueId {
        match self.const_states[const_id] {
            ConstState::Evaluated(value_id) => value_id,
            _ => unreachable!("all consts are evaluated before lowering"),
        }
    }

    fn try_name_type(&mut self, value_id: ValueId, name: StrId) {
        if let Value::Type(type_id) = self.values.lookup(value_id) {
            self.types.try_set_struct_name(type_id, name);
        }
    }
}

pub fn evaluate(hir: &Hir, big_nums: &mut BigNumInterner, session: &mut Session) -> Mir {
    let mut eval = Evaluator::new(hir, big_nums, session);

    let mut interpreter = ComptimeInterpreter::new();
    for const_id in hir.consts.iter_idx() {
        eval.ensure_const_evaluated(&mut interpreter, const_id);
    }

    let init = lower::lower_entry_point_as_fn(&mut eval, hir.init);
    let run = hir.run.map(|block| lower::lower_entry_point_as_fn(&mut eval, block));

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
