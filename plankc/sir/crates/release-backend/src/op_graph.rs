use crate::{LayoutsTracker, layouts::LayoutMember, op_model::is_flippable};
use hashbrown::HashMap;
use sir_data::{
    BlockView, ControlView, Idx, IndexVec, Operation, OperationIdx, Span, newtype_index,
};

newtype_index! {
    pub struct OpNodeId;
    pub struct ValueNodeId;
}

pub enum OpNodeKind {
    Flippable,
    RetDestPush(OperationIdx),
    Normal,
    Control,
}

pub struct OpNode {
    pub consumes_fifo: Vec<ValueNodeId>,
    pub produces_fifo: Vec<ValueNodeId>,
    pub kind: OpNodeKind,
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

    fn output(source: OpNodeId) -> Self {
        Self { source: Some(source), used_by: Vec::with_capacity(2) }
    }
}

pub struct OpGraph {
    pub operations: IndexVec<OpNodeId, OpNode>,
    pub values: IndexVec<ValueNodeId, ValueNode>,
    pub inputs_end: ValueNodeId,
    pub end_stack_fifo: Vec<ValueNodeId>,
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

    let terminating = matches!(block.control(), ControlView::LastOpTerminates)
        .then(|| block.operations().last().expect("last op terminates but no last"));

    let block_outputs = block.outputs();
    let mut end_stack_fifo = Vec::with_capacity(block_outputs.len() + 2);

    for op in block.operations() {
        if terminating.is_some_and(|terminating| terminating.id() == op.id()) {
            end_stack_fifo.extend(op.inputs().iter().map(|input| local_to_value[input]));
            break;
        }

        let op_node = operations.push(OpNode {
            consumes_fifo: Vec::new(),
            produces_fifo: Vec::new(),
            kind: if is_flippable(op.op().kind()) {
                OpNodeKind::Flippable
            } else {
                OpNodeKind::Normal
            },
            happens_before: Vec::with_capacity(1),
        });

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
                let value = values.push(ValueNode::output(op_node));
                local_to_value.insert(output, value);
                value
            })
            .collect();
    }

    'handle_control: {
        let value = match block.control() {
            ControlView::LastOpTerminates => {
                // do nothing, already handled
                break 'handle_control;
            }
            ControlView::ContinuesTo(_) => {
                // doesn't have any external value
                break 'handle_control;
            }
            ControlView::InternalReturn => ret_dest_value.expect("no return dest for iret"),
            ControlView::Switch(switch) => local_to_value[&switch.condition()],
            ControlView::Branches { condition, .. } => local_to_value[&condition],
        };
        end_stack_fifo.push(value);
    }

    end_stack_fifo.extend(layouts.get_output_layout(block.id()).members_fifo().iter().map(
        |&member| match member {
            LayoutMember::ReturnDest => ret_dest_value.expect("no return dest despite in output"),
            LayoutMember::InputOutput(position) => {
                local_to_value[&block_outputs[position as usize]]
            }
            LayoutMember::Local(local) => local_to_value[&local],
        },
    ));

    OpGraph { operations, values, end_stack_fifo, inputs_end }
}
