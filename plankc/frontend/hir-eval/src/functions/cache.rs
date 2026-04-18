use std::cell::{Cell, UnsafeCell};

use hashbrown::{DefaultHashBuilder, HashTable, hash_table::Entry};
use plank_core::{
    IndexVec, chunked_arena::ChunkedArena, list_of_lists::ListOfLists, newtype_index,
};
use plank_hir::ValueId;
use plank_mir as mir;
use plank_session::{MaybePoisoned, Poisoned};

use crate::evaluator::State;

newtype_index! {
    pub(crate) struct LoweredFnIdx;
}

pub(crate) type Param = MaybePoisoned<ValueId>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct FunctionKey<'a> {
    pub closure: ValueId,
    pub params: &'a [Param],
}

impl<'a> FunctionKey<'a> {
    pub fn new(closure: ValueId, comptime_params: &'a [Param]) -> Self {
        Self { closure, params: comptime_params }
    }
}

struct LoweredFn {
    state: State<MaybePoisoned<mir::FnId>>,
    closure: ValueId,
}

pub(crate) struct LoweredFunctionsCache {
    functions: IndexVec<LoweredFnIdx, LoweredFn>,
    comptime_params: ListOfLists<LoweredFnIdx, Param>,
    dedup: HashTable<LoweredFnIdx>,
    hasher: DefaultHashBuilder,
}

impl LoweredFunctionsCache {
    pub fn new() -> Self {
        Self {
            functions: IndexVec::new(),
            comptime_params: ListOfLists::new(),
            dedup: HashTable::new(),
            hasher: DefaultHashBuilder::default(),
        }
    }

    pub fn try_set_lowered(
        &mut self,
        id: LoweredFnIdx,
        fn_id: MaybePoisoned<mir::FnId>,
    ) -> MaybePoisoned<mir::FnId> {
        match &mut self.functions[id].state {
            State::Done(Err(Poisoned)) => Err(Poisoned),
            State::Done(Ok(_)) => {
                unreachable!("invariant: cache corrupted")
            }
            s @ State::InProgress => {
                *s = State::Done(fn_id);
                fn_id
            }
        }
    }

    pub fn retrieve_or_create_entry<'a>(
        &mut self,
        func: FunctionKey<'a>,
    ) -> Result<&mut State<MaybePoisoned<mir::FnId>>, LoweredFnIdx> {
        use std::hash::BuildHasher;
        let hash = self.hasher.hash_one(func);
        let entry = self.dedup.entry(
            hash,
            |&idx| {
                let closure = self.functions[idx].closure;
                closure == func.closure && func.params == &self.comptime_params[idx]
            },
            |&idx| {
                let closure = self.functions[idx].closure;
                let comptime_params = &self.comptime_params[idx];
                self.hasher.hash_one(FunctionKey { closure, params: comptime_params })
            },
        );
        match entry {
            Entry::Occupied(occupied) => {
                let id = *occupied.get();
                Ok(&mut self.functions[id].state)
            }
            Entry::Vacant(vacant) => {
                let new_entry_id = self
                    .functions
                    .push(LoweredFn { state: State::InProgress, closure: func.closure });
                let id2 = self.comptime_params.push_copy_slice(func.params);
                assert_eq!(new_entry_id, id2);
                vacant.insert(new_entry_id);
                Err(new_entry_id)
            }
        }
    }
}

struct EvaluatedHeader {
    result: Cell<State<MaybePoisoned<ValueId>>>,
    closure: ValueId,
    params: u32,
}

pub(crate) struct EvaluatedFn<'a> {
    pub result: &'a Cell<State<MaybePoisoned<ValueId>>>,
    pub closure: ValueId,
    pub params: &'a [Param],
}

impl EvaluatedFn<'_> {
    pub fn key(&self) -> FunctionKey<'_> {
        FunctionKey { closure: self.closure, params: self.params }
    }
}

const EVALUATED_ELEMENT_ALIGN: usize = align_of::<(EvaluatedHeader, [Param; 1])>();
const HEADER_TO_PARAMS_OFFSET: usize =
    size_of::<EvaluatedHeader>().next_multiple_of(align_of::<Param>());

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct EvaledRef(u32);

pub(crate) struct EvaluatedFunctionCache {
    arena: ChunkedArena<EVALUATED_ELEMENT_ALIGN>,
    dedup: UnsafeCell<HashTable<EvaledRef>>,
    hasher: DefaultHashBuilder,
}

impl EvaluatedFunctionCache {
    pub fn new() -> Self {
        Self {
            arena: ChunkedArena::new(),
            dedup: UnsafeCell::new(HashTable::new()),
            hasher: DefaultHashBuilder::default(),
        }
    }

    pub fn lookup<'s, 'k>(
        &'s self,
        key: FunctionKey<'k>,
    ) -> Result<&'s Cell<State<MaybePoisoned<ValueId>>>, EvaluatedFn<'s>> {
        use std::hash::BuildHasher;
        let hash = self.hasher.hash_one(key);
        let dedup = unsafe { &mut *self.dedup.get() };
        let entry = dedup.entry(
            hash,
            |&evaled_ref| self.get(evaled_ref).key() == key,
            |&evaled_ref| self.hasher.hash_one(self.get(evaled_ref).key()),
        );
        match entry {
            Entry::Occupied(occupied) => {
                let evaluated = self.get(*occupied.get());
                Ok(evaluated.result)
            }
            Entry::Vacant(vacant) => unsafe {
                let min_size = HEADER_TO_PARAMS_OFFSET + std::mem::size_of_val(key.params);
                let (eval_ref_offset, new_eval_ptr) = self.arena.alloc_append(min_size);
                vacant.insert(EvaledRef(eval_ref_offset));

                let header = new_eval_ptr as *mut EvaluatedHeader;
                let params = key.params.len() as u32;
                header.write(EvaluatedHeader {
                    result: Cell::new(State::InProgress),
                    closure: key.closure,
                    params,
                });
                let params_start = header.byte_add(HEADER_TO_PARAMS_OFFSET) as *mut Param;
                for (i, &param) in (0..params as usize).zip(key.params) {
                    params_start.add(i).write(param);
                }
                let header = &*header;

                Err(EvaluatedFn {
                    result: &header.result,
                    closure: header.closure,
                    params: core::slice::from_raw_parts(params_start, params as usize),
                })
            },
        }
    }

    fn get<'s>(&'s self, evaled_ref: EvaledRef) -> EvaluatedFn<'s> {
        unsafe {
            // Safety: `EvaledRef` only ever derived from `arena.alloc_append`.
            let header_start = self.arena.get(evaled_ref.0);
            // We always write header first.
            let header = &*(header_start as *const EvaluatedHeader);
            let params_start = header_start.byte_add(HEADER_TO_PARAMS_OFFSET) as *const Param;

            EvaluatedFn {
                result: &header.result,
                closure: header.closure,
                params: core::slice::from_raw_parts(params_start, header.params as usize),
            }
        }
    }
}

impl Default for EvaluatedFunctionCache {
    fn default() -> Self {
        Self::new()
    }
}
