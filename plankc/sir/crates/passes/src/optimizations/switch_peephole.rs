use crate::{AnalysesStore, Pass};
use sir_data::{Branch, Control, EthIRProgram, Switch};

#[derive(Default)]
pub struct SwitchPeephole;

impl Pass for SwitchPeephole {
    fn run(&mut self, program: &mut EthIRProgram, _store: &AnalysesStore) {
        for bb in program.basic_blocks.iter_idx() {
            let Control::Switch(Switch { cases, fallback, condition }) =
                program.basic_blocks[bb].control
            else {
                continue;
            };

            let Some(fallback) = fallback else {
                continue;
            };

            let cases_data = program.cases[cases];
            if cases_data.cases_count != 1 {
                continue;
            }

            let Some((case_value, target)) = cases_data.iter(program).next() else {
                continue;
            };

            if !case_value.is_zero() {
                continue;
            }

            program.basic_blocks[bb].control = Control::Branches(Branch {
                condition,
                non_zero_target: fallback,
                zero_target: target,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::run_pass_and_display;
    use sir_data::assert_ir_display;

    #[test]
    fn lowers_zero_case_with_default_to_branch() {
        let actual = run_pass_and_display::<SwitchPeephole>(
            r#"
            fn init:
                a {
                    sel = const 0
                    switch sel {
                        0 => @b
                        default => @c
                    }
                }
                b {
                    => @c
                }
                c {
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
                    $0 = const 0x0
                    => $0 ? @2 : @1
                }

                @1 {
                    => @2
                }

                @2 {
                    stop
                }
            "#,
        );
    }

    #[test]
    fn does_not_lower_non_zero_case_with_default() {
        let input = r#"
            fn init:
                a {
                    sel = const 0
                    switch sel {
                        1 => @c
                        default => @d
                    }
                }
                b {
                    => @c
                }
                c {
                    stop
                }
                d {
                    stop
                }
        "#;

        let actual = run_pass_and_display::<SwitchPeephole>(input);
        assert_ir_display(
            &actual,
            r#"
            Init: @0
            Functions:
                fn @0 -> entry @0  (outputs: 0)

            Basic Blocks:
                @0 {
                    $0 = const 0x0
                    switch $0 {
                        0x1 => @2,
                        else => @3
                    }

                }

                @1 {
                    => @2
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
    fn does_not_lower_multi_case_switch_with_zero_case_and_default() {
        let input = r#"
            fn init:
                a {
                    sel = const 0
                    switch sel {
                        0 => @b
                        1 => @c
                        default => @d
                    }
                }
                b {
                    => @c
                }
                c {
                    stop
                }
                d {
                    stop
                }
        "#;

        let actual = run_pass_and_display::<SwitchPeephole>(input);
        assert_ir_display(
            &actual,
            r#"
            Init: @0
            Functions:
                fn @0 -> entry @0  (outputs: 0)

            Basic Blocks:
                @0 {
                    $0 = const 0x0
                    switch $0 {
                        0x0 => @1,
                        0x1 => @2,
                        else => @3
                    }

                }

                @1 {
                    => @2
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
}
