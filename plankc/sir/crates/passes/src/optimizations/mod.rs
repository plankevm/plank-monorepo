pub(crate) mod constant_propagation;
pub(crate) mod copy_propagation;
pub(crate) mod defragmenter;
pub(crate) mod unused_operation_elimination;

pub use defragmenter::Defragmenter;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OptimizationPass {
    Sccp,
    CopyPropagation,
    UnusedElimination,
    Defragment,
}

impl OptimizationPass {
    pub fn from_char(c: char) -> Option<Self> {
        match c {
            's' => Some(Self::Sccp),
            'c' => Some(Self::CopyPropagation),
            'u' => Some(Self::UnusedElimination),
            'd' => Some(Self::Defragment),
            _ => None,
        }
    }
}

pub const OPTIMIZE_HELP: &str = "Optimization passes to run in order. Each character is a pass:\n\
    s = SCCP (constant propagation),\n\
    c = copy propagation,\n\
    u = unused operation elimination,\n\
    d = defragment.\n\
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
    use sir_parser::{EmitConfig, parse_or_panic};
    use sir_test_utils::assert_trim_strings_eq_with_diff;

    fn optimize(source: &str, passes: &str) -> String {
        let mut program = parse_or_panic(source, EmitConfig::init_only());
        PassManager::new(&mut program).run_optimizations(passes);
        sir_data::display_program(&program)
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
        let expected = r#"
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
        "#;

        let actual = optimize(SWITCH_ON_COPY_WITH_DEAD_CODE, "csud");
        assert_trim_strings_eq_with_diff(&actual, expected, "csud");
    }

    #[test]
    fn test_cusd() {
        let expected = r#"
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
        "#;

        let actual = optimize(SWITCH_ON_COPY_WITH_DEAD_CODE, "cusd");
        assert_trim_strings_eq_with_diff(&actual, expected, "cusd");
    }

    #[test]
    fn test_ucsd() {
        let expected = r#"
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
        "#;

        let actual = optimize(SWITCH_ON_COPY_WITH_DEAD_CODE, "ucsd");
        assert_trim_strings_eq_with_diff(&actual, expected, "ucsd");
    }

    #[test]
    fn test_uscd() {
        let expected = r#"
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
        "#;

        let actual = optimize(SWITCH_ON_COPY_WITH_DEAD_CODE, "uscd");
        assert_trim_strings_eq_with_diff(&actual, expected, "uscd");
    }

    #[test]
    fn test_scsud() {
        let expected = r#"
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
        "#;

        let actual = optimize(SWITCH_ON_COPY_WITH_DEAD_CODE, "scsud");
        assert_trim_strings_eq_with_diff(&actual, expected, "scsud");
    }
}
