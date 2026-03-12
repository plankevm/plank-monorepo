use crate::{AnalysesStore, Pass, analyses::Predecessors};
use plank_core::IncIterable;
use sir_data::{BasicBlock, BasicBlockId, Branch, Control, EthIRProgram, LocalIdx, Span, Switch};

#[derive(Default)]
pub struct CriticalEdgeSplitting;

impl Pass for CriticalEdgeSplitting {
    fn run(&mut self, program: &mut EthIRProgram, store: &AnalysesStore) {
        let predecessors = store.predecessors(program);

        for bb in program.basic_blocks.iter_idx() {
            match program.basic_blocks[bb].control {
                Control::Branches(Branch { condition, non_zero_target, zero_target }) => {
                    program.basic_blocks[bb].control = Control::Branches(Branch {
                        condition,
                        non_zero_target: split_edge(program, &predecessors, bb, non_zero_target),
                        zero_target: split_edge(program, &predecessors, bb, zero_target),
                    });
                }
                Control::Switch(Switch { cases, fallback, .. }) => {
                    let cases_data = program.cases[cases];
                    for target_idx in cases_data.target_indices().iter() {
                        let target = program.cases_bb_ids[target_idx];
                        program.cases_bb_ids[target_idx] =
                            split_edge(program, &predecessors, bb, target);
                    }
                    if let Some(fallback) = fallback {
                        let new_fallback = split_edge(program, &predecessors, bb, fallback);
                        let Control::Switch(ref mut switch) = program.basic_blocks[bb].control
                        else {
                            unreachable!()
                        };
                        switch.fallback = Some(new_fallback);
                    }
                }
                _ => {}
            }
        }
    }
}

fn split_edge(
    program: &mut EthIRProgram,
    predecessors: &Predecessors,
    source: BasicBlockId,
    target: BasicBlockId,
) -> BasicBlockId {
    if predecessors.of(target).len() > 1 {
        insert_forwarding_block(program, source, target)
    } else {
        target
    }
}

fn insert_forwarding_block(
    program: &mut EthIRProgram,
    source: BasicBlockId,
    target: BasicBlockId,
) -> BasicBlockId {
    let source_outputs = program.basic_blocks[source].outputs;
    let empty_ops = Span::new(program.operations.next_idx(), program.operations.next_idx());

    let inputs_start = program.locals.next_idx();
    for _ in source_outputs.iter() {
        program.locals.push(program.next_free_local_id.get_and_inc());
    }
    let inputs = Span::new(inputs_start, program.locals.next_idx());
    let outputs = copy_span(program, inputs);

    program.basic_blocks.push(BasicBlock {
        inputs,
        outputs,
        operations: empty_ops,
        control: Control::ContinuesTo(target),
    })
}

fn copy_span(program: &mut EthIRProgram, span: Span<LocalIdx>) -> Span<LocalIdx> {
    let start = program.locals.next_idx();
    for idx in span.iter() {
        program.locals.push(program.locals[idx]);
    }
    Span::new(start, program.locals.next_idx())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::run_pass;
    use sir_parser::EmitConfig;

    #[test]
    fn test_switch_critical_edge() {
        //     A
        //    / \
        //   B   C
        //  / \  |
        // E   D-+
        //
        // B uses a switch, so B→D is a critical edge (B has multiple successors, D has multiple
        // predecessors).
        let mut program = sir_parser::parse_or_panic(
            r#"
            fn init:
                a {
                    cond = const 0
                    => cond ? @b : @c
                }
                b {
                    sel = const 0
                    switch sel {
                        0 => @d
                        default => @e
                    }
                }
                c {
                    => @d
                }
                d {
                    stop
                }
                e {
                    stop
                }
            "#,
            EmitConfig::init_only(),
        );

        let original_block_count = program.basic_blocks.len();
        let store = AnalysesStore::default();
        run_pass(&mut CriticalEdgeSplitting, &mut program, &store);

        assert!(
            program.basic_blocks.len() > original_block_count,
            "critical edge splitting should insert forwarding blocks"
        );
    }
}
