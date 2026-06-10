use pretty_assertions::assert_eq;
use sir_data::operation::effects::Effect;
use sir_parser::{EmitConfig, parse_or_panic_with_sources};
use sir_passes::AnalysesStore;

#[track_caller]
fn assert_function_effects(
    source: &str,
    config: EmitConfig<'_>,
    expected: impl AsRef<[(&'static str, Effect)]>,
) {
    let (program, sources) = parse_or_panic_with_sources(source, config);
    let store = AnalysesStore::default();
    let effects = store.function_effects(&program);

    for &(name, expected) in expected.as_ref() {
        let Some(fn_id) = sources.function_by_name(&program, name) else {
            panic!("function {name:?} not found");
        };

        assert_eq!(effects.effect_of(fn_id), expected, "effect mismatch for function {name:?}",);
    }
}

#[test]
fn simple() {
    assert_function_effects(
        r#"
        fn init:
            entry {
                x = add 0 0
                icall @pure
                stop
            }

        fn pure:
            pure {
                iret
            }
        "#,
        EmitConfig::init_only(),
        [("init", Effect::TERMINATE), ("pure", Effect::PURE)],
    );
}

#[test]
fn composed_effects() {
    assert_function_effects(
        r#"
        fn init:
            entry {
                sstore 0 0
                icall @simple
                stop
            }

        fn simple:
            simple_entry {
                y = mload256 0
                iret
            }
        "#,
        EmitConfig::init_only(),
        [
            ("init", Effect::TERMINATE | Effect::PERSISTENT_WRITE | Effect::MEMORY_READ),
            ("simple", Effect::MEMORY_READ),
        ],
    );
}

#[test]
fn infinite_loop() {
    assert_function_effects(
        r#"
        fn init:
            entry {
                icall @infinity
                stop
            }

        fn infinity:
            infinity {
                => @infinity
            }
        "#,
        EmitConfig::init_only(),
        [("init", Effect::TERMINATE), ("infinity", Effect::REVERT)],
    );
}

#[test]
fn diamond() {
    assert_function_effects(
        r#"
        fn init:
            entry {
                icall @diamond
                stop
            }

        fn diamond:
            diamond {
                cv = callvalue
                => cv ? @a : @b
            }
            a {
                ptr = sallocany 32
                mstore256 ptr 34
                => @end
            }
            b {
                sstore 0 0
                => @end
            }
            end {
                iret
            }
        "#,
        EmitConfig::init_only(),
        [
            ("init", Effect::TERMINATE | Effect::MEMORY_WRITE | Effect::PERSISTENT_WRITE),
            ("diamond", Effect::MEMORY_WRITE | Effect::PERSISTENT_WRITE),
        ],
    );
}

#[test]
fn simplifies() {
    assert_function_effects(
        r#"
        fn init:
            init {
                ptr = mallocany 32
                mstore256 ptr 3333
                x = mload256 ptr
                stop
            }

        fn main:
            main {
                x = sload 0
                y = add x 1
                sstore 0 y
                => x ? @end : @rev
            }
            end {
                stop
            }
            rev {
                invalid
            }
        "#,
        EmitConfig::default(),
        [
            ("init", Effect::TERMINATE | Effect::MEMORY_WRITE | Effect::ALLOC_ADVANCE),
            ("main", Effect::TERMINATE | Effect::PERSISTENT_WRITE),
        ],
    );
}

#[test]
fn conservatively_assumes_loop_reverts() {
    assert_function_effects(
        r#"

        fn init:
            init {
                icall @write_range
                stop
            }

        fn write_range:
            start -> i {
                i = const 0
                => @body
            }
            body ii -> iii {
                sstore ii ii
                iii = add ii 1
                repeat = lt iii 10
                => repeat ? @body : @exit
            }
            exit _i {
                iret
            }
        "#,
        EmitConfig::init_only(),
        [
            ("init", Effect::TERMINATE | Effect::PERSISTENT_WRITE),
            ("write_range", Effect::REVERT | Effect::PERSISTENT_WRITE),
        ],
    );
}

#[test]
fn deep_calls() {
    assert_function_effects(
        r#"
        fn init:
            init {
                icall @a
                stop
            }

        fn a:
            a {
                sstore 0 0
                icall @b
                iret
            }

        fn b:
            b {
                x = selfbalance
                iret
            }
        "#,
        EmitConfig::init_only(),
        [
            ("init", Effect::TERMINATE | Effect::PERSISTENT_WRITE | Effect::ACCOUNTS_READ),
            ("a", Effect::PERSISTENT_WRITE | Effect::ACCOUNTS_READ),
            ("b", Effect::ACCOUNTS_READ),
        ],
    );
}

#[test]
fn multiple_calls() {
    assert_function_effects(
        r#"
        fn init:
            init {
                icall @b
                icall @a
                stop
            }

        fn a:
            a {
                sstore 0 0
                icall @b
                iret
            }

        fn b:
            b {
                x = selfbalance
                iret
            }
        "#,
        EmitConfig::init_only(),
        [
            ("init", Effect::TERMINATE | Effect::PERSISTENT_WRITE | Effect::ACCOUNTS_READ),
            ("a", Effect::PERSISTENT_WRITE | Effect::ACCOUNTS_READ),
            ("b", Effect::ACCOUNTS_READ),
        ],
    );
}

#[test]
fn switch() {
    assert_function_effects(
        r#"
        fn init:
            init {
                icall @inner
                stop
            }

        fn inner:
            entry {
                v = callvalue
                switch v {
                    0 => @a
                    1 => @b
                    default => @c
                }
            }
            a {
                log0 0 0
                iret
            }
            b {
                x = mload256 34
                iret
            }
            c {
                sstore 3 67
                iret
            }

        "#,
        EmitConfig::init_only(),
        [
            (
                "init",
                Effect::MEMORY_READ | Effect::PERSISTENT_WRITE | Effect::TERMINATE | Effect::LOGS,
            ),
            ("inner", Effect::MEMORY_READ | Effect::PERSISTENT_WRITE | Effect::LOGS),
        ],
    );
}
