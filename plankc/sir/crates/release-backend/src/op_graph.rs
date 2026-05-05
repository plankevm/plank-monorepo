use crate::{LayoutMember, LayoutsTracker};
use hashbrown::HashMap;
use sir_data::{BlockView, Idx, IndexVec, newtype_index};

newtype_index! {
    struct OpNodeId;
    struct ValueNodeId;
}

struct OpNode {
    consumes: Vec<ValueNodeId>,
    produces: Vec<ValueNodeId>,
    can_flip: bool,
    must_be_before: Vec<OpNodeId>,
}

struct ValueNode {
    source: Option<OpNodeId>,
    used_by: Vec<OpNodeId>,
}

impl ValueNode {
    fn input() -> Self {
        Self { source: None, used_by: Vec::new() }
    }
}

struct OpGraph {
    operations: IndexVec<OpNodeId, OpNode>,
    values: IndexVec<ValueNodeId, ValueNode>,
    inputs_end: ValueNodeId,
    outputs: Vec<ValueNodeId>,
}

impl OpGraph {
    fn is_input(&self, id: ValueNodeId) -> bool {
        id < self.inputs_end
    }
}

fn build_graph_simple<'ir>(block: BlockView<'ir>, layouts: &LayoutsTracker<'ir>) -> OpGraph {
    let mut operations = IndexVec::with_capacity(block.operations().size_hint().0);
    let mut values = IndexVec::new();

    let mut local_to_value = HashMap::new();
    let mut ret_dest_value = None;

    let input_layout = layouts.get_input_layout(block.id());
    let inputs = block.inputs();

    for &member in input_layout.all_members() {
        let vid = values.push(ValueNode::input());
        let prev = match member {
            LayoutMember::ReturnDest => ret_dest_value.replace(vid),
            LayoutMember::InputOutput(position) => {
                local_to_value.insert(inputs[position as usize], vid)
            }
            LayoutMember::Local(local) => local_to_value.insert(local, vid),
        };
        assert!(prev.is_none());
    }
    let inputs_end = values.len_idx();

    for op in block.operations() {}

    OpGraph { operations, values, inputs_end }
}
