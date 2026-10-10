use plank_test_utils::dedent_preserve_blank_lines;
use sir_parser::EmitConfig;
use sir_passes::AnalysesStore;

#[track_caller]
fn assert_asm(source: &str, config: EmitConfig<'_>, expected: &str) {
    let program = sir_parser::parse_or_panic(source, config);
    let analyses = AnalysesStore::default();
    let (asm, _) = crate::ir_to_asm(&program, &analyses);
    let expected = dedent_preserve_blank_lines(expected);
    pretty_assertions::assert_str_eq!(asm.to_string().trim(), expected.trim());
}

#[test]
fn linear_blocks_fall_through() {
    assert_asm(
        r#"
        fn init:
            entry {
                => @next
            }
            next {
                stop
            }
        "#,
        EmitConfig::init_only(),
        r#"
            .mark2:
            .mark3:
              STOP
            .mark0:
            .mark1:
        "#,
    );
}

#[test]
fn zero_branch_falls_through() {
    assert_asm(
        r#"
        fn init:
            entry {
                condition = calldatasize
                => condition ? @non_zero : @zero
            }
            non_zero {
                invalid
            }
            zero {
                stop
            }
        "#,
        EmitConfig::init_only(),
        r#"
            .mark2:
              CALLDATASIZE
              PUSH .mark3
              JUMPI
            .mark4:
              STOP
            .mark3:
              JUMPDEST
              INVALID
            .mark0:
            .mark1:
        "#,
    );
}

#[test]
fn switch_fallback_falls_through() {
    assert_asm(
        r#"
        fn init:
            entry {
                selector = calldatasize
                switch selector {
                    0 => @case
                    default => @fallback
                }
            }
            case {
                invalid
            }
            fallback {
                stop
            }
        "#,
        EmitConfig::init_only(),
        r#"
            .mark2:
              CALLDATASIZE
              PUSH0
              MSTORE
              PUSH0
              MLOAD
              PUSH0
              EQ
              PUSH .mark3
              JUMPI
            .mark4:
              STOP
            .mark3:
              JUMPDEST
              INVALID
            .mark0:
            .mark1:
        "#,
    );
}

#[test]
fn switch_case_falls_through_when_fallback_is_unavailable() {
    assert_asm(
        r#"
        fn init:
            entry {
                => @dispatch
            }
            dispatch {
                selector = calldatasize
                switch selector {
                    0 => @case_zero
                    1 => @case_one
                    default => @entry
                }
            }
            case_zero {
                invalid
            }
            case_one {
                stop
            }
        "#,
        EmitConfig::init_only(),
        r#"
            .mark2:
              JUMPDEST
            .mark3:
              CALLDATASIZE
              PUSH0
              MSTORE
              PUSH0
              MLOAD
              PUSH1 0x01
              EQ
              PUSH .mark5
              JUMPI
              PUSH0
              MLOAD
              PUSH0
              EQ
              ISZERO
              PUSH .mark2
              JUMPI
            .mark4:
              INVALID
            .mark5:
              JUMPDEST
              STOP
            .mark0:
            .mark1:
        "#,
    );
}

#[test]
fn switch_case_falls_through_without_fallback() {
    assert_asm(
        r#"
        fn init:
            entry {
                selector = calldatasize
                switch selector {
                    0 => @case_zero
                    1 => @case_one
                }
            }
            case_zero {
                invalid
            }
            case_one {
                stop
            }
        "#,
        EmitConfig::init_only(),
        r#"
            .mark2:
              CALLDATASIZE
              PUSH0
              MSTORE
              PUSH0
              MLOAD
              PUSH1 0x01
              EQ
              PUSH .mark4
              JUMPI
            .mark3:
              INVALID
            .mark4:
              JUMPDEST
              STOP
            .mark0:
            .mark1:
        "#,
    );
}

#[test]
fn join_keeps_jumpdest() {
    assert_asm(
        r#"
        fn init:
            entry {
                condition = calldatasize
                => condition ? @left : @right
            }
            left {
                => @join
            }
            right {
                => @join
            }
            join {
                stop
            }
        "#,
        EmitConfig::init_only(),
        r#"
            .mark2:
              CALLDATASIZE
              PUSH .mark3
              JUMPI
            .mark4:
            .mark5:
              JUMPDEST
              STOP
            .mark3:
              JUMPDEST
              PUSH .mark5
              JUMP
            .mark0:
            .mark1:
        "#,
    );
}

#[test]
fn cycle_keeps_jumpdest() {
    assert_asm(
        r#"
        fn init:
            entry {
                => @loop
            }
            loop {
                => @entry
            }
        "#,
        EmitConfig::init_only(),
        r#"
            .mark2:
              JUMPDEST
            .mark3:
              PUSH .mark2
              JUMP
            .mark0:
            .mark1:
        "#,
    );
}

#[test]
fn internal_call_entry_keeps_jumpdest() {
    assert_asm(
        r#"
        fn init:
            entry {
                icall @helper
                stop
            }
        fn helper:
            entry {
                iret
            }
        "#,
        EmitConfig::init_only(),
        r#"
            .mark3:
              PUSH .mark4
            .mark2:
              JUMPDEST
              JUMP
            .mark4:
              JUMPDEST
              STOP
            .mark0:
            .mark1:
        "#,
    );
}

#[test]
fn runtime_entry_does_not_need_jumpdest() {
    assert_asm(
        r#"
        fn init:
            entry {
                stop
            }
        fn main:
            entry {
                stop
            }
        "#,
        EmitConfig::new("init", "main"),
        r#"
            .mark2:
              STOP
            .mark0:
            .mark5:
              STOP
            .mark1:
        "#,
    );
}

#[test]
fn first_internal_call_falls_through_to_callee() {
    assert_asm(
        r#"
        fn init:
            entry {
                icall @helper
                icall @helper
                stop
            }
        fn helper:
            entry {
                iret
            }
        "#,
        EmitConfig::init_only(),
        r#"
            .mark3:
              PUSH .mark4
            .mark2:
              JUMPDEST
              JUMP
            .mark4:
              JUMPDEST
              PUSH .mark5
              PUSH .mark2
              JUMP
            .mark5:
              JUMPDEST
              STOP
            .mark0:
            .mark1:
        "#,
    );
}

#[test]
fn callee_entry_follows_loop_latch() {
    assert_asm(
        r#"
        fn init:
            entry {
                icall @helper
                stop
            }
        fn helper:
            header {
                condition = gas
                => condition ? @latch : @exit
            }
            latch {
                => @header
            }
            exit {
                iret
            }
        "#,
        EmitConfig::init_only(),
        r#"
            .mark5:
              PUSH .mark6
              PUSH .mark2
              JUMP
            .mark6:
              JUMPDEST
              STOP
            .mark3:
              JUMPDEST
            .mark2:
              JUMPDEST
              GAS
              PUSH .mark3
              JUMPI
            .mark4:
              JUMP
            .mark0:
            .mark1:
        "#,
    );
}

#[test]
fn init_and_runtime_layouts_do_not_share_state() {
    assert_asm(
        r#"
        fn init:
            entry {
                icall @init_helper
                stop
            }
        fn init_helper:
            entry {
                iret
            }
        fn main:
            entry {
                icall @runtime_helper
                stop
            }
        fn runtime_helper:
            entry {
                iret
            }
        "#,
        EmitConfig::new("init", "main"),
        r#"
            .mark3:
              PUSH .mark6
            .mark2:
              JUMPDEST
              JUMP
            .mark6:
              JUMPDEST
              STOP
            .mark0:
            .mark10:
              PUSH (.mark11 - .mark0)
            .mark9:
              JUMPDEST
              JUMP
            .mark11:
              JUMPDEST
              STOP
            .mark1:
        "#,
    );
}

#[test]
fn never_call_wrapper_is_replaced_with_prepared_argument() {
    assert_asm(
        r#"
        fn init:
            entry -> value {
                value = caller
                condition = calldatasize
                => condition ? @terminate : @continue
            }
            terminate argument {
                icall_never @halt argument
            }
            continue unused {
                stop
            }
        fn halt:
            entry offset {
                revert offset offset
            }
        "#,
        EmitConfig::init_only(),
        r#"
            .mark3:
              CALLER
              CALLDATASIZE
              PUSH .mark2
              JUMPI
            .mark5:
              STOP
            .mark2:
              JUMPDEST
              DUP1
              REVERT
            .mark0:
            .mark1:
        "#,
    );
}

#[test]
fn never_call_wrapper_with_assigned_predecessor_is_retained() {
    assert_asm(
        r#"
        fn init:
            entry {
                condition = calldatasize
                => condition ? @continue : @terminate
            }
            terminate {
                icall_never @halt
            }
            continue {
                stop
            }
        fn halt:
            entry {
                invalid
            }
        "#,
        EmitConfig::init_only(),
        r#"
            .mark3:
              CALLDATASIZE
              PUSH .mark5
              JUMPI
            .mark4:
            .mark2:
              JUMPDEST
              INVALID
            .mark5:
              JUMPDEST
              STOP
            .mark0:
            .mark1:
        "#,
    );
}

#[test]
fn never_call_wrapper_with_extra_operations_is_not_replaced() {
    assert_asm(
        r#"
        fn init:
            entry {
                condition = calldatasize
                => condition ? @terminate : @continue
            }
            terminate {
                value = const 0
                icall_never @halt value
            }
            continue {
                stop
            }
        fn halt:
            entry offset {
                revert offset offset
            }
        "#,
        EmitConfig::init_only(),
        r#"
            .mark3:
              CALLDATASIZE
              PUSH .mark4
              JUMPI
            .mark5:
              STOP
            .mark2:
              JUMPDEST
              DUP1
              REVERT
            .mark4:
              JUMPDEST
              PUSH0
              PUSH .mark2
              JUMP
            .mark0:
            .mark1:
        "#,
    );
}
