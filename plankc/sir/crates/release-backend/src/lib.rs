use hashbrown::{self as _, HashSet}; // TODO: Remove
use plank_core::DenseIndexMap;
use sir_assembler::Assembler;
use sir_data::{BasicBlockId, EthIRProgram, FunctionId, LocalId, newtype_index};
use sir_passes::{
    AnalysesStore, ControlFlowGraphInOutBundling, InOutGroupId, analyses::Unreachable,
};

mod op_graph;
mod simple_instr_effects;

newtype_index! {
    pub(crate) struct LayoutIdx;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum LayoutMember {
    ReturnDest,
    InputOutput(u32),
    Local(LocalId),
}

#[derive(Default)]
pub(crate) struct Layout {
    members: Vec<LayoutMember>,
    stack_end: u16,
}

impl Layout {
    const EMPTY: &'static Layout = &Layout { members: Vec::new(), stack_end: 0 };

    fn add(&mut self, member: LayoutMember) -> bool {
        if self.members.contains(&member) {
            return false;
        }
        self.members.push(member);
        true
    }

    fn parts(&self) -> (&[LayoutMember], &[LayoutMember]) {
        self.members.split_at(self.stack_end as usize)
    }

    pub fn all_members(&self) -> &[LayoutMember] {
        &self.members
    }

    pub fn stack(&self) -> &[LayoutMember] {
        self.parts().0
    }

    pub fn spilled(&self) -> &[LayoutMember] {
        self.parts().1
    }
}

pub(crate) struct LayoutsTracker<'ir> {
    cfg_layouts: DenseIndexMap<InOutGroupId, Layout>,
    function_dest_position: DenseIndexMap<FunctionId, u16>,
    in_out_bundling: ControlFlowGraphInOutBundling,
    program: &'ir EthIRProgram,
}

impl<'ir> LayoutsTracker<'ir> {
    fn new(
        cfg_layouts: DenseIndexMap<InOutGroupId, Layout>,
        program: &'ir EthIRProgram,
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
        let Some(group) = self.in_out_bundling.get_in_group(bb) else { return Layout::EMPTY };
        &self.cfg_layouts[group]
    }

    fn refresh_function_dest_positions(&mut self) {
        for func in self.program.functions_iter() {
            let Some(in_group) = self.in_out_bundling.get_in_group(func.entry().id()) else {
                continue;
            };
            let stack_layout = self.cfg_layouts[in_group].stack();
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

pub fn lower(asm: &mut Assembler, program: &EthIRProgram, analyses: &AnalysesStore) {
    asm.clear();

    let liveness = analyses.local_liveness(program);
    let ownership = analyses.basic_block_ownership(program);
    let in_out_bundling = ControlFlowGraphInOutBundling::new(program, analyses);

    let mut layout_sets = DenseIndexMap::with_capacity(in_out_bundling.total_groups() as usize);

    for bb in program.blocks() {
        let owner = match ownership.get_owner(bb.id()) {
            Ok(owner) => owner,
            Err(Unreachable) => continue,
        };

        let Some(in_group) = in_out_bundling.get_in_group(bb.id()) else { continue };
        // Blocks will request their dependencies on the input side so we don't need to do anything
        // extra on the output side, also let's the output layout for terminating blocks be
        // naturally empty.

        if !layout_sets.contains(in_group) {
            layout_sets.insert_no_prev(in_group, Layout::default());
        }
        let layout = &mut layout_sets[in_group];

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
        set.members.sort();
        set.stack_end = set.members.len().try_into().unwrap();
    }

    let layouts = LayoutsTracker::new(layout_sets, program, in_out_bundling);
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
