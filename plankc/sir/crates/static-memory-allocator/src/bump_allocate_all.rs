use hashbrown::{HashMap, HashSet};
use sir_data::{
    BasicBlockId, ControlView, DenseIndexSet, EthIRProgram, FunctionId, Idx, Operation,
    StaticAllocId,
};
use sir_stack_scheduling::{ScheduledOps, stack::StackOps};

use crate::{DynFreePointer, EvmMemAddr, Layout};

const EVM_WORD_IN_BYTES: u32 = 0x20;

pub struct BumpAllocateAll;

impl BumpAllocateAll {
    pub fn generate(ir: &EthIRProgram, entry_func: FunctionId, stack_ops: &ScheduledOps) -> Layout {
        let mut layout_generator = MemoryLayoutCollector {
            ir,
            stack_ops,
            seen_functions: DenseIndexSet::with_capacity_in_bits(ir.functions.len()),
            seen_blocks: DenseIndexSet::with_capacity_in_bits(ir.basic_blocks.len()),
            function_worklist: Vec::with_capacity(ir.functions.len()),
            block_worklist: Vec::with_capacity(ir.basic_blocks.len()),
            bump: StaticBumpTracker { next_free: EvmMemAddr::new(0) },
            dyn_free_pointer: None,
            switch_store: None,
            alloc_start: HashMap::with_capacity(ir.next_static_alloc_id.get().idx()),
            alloc_needs_zeroing: HashSet::with_capacity(ir.next_static_alloc_id.get().idx()),
        };

        layout_generator.seen_functions.add(entry_func);
        layout_generator.function_worklist.push(entry_func);
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
struct StaticBumpTracker {
    next_free: EvmMemAddr,
}

impl StaticBumpTracker {
    fn alloc(&mut self, bytes: u32) -> EvmMemAddr {
        let addr = self.next_free;
        let next_addr =
            addr.get().checked_add(bytes).expect("static memory layout exceeded u32 address space");
        self.next_free = EvmMemAddr::new(next_addr);
        addr
    }
}

struct MemoryLayoutCollector<'ir, 'ops> {
    ir: &'ir EthIRProgram,
    stack_ops: &'ops ScheduledOps,
    seen_functions: DenseIndexSet<FunctionId>,
    seen_blocks: DenseIndexSet<BasicBlockId>,
    function_worklist: Vec<FunctionId>,
    block_worklist: Vec<BasicBlockId>,
    bump: StaticBumpTracker,
    dyn_free_pointer: Option<EvmMemAddr>,
    switch_store: Option<EvmMemAddr>,
    alloc_start: HashMap<StaticAllocId, EvmMemAddr>,
    alloc_needs_zeroing: HashSet<StaticAllocId>,
}

impl<'ir, 'ops> MemoryLayoutCollector<'ir, 'ops> {
    fn collect_function(&mut self, function: FunctionId) {
        let entry_block = self.ir.functions[function].entry();
        self.seen_blocks.add(entry_block);
        self.block_worklist.push(entry_block);

        while let Some(block) = self.block_worklist.pop() {
            let block = self.ir.block(block);

            for operation in block.operations() {
                self.collect_operation(operation.op());
            }

            for &stack_op in self.stack_ops.get(block.id()).expect("reachable block not scheduled")
            {
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

            if let ControlView::Switch(_) = block.control()
                && self.switch_store.is_none()
            {
                self.switch_store = Some(self.bump.alloc(EVM_WORD_IN_BYTES));
            }

            self.block_worklist
                .extend(block.successors().filter(|&block| self.seen_blocks.add(block)));
        }
    }

    fn collect_operation(&mut self, operation: Operation) {
        match operation {
            Operation::DynamicAllocZeroed(_)
            | Operation::DynamicAllocAnyBytes(_)
            | Operation::AcquireFreePointer(_)
                if self.dyn_free_pointer.is_none() =>
            {
                self.dyn_free_pointer = Some(self.bump.alloc(EVM_WORD_IN_BYTES));
            }
            Operation::StaticAllocZeroed(data) => {
                self.alloc_static(data.alloc_id, data.size, true);
            }
            Operation::StaticAllocAnyBytes(data) => {
                self.alloc_static(data.alloc_id, data.size, false);
            }
            Operation::InternalCall(data) if self.seen_functions.add(data.function) => {
                self.function_worklist.push(data.function);
            }
            _ => {}
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
