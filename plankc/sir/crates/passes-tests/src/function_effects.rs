use pretty_assertions::assert_eq;
use sir_data::operation::effects::Effect;
use sir_parser::{EmitConfig, parse_or_panic_with_sources};
use sir_passes::AnalysesStore;

fn assert_function_effects(
    source: &str,
    config: EmitConfig<'_>,
    expected: impl AsRef<[(&'static str, Effect)]>,
) {
    let (program, sources) = parse_or_panic_with_sources(source, config);
    let store = AnalysesStore::default();
    let effects = store.function_effects(&program);

    for &(name, expected) in expected.as_ref() {
        let fn_id = sources
            .function_by_name(&program, name)
            .unwrap_or_else(|| panic!("function {name:?} not found"));

        assert_eq!(effects.effect_of(fn_id), expected, "effect mismatch for function {name:?}",);
    }
}

#[test]
fn simple() {
    assert_function_effects(
        r#"
        fn init:
            entry {
                y = const 0
                x = add y y
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
                y = const 0
                sstore y y
                icall @simple
                stop
            }

        fn simple:
            simple_entry {
                c0 = const 0
                y = mload256 c0
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
        [("icall", Effect::TERMINATE), ("infinity", Effect::REVERT)],
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
                c32 = const 32
                ptr = mallocany c32
                b = mstore256
                => @end
            }
            b {
                c0 = const 0
                sstore c0 c0
                => @end
            }
            end {
                iret
            }
        "#,
        EmitConfig::init_only(),
        [("init", Effect::TERMINATE), ("diamond", Effect::MEMORY_WRITE | Effect::PERSISTENT_WRITE)],
    );
}

#[test]
fn simplifies() {
    assert_function_effects(
        r#"
        fn init:
            init {

            }

        "#,
        EmitConfig::default(),
        [],
    );
}
