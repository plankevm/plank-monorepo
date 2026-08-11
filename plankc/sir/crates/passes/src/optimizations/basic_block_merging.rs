use plank_core::{DenseIndexSet, Span};
use sir_data::{BasicBlockId, Control, EthIRProgram, Operation, operation::InlineOperands};

use crate::{AnalysesMask, AnalysesStore, Pass, Predecessors};

#[derive(Default)]
pub struct BasicBlockMerger {
    entries: DenseIndexSet<BasicBlockId>,
}

impl Pass for BasicBlockMerger {
    fn run(&mut self, program: &mut EthIRProgram, store: &AnalysesStore) {
        self.entries.clear();
        let rpo = store.reverse_post_order(program);
        for &fn_id in rpo.functions_rpo() {
            self.entries.add(program.functions[fn_id].entry());
        }

        let mut predecessors = store.predecessors_mut(program);

        for &curr in rpo.blocks_rpo() {
            // skip blocks that have been merged
            if predecessors.of(curr).is_empty() && !self.entries.contains(curr) {
                continue;
            }

            if let Control::ContinuesTo(succ) = program.basic_blocks[curr].control
                && predecessors.of(succ) == [curr]
                && !self.entries.contains(succ)
            {
                self.merge_chain(curr, program, &mut predecessors);
            }
        }

        drop(predecessors);
        store.predecessors.mark_valid();
    }

    fn preserves(&self) -> AnalysesMask {
        AnalysesMask::FunctionEffects | AnalysesMask::Predecessors
    }
}

impl BasicBlockMerger {
    fn merge_chain(
        &mut self,
        head: BasicBlockId,
        program: &mut EthIRProgram,
        predecessors: &mut Predecessors,
    ) {
        let operations_start = program.operations.next_idx();
        let mut current = head;
        loop {
            for op in program.basic_blocks[current].operations {
                program.clone_operation(op);
            }

            let Control::ContinuesTo(succ) = program.basic_blocks[current].control else {
                break;
            };
            if predecessors.of(succ) != [current] || self.entries.contains(succ) {
                break;
            }

            for (succ_input, current_output) in program.basic_blocks[succ]
                .inputs
                .into_iter()
                .zip(program.basic_blocks[current].outputs)
            {
                program.operations.push(Operation::SetCopy(InlineOperands {
                    ins: [program.locals[current_output]],
                    outs: [program.locals[succ_input]],
                }));
            }
            predecessors.clear_predecessors(succ);
            current = succ;
        }

        program.basic_blocks[head].operations =
            Span::new(operations_start, program.operations.next_idx());

        let outputs_start = program.locals.next_idx();
        for idx in program.basic_blocks[current].outputs {
            program.locals.push(program.locals[idx]);
        }
        program.basic_blocks[head].outputs = Span::new(outputs_start, program.locals.next_idx());
        program.basic_blocks[head].control = program.basic_blocks[current].control;

        for bb in program.block(current).successors() {
            predecessors.replace_predecessor_edge(bb, current, head);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::BasicBlockMerger;
    use crate::{AnalysesStore, Legalizer, run_pass};
    use sir_data::{EthIRProgram, assert_ir_display};
    use sir_parser::{EmitConfig, parse_or_panic};

    fn merge(source: &str) -> EthIRProgram {
        let mut program = parse_or_panic(source, EmitConfig::init_only());
        let store = AnalysesStore::default();
        run_pass(&mut BasicBlockMerger::default(), &mut program, &store);
        Legalizer::default().run(&program, &store).unwrap_or_else(|err| {
            panic!("legalization failed after block merging: {err}\n{program}")
        });
        program
    }

    #[test]
    fn merges_linear_chain() {
        let actual = merge(
            r#"
            fn init:
                entry -> entry_out {
                    entry_out = const 1
                    => @middle
                }
                middle middle_in -> middle_out {
                    two = const 2
                    middle_out = add middle_in two
                    => @exit
                }
                exit exit_in {
                    result = mul exit_in exit_in
                    stop
                }
            "#,
        );

        assert_ir_display(
            &actual,
            r#"
            Init: @0
            Functions:
                fn @0 -> entry @0  (outputs: 0)

            Basic Blocks:
                @0 {
                    $0 = const 0x1
                    $1 = copy $0
                    $2 = const 0x2
                    $3 = add $1 $2
                    $4 = copy $3
                    $5 = mul $4 $4
                    stop
                }

                @1 $1 -> $3 {
                    $2 = const 0x2
                    $3 = add $1 $2
                    => @2
                }

                @2 $4 {
                    $5 = mul $4 $4
                    stop
                }
            "#,
        );
    }

    #[test]
    fn merges_branch() {
        let actual = merge(
            r#"
            fn init:
                entry -> condition {
                    condition = const 1
                    => @dispatch
                }
                dispatch dispatch_condition {
                    => dispatch_condition ? @nonzero : @zero
                }
                nonzero { stop }
                zero { stop }
            "#,
        );

        assert_ir_display(
            &actual,
            r#"
            Init: @0
            Functions:
                fn @0 -> entry @0  (outputs: 0)

            Basic Blocks:
                @0 {
                    $0 = const 0x1
                    $1 = copy $0
                    => $1 ? @2 : @3
                }

                @1 $1 {
                    => $1 ? @2 : @3
                }

                @2 {
                    stop
                }

                @3 {
                    stop
                }
            "#,
        );
    }

    #[test]
    fn merges_switch() {
        let actual = merge(
            r#"
            fn init:
                entry -> selector {
                    selector = const 1
                    => @dispatch
                }
                dispatch dispatch_selector {
                    switch dispatch_selector {
                        0 => @zero
                        1 => @one
                        default => @fallback
                    }
                }
                zero { stop }
                one { stop }
                fallback { stop }
            "#,
        );

        assert_ir_display(
            &actual,
            r#"
            Init: @0
            Functions:
                fn @0 -> entry @0  (outputs: 0)

            Basic Blocks:
                @0 {
                    $0 = const 0x1
                    $1 = copy $0
                    switch $1 {
                        0x0 => @2,
                        0x1 => @3,
                        else => @4
                    }

                }

                @1 $1 {
                    switch $1 {
                        0x0 => @2,
                        0x1 => @3,
                        else => @4
                    }

                }

                @2 {
                    stop
                }

                @3 {
                    stop
                }

                @4 {
                    stop
                }
            "#,
        );
    }

    #[test]
    fn merges_block_with_descendant_use() {
        let actual = merge(
            r#"
            fn init:
                entry -> value {
                    value = const 1
                    => @dispatch
                }
                dispatch dispatch_value {
                    condition = const 0
                    => condition ? @use_value : @done
                }
                use_value {
                    result = add dispatch_value dispatch_value
                    stop
                }
                done { stop }
            "#,
        );

        assert_ir_display(
            &actual,
            r#"
            Init: @0
            Functions:
                fn @0 -> entry @0  (outputs: 0)

            Basic Blocks:
                @0 {
                    $0 = const 0x1
                    $1 = copy $0
                    $2 = const 0x0
                    => $2 ? @2 : @3
                }

                @1 $1 {
                    $2 = const 0x0
                    => $2 ? @2 : @3
                }

                @2 {
                    $3 = add $1 $1
                    stop
                }

                @3 {
                    stop
                }
            "#,
        );
    }

    #[test]
    fn does_not_merge_block_with_multiple_predecessors() {
        let actual = merge(
            r#"
            fn init:
                entry {
                    condition = const 1
                    => condition ? @left : @right
                }
                left -> left_out {
                    left_out = const 2
                    => @join
                }
                right -> right_out {
                    right_out = const 3
                    => @join
                }
                join input {
                    stop
                }
            "#,
        );

        assert_ir_display(
            &actual,
            r#"
            Init: @0
            Functions:
                fn @0 -> entry @0  (outputs: 0)

            Basic Blocks:
                @0 {
                    $0 = const 0x1
                    => $0 ? @1 : @2
                }

                @1 -> $1 {
                    $1 = const 0x2
                    => @3
                }

                @2 -> $2 {
                    $2 = const 0x3
                    => @3
                }

                @3 $3 {
                    stop
                }
            "#,
        );
    }

    #[test]
    fn does_not_merge_function_entry() {
        let actual = merge(
            r#"
            fn init:
                entry {
                    condition = const 0
                    => condition ? @backedge : @exit
                }
                backedge {
                    => @entry
                }
                exit { stop }
            "#,
        );

        assert_ir_display(
            &actual,
            r#"
            Init: @0
            Functions:
                fn @0 -> entry @0  (outputs: 0)

            Basic Blocks:
                @0 {
                    $0 = const 0x0
                    => $0 ? @1 : @2
                }

                @1 {
                    => @0
                }

                @2 {
                    stop
                }
            "#,
        );
    }

    #[test]
    fn merges_cycle_into_self_loop() {
        let actual = merge(
            r#"
            fn init:
                entry { => @cycle }
                cycle { => @entry }
            "#,
        );

        assert_ir_display(
            &actual,
            r#"
            Init: @0
            Functions:
                fn @0 -> entry @0  (outputs: 0)

            Basic Blocks:
                @0 {
                    => @0
                }

                @1 {
                    => @0
                }
            "#,
        );
    }
}
