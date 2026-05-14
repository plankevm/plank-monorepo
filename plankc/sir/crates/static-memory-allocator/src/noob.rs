use hashbrown::{HashMap, HashSet};
use plank_core::list_of_lists::ListOfLists;
use sir_data::{
    BasicBlockId, Control, DenseIndexSet, EthIRProgram, FunctionId, Idx, Operation, StaticAllocId,
};
use sir_passes::AnalysesStore;
use sir_stack_scheduling::stack::StackOps;

use crate::{DynFreePointer, EntrypointKind, EvmMemAddr, Layout, LayoutGenerator};

const EVM_WORD_IN_BYTES: u32 = 0x20;

pub struct EvmStaticAllocator;

impl LayoutGenerator for EvmStaticAllocator {
    fn generate(
        &mut self,
        ir: &EthIRProgram,
        _analyses: &AnalysesStore,
        entrypoint: EntrypointKind,
        stack_ops: &ListOfLists<BasicBlockId, StackOps>,
    ) -> Layout {
        let entry_func = match entrypoint {
            EntrypointKind::Init => ir.init_entry,
            EntrypointKind::Run => ir.main_entry.expect("main entrypoint specified but missing"),
        };

        let mut layout_generator = MemoryLayoutCollector {
            ir,
            stack_ops,
            seen_functions: DenseIndexSet::with_capacity_in_bits(ir.functions.len()),
            seen_blocks: DenseIndexSet::with_capacity_in_bits(ir.basic_blocks.len()),
            function_worklist: Vec::with_capacity(ir.functions.len()),
            block_worklist: Vec::with_capacity(ir.basic_blocks.len()),
            bump: EvmStaticBumpAllocator::new(),
            dyn_free_pointer: None,
            switch_store: None,
            alloc_start: HashMap::with_capacity(ir.next_static_alloc_id.get().idx()),
            alloc_needs_zeroing: HashSet::with_capacity(ir.next_static_alloc_id.get().idx()),
        };

        layout_generator.seen_functions.add(entry_func);
        layout_generator.collect_function(entry_func);
        while let Some(function) = layout_generator.function_worklist.pop() {
            layout_generator.collect_function(function);
        }

        Layout {
            dyn_free_pointer: layout_generator.dyn_free_pointer.map(|store_slot| DynFreePointer {
                store_slot,
                start_value: layout_generator.bump.next_free,
            }),
            switch_store: layout_generator.switch_store,
            alloc_start: layout_generator.alloc_start,
            alloc_needs_zeroing: layout_generator.alloc_needs_zeroing,
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct EvmStaticBumpAllocator {
    next_free: EvmMemAddr,
}

impl EvmStaticBumpAllocator {
    const fn new() -> Self {
        Self { next_free: EvmMemAddr::new(0) }
    }

    fn alloc(&mut self, bytes: u32) -> EvmMemAddr {
        let next_addr = self
            .next_free
            .get()
            .checked_add(bytes)
            .expect("static memory layout exceeded u32 address space");
        std::mem::replace(&mut self.next_free, EvmMemAddr::new(next_addr))
    }
}

struct MemoryLayoutCollector<'ir, 'ops> {
    ir: &'ir EthIRProgram,
    stack_ops: &'ops ListOfLists<BasicBlockId, StackOps>,
    seen_functions: DenseIndexSet<FunctionId>,
    seen_blocks: DenseIndexSet<BasicBlockId>,
    function_worklist: Vec<FunctionId>,
    block_worklist: Vec<BasicBlockId>,
    bump: EvmStaticBumpAllocator,
    dyn_free_pointer: Option<EvmMemAddr>,
    switch_store: Option<EvmMemAddr>,
    alloc_start: HashMap<StaticAllocId, EvmMemAddr>,
    alloc_needs_zeroing: HashSet<StaticAllocId>,
}

impl<'ir, 'ops> MemoryLayoutCollector<'ir, 'ops> {
    fn collect_function(&mut self, function: FunctionId) {
        self.block_worklist.push(self.ir.functions[function].entry());

        while let Some(block) = self.block_worklist.pop() {
            if !self.seen_blocks.add(block) {
                continue;
            }

            self.collect_block(block);
        }
    }

    fn collect_block(&mut self, block: BasicBlockId) {
        let block_data = &self.ir.basic_blocks[block];

        for operation in block_data.operations.iter().map(|op| self.ir.operations[op]) {
            self.collect_operation(operation);
        }

        if let Some(stack_ops) = self.stack_ops.get(block) {
            for &stack_op in stack_ops {
                self.collect_stack_op(stack_op);
            }
        }

        if let Control::Switch(_) = block_data.control {
            self.switch_store.get_or_insert_with(|| self.bump.alloc(EVM_WORD_IN_BYTES));
        }

        self.block_worklist.extend(block_data.control.iter_outgoing(self.ir));
    }

    fn collect_operation(&mut self, operation: Operation) {
        match operation {
            Operation::DynamicAllocZeroed(_)
            | Operation::DynamicAllocAnyBytes(_)
            | Operation::AcquireFreePointer(_) => {
                self.dyn_free_pointer.get_or_insert_with(|| self.bump.alloc(EVM_WORD_IN_BYTES));
            }
            Operation::StaticAllocZeroed(data) => {
                self.alloc_static(data.alloc_id, data.size, true);
            }
            Operation::StaticAllocAnyBytes(data) => {
                self.alloc_static(data.alloc_id, data.size, false);
            }
            Operation::InternalCall(data) => {
                if self.seen_functions.add(data.function) {
                    self.function_worklist.push(data.function);
                }
            }
            _ => {}
        }
    }

    fn collect_stack_op(&mut self, stack_op: StackOps) {
        match stack_op {
            StackOps::Store(id) => self.alloc_static(id, EVM_WORD_IN_BYTES, false),
            StackOps::Load(id) => {
                assert!(self.alloc_start.contains_key(&id), "stack load from unallocated")
            }
            StackOps::Swap(_)
            | StackOps::Dup(_)
            | StackOps::Pop
            | StackOps::Op(_)
            | StackOps::CallRetPush(_)
            | StackOps::Exchange(_, _) => {}
        }
    }

    fn alloc_static(&mut self, id: StaticAllocId, size: u32, needs_zeroing: bool) {
        self.alloc_start.entry(id).or_insert_with(|| {
            if needs_zeroing {
                self.alloc_needs_zeroing.insert(id);
            }
            self.bump.alloc(size)
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use plank_core::list_of_lists::ListOfLists;
    use sir_data::{
        Control,
        builder::EthIRBuilder,
        operation::{InlineOperands, InternalCallData, StaticAllocData},
    };

    #[test]
    fn maps_reachable_static_allocs_and_special_slots() {
        let mut builder = EthIRBuilder::new();
        let mut init = builder.begin_function();

        let size = init.new_local();
        let ptr = init.new_local();
        let zeroed_ptr = init.new_local();
        let any_ptr = init.new_local();

        let mut entry = init.begin_basic_block();
        let zeroed_alloc = entry.as_mut().new_static_alloc();
        entry.add_operation(Operation::StaticAllocZeroed(StaticAllocData {
            size: 64,
            ptr_out: zeroed_ptr,
            alloc_id: zeroed_alloc,
        }));
        entry.add_operation(Operation::DynamicAllocAnyBytes(InlineOperands {
            ins: [size],
            outs: [ptr],
        }));
        let entry_id = entry.finish_with_placeholder_control();

        let mut next = init.begin_basic_block();
        let any_alloc = next.as_mut().new_static_alloc();
        next.add_operation(Operation::StaticAllocAnyBytes(StaticAllocData {
            size: 32,
            ptr_out: any_ptr,
            alloc_id: any_alloc,
        }));
        next.add_operation(Operation::Stop(()));
        let next_id = next.finish_terminating().unwrap();
        init.set_control(entry_id, Control::ContinuesTo(next_id)).unwrap();
        let init_id = init.finish(entry_id);

        let mut dead = builder.begin_function();
        let dead_ptr = dead.new_local();
        let mut dead_entry = dead.begin_basic_block();
        let dead_alloc = dead_entry.as_mut().new_static_alloc();
        dead_entry.add_operation(Operation::StaticAllocZeroed(StaticAllocData {
            size: 32,
            ptr_out: dead_ptr,
            alloc_id: dead_alloc,
        }));
        dead_entry.add_operation(Operation::Stop(()));
        let dead_entry_id = dead_entry.finish_terminating().unwrap();
        dead.finish(dead_entry_id);

        let ir = builder.build(init_id, None);
        let stack_ops = ListOfLists::new();
        let layout = EvmStaticAllocator.generate(
            &ir,
            &AnalysesStore::default(),
            EntrypointKind::Init,
            &stack_ops,
        );

        assert_eq!(layout.alloc_start[&zeroed_alloc].get(), 0);
        assert_eq!(layout.alloc_start[&any_alloc].get(), 96);
        assert!(!layout.alloc_start.contains_key(&dead_alloc));
        assert!(layout.alloc_needs_zeroing.contains(&zeroed_alloc));
        assert!(!layout.alloc_needs_zeroing.contains(&any_alloc));
        assert_eq!(layout.switch_store, None);
        let dyn_free_pointer =
            layout.dyn_free_pointer.expect("reachable malloc allocates free pointer");
        assert_eq!(dyn_free_pointer.store_slot.get(), 64);
        assert_eq!(dyn_free_pointer.start_value.get(), 128);
    }

    #[test]
    fn allocates_switch_store_for_reachable_switch() {
        let mut builder = EthIRBuilder::new();
        let mut init = builder.begin_function();

        let condition = init.new_local();

        let mut fallback = init.begin_basic_block();
        fallback.add_operation(Operation::Stop(()));
        let fallback_id = fallback.finish_terminating().unwrap();

        let switch = init.begin_switch().finish(condition, Some(fallback_id));

        let entry = init.begin_basic_block().finish_with_switch(switch);
        let init_id = init.finish(entry);

        let ir = builder.build(init_id, None);
        let stack_ops = ListOfLists::new();
        let layout = EvmStaticAllocator.generate(
            &ir,
            &AnalysesStore::default(),
            EntrypointKind::Init,
            &stack_ops,
        );

        assert_eq!(layout.switch_store.map(EvmMemAddr::get), Some(0));
        assert!(layout.dyn_free_pointer.is_none());
    }

    #[test]
    fn follows_internal_calls_and_stack_allocs() {
        let mut builder = EthIRBuilder::new();

        let mut callee = builder.begin_function();
        let callee_ptr = callee.new_local();
        let mut callee_entry = callee.begin_basic_block();
        let callee_alloc = callee_entry.as_mut().new_static_alloc();
        callee_entry.add_operation(Operation::StaticAllocAnyBytes(StaticAllocData {
            size: 16,
            ptr_out: callee_ptr,
            alloc_id: callee_alloc,
        }));
        let callee_entry_id = callee_entry.finish_with_internal_return().unwrap();
        let callee_id = callee.finish(callee_entry_id);

        let mut init = builder.begin_function();
        let mut entry = init.begin_basic_block();
        let call_locals = entry.as_mut().alloc_locals(&[]);
        entry.add_operation(Operation::InternalCall(InternalCallData {
            function: callee_id,
            ins_start: call_locals.start,
            outs_start: call_locals.end,
        }));
        entry.add_operation(Operation::Stop(()));
        let entry_id = entry.finish_terminating().unwrap();
        let init_id = init.finish(entry_id);

        let ir = builder.build(init_id, None);
        let stack_alloc = ir.next_static_alloc_id.get();
        ir.next_static_alloc_id.set(stack_alloc + 1);

        let mut stack_ops = ListOfLists::new();
        assert_eq!(stack_ops.push_copy_slice(&[]), callee_entry_id);
        assert_eq!(stack_ops.push_copy_slice(&[StackOps::Store(stack_alloc)]), entry_id);

        let layout = EvmStaticAllocator.generate(
            &ir,
            &AnalysesStore::default(),
            EntrypointKind::Init,
            &stack_ops,
        );

        assert_eq!(layout.alloc_start[&stack_alloc].get(), 0);
        assert_eq!(layout.alloc_start[&callee_alloc].get(), EVM_WORD_IN_BYTES);
        assert!(layout.dyn_free_pointer.is_none());
    }
}
