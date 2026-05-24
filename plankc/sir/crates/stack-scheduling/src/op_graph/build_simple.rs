use crate::{
    layouts::{Layout, LayoutMember, LayoutsTracker},
    op_graph::{OpGraph, OpGraphBuilder, OpNodeKind},
    op_model::is_flippable,
};
use hashbrown::HashMap;
use sir_data::{BlockView, ControlView, EthIRProgram, Operation};

pub fn build_graph_simple<'ir>(
    program: &'ir EthIRProgram,
    block: BlockView<'ir>,
    layouts: &LayoutsTracker<'ir>,
    input_layout: &Layout,
    output_layout: &Layout,
) -> OpGraph {
    let estimated_ops = (block.operations().count() * 11).div_ceil(10);
    let estimated_values = estimated_ops * 2 + input_layout.len();
    let mut graph = OpGraphBuilder::with_capacity(estimated_ops, estimated_values);

    let mut local_to_value = HashMap::new();
    let mut ret_dest_value = None;

    let inputs = block.inputs();
    for &member in input_layout.members_fifo() {
        let vid = graph.push_input_value();
        match member {
            LayoutMember::ReturnDest => ret_dest_value.replace(vid),
            LayoutMember::InputOutput(position) => {
                local_to_value.insert(inputs[position as usize], vid)
            }
            LayoutMember::Local(local) => local_to_value.insert(local, vid),
        };
    }

    let mut graph = graph.end_inputs_begin_ops();

    let mut previous_in_chain = None;
    for op in block.operations() {
        let return_dest = 'return_dest: {
            let Operation::InternalCall(icall) = op.op() else {
                break 'return_dest None;
            };
            let callee = program.function(icall.function);
            let callee_entry_layout = layouts.get_input_layout(callee.entry().id());
            if !callee_entry_layout.contains(&LayoutMember::ReturnDest) {
                break 'return_dest None;
            }

            let kind = OpNodeKind::RetDestPush(op.id());
            let ret_dest_push = graph.begin_op(kind);
            let ret_dest_value = ret_dest_push.end_inputs_begin_outputs().add_output();

            Some(ret_dest_value)
        };

        let kind = if is_flippable(op.op().kind()) {
            OpNodeKind::Flippable(op.id())
        } else {
            OpNodeKind::Normal(op.id())
        };
        let mut op_builder = graph.begin_op(kind);
        let op_id = op_builder.id();
        if let Some(prev) = previous_in_chain.replace(op_id) {
            op_builder.add_predecessor(prev);
        }

        match op.op() {
            Operation::InternalCall(icall) => {
                let call_inputs = icall.get_inputs(program);
                let callee = program.function(icall.function);
                let callee_entry_layout = layouts.get_input_layout(callee.entry().id());
                for &member in callee_entry_layout.members_fifo() {
                    let value = match member {
                        LayoutMember::InputOutput(i) => local_to_value[&call_inputs[i as usize]],
                        LayoutMember::ReturnDest => return_dest.expect("return dest created first"),
                        LayoutMember::Local(_) => {
                            unreachable!("function entry should not have non-input members")
                        }
                    };
                    op_builder.add_input(value);
                }
            }
            _non_icall => {
                for input in op.inputs() {
                    op_builder.add_input(local_to_value[input]);
                }
            }
        }

        let mut op_builder = op_builder.end_inputs_begin_outputs();
        for &local in op.outputs() {
            let vid = op_builder.add_output();
            let prev = local_to_value.insert(local, vid);
            assert!(prev.is_none());
        }
    }

    let mut graph = graph.end_ops_begin_end_stack();

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
        graph.push_end_stack_value(value);
    }

    let block_outputs = block.outputs();
    for &member in output_layout.members_fifo() {
        let value = match member {
            LayoutMember::ReturnDest => ret_dest_value.expect("no return dest despite in output"),
            LayoutMember::InputOutput(position) => {
                local_to_value[&block_outputs[position as usize]]
            }
            LayoutMember::Local(local) => local_to_value[&local],
        };
        graph.push_end_stack_value(value);
    }

    graph.finish()
}
