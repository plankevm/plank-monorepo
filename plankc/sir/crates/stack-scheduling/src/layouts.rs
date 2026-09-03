use crate::LayoutOrdering;
use hashbrown::HashSet;
use plank_core::{DenseIndexMap, DenseIndexSet, newtype_index};
use sir_data::{BasicBlockId, ControlView, EthIRProgram, FunctionId, LocalId, Operation};
use sir_passes::{
    AnalysesStore, ControlFlowGraphInOutBundling, InOutGroupId, analyses::Unreachable,
};
use std::collections::BTreeMap;

newtype_index! {
    pub(crate) struct LayoutIdx;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum LayoutMember {
    ReturnDest,
    InputOutput(u32),
    Local(LocalId),
}

#[derive(Debug, Default)]
pub struct Layout {
    members_fifo: Vec<LayoutMember>,
}

impl Layout {
    const EMPTY: &'static Layout = &Layout { members_fifo: Vec::new() };

    fn add(&mut self, member: LayoutMember) -> bool {
        if self.members_fifo.contains(&member) {
            return false;
        }
        self.members_fifo.push(member);
        true
    }

    pub fn members_fifo(&self) -> &[LayoutMember] {
        &self.members_fifo
    }
}

impl std::ops::Deref for Layout {
    type Target = [LayoutMember];

    fn deref(&self) -> &Self::Target {
        &self.members_fifo
    }
}

pub struct LayoutsTracker<'ir> {
    cfg_layouts: DenseIndexMap<InOutGroupId, Layout>,
    function_dest_position: DenseIndexMap<FunctionId, u16>,
    in_out_bundling: ControlFlowGraphInOutBundling,
    program: &'ir EthIRProgram,
}

impl<'ir> LayoutsTracker<'ir> {
    pub fn new(
        program: &'ir EthIRProgram,
        cfg_layouts: DenseIndexMap<InOutGroupId, Layout>,
        in_out_bundling: ControlFlowGraphInOutBundling,
    ) -> Self {
        let mut tracker = Self {
            cfg_layouts,
            function_dest_position: DenseIndexMap::with_capacity(program.functions.len()),
            in_out_bundling,
            program,
        };
        tracker.refresh_function_dest_positions();
        tracker
    }

    pub fn get_input_layout(&self, bb: BasicBlockId) -> &Layout {
        let Some(group) = self.in_out_bundling.get_in_group(bb) else {
            unreachable!("getting input layout for block without IO group");
        };
        &self.cfg_layouts[group]
    }

    pub fn get_input_output(&self, bb: BasicBlockId) -> Option<(&Layout, &Layout)> {
        let in_group = self.in_out_bundling.get_in_group(bb)?;
        let out_group = self.in_out_bundling.get_out_group(bb)?;

        let input_layout = self.cfg_layouts.get(in_group)?;
        let output_layout = self.cfg_layouts.get(out_group).unwrap_or(Layout::EMPTY);
        Some((input_layout, output_layout))
    }

    pub fn get_function_dest_position(&self, function: FunctionId) -> Option<u16> {
        self.function_dest_position.get(function).copied()
    }

    fn refresh_function_dest_positions(&mut self) {
        for func in self.program.functions_iter() {
            let Some(in_group) = self.in_out_bundling.get_in_group(func.entry().id()) else {
                continue;
            };
            let stack_layout = &self.cfg_layouts[in_group];
            if let Some(position) =
                stack_layout.iter().position(|&member| member == LayoutMember::ReturnDest)
            {
                self.function_dest_position.insert(func.id(), position.try_into().unwrap());
            } else {
                self.function_dest_position.remove(func.id());
            }
        }
    }
}

pub fn build_basic_block_layout_sets(
    program: &EthIRProgram,
    analyses: &AnalysesStore,
    in_out_bundling: &ControlFlowGraphInOutBundling,
) -> DenseIndexMap<InOutGroupId, Layout> {
    let liveness = analyses.local_liveness(program);
    let ownership = analyses.basic_block_ownership(program);
    let mut layout_sets = DenseIndexMap::<InOutGroupId, Layout>::with_capacity(
        in_out_bundling.total_groups() as usize,
    );

    for bb in program.blocks() {
        let owner = match ownership.get_owner(bb.id()) {
            Ok(owner) => owner,
            Err(Unreachable) => continue,
        };

        // `iret` needs to get special-cased because it's a terminator in terms of the CFG but its
        // outputs matter because
        if matches!(bb.control(), ControlView::InternalReturn)
            && let Some(out_group) = in_out_bundling.get_out_group(bb.id())
        {
            let layout = layout_sets.entry(out_group).or_insert_default();
            for i in 0..bb.outputs().len() {
                layout.add(LayoutMember::InputOutput(i as u32));
            }
        }

        let Some(in_group) = in_out_bundling.get_in_group(bb.id()) else { continue };

        // Blocks will request their dependencies on the input side so we don't need to do anything
        // extra on the output side, also let's the output layout for terminating blocks be
        // naturally empty.

        let layout = layout_sets.entry(in_group).or_insert_default();

        if owner != program.init_entry && Some(owner) != program.main_entry {
            layout.add(LayoutMember::ReturnDest);
        }

        // WARNING: Iteration over `HashSet` is non-deterministic, must sort!!!
        for &local in liveness.get_live_at_entry(bb.id()) as &HashSet<LocalId> {
            layout.add('member: {
                for (&input, i) in bb.inputs().iter().zip(0..) {
                    if input == local {
                        break 'member LayoutMember::InputOutput(i);
                    }
                }
                LayoutMember::Local(local)
            });
        }
    }

    // Restore determinism after collecting members from liveness sets.
    for (_, layout) in layout_sets.iter_mut() {
        layout.members_fifo.sort();
    }

    layout_sets
}

pub(crate) fn order_layouts(
    program: &EthIRProgram,
    analyses: &AnalysesStore,
    in_out_bundling: &ControlFlowGraphInOutBundling,
    layouts: &mut DenseIndexMap<InOutGroupId, Layout>,
    ordering: LayoutOrdering,
) {
    if ordering == LayoutOrdering::Naive {
        return;
    }

    let mut input_blocks = DenseIndexMap::<InOutGroupId, Vec<BasicBlockId>>::with_capacity(
        in_out_bundling.total_groups() as usize,
    );
    let mut output_blocks = DenseIndexMap::<InOutGroupId, Vec<BasicBlockId>>::with_capacity(
        in_out_bundling.total_groups() as usize,
    );
    for block in program.blocks() {
        if let Some(group) = in_out_bundling.get_in_group(block.id()) {
            input_blocks.entry(group).or_insert_default().push(block.id());
        }
        if ordering == LayoutOrdering::SuccessorAndProducer
            && let Some(group) = in_out_bundling.get_out_group(block.id())
        {
            output_blocks.entry(group).or_insert_default().push(block.id());
        }
    }

    let mut ordered = DenseIndexSet::with_capacity_in_bits(in_out_bundling.total_groups() as usize);
    let rpo = analyses.reverse_post_order(program);
    let callee_first_groups =
        rpo.blocks_postorder().filter_map(|&block| in_out_bundling.get_in_group(block));
    for group in callee_first_groups.chain(in_out_bundling.iter_groups()) {
        if !ordered.add(group) {
            continue;
        }
        order_layout_group(
            group,
            program,
            in_out_bundling,
            &input_blocks,
            &output_blocks,
            layouts,
            ordering,
        );
    }
}

fn order_layout_group(
    group: InOutGroupId,
    program: &EthIRProgram,
    in_out_bundling: &ControlFlowGraphInOutBundling,
    input_blocks: &DenseIndexMap<InOutGroupId, Vec<BasicBlockId>>,
    output_blocks: &DenseIndexMap<InOutGroupId, Vec<BasicBlockId>>,
    layouts: &mut DenseIndexMap<InOutGroupId, Layout>,
    ordering: LayoutOrdering,
) {
    let Some(layout) = layouts.get(group) else {
        return;
    };
    // Each block ranks touched members on the common layout-width scale. Producer ranks, when
    // enabled, contribute equally with successor ranks.
    let mut scores = vec![0usize; layout.len()];
    if ordering == LayoutOrdering::SuccessorAndProducer
        && let Some(blocks) = output_blocks.get(group)
    {
        for &block_id in blocks {
            let creations = creations_in_block(program, block_id, &layouts[group]);
            add_desirability(&mut scores, &creations);
        }
    }
    if let Some(blocks) = input_blocks.get(group) {
        for &block_id in blocks {
            let first_uses =
                first_uses_in_block(program, block_id, &layouts[group], in_out_bundling, layouts);
            add_desirability(&mut scores, &first_uses);
        }
    }

    let members = std::mem::take(&mut layouts[group].members_fifo);
    let mut ranked_members = members.into_iter().zip(scores).collect::<Vec<_>>();
    ranked_members.sort_by(|&(left, left_score), &(right, right_score)| {
        match (left == LayoutMember::ReturnDest, right == LayoutMember::ReturnDest) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => right_score.cmp(&left_score).then_with(|| left.cmp(&right)),
        }
    });
    layouts[group].members_fifo = ranked_members.into_iter().map(|(member, _)| member).collect();
}

fn add_desirability(scores: &mut [usize], priority: &[usize]) {
    let layout_size = scores.len();
    for (rank, &member_index) in priority.iter().enumerate() {
        scores[member_index] += layout_size - rank;
    }
}

fn index_layout_locals(layout: &Layout, positional_locals: &[LocalId]) -> BTreeMap<LocalId, usize> {
    let mut local_to_member = BTreeMap::new();
    for (member_index, &member) in layout.members_fifo().iter().enumerate() {
        let local = match member {
            LayoutMember::ReturnDest => continue,
            LayoutMember::InputOutput(position) => positional_locals[position as usize],
            LayoutMember::Local(local) => local,
        };
        local_to_member.insert(local, member_index);
    }
    local_to_member
}

fn creations_in_block(
    program: &EthIRProgram,
    block_id: BasicBlockId,
    output_layout: &Layout,
) -> Vec<usize> {
    let block = program.block(block_id);
    let local_to_member = index_layout_locals(output_layout, block.outputs());
    let mut seen = vec![false; output_layout.len()];
    let mut creations = Vec::with_capacity(output_layout.len());
    for operation in block.operations().rev() {
        for &output in operation.outputs() {
            let Some(&member_index) = local_to_member.get(&output) else {
                continue;
            };
            if !seen[member_index] {
                seen[member_index] = true;
                creations.push(member_index);
            }
        }
    }
    creations
}

fn first_uses_in_block(
    program: &EthIRProgram,
    block_id: BasicBlockId,
    input_layout: &Layout,
    in_out_bundling: &ControlFlowGraphInOutBundling,
    layouts: &DenseIndexMap<InOutGroupId, Layout>,
) -> Vec<usize> {
    let block = program.block(block_id);
    let local_to_member = index_layout_locals(input_layout, block.inputs());
    let mut seen = vec![false; input_layout.len()];
    let mut first_uses = Vec::with_capacity(input_layout.len());
    let mut record_use = |local: LocalId| {
        let Some(&member_index) = local_to_member.get(&local) else {
            return;
        };
        if !seen[member_index] {
            seen[member_index] = true;
            first_uses.push(member_index);
        }
    };

    for operation in block.operations() {
        match operation.op() {
            Operation::InternalCall(call) => {
                let callee_entry = program.function(call.function).entry().id();
                let callee_group = in_out_bundling
                    .get_in_group(callee_entry)
                    .expect("internal-call target without an input layout group");
                let call_inputs = call.get_inputs(program);
                for &member in layouts[callee_group].members_fifo() {
                    match member {
                        LayoutMember::ReturnDest => {}
                        LayoutMember::InputOutput(position) => {
                            record_use(call_inputs[position as usize]);
                        }
                        LayoutMember::Local(_) => {
                            unreachable!("function entry should not have non-input members")
                        }
                    }
                }
            }
            _ => {
                for &input in operation.inputs() {
                    record_use(input);
                }
            }
        }
    }

    match block.control() {
        ControlView::Branches { condition, .. } => record_use(condition),
        ControlView::Switch(switch) => record_use(switch.condition()),
        ControlView::LastOpTerminates
        | ControlView::InternalReturn
        | ControlView::ContinuesTo(_) => {}
    }

    first_uses
}

#[cfg(test)]
mod tests {
    use super::*;
    use sir_parser::{EmitConfig, parse_or_panic, parse_or_panic_with_sources};

    fn build_layouts(
        program: &EthIRProgram,
        analyses: &AnalysesStore,
        ordering: LayoutOrdering,
    ) -> (ControlFlowGraphInOutBundling, DenseIndexMap<InOutGroupId, Layout>) {
        let bundling = ControlFlowGraphInOutBundling::new(program, analyses);
        let mut layouts = build_basic_block_layout_sets(program, analyses, &bundling);
        order_layouts(program, analyses, &bundling, &mut layouts, ordering);
        (bundling, layouts)
    }

    #[test]
    fn orders_layout_by_first_operand_use() {
        let program = parse_or_panic(
            r#"
            fn init:
                entry -> first second {
                    first = caller
                    second = callvalue
                    => @use
                }
                use lhs rhs {
                    result = sub rhs lhs
                    stop
                }
            "#,
            EmitConfig::init_only(),
        );
        let analyses = AnalysesStore::default();
        let (naive_bundling, naive_layouts) =
            build_layouts(&program, &analyses, LayoutOrdering::Naive);
        let use_block = BasicBlockId::new(1);
        let naive_group = naive_bundling.get_in_group(use_block).unwrap();
        assert_eq!(
            naive_layouts[naive_group].members_fifo(),
            &[LayoutMember::InputOutput(0), LayoutMember::InputOutput(1)]
        );

        let (bundling, layouts) = build_layouts(&program, &analyses, LayoutOrdering::SuccessorOnly);
        let group = bundling.get_in_group(use_block).unwrap();

        assert_eq!(
            layouts[group].members_fifo(),
            &[LayoutMember::InputOutput(1), LayoutMember::InputOutput(0)]
        );
    }

    #[test]
    fn combines_successor_and_producer_scores() {
        let program = parse_or_panic(
            r#"
            fn init:
                entry -> first second third {
                    first = caller
                    second = callvalue
                    third = calldatasize
                    condition = returndatasize
                    => condition ? @left : @right
                }
                left a b c {
                    use_a = iszero a
                    use_b = iszero b
                    use_c = iszero c
                    stop
                }
                right x y z {
                    use_z = iszero z
                    use_x = iszero x
                    stop
                }
            "#,
            EmitConfig::init_only(),
        );
        let analyses = AnalysesStore::default();
        let (successor_bundling, successor_layouts) =
            build_layouts(&program, &analyses, LayoutOrdering::SuccessorOnly);
        let left = BasicBlockId::new(1);
        let right = BasicBlockId::new(2);
        let successor_group = successor_bundling.get_in_group(left).unwrap();

        assert_eq!(successor_bundling.get_in_group(right), Some(successor_group));
        assert_eq!(
            successor_layouts[successor_group].members_fifo(),
            &[
                LayoutMember::InputOutput(0),
                LayoutMember::InputOutput(2),
                LayoutMember::InputOutput(1),
            ]
        );

        let (bundling, layouts) =
            build_layouts(&program, &analyses, LayoutOrdering::SuccessorAndProducer);
        let group = bundling.get_in_group(left).unwrap();

        assert_eq!(bundling.get_in_group(right), Some(group));
        assert_eq!(
            layouts[group].members_fifo(),
            &[
                LayoutMember::InputOutput(2),
                LayoutMember::InputOutput(0),
                LayoutMember::InputOutput(1),
            ]
        );
    }

    #[test]
    fn internal_call_uses_callee_entry_order() {
        let (program, sources) = parse_or_panic_with_sources(
            r#"
            fn init:
                entry -> first second {
                    first = caller
                    second = callvalue
                    => @call
                }
                call arg0 arg1 {
                    result = icall @callee arg0 arg1
                    stop
                }
            fn callee:
                entry lhs rhs -> result {
                    result = sub rhs lhs
                    iret
                }
            "#,
            EmitConfig::init_only(),
        );
        let analyses = AnalysesStore::default();
        let (bundling, layouts) = build_layouts(&program, &analyses, LayoutOrdering::SuccessorOnly);
        let init_entry = program.function(program.init_entry).entry();
        let ControlView::ContinuesTo(call_block) = init_entry.control() else {
            panic!("expected init entry to continue to call block");
        };
        let callee = sources.function_by_name(&program, "callee").unwrap();
        let callee_entry = program.function(callee).entry().id();

        assert_eq!(
            layouts[bundling.get_in_group(callee_entry).unwrap()].members_fifo(),
            &[LayoutMember::ReturnDest, LayoutMember::InputOutput(1), LayoutMember::InputOutput(0),]
        );
        assert_eq!(
            layouts[bundling.get_in_group(call_block).unwrap()].members_fifo(),
            &[LayoutMember::InputOutput(1), LayoutMember::InputOutput(0)]
        );
    }
}
