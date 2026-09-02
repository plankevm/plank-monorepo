use crate::model::{BlockFinalization, RepresentativeGraph, RepresentativeOperation};
use plank_core::{Idx, IndexVec, Span, newtype_index};

newtype_index! {
    pub struct OperationId;
    pub struct ValueId;
    struct InputArenaId;
    struct EffectPredecessorArenaId;
}

pub struct Graph {
    pub finalization: BlockFinalization,
    input_values: Span<ValueId>,
    values: IndexVec<ValueId, ()>,
    operations: IndexVec<OperationId, Operation>,
    inputs_arena: IndexVec<InputArenaId, ValueId>,
    effect_predecessors_arena: IndexVec<EffectPredecessorArenaId, OperationId>,
    outputs_fifo: Box<[ValueId]>,
}

#[derive(Clone, Copy)]
pub struct Operation {
    inputs: Span<InputArenaId>,
    outputs: Span<ValueId>,
    effect_predecessors: Span<EffectPredecessorArenaId>,
    pub flippable: bool,
}

impl Graph {
    pub fn from_representative(representative: RepresentativeGraph) -> Result<Self, String> {
        let mut values = IndexVec::new();
        let input_start = values.len_idx();
        for _ in 0..representative.input_count {
            values.push(());
        }
        let input_values = Span::new(input_start, values.len_idx());

        let mut operations = IndexVec::with_capacity(representative.operations.len());
        let mut inputs_arena = IndexVec::new();
        let mut effect_predecessors_arena = IndexVec::new();
        for operation in representative.operations {
            add_operation(
                operation,
                &mut values,
                &mut operations,
                &mut inputs_arena,
                &mut effect_predecessors_arena,
            )?;
        }

        let outputs_fifo = representative
            .outputs_fifo
            .iter()
            .map(|&raw| resolve_value(&values, raw))
            .collect::<Result<_, _>>()?;
        Ok(Self {
            finalization: representative.finalization,
            input_values,
            values,
            operations,
            inputs_arena,
            effect_predecessors_arena,
            outputs_fifo,
        })
    }

    pub fn operation_ids(&self) -> impl Iterator<Item = OperationId> + '_ {
        self.operations.iter_idx()
    }

    pub fn total_values(&self) -> usize {
        self.values.len()
    }

    pub fn input_values(&self) -> impl DoubleEndedIterator<Item = ValueId> + '_ {
        self.input_values.iter()
    }

    pub fn outputs_fifo(&self) -> &[ValueId] {
        &self.outputs_fifo
    }

    pub fn operation(&self, id: OperationId) -> Operation {
        self.operations[id]
    }

    pub fn operation_inputs(&self, operation: Operation) -> &[ValueId] {
        &self.inputs_arena[operation.inputs]
    }

    pub fn operation_outputs(
        &self,
        operation: Operation,
    ) -> impl DoubleEndedIterator<Item = ValueId> {
        operation.outputs.iter()
    }

    pub fn operation_effect_predecessors(&self, operation: Operation) -> &[OperationId] {
        &self.effect_predecessors_arena[operation.effect_predecessors]
    }

    pub fn resolve_operation(&self, raw: u32) -> Result<OperationId, String> {
        self.operations
            .iter_idx()
            .find(|operation| operation.get() == raw)
            .ok_or_else(|| format!("schedule refers to missing op{raw}"))
    }
}

fn add_operation(
    representative: RepresentativeOperation,
    values: &mut IndexVec<ValueId, ()>,
    operations: &mut IndexVec<OperationId, Operation>,
    inputs_arena: &mut IndexVec<InputArenaId, ValueId>,
    effect_predecessors_arena: &mut IndexVec<EffectPredecessorArenaId, OperationId>,
) -> Result<(), String> {
    let input_start = inputs_arena.len_idx();
    for raw in representative.inputs_fifo {
        inputs_arena.push(resolve_value(values, raw)?);
    }
    let inputs = Span::new(input_start, inputs_arena.len_idx());

    let predecessor_start = effect_predecessors_arena.len_idx();
    for raw in representative.effect_predecessors {
        let predecessor = operations
            .iter_idx()
            .find(|operation| operation.get() == raw)
            .ok_or_else(|| format!("operation refers to missing predecessor op{raw}"))?;
        effect_predecessors_arena.push(predecessor);
    }
    let effect_predecessors = Span::new(predecessor_start, effect_predecessors_arena.len_idx());

    let output_start = values.len_idx();
    for _ in 0..representative.output_count {
        values.push(());
    }
    let outputs = Span::new(output_start, values.len_idx());
    operations.push(Operation {
        inputs,
        outputs,
        effect_predecessors,
        flippable: representative.flippable,
    });
    Ok(())
}

fn resolve_value(values: &IndexVec<ValueId, ()>, raw: u32) -> Result<ValueId, String> {
    values
        .iter_idx()
        .find(|value| value.get() == raw)
        .ok_or_else(|| format!("graph refers to missing v{raw}"))
}
