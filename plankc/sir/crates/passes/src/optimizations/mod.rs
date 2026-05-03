pub(crate) mod constant_propagation;
pub(crate) mod copy_propagation;
pub(crate) mod defragmenter;
pub(crate) mod switch_peephole;
pub(crate) mod unused_operation_elimination;

pub use defragmenter::Defragmenter;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OptimizationPass {
    Sccp,
    CopyPropagation,
    UnusedElimination,
    Defragment,
    SwitchPeephole,
}

impl OptimizationPass {
    pub fn from_char(c: char) -> Option<Self> {
        match c {
            's' => Some(Self::Sccp),
            'c' => Some(Self::CopyPropagation),
            'u' => Some(Self::UnusedElimination),
            'd' => Some(Self::Defragment),
            'l' => Some(Self::SwitchPeephole),
            _ => None,
        }
    }
}

pub const OPTIMIZE_HELP: &str = "Optimization passes to run in order. Each character is a pass:\n\
    s = SCCP (constant propagation),\n\
    c = copy propagation,\n\
    u = unused operation elimination,\n\
    d = defragment.\n\
    l = switch peephole \n\
    Example: -O csud";

pub fn parse_optimizations_string(s: &str) -> Result<String, String> {
    for c in s.chars() {
        if OptimizationPass::from_char(c).is_none() {
            return Err(format!(
                "invalid optimization pass '{}', valid passes: s (SCCP), c (copy propagation), u (unused elimination), d (defragment)",
                c
            ));
        }
    }
    Ok(s.to_string())
}

#[cfg(test)]
mod tests {
    use crate::PassManager;
    use sir_data::assert_ir_display;
    use sir_parser::{EmitConfig, parse_or_panic};

    fn optimize(source: &str, passes: &str) -> sir_data::EthIRProgram {
        let mut program = parse_or_panic(source, EmitConfig::init_only());
        PassManager::new(&mut program).run_optimizations(passes);
        program
    }

    const SWITCH_ON_COPY_WITH_DEAD_CODE: &str = r#"
        fn init:
            entry {
                x = const 1
                y = copy x
                switch y {
                    1 => @one
                    default => @other
                }
            }
            one {
                dead = const 42
                stop
            }
            other {
                cond = const 0
                => cond ? @other_yes : @one
            }
            other_yes { stop }
    "#;

    #[test]
    fn test_csud() {
        let actual = optimize(SWITCH_ON_COPY_WITH_DEAD_CODE, "csud");
        assert_ir_display(
            &actual,
            r#"
            Init: @0
            Functions:
                fn @0 -> entry @0  (outputs: 0)

            Basic Blocks:
                @0 {
                    => @1
                }

                @1 {
                    stop
                }
            "#,
        );
    }

    #[test]
    fn test_cusd() {
        let actual = optimize(SWITCH_ON_COPY_WITH_DEAD_CODE, "cusd");
        assert_ir_display(
            &actual,
            r#"
            Init: @0
            Functions:
                fn @0 -> entry @0  (outputs: 0)

            Basic Blocks:
                @0 {
                    $0 = const 0x1
                    => @1
                }

                @1 {
                    stop
                }
            "#,
        );
    }

    #[test]
    fn test_ucsd() {
        let actual = optimize(SWITCH_ON_COPY_WITH_DEAD_CODE, "ucsd");
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
                    => @1
                }

                @1 {
                    stop
                }
            "#,
        );
    }

    #[test]
    fn test_uscd() {
        let actual = optimize(SWITCH_ON_COPY_WITH_DEAD_CODE, "uscd");
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
                    switch $0 {
                        0x1 => @1,
                        else => @2
                    }

                }

                @1 {
                    stop
                }

                @2 {
                    $2 = const 0x0
                    => @1
                }
            "#,
        );
    }

    #[test]
    fn test_scsud() {
        let actual = optimize(SWITCH_ON_COPY_WITH_DEAD_CODE, "scsud");
        assert_ir_display(
            &actual,
            r#"
            Init: @0
            Functions:
                fn @0 -> entry @0  (outputs: 0)

            Basic Blocks:
                @0 {
                    => @1
                }

                @1 {
                    stop
                }
            "#,
        );
    }
}
