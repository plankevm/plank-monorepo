use crate::{CallArgumentStrategy, LayoutOrdering};
use hashbrown::HashSet;
use plank_core::{DenseIndexMap, DenseIndexSet, newtype_index};
use sir_data::{BasicBlockId, ControlView, EthIRProgram, FunctionId, LocalId, Operation};
use sir_passes::{
    AnalysesStore, ControlFlowGraphInOutBundling, InOutGroupId, analyses::Unreachable,
};
use std::collections::{BTreeMap, BTreeSet};

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

    fn remove(&mut self, member: LayoutMember) {
        self.members_fifo.retain(|&candidate| candidate != member);
    }
}

#[derive(Debug, Default)]
pub(crate) struct GlobalSpills {
    values: Vec<GlobalSpill>,
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum GlobalSpill {
    Layout(InOutGroupId, LayoutMember),
    CallArgument(FunctionId, u32),
}

impl GlobalSpills {
    pub fn len(&self) -> usize {
        self.values.len()
    }

    pub fn for_group(
        &self,
        group: InOutGroupId,
    ) -> impl Iterator<Item = (usize, LayoutMember)> + '_ {
        self.values.iter().enumerate().filter_map(move |(index, &spill)| {
            let GlobalSpill::Layout(candidate_group, member) = spill else { return None };
            (candidate_group == group).then_some((index, member))
        })
    }

    pub fn for_function(&self, function: FunctionId) -> impl Iterator<Item = (usize, u32)> + '_ {
        self.values.iter().enumerate().filter_map(move |(index, &spill)| {
            let GlobalSpill::CallArgument(candidate_function, position) = spill else {
                return None;
            };
            (candidate_function == function).then_some((index, position))
        })
    }

    pub fn iter(&self) -> impl Iterator<Item = GlobalSpill> + '_ {
        self.values.iter().copied()
    }

    pub fn remove_from_layouts(
        &self,
        program: &EthIRProgram,
        in_out_bundling: &ControlFlowGraphInOutBundling,
        layouts: &mut DenseIndexMap<InOutGroupId, Layout>,
    ) {
        for &spill in &self.values {
            let (group, member) = match spill {
                GlobalSpill::Layout(group, member) => (group, member),
                GlobalSpill::CallArgument(function, position) => {
                    let entry = program.function(function).entry().id();
                    let group = in_out_bundling
                        .get_in_group(entry)
                        .expect("called function without entry layout");
                    (group, LayoutMember::InputOutput(position))
                }
            };
            layouts[group].remove(member);
        }
    }

    pub fn extend_call_arguments(
        &mut self,
        arguments: impl IntoIterator<Item = (FunctionId, u32)>,
    ) {
        self.values.extend(
            arguments
                .into_iter()
                .map(|(function, position)| GlobalSpill::CallArgument(function, position)),
        );
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

    pub(crate) fn get_input_group(&self, bb: BasicBlockId) -> Option<InOutGroupId> {
        self.in_out_bundling.get_in_group(bb)
    }

    pub(crate) fn get_output_group(&self, bb: BasicBlockId) -> Option<InOutGroupId> {
        self.in_out_bundling.get_out_group(bb)
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

fn max_callsite_overflow(
    program: &EthIRProgram,
    analyses: &AnalysesStore,
    in_out_bundling: &ControlFlowGraphInOutBundling,
    layouts: &DenseIndexMap<InOutGroupId, Layout>,
    max_swap_depth: usize,
) -> DenseIndexMap<FunctionId, usize> {
    let liveness = analyses.local_liveness(program);
    let rpo = analyses.reverse_post_order(program);
    let mut max_overflow_by_callee = DenseIndexMap::<FunctionId, usize>::new();

    for &block_id in rpo.blocks_rpo() {
        let block = program.block(block_id);
        let mut live = liveness.get_live_at_exit(block_id).clone();
        match block.control() {
            ControlView::Branches { condition, .. } => {
                live.insert(condition);
            }
            ControlView::Switch(switch) => {
                live.insert(switch.condition());
            }
            ControlView::InternalReturn => live.extend(block.outputs()),
            ControlView::LastOpTerminates | ControlView::ContinuesTo(_) => {}
        }

        for operation in block.operations().rev() {
            if let Operation::InternalCall(call) = operation.op() {
                let callee_entry = program.function(call.function).entry().id();
                let callee_group = in_out_bundling
                    .get_in_group(callee_entry)
                    .expect("internal-call target without an input layout group");
                let entry_layout = &layouts[callee_group];
                let live_across = live.len()
                    - operation.outputs().iter().filter(|&&output| live.contains(&output)).count();
                let pressure = live_across + entry_layout.len();
                let accessible_entries = max_swap_depth.saturating_add(1);
                let overflow = pressure.saturating_sub(accessible_entries);

                if overflow != 0 {
                    let max_overflow =
                        max_overflow_by_callee.entry(call.function).or_insert_default();
                    *max_overflow = (*max_overflow).max(overflow);
                }
            }

            for output in operation.outputs() {
                live.remove(output);
            }
            live.extend(operation.inputs());
        }
    }

    max_overflow_by_callee
}

pub(crate) fn select_memory_call_arguments(
    program: &EthIRProgram,
    analyses: &AnalysesStore,
    in_out_bundling: &ControlFlowGraphInOutBundling,
    layouts: &DenseIndexMap<InOutGroupId, Layout>,
    max_swap_depth: usize,
    strategy: CallArgumentStrategy,
) -> Vec<(FunctionId, u32)> {
    if strategy == CallArgumentStrategy::StackOnly {
        return Vec::new();
    }
    let max_overflow_by_callee =
        max_callsite_overflow(program, analyses, in_out_bundling, layouts, max_swap_depth);
    let predecessors = analyses.predecessors(program);
    let mut selected = Vec::new();
    for (function, &overflow) in max_overflow_by_callee.iter() {
        let entry = program.function(function).entry().id();
        if !predecessors.of(entry).is_empty() {
            continue;
        }
        let group =
            in_out_bundling.get_in_group(entry).expect("called function without entry layout");
        let arguments = layouts[group]
            .iter()
            .filter_map(|&member| match member {
                LayoutMember::InputOutput(position) => Some(position),
                LayoutMember::ReturnDest | LayoutMember::Local(_) => None,
            })
            .collect::<Vec<_>>();
        let count = match strategy {
            CallArgumentStrategy::StackOnly => unreachable!(),
            CallArgumentStrategy::FullReliefOnly if overflow > arguments.len() => 0,
            CallArgumentStrategy::FullReliefOnly | CallArgumentStrategy::PartialRelief => {
                overflow.min(arguments.len())
            }
        };
        selected
            .extend(arguments.into_iter().rev().take(count).map(|position| (function, position)));
    }
    selected
}

pub(crate) fn select_global_spills(
    program: &EthIRProgram,
    analyses: &AnalysesStore,
    in_out_bundling: &ControlFlowGraphInOutBundling,
    layouts: &DenseIndexMap<InOutGroupId, Layout>,
    access_limit: usize,
) -> GlobalSpills {
    let liveness = analyses.local_liveness(program);
    let mut input_blocks = DenseIndexMap::<InOutGroupId, Vec<BasicBlockId>>::with_capacity(
        in_out_bundling.total_groups() as usize,
    );
    let mut groups_with_predecessors =
        DenseIndexSet::with_capacity_in_bits(in_out_bundling.total_groups() as usize);
    let mut function_entry_groups =
        DenseIndexSet::with_capacity_in_bits(in_out_bundling.total_groups() as usize);
    for block in program.blocks() {
        if let Some(group) = in_out_bundling.get_in_group(block.id()) {
            input_blocks.entry(group).or_insert_default().push(block.id());
        }
        if let Some(group) = in_out_bundling.get_out_group(block.id()) {
            groups_with_predecessors.add(group);
        }
    }
    for function in program.functions_iter() {
        if let Some(group) = in_out_bundling.get_in_group(function.entry().id()) {
            function_entry_groups.add(group);
        }
    }

    let mut global_spills = GlobalSpills::default();
    for (group, layout) in layouts.iter() {
        if !groups_with_predecessors.contains(group) || function_entry_groups.contains(group) {
            continue;
        }
        let Some(blocks) = input_blocks.get(group) else { continue };

        let mut block_uses = Vec::with_capacity(blocks.len());
        let mut used_by_group = BTreeSet::new();
        for &block_id in blocks {
            let first_uses =
                first_uses_in_block(program, block_id, layout, in_out_bundling, layouts);
            used_by_group.extend(first_uses.iter().copied());
            block_uses.push((block_id, first_uses));
        }

        let mut candidates = BTreeSet::new();
        for (block_id, first_uses) in block_uses {
            let selected = obstructing_members_to_spill(
                program,
                &liveness,
                layout,
                block_id,
                &first_uses,
                &used_by_group,
                access_limit,
            );
            if !selected.is_empty() {
                candidates.extend(selected);
            }
        }

        global_spills.values.extend(
            candidates.into_iter().map(|member_index| {
                GlobalSpill::Layout(group, layout.members_fifo()[member_index])
            }),
        );
    }
    global_spills
}

fn obstructing_members_to_spill(
    program: &EthIRProgram,
    liveness: &sir_passes::analyses::LocalLiveness,
    layout: &Layout,
    block_id: BasicBlockId,
    first_uses: &[usize],
    used_by_group: &BTreeSet<usize>,
    access_limit: usize,
) -> Vec<usize> {
    let block = program.block(block_id);
    let used_here = first_uses.iter().copied().collect::<BTreeSet<_>>();
    let mut pending = used_here.clone();
    let mut stack = layout
        .members_fifo()
        .iter()
        .enumerate()
        .filter_map(|(member_index, &member)| {
            if member == LayoutMember::ReturnDest {
                return Some((member_index, true));
            }
            let local = layout_member_local(member, block.inputs())?;
            let live_out = liveness.get_live_at_exit(block_id).contains(&local);
            (used_here.contains(&member_index) || live_out).then_some((member_index, live_out))
        })
        .collect::<Vec<_>>();
    let mut selected = Vec::new();

    loop {
        let accessible = stack.len().min(access_limit.saturating_add(1));
        let mut removed = false;
        for position in 0..accessible {
            let (member_index, live_out) = stack[position];
            if !pending.remove(&member_index) {
                continue;
            }
            if !live_out {
                stack.remove(position);
                removed = true;
                break;
            }
        }
        if removed {
            continue;
        }
        if pending.is_empty() {
            return selected;
        }

        let Some(position) = (0..accessible).find(|&position| {
            let (member_index, live_out) = stack[position];
            live_out
                && layout.members_fifo()[member_index] != LayoutMember::ReturnDest
                && !used_by_group.contains(&member_index)
        }) else {
            return selected;
        };
        selected.push(stack.remove(position).0);
    }
}

pub(crate) fn layout_member_local(
    member: LayoutMember,
    positional_locals: &[LocalId],
) -> Option<LocalId> {
    match member {
        LayoutMember::ReturnDest => None,
        LayoutMember::InputOutput(position) => Some(positional_locals[position as usize]),
        LayoutMember::Local(local) => Some(local),
    }
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

    #[test]
    fn finds_value_unused_before_pressure_limit() {
        let program = parse_or_panic(
            r#"
            fn init:
                entry -> x0 y0 z0 target0 {
                    x0 = caller
                    y0 = callvalue
                    z0 = calldatasize
                    target0 = returndatasize
                    => @middle
                }
                middle x1 y1 z1 target1 -> x1 y1 z1 {
                    use_target = iszero target1
                    => @use
                }
                use x2 y2 z2 {
                    use_x = iszero x2
                    use_y = iszero y2
                    use_z = iszero z2
                    stop
                }
            "#,
            EmitConfig::init_only(),
        );
        let analyses = AnalysesStore::default();
        let (bundling, layouts) = build_layouts(&program, &analyses, LayoutOrdering::Naive);

        let constrained = select_global_spills(&program, &analyses, &bundling, &layouts, 2);
        assert_eq!(constrained.len(), 1);

        let reachable = select_global_spills(&program, &analyses, &bundling, &layouts, 3);
        assert_eq!(reachable.len(), 0);
    }

    #[test]
    fn does_not_spill_function_entry_loop_layout() {
        let program = parse_or_panic(
            r#"
            fn init:
                entry {
                    x = caller
                    y = callvalue
                    z = calldatasize
                    target = returndatasize
                    icall @loop x y z target
                    stop
                }
            fn loop:
                entry x0 y0 z0 target0 -> x0 y0 z0 target0 {
                    use_target = iszero target0
                    => @backedge
                }
                backedge x1 y1 z1 target1 -> x1 y1 z1 target1 {
                    => @entry
                }
            "#,
            EmitConfig::init_only(),
        );
        let analyses = AnalysesStore::default();
        let (bundling, layouts) = build_layouts(&program, &analyses, LayoutOrdering::Naive);
        let spills = select_global_spills(&program, &analyses, &bundling, &layouts, 2);
        assert_eq!(spills.len(), 0);
    }

    #[test]
    fn selects_call_arguments_from_caller_pressure() {
        let program = parse_or_panic(
            r#"
            fn init:
                entry {
                    p = caller
                    q = callvalue
                    r = calldatasize
                    a = returndatasize
                    b = gas
                    result = icall @callee a b
                    pq = add p q
                    pqr = add pq r
                    sstore pqr result
                    stop
                }
            fn callee:
                entry a b -> result {
                    result = add a b
                    iret
                }
            "#,
            EmitConfig::init_only(),
        );
        let analyses = AnalysesStore::default();
        let (bundling, layouts) = build_layouts(&program, &analyses, LayoutOrdering::Naive);
        let callee = program
            .operations()
            .find_map(|operation| match operation.op() {
                Operation::InternalCall(call) => Some(call.function),
                _ => None,
            })
            .unwrap();
        assert_eq!(
            select_memory_call_arguments(
                &program,
                &analyses,
                &bundling,
                &layouts,
                4,
                CallArgumentStrategy::FullReliefOnly,
            )
            .as_slice(),
            &[(callee, 1)]
        );

        assert!(
            select_memory_call_arguments(
                &program,
                &analyses,
                &bundling,
                &layouts,
                2,
                CallArgumentStrategy::FullReliefOnly,
            )
            .is_empty()
        );
        assert_eq!(
            select_memory_call_arguments(
                &program,
                &analyses,
                &bundling,
                &layouts,
                2,
                CallArgumentStrategy::PartialRelief,
            )
            .len(),
            2
        );
    }

    #[test]
    fn does_not_select_call_arguments_for_a_looping_function_entry() {
        let program = parse_or_panic(
            r#"
            fn init:
                entry {
                    p = caller
                    q = callvalue
                    r = calldatasize
                    a = returndatasize
                    b = gas
                    icall @callee a b
                    pq = add p q
                    pqr = add pq r
                    sstore pqr pqr
                    stop
                }
            fn callee:
                entry a0 b0 -> a0 b0 {
                    => @backedge
                }
                backedge a1 b1 -> a1 b1 {
                    => @entry
                }
            "#,
            EmitConfig::init_only(),
        );
        let analyses = AnalysesStore::default();
        let (bundling, layouts) = build_layouts(&program, &analyses, LayoutOrdering::Naive);

        assert!(
            select_memory_call_arguments(
                &program,
                &analyses,
                &bundling,
                &layouts,
                4,
                CallArgumentStrategy::PartialRelief,
            )
            .is_empty()
        );
    }
}
