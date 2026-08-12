use super::{ScheduleConfig, searching_schedule};
use crate::{
    layouts::{LayoutsTracker, build_basic_block_layout_sets},
    op_graph::build_graph_effectful,
    stack::{ShuffleConfig, StackOps},
    tests::format_scheduled_block,
};
use plank_test_utils::dedent_preserve_blank_lines;
use sir_data::StaticAllocId;
use sir_parser::{EmitConfig, parse_or_panic_with_sources};
use sir_passes::{AnalysesStore, ControlFlowGraphInOutBundling};
use std::{collections::HashSet, num::NonZero};

const TEST_BEAM_WIDTH: usize = 16;

#[track_caller]
fn assert_searches_to(config: ShuffleConfig, block_source: &str, expected: &str) {
    let block_source = dedent_preserve_blank_lines(block_source);
    let source = format!(
        "fn test:\n{}\nfn __test_init:\n    entry {{\n        stop\n    }}",
        block_source.trim()
    );
    let (mut program, sources) =
        parse_or_panic_with_sources(&source, EmitConfig::init_only_with_name("__test_init"));
    let test_function = sources.function_by_name(&program, "test").expect("missing test function");
    assert_eq!(program.basic_blocks.len(), 2, "test function must contain exactly one block");
    // The dummy init only satisfies parser entrypoint restrictions; analyses should see the test
    // function as the isolated entrypoint.
    program.init_entry = test_function;
    let block = program.function(test_function).entry();

    let analyses = AnalysesStore::default();
    let in_out_bundling = ControlFlowGraphInOutBundling::new(&program, &analyses);
    let layout_sets = build_basic_block_layout_sets(&program, &analyses, &in_out_bundling);
    let layouts = LayoutsTracker::new(&program, layout_sets, in_out_bundling);
    let (input_layout, output_layout) =
        layouts.get_input_output(block.id()).expect("test block should be reachable");
    let graph =
        build_graph_effectful(&program, block, &layouts, input_layout, output_layout, &analyses);

    let mut ops = Vec::new();
    let next_alloc_id = searching_schedule(
        |op| ops.push(op),
        block,
        program.next_static_alloc_id,
        config,
        ScheduleConfig { beam_width: NonZero::new(TEST_BEAM_WIDTH).unwrap() },
        &graph,
    );

    assert_schedule_invariants(program.next_static_alloc_id, next_alloc_id, config, &ops);

    let actual = format_scheduled_block(&program, &layouts, block.id(), &ops);
    let expected = dedent_preserve_blank_lines(expected);
    pretty_assertions::assert_str_eq!(actual.trim(), expected.trim());
}

fn assert_schedule_invariants(
    first_spill_alloc_id: StaticAllocId,
    next_alloc_id: StaticAllocId,
    config: ShuffleConfig,
    ops: &[StackOps],
) {
    let mut stores = HashSet::new();
    for &op in ops {
        assert!(op.is_valid(config), "invalid stack operation {op}");
        match op {
            StackOps::Store(id) => {
                assert!(id >= first_spill_alloc_id, "spill overlaps an IR allocation");
                assert!(id < next_alloc_id, "spill exceeds returned allocation range");
                assert!(stores.insert(id), "spill allocation stored more than once");
            }
            StackOps::Load(id) => assert!(stores.contains(&id), "load without preceding store"),
            _ => {}
        }
    }

    let total_stores = u32::try_from(stores.len()).expect("overflow");
    assert_eq!(first_spill_alloc_id + total_stores, next_alloc_id);
}

#[test]
fn empty_loop_block() {
    assert_searches_to(
        ShuffleConfig::default(),
        r#"
        entry {
            => @entry
        }
        "#,
        r#"
        @0 []
            => []
            (jmp @0)
        "#,
    );
}

#[test]
fn preserves_loop_inputs_and_outputs() {
    assert_searches_to(
        ShuffleConfig::default(),
        r#"
        entry first second -> first second {
            condition = lt first second
            => condition ? @entry : @entry
        }
        "#,
        r#"
        @0 [$0, $1]
            dup 1
            dup 1
            lt
            => [$2 | $0, $1]
            (br @0 @0)
        "#,
    );
}

#[test]
fn reorders_loop_inputs_and_outputs() {
    assert_searches_to(
        ShuffleConfig::default(),
        r#"
        entry first second -> second first {
            condition = lt first second
            => condition ? @entry : @entry
        }
        "#,
        r#"
        @0 [$0, $1]
            dup 1
            dup 1
            lt
            swap 1
            swap 2
            swap 1
            => [$2 | $1, $0]
            (br @0 @0)
        "#,
    );
}

#[test]
fn lowers_terminator_inputs() {
    assert_searches_to(
        ShuffleConfig::default(),
        r#"
        entry {
            one = const 1
            two = const 2
            return one two
        }
        "#,
        r#"
        @0 []
            const 0x2
            const 0x1
            return
            => []
            (return)
        "#,
    );
}

#[test]
fn schedules_independent_operations() {
    assert_searches_to(
        ShuffleConfig::default(),
        r#"
        entry {
            a = caller
            b = callvalue
            x = not a
            y = iszero b
            z = add x y
            sstore z z
            stop
        }
        "#,
        r#"
        @0 []
            callvalue
            iszero
            caller
            not
            add
            dup 0
            sstore
            stop
            => []
            (stop)
        "#,
    );
}

#[test]
fn schedules_dependency_diamond() {
    assert_searches_to(
        ShuffleConfig::default(),
        r#"
        entry {
            input = caller
            left = not input
            right = iszero input
            output = xor left right
            sstore output output
            stop
        }
        "#,
        r#"
        @0 []
            caller
            dup 0
            iszero
            swap 1
            not
            xor
            dup 0
            sstore
            stop
            => []
            (stop)
        "#,
    );
}

#[test]
fn schedules_repeated_operand() {
    assert_searches_to(
        ShuffleConfig::default(),
        r#"
        entry {
            x = const 3
            y = const 2
            z = addmod x y x
            sstore z z
            stop
        }
        "#,
        r#"
        @0 []
            const 0x3
            const 0x2
            dup 1
            addmod
            dup 0
            sstore
            stop
            => []
            (stop)
        "#,
    );
}

#[test]
fn preserves_effect_order() {
    assert_searches_to(
        ShuffleConfig::default(),
        r#"
        entry {
            ptr = const 0
            value = const 1
            mstore256 ptr value
            first = mload32 ptr
            second = mload32 ptr
            mstore256 ptr second
            return 0 0
        }
        "#,
        r#"
        @0 []
            const 0x0
            const 0x0
            const 0x0
            const 0x1
            dup 3
            mstore
            dup 2
            mload
            dup 3
            mload
            swap 4
            mstore
            return
            => []
            (return)
        "#,
    );
}

#[test]
fn spills_with_shallow_stack_access() {
    assert_searches_to(
        ShuffleConfig::max_swap_no_exchange(1),
        r#"
        entry {
            x = const 3
            y = const 2
            z = addmod x y x
            sstore z z
            stop
        }
        "#,
        r#"
        @0 []
            const 0x3
            const 0x2
            store :0
            dup 0
            load :0
            swap 1
            addmod
            dup 0
            sstore
            stop
            => []
            (stop)
        "#,
    );
}

#[test]
fn finalizes_branch_condition() {
    assert_searches_to(
        ShuffleConfig::default(),
        r#"
        entry {
            unused = caller
            condition = callvalue
            => condition ? @entry : @entry
        }
        "#,
        r#"
        @0 []
            callvalue
            caller
            pop
            => [$1 | ]
            (br @0 @0)
        "#,
    );
}
