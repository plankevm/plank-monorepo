use crate::{LayoutsTracker, layouts::LayoutMember, op_model::is_flippable};
use hashbrown::HashMap;
use sir_data::{BlockView, ControlView, Idx, IndexVec, Span, newtype_index};

newtype_index! {
    pub struct OpNodeId;
    pub struct ValueNodeId;
}

pub struct OpNode {
    pub consumes_fifo: Vec<ValueNodeId>,
    pub produces_fifo: Vec<ValueNodeId>,
    pub can_flip: bool,
    /// The set of nodes that be executed *after* this node, regardless of data dependencies
    pub happens_before: Vec<OpNodeId>,
}

pub struct ValueNode {
    pub source: Option<OpNodeId>,
    pub used_by: Vec<OpNodeId>,
}

impl ValueNode {
    fn input() -> Self {
        Self { source: None, used_by: Vec::new() }
    }
}

pub struct OpGraph {
    pub operations: IndexVec<OpNodeId, OpNode>,
    pub values: IndexVec<ValueNodeId, ValueNode>,
    pub inputs_end: ValueNodeId,
    pub outputs_fifo: Vec<ValueNodeId>,
    pub control_op: Option<OpNodeId>,
}

impl OpGraph {
    pub fn input_values_fifo(&self) -> Span<ValueNodeId> {
        Span::new(ValueNodeId::ZERO, self.inputs_end)
    }

    pub fn is_input(&self, id: ValueNodeId) -> bool {
        id < self.inputs_end
    }
}

pub fn build_graph_simple<'ir>(block: BlockView<'ir>, layouts: &LayoutsTracker<'ir>) -> OpGraph {
    let mut operations =
        IndexVec::<OpNodeId, OpNode>::with_capacity(block.operations().size_hint().0);
    let mut values = IndexVec::<ValueNodeId, ValueNode>::new();

    let mut local_to_value = HashMap::new();
    let mut ret_dest_value = None;

    let input_layout = layouts.get_input_layout(block.id());
    let inputs = block.inputs();

    for &member in input_layout.members_fifo() {
        let vid = values.push(ValueNode::input());
        match member {
            LayoutMember::ReturnDest => ret_dest_value.replace(vid),
            LayoutMember::InputOutput(position) => {
                local_to_value.insert(inputs[position as usize], vid)
            }
            LayoutMember::Local(local) => local_to_value.insert(local, vid),
        };
    }
    let inputs_end = values.len_idx();

    let mut last_op = None;

    for op in block.operations() {
        let op_node = operations.push(OpNode {
            consumes_fifo: Vec::new(),
            produces_fifo: Vec::new(),
            can_flip: is_flippable(op.op().kind()),
            happens_before: Vec::with_capacity(1),
        });

        // TODO: Track operation effects to build a more loose "must be before" graph.
        if let Some(last_op) = last_op.replace(op_node) {
            operations[last_op].happens_before.push(op_node);
        }

        operations[op_node].consumes_fifo = op
            .inputs()
            .iter()
            .map(|input| {
                let value = local_to_value[input];
                values[value].used_by.push(op_node);
                value
            })
            .collect();
        operations[op_node].produces_fifo = op
            .outputs()
            .iter()
            .map(|&output| {
                let value = values
                    .push(ValueNode { source: Some(op_node), used_by: Vec::with_capacity(2) });
                local_to_value.insert(output, value);
                value
            })
            .collect();
    }

    let control_op = 'control_op: {
        let value = match block.control() {
            ControlView::LastOpTerminates | ControlView::ContinuesTo(_) => {
                // Do nothing. For `LastOpTerminates` operation is part of block, `ContinuesTo`
                // needs no explicit operation in the graph.
                break 'control_op None;
            }
            ControlView::InternalReturn => ret_dest_value.expect("no return dest for iret"),
            ControlView::Switch(switch) => local_to_value[&switch.condition()],
            ControlView::Branches { condition, .. } => local_to_value[&condition],
        };
        let control_op = operations.push(OpNode {
            consumes_fifo: vec![value],
            produces_fifo: vec![],
            can_flip: false,
            happens_before: vec![],
        });
        values[value].used_by.push(control_op);
        if let Some(last_op) = last_op.replace(control_op) {
            operations[last_op].happens_before.push(control_op);
        }
        Some(control_op)
    };

    let block_outputs = block.outputs();
    let outputs_fifo = layouts
        .get_output_layout(block.id())
        .members_fifo()
        .iter()
        .map(|member| match *member {
            LayoutMember::ReturnDest => ret_dest_value.expect("no return dest despite in output"),
            LayoutMember::InputOutput(position) => {
                local_to_value[&block_outputs[position as usize]]
            }
            LayoutMember::Local(local) => local_to_value[&local],
        })
        .collect();

    OpGraph { operations, values, outputs_fifo, inputs_end, control_op }
}
