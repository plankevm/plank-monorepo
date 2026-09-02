use plank_core::{Idx, IncIterable, IndexVec, Span};
use sir_data::{
    Control, EthIRProgram, LargeConstId, LocalId, Operation, OperationIdx,
    operation::{SetLargeConstData, SetSmallConstData},
};

#[derive(Clone, Copy)]
enum Constant {
    Small(u32),
    Large(LargeConstId),
}

impl Constant {
    fn operation(self, output: LocalId) -> Operation {
        match self {
            Self::Small(value) => {
                Operation::SetSmallConst(SetSmallConstData { sets: output, value })
            }
            Self::Large(value) => {
                Operation::SetLargeConst(SetLargeConstData { sets: output, value })
            }
        }
    }
}

pub fn inline_constants_at_each_use(program: &mut EthIRProgram) {
    let mut constants = IndexVec::<LocalId, Option<Constant>>::new();
    constants.resize(program.next_free_local_id.idx(), None);
    for operation in program.operations.iter() {
        match operation {
            Operation::SetSmallConst(data) => {
                constants[data.sets] = Some(Constant::Small(data.value))
            }
            Operation::SetLargeConst(data) => {
                constants[data.sets] = Some(Constant::Large(data.value))
            }
            _ => {}
        }
    }

    let old_operations = std::mem::take(&mut program.operations);
    let mut operations = IndexVec::<OperationIdx, Operation>::with_capacity(old_operations.len());
    let block_ids = program.basic_blocks.iter_idx().collect::<Vec<_>>();

    for block_id in block_ids {
        let old_span = program.basic_blocks[block_id].operations;
        let new_start = operations.next_idx();

        for operation_id in old_span.iter() {
            let mut operation = old_operations[operation_id];
            if matches!(operation, Operation::SetSmallConst(_) | Operation::SetLargeConst(_)) {
                continue;
            }

            let (locals, next_local) = (&mut program.locals, &mut program.next_free_local_id);
            for input in operation.inputs_mut(locals) {
                let Some(constant) = constants.get(*input).copied().flatten() else { continue };
                let fresh = next_local.get_and_inc();
                operations.push(constant.operation(fresh));
                *input = fresh;
            }
            operations.push(operation);
        }

        let outputs = program.basic_blocks[block_id].outputs;
        for output_index in outputs.iter() {
            let original = program.locals[output_index];
            let Some(constant) = constants.get(original).copied().flatten() else { continue };
            let fresh = program.next_free_local_id.get_and_inc();
            operations.push(constant.operation(fresh));
            program.locals[output_index] = fresh;
        }

        let control_input = match program.basic_blocks[block_id].control {
            Control::Branches(branch) => Some(branch.condition),
            Control::Switch(switch) => Some(switch.condition),
            Control::LastOpTerminates | Control::InternalReturn | Control::ContinuesTo(_) => None,
        };
        if let Some(original) = control_input
            && let Some(constant) = constants.get(original).copied().flatten()
        {
            let fresh = program.next_free_local_id.get_and_inc();
            operations.push(constant.operation(fresh));
            match &mut program.basic_blocks[block_id].control {
                Control::Branches(branch) => branch.condition = fresh,
                Control::Switch(switch) => switch.condition = fresh,
                Control::LastOpTerminates | Control::InternalReturn | Control::ContinuesTo(_) => {
                    unreachable!()
                }
            }
        }

        program.basic_blocks[block_id].operations = Span::new(new_start, operations.next_idx());
    }

    program.operations = operations;
}

#[cfg(test)]
mod tests {
    use super::*;
    use plank_test_utils::dedent_preserve_indent;
    use sir_parser::{EmitConfig, parse_or_panic};
    use sir_passes::{AnalysesStore, Legalizer};

    #[track_caller]
    fn assert_inlines_to(input: &str, expected: &str) {
        let mut program = parse_or_panic(input, EmitConfig::init_only());
        inline_constants_at_each_use(&mut program);
        Legalizer::default().run(&program, &AnalysesStore::default()).unwrap();
        pretty_assertions::assert_str_eq!(
            dedent_preserve_indent(&format!("{program}")),
            dedent_preserve_indent(expected),
        );
    }

    #[test]
    fn clones_a_small_constant_before_each_operation_input() {
        assert_inlines_to(
            r#"
            fn init:
                entry {
                    constant = const 7
                    sum = add constant constant
                    stop
                }
            "#,
            r#"
            fn init:
                bb0 {
                    v2 = const 7
                    v3 = const 7
                    v1 = add v2 v3
                    stop
                }
            "#,
        );
    }

    #[test]
    fn clones_a_large_constant_for_block_output_and_control_uses() {
        assert_inlines_to(
            r#"
            fn init:
                entry -> constant {
                    constant = large_const 0x100000000
                    => constant ? @non_zero : @zero
                }
                non_zero non_zero_input {
                    stop
                }
                zero zero_input {
                    stop
                }
            "#,
            r#"
            fn init:
                bb0 -> v3 {
                    v3 = large_const 0x100000000
                    v4 = large_const 0x100000000
                    => v4 ? @bb1 : @bb2
                }
                bb2 v2 {
                    stop
                }
                bb1 v1 {
                    stop
                }
            "#,
        );
    }
}
