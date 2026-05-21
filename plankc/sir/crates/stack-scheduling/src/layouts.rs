use hashbrown::HashSet;
use plank_core::{DenseIndexMap, newtype_index};
use sir_data::{BasicBlockId, ControlView, EthIRProgram, FunctionId, LocalId};
use sir_passes::{
    AnalysesStore, ControlFlowGraphInOutBundling, InOutGroupId, analyses::Unreachable,
};

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

    // Sort to restore determinism.
    for (_, set) in layout_sets.iter_mut() {
        set.members_fifo.sort();
    }

    layout_sets
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn return_dest_falls_to_bottom() {
        let mut members = vec![
            LayoutMember::InputOutput(3),
            LayoutMember::ReturnDest,
            LayoutMember::Local(LocalId::new(34)),
            LayoutMember::InputOutput(2),
        ];
        members.sort();

        assert_eq!(
            members,
            &[
                LayoutMember::ReturnDest,
                LayoutMember::InputOutput(2),
                LayoutMember::InputOutput(3),
                LayoutMember::Local(LocalId::new(34)),
            ]
        )
    }
}
