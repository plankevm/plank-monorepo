use crate::{
    layouts::{Layout, LayoutMember, LayoutsTracker},
    op_graph::{OpGraph, OpGraphBuilder, OpNodeId, OpNodeKind, builder::OpBuilder},
    op_model::is_flippable,
};
use hashbrown::HashMap;
use sir_data::{BlockView, ControlView, EthIRProgram, Operation, operation::effects::Effect};
use sir_passes::AnalysesStore;

/// Channels of `(minor, major)` effect pairs. Within a channel, minor effects commute with each
/// other while major effects must be ordered relative to all other effects on the channel.
/// Effects on different channels never conflict.
const EFFECT_CHANNELS: [(Effect, Effect); 7] = [
    (Effect::MEMORY_READ, Effect::MEMORY_WRITE),
    (Effect::RETURNDATA_READ, Effect::RETURNDATA_WRITE),
    (Effect::ACCOUNTS_READ, Effect::ACCOUNTS_WRITE),
    (Effect::PERSISTENT_READ, Effect::PERSISTENT_WRITE),
    (Effect::TRANSIENT_READ, Effect::TRANSIENT_WRITE),
    (Effect::REVERT, Effect::TERMINATE),
    (Effect::ALLOC_ADVANCE, Effect::ALLOC_USE_FREE),
];

#[derive(Default)]
struct ChannelState {
    last_major: Option<OpNodeId>,
    minors_since_major: Vec<OpNodeId>,
}

#[derive(Default)]
struct EffectOrderTracker {
    channels: [ChannelState; EFFECT_CHANNELS.len()],
    /// `LOGS` has no commuting minor side: log order is observable, so logging ops form a
    /// chain. Tracked separately instead of as a degenerate minor-less channel.
    last_logs: Option<OpNodeId>,
}

impl EffectOrderTracker {
    fn add_effect_predecessors<Phase>(
        &mut self,
        op_builder: &mut OpBuilder<'_, Phase>,
        effect: Effect,
    ) {
        // Terminating commits all prior side effects and prevents all subsequent ones from
        // occurring, so a terminating operation must be ordered relative to every channel.
        let effect = if effect.contains(Effect::TERMINATE) {
            effect | Effect::MAJOR | Effect::LOGS
        } else {
            effect
        };

        for (channel, (minor, major)) in self.channels.iter_mut().zip(EFFECT_CHANNELS) {
            if effect.intersects(major) {
                if let Some(last_major) = channel.last_major {
                    op_builder.add_predecessor(last_major);
                }
                for &minor_op in &channel.minors_since_major {
                    op_builder.add_predecessor(minor_op);
                }
                channel.minors_since_major.clear();
                channel.last_major = Some(op_builder.id());
            } else if effect.intersects(minor) {
                if let Some(last_major) = channel.last_major {
                    op_builder.add_predecessor(last_major);
                }
                channel.minors_since_major.push(op_builder.id());
            }
        }

        if effect.contains(Effect::LOGS)
            && let Some(last_logs) = self.last_logs.replace(op_builder.id())
        {
            op_builder.add_predecessor(last_logs);
        }
    }
}

/// Builds an [`OpGraph`] whose ordering constraints stem only from dataflow and effect
/// conflicts, unlike [`build_graph_simple`](super::build_graph_simple) which totally orders all
/// operations by program order. The resulting partial order admits many topological sorts,
/// forming the search space for stack schedule optimization.
pub fn build_graph_effectful<'ir>(
    program: &'ir EthIRProgram,
    block: BlockView<'ir>,
    layouts: &LayoutsTracker<'ir>,
    input_layout: &Layout,
    output_layout: &Layout,
    analyses: &AnalysesStore,
) -> OpGraph {
    let function_effects = analyses.function_effects(program);

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
    let mut effect_order = EffectOrderTracker::default();

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

        let effect =
            Effect::of(op.op()).unwrap_or_else(|callee| function_effects.effect_of(callee));
        effect_order.add_effect_predecessors(&mut op_builder, effect);

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layouts::build_basic_block_layout_sets;
    use plank_test_utils::dedent_preserve_blank_lines;
    use sir_parser::EmitConfig;
    use sir_passes::ControlFlowGraphInOutBundling;
    use std::fmt::Write;

    #[track_caller]
    fn assert_graphs(source: &str, expected: &str) {
        let source = dedent_preserve_blank_lines(source);
        let program = sir_parser::parse_or_panic(&source, EmitConfig::init_only());

        let analyses = AnalysesStore::default();
        let in_out_bundling = ControlFlowGraphInOutBundling::new(&program, &analyses);
        let layout_sets = build_basic_block_layout_sets(&program, &analyses, &in_out_bundling);
        let layouts = LayoutsTracker::new(&program, layout_sets, in_out_bundling);

        let mut out = String::new();
        for block in program.blocks() {
            let Some((input_layout, output_layout)) = layouts.get_input_output(block.id()) else {
                continue;
            };
            let graph = build_graph_effectful(
                &program,
                block,
                &layouts,
                input_layout,
                output_layout,
                &analyses,
            );

            writeln!(out, "@{}", block.id()).unwrap();
            for op_id in graph.op_ids() {
                let op = graph.get_op(op_id);
                let name = match op.kind {
                    OpNodeKind::Flippable(op_idx) | OpNodeKind::Normal(op_idx) => {
                        program.operations[op_idx].kind().mnemonic()
                    }
                    OpNodeKind::RetDestPush(_) => "ret_dest_push",
                };
                write!(out, "    #{op_id} {name} [").unwrap();
                for (i, pred) in op.predecessors.iter().enumerate() {
                    if i != 0 {
                        out.push_str(", ");
                    }
                    write!(out, "#{pred}").unwrap();
                }
                out.push_str("]\n");
            }
        }

        let expected = dedent_preserve_blank_lines(expected);
        pretty_assertions::assert_str_eq!(out.trim(), expected.trim());
    }

    #[test]
    fn pure_ops_only_constrained_by_dataflow() {
        assert_graphs(
            r#"
            fn init:
                entry {
                    a = const 1
                    b = const 2
                    c = add a b
                    d = not b
                    stop
                }
            "#,
            r#"
            @0
                #0 const []
                #1 const []
                #2 add [#0, #1]
                #3 not [#1]
                #4 stop []
            "#,
        );
    }

    #[test]
    fn memory_reads_commute_between_writes() {
        assert_graphs(
            r#"
            fn init:
                entry {
                    p = const 0
                    v = const 1
                    mstore256 p v
                    a = mload32 p
                    b = mload32 p
                    mstore256 p b
                    stop
                }
            "#,
            r#"
            @0
                #0 const []
                #1 const []
                #2 mstore [#0, #1]
                #3 mload [#0, #2]
                #4 mload [#0, #2]
                #5 mstore [#0, #2, #3, #4]
                #6 stop [#5]
            "#,
        );
    }

    #[test]
    fn storage_reads_commute_between_writes() {
        assert_graphs(
            r#"
            fn init:
                entry {
                    k = const 0
                    a = sload k
                    b = sload k
                    sstore k a
                    c = sload k
                    stop
                }
            "#,
            r#"
            @0
                #0 const []
                #1 sload [#0]
                #2 sload [#0]
                #3 sstore [#0, #1, #2]
                #4 sload [#0, #3]
                #5 stop [#3, #4]
            "#,
        );
    }

    #[test]
    fn logs_chain_and_terminator_is_last() {
        assert_graphs(
            r#"
            fn init:
                entry {
                    p = const 0
                    l = const 32
                    log0 p l
                    log0 p l
                    return p l
                }
            "#,
            r#"
            @0
                #0 const []
                #1 const []
                #2 log0 [#0, #1]
                #3 log0 [#0, #1, #2]
                #4 return [#0, #1, #2, #3]
            "#,
        );
    }

    #[test]
    fn maybe_reverting_icalls_commute_but_precede_terminator() {
        assert_graphs(
            r#"
            fn init:
                entry {
                    x = const 1
                    icall @may_revert x
                    icall @may_revert x
                    stop
                }
            fn may_revert:
                entry c {
                    => c ? @bad : @ok
                }
                bad {
                    revert 0 0
                }
                ok {
                    iret
                }
            "#,
            r#"
            @0
            @1
                #0 const []
                #1 const []
                #2 revert [#0, #1]
            @2
            @3
                #0 const []
                #1 ret_dest_push []
                #2 icall [#0, #1]
                #3 ret_dest_push []
                #4 icall [#0, #3]
                #5 stop [#2, #4]
            "#,
        );
    }

    #[test]
    fn maybe_terminating_icall_orders_against_all_channels() {
        assert_graphs(
            r#"
            fn init:
                entry {
                    k = const 0
                    a = sload k
                    icall @may_stop k
                    b = sload k
                    log0 k k
                    stop
                }
            fn may_stop:
                entry c {
                    => c ? @halt : @ok
                }
                halt {
                    stop
                }
                ok {
                    iret
                }
            "#,
            r#"
            @0
            @1
                #0 stop []
            @2
            @3
                #0 const []
                #1 sload [#0]
                #2 ret_dest_push []
                #3 icall [#0, #1, #2]
                #4 sload [#0, #3]
                #5 log0 [#0, #3]
                #6 stop [#3, #4, #5]
            "#,
        );
    }

    #[test]
    fn maybe_terminating_icalls_chain() {
        assert_graphs(
            r#"
            fn init:
                entry {
                    x = const 1
                    icall @may_stop x
                    icall @may_stop x
                    stop
                }
            fn may_stop:
                entry c {
                    => c ? @halt : @ok
                }
                halt {
                    stop
                }
                ok {
                    iret
                }
            "#,
            r#"
            @0
            @1
                #0 stop []
            @2
            @3
                #0 const []
                #1 ret_dest_push []
                #2 icall [#0, #1]
                #3 ret_dest_push []
                #4 icall [#0, #2, #3]
                #5 stop [#4]
            "#,
        );
    }

    #[test]
    fn maybe_reverting_icall_commutes_with_storage_writes() {
        assert_graphs(
            r#"
            fn init:
                entry {
                    sstore 0 0
                    icall @may_revert 0
                    sstore 0 0
                    stop
                }
            fn may_revert:
                entry c {
                    => c ? @bad : @ok
                }
                bad {
                    revert 0 0
                }
                ok {
                    iret
                }
            "#,
            r#"
            @0
            @1
                #0 const []
                #1 const []
                #2 revert [#0, #1]
            @2
            @3
                #0 const []
                #1 const []
                #2 sstore [#0, #1]
                #3 const []
                #4 ret_dest_push []
                #5 icall [#3, #4]
                #6 const []
                #7 const []
                #8 sstore [#2, #6, #7]
                #9 stop [#5, #8]
            "#,
        );
    }

    #[test]
    fn revert_orders_after_memory_writes_not_storage_writes() {
        assert_graphs(
            r#"
            fn init:
                entry {
                    p = const 0
                    sstore p p
                    mstore256 p p
                    revert p p
                }
            "#,
            r#"
            @0
                #0 const []
                #1 sstore [#0]
                #2 mstore [#0]
                #3 revert [#0, #2]
            "#,
        );
    }

    #[test]
    fn maybe_reverting_icall_with_no_terminator_at_block_end() {
        assert_graphs(
            r#"
            fn init:
                entry {
                    x = const 1
                    icall @may_revert x
                    => @next
                }
                next {
                    stop
                }
            fn may_revert:
                entry c {
                    => c ? @bad : @ok
                }
                bad {
                    z = const 0
                    revert z z
                }
                ok {
                    iret
                }
            "#,
            r#"
            @0
            @1
                #0 const []
                #1 revert [#0]
            @2
            @3
                #0 const []
                #1 ret_dest_push []
                #2 icall [#0, #1]
            @4
                #0 stop []
            "#,
        );
    }

    #[test]
    fn major_effect_callees_chain_despite_revert() {
        assert_graphs(
            r#"
            fn init:
                entry {
                    x = const 1
                    icall @store_may_revert x
                    icall @store_may_revert x
                    y = sload x
                    stop
                }
            fn store_may_revert:
                entry c {
                    k = const 0
                    sstore k c
                    => c ? @bad : @ok
                }
                bad {
                    z = const 0
                    revert z z
                }
                ok {
                    iret
                }
            "#,
            r#"
            @0
                #0 const []
                #1 sstore [#0]
            @1
                #0 const []
                #1 revert [#0]
            @2
            @3
                #0 const []
                #1 ret_dest_push []
                #2 icall [#0, #1]
                #3 ret_dest_push []
                #4 icall [#0, #2, #3]
                #5 sload [#0, #4]
                #6 stop [#2, #4, #5]
            "#,
        );
    }

    #[test]
    fn transient_channel_independent_of_persistent() {
        assert_graphs(
            r#"
            fn init:
                entry {
                    k = const 0
                    a = tload k
                    b = tload k
                    sstore k k
                    tstore k a
                    c = tload k
                    stop
                }
            "#,
            r#"
            @0
                #0 const []
                #1 tload [#0]
                #2 tload [#0]
                #3 sstore [#0]
                #4 tstore [#0, #1, #2]
                #5 tload [#0, #4]
                #6 stop [#3, #4, #5]
            "#,
        );
    }

    #[test]
    fn returndata_reads_commute_between_writes() {
        assert_graphs(
            r#"
            fn init:
                entry {
                    z = const 0
                    r1 = staticcall z z z z z z
                    a = returndatasize
                    b = returndatasize
                    r2 = staticcall z z z z z z
                    stop
                }
            "#,
            r#"
            @0
                #0 const []
                #1 staticcall [#0]
                #2 returndatasize [#1]
                #3 returndatasize [#1]
                #4 staticcall [#0, #1, #2, #3]
                #5 stop [#1, #4]
            "#,
        );
    }

    #[test]
    fn allocations_commute_until_free_pointer_acquired() {
        assert_graphs(
            r#"
            fn init:
                entry {
                    a = mallocany 32
                    b = mallocany 32
                    f = freeptr
                    mstore256 a 3
                    mstore256 b 4
                    mstore256 f 0xffff
                    x = mload256 f
                    c = malloc 32
                    d = mload256 c
                    stop
                }
            "#,
            r#"
            @0
                #0 const []
                #1 mallocany [#0]
                #2 const []
                #3 mallocany [#2]
                #4 freeptr [#1, #3]
                #5 const []
                #6 mstore [#1, #5]
                #7 const []
                #8 mstore [#3, #6, #7]
                #9 const []
                #10 mstore [#4, #8, #9]
                #11 mload [#4, #10]
                #12 const []
                #13 malloc [#4, #12]
                #14 mload [#10, #13]
                #15 stop [#4, #10, #11, #13, #14]
            "#,
        );
    }

    #[test]
    fn looping_callee_treated_as_maybe_reverting() {
        assert_graphs(
            r#"
            fn init:
                entry {
                    x = const 1
                    icall @spin x
                    icall @spin x
                    stop
                }
            fn spin:
                entry c {
                    => @loop
                }
                loop {
                    t = const 1
                    => t ? @loop : @done
                }
                done {
                    iret
                }
            "#,
            r#"
            @0
            @1
                #0 const []
            @2
            @3
                #0 const []
                #1 ret_dest_push []
                #2 icall [#1]
                #3 ret_dest_push []
                #4 icall [#3]
                #5 stop [#2, #4]
            "#,
        );
    }

    #[test]
    fn icall_effects_from_function_effect_analysis() {
        assert_graphs(
            r#"
            fn init:
                entry {
                    x = const 1
                    icall @store_it x
                    icall @store_it x
                    y = icall @pure_id x
                    stop
                }
            fn store_it:
                entry v {
                    k = const 0
                    sstore k v
                    iret
                }
            fn pure_id:
                entry a -> a {
                    iret
                }
            "#,
            r#"
            @0
                #0 const []
                #1 sstore [#0]
            @1
            @2
                #0 const []
                #1 ret_dest_push []
                #2 icall [#0, #1]
                #3 ret_dest_push []
                #4 icall [#0, #2, #3]
                #5 ret_dest_push []
                #6 icall [#0, #5]
                #7 stop [#4]
            "#,
        );
    }
}
