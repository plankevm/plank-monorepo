use std::fmt::Write;

use plank_test_utils::dedent_preserve_blank_lines;
use sir_data::{BlockView, ControlView, EthIRProgram, Idx, Operation, OperationIdx};
use sir_parser::EmitConfig;
use sir_passes::AnalysesStore;

use super::{
    layouts::{Layout, LayoutMember},
    op_graph::{OpGraph, build_graph_simple},
    stack::{ScheduleConfig, StackOps},
};

fn assert_lowers_to(config: ScheduleConfig, source: &str, expected: &str) {
    let source = dedent_preserve_blank_lines(source);
    let program = sir_parser::parse_or_panic(&source, EmitConfig::init_only());

    let actual = format_scheduled(&program, config);
    let expected = dedent_preserve_blank_lines(expected);

    pretty_assertions::assert_str_eq!(actual.trim(), expected.trim());
}

fn format_scheduled(program: &EthIRProgram, config: ScheduleConfig) -> String {
    let analyses = AnalysesStore::default();
    let (lowered, layouts) = crate::schedule(program, &analyses, config);

    let mut out = String::new();
    for (block_id, ops) in lowered.enumerate_idx() {
        let Some((input_layout, output_layout)) = layouts.get_input_output(block_id) else {
            continue;
        };
        let block = program.block(block_id);
        let graph = build_graph_simple(program, block, &layouts, input_layout, output_layout);

        write!(out, "@{block_id} ").unwrap();
        fmt_layout(&mut out, layouts.get_input_layout(block_id), block);
        writeln!(out).unwrap();

        for &op in ops {
            write!(out, "    ").unwrap();
            fmt_stack_op(&mut out, program, op);
            writeln!(out).unwrap();
        }

        write!(out, "    => ").unwrap();
        fmt_end_stack_layout(&mut out, program, &graph, layouts.get_input_layout(block_id), block);
        writeln!(out).unwrap();

        write!(out, "    ").unwrap();
        fmt_control(&mut out, block);
        writeln!(out).unwrap();
    }
    out
}

fn fmt_layout(out: &mut String, layout: &Layout, block: BlockView<'_>) {
    out.push('[');
    for (idx, &member) in layout.members_fifo().iter().enumerate() {
        if idx != 0 {
            out.push_str(", ");
        }
        fmt_layout_member(out, member, block);
    }
    out.push(']');
}

fn fmt_layout_member(out: &mut String, member: LayoutMember, block: BlockView<'_>) {
    match member {
        LayoutMember::ReturnDest => out.push_str("return_dest"),
        LayoutMember::InputOutput(position) => {
            let local = block.inputs()[position as usize];
            write!(out, "${local}").unwrap();
        }
        LayoutMember::Local(local) => write!(out, "${local}").unwrap(),
    }
}

fn fmt_stack_op(out: &mut String, program: &EthIRProgram, op: StackOps) {
    match op {
        StackOps::Swap(depth) => write!(out, "swap {depth}").unwrap(),
        StackOps::Dup(depth) => write!(out, "dup {depth}").unwrap(),
        StackOps::Pop => out.push_str("pop"),
        StackOps::Op(op) => fmt_op(out, program, op),
        StackOps::CallRetPush(operation) => write!(out, "call_ret_push #{operation}").unwrap(),
        StackOps::Exchange(n, m) => write!(out, "exchange {n} {m}").unwrap(),
        StackOps::Store(alloc) => write!(out, "store :{alloc}").unwrap(),
        StackOps::Load(alloc) => write!(out, "load :{alloc}").unwrap(),
    }
}

fn fmt_op(out: &mut String, program: &EthIRProgram, op: OperationIdx) {
    match program.operations[op] {
        Operation::SetSmallConst(data) => write!(out, "const {:#x}", data.value).unwrap(),
        Operation::SetLargeConst(data) => {
            write!(out, "large_const {:#x}", program.large_consts[data.value]).unwrap()
        }
        Operation::InternalCall(_) => write!(out, "icall #{op}").unwrap(),
        op => out.push_str(op.kind().mnemonic()),
    }
}

fn fmt_end_stack_layout(
    out: &mut String,
    program: &EthIRProgram,
    graph: &OpGraph,
    input_layout: &Layout,
    block: BlockView<'_>,
) {
    let terminator_inputs = terminator_input_count(block);

    out.push('[');
    for (idx, &value) in graph.end_stack_fifo.iter().enumerate() {
        if terminator_inputs != 0 && idx == terminator_inputs {
            if idx != 0 {
                out.push(' ');
            }
            out.push('|');
            if idx != graph.end_stack_fifo.len() {
                out.push(' ');
            }
        } else if idx != 0 {
            out.push_str(", ");
        }
        fmt_value(out, program, graph, input_layout, block, value);
    }
    if terminator_inputs == graph.end_stack_fifo.len() && terminator_inputs != 0 {
        out.push_str(" | ");
    }
    out.push(']');
}

fn terminator_input_count(block: BlockView<'_>) -> usize {
    match block.control() {
        ControlView::LastOpTerminates => {
            block.operations().last().expect("last op terminates but no last op").inputs().len()
        }
        ControlView::InternalReturn | ControlView::Branches { .. } | ControlView::Switch(_) => 1,
        ControlView::ContinuesTo(_) => 0,
    }
}

fn fmt_value(
    out: &mut String,
    program: &EthIRProgram,
    graph: &OpGraph,
    input_layout: &Layout,
    block: BlockView<'_>,
    value: super::op_graph::ValueNodeId,
) {
    if graph.is_input(value) {
        fmt_layout_member(out, input_layout.members_fifo()[value.idx()], block);
        return;
    }

    let source = graph.values[value].source.expect("non-input value should have source");
    let output_position = graph.operations[source]
        .produces_fifo
        .iter()
        .position(|&output| output == value)
        .expect("value should be in producing op outputs");
    let op_idx = match graph.operations[source].kind {
        super::op_graph::OpNodeKind::Flippable(op_idx)
        | super::op_graph::OpNodeKind::Normal(op_idx) => op_idx,
        super::op_graph::OpNodeKind::RetDestPush(_) => {
            panic!("return destination push should not produce local outputs")
        }
    };
    let local = program.operations[op_idx].outputs(program)[output_position];
    write!(out, "${local}").unwrap();
}

fn fmt_control(out: &mut String, block: BlockView<'_>) {
    out.push('(');
    match block.control() {
        ControlView::LastOpTerminates => {
            let terminator = block.operations().last().expect("last op terminates but no last op");
            out.push_str(terminator.op().kind().mnemonic());
        }
        ControlView::InternalReturn => out.push_str("iret"),
        ControlView::ContinuesTo(target) => write!(out, "jmp @{target}").unwrap(),
        ControlView::Branches { non_zero_target, zero_target, .. } => {
            write!(out, "br @{non_zero_target} @{zero_target}").unwrap()
        }
        ControlView::Switch(switch) => write!(out, "switch ${}", switch.condition()).unwrap(),
    }
    out.push(')');
}

#[test]
fn lowers_terminator_inputs() {
    assert_lowers_to(
        ScheduleConfig::default(),
        r#"
        fn init:
            entry {
                one = const 1
                two = const 2
                return one two
            }
        "#,
        r#"
        @0 []
            const 0x1
            const 0x2
            dup 0
            dup 2
            return
            => []
            (return)
        "#,
    );
}

#[test]
fn lowers_binary_operation_inputs() {
    assert_lowers_to(
        ScheduleConfig::default(),
        r#"
        fn init:
            entry {
                one = const 1
                two = const 2
                sum = add one two
                stop
            }
        "#,
        r#"
        @0 []
            const 0x1
            const 0x2
            dup 0
            dup 2
            add
            stop
            => []
            (stop)
        "#,
    );
}

#[test]
fn lowers_memory_hash_and_store() {
    assert_lowers_to(
        ScheduleConfig::default(),
        r#"
        fn init:
            entry {
                zero = const 0
                word = const 32
                two_words = const 64
                one = const 1
                first = calldataload zero
                second = calldataload word
                ptr = malloc two_words
                mstore256 ptr first
                second_ptr = add ptr word
                mstore256 second_ptr second
                hash = keccak256 ptr two_words
                sstore hash one
                stop
            }
        "#,
        r#"
        @0 []
            const 0x0
            const 0x20
            const 0x40
            const 0x1
            dup 3
            calldataload
            dup 3
            calldataload
            dup 3
            malloc
            dup 2
            dup 1
            mstore
            dup 5
            dup 1
            add
            dup 2
            dup 1
            mstore
            dup 5
            dup 2
            keccak256
            dup 5
            dup 1
            sstore
            stop
            => []
            (stop)
        "#,
    );
}

#[test]
fn lowers_calldata_sum_loop() {
    assert_lowers_to(
        ScheduleConfig::default(),
        r#"
        fn init:
            entry -> len0 idx0 off0 sum0 {
                zero = const 0
                len0 = calldataload zero
                idx0 = const 0
                off0 = const 32
                sum0 = const 0
                => @loop
            }
            loop len1 idx1 off1 sum1 -> len1 idx1 off1 sum1 {
                keep_going = lt idx1 len1
                => keep_going ? @body : @done
            }
            body len3 idx3 off3 sum3 -> len3 idx4 off4 sum4 {
                value = calldataload off3
                sum4 = add sum3 value
                one = const 1
                idx4 = add idx3 one
                word = const 32
                off4 = add off3 word
                => @loop
            }
            done len5 idx5 off5 sum5 {
                word_out = const 32
                ptr = malloc word_out
                mstore256 ptr sum5
                return ptr word_out
            }
        "#,
        r#"
        @0 []
            const 0x0
            dup 0
            calldataload
            const 0x0
            const 0x20
            const 0x0
            store :0
            store :1
            store :2
            store :3
            pop
            load :0
            load :1
            load :2
            load :3
            => [$1, $2, $3, $4]
            (jmp @1)
        @1 [$5, $6, $7, $8]
            dup 0
            dup 2
            lt
            store :4
            store :5
            store :6
            store :7
            store :8
            load :8
            load :7
            load :6
            load :5
            load :4
            => [$9 | $5, $6, $7, $8]
            (br @2 @3)
        @2 [$10, $11, $12, $13]
            dup 2
            calldataload
            dup 0
            dup 5
            add
            const 0x1
            dup 0
            dup 5
            add
            const 0x20
            dup 0
            dup 8
            add
            store :9
            pop
            store :10
            pop
            store :11
            pop
            store :12
            pop
            pop
            pop
            load :11
            load :9
            load :10
            load :12
            => [$10, $17, $19, $15]
            (jmp @1)
        @3 [$20, $21, $22, $23]
            const 0x20
            dup 0
            malloc
            dup 5
            dup 1
            mstore
            dup 1
            dup 1
            return
            => []
            (return)
        "#,
    );
}

#[test]
fn lowers_branch_layouts() {
    assert_lowers_to(
        ScheduleConfig::default(),
        r#"
        fn init:
            entry -> zero value {
                zero = const 0
                value = const 7
                => @branch
            }
            branch flag carried -> carried {
                => flag ? @left : @right
            }
            left left_value {
                stop
            }
            right right_value {
                invalid
            }
        "#,
        r#"
        @0 []
            const 0x0
            const 0x7
            pop
            store :0
            load :0
            => [$0]
            (jmp @1)
        @1 [$2]
            store :1
            load :1
            => [$2 | ]
            (br @2 @3)
        @2 []
            stop
            => []
            (stop)
        @3 []
            invalid
            => []
            (invalid)
        "#,
    );
}

#[test]
fn simple_icall() {
    assert_lowers_to(
        ScheduleConfig::default(),
        r#"
        fn init:
            entry {
                value = caller
                other = const 0
                stuff = icall @ident value other
                sstore stuff other
                stop
            }
        fn ident:
            entry x y -> x {
                iret
            }
        "#,
        r#"
        @0 [return_dest, $0]
            store :0
            store :1
            load :1
            load :0
            => [return_dest | $0]
            (iret)
        @1 []
            call_ret_push #2
            caller
            const 0x0
            dup 1
            dup 3
            icall #2
            dup 1
            dup 1
            sstore
            stop
            => []
            (stop)
        "#,
    );
}

#[test]
fn simple_op_use_spill() {
    assert_lowers_to(
        ScheduleConfig {
            max_swap_depth: 3,
            max_dup_depth: 2,
            max_exchange_range: 3,
            exchange_cost: 9,
        },
        r#"
        fn init:
            entry {
                a = const 1
                b1 = const 0
                b2 = const 0
                b3 = const 0
                b4 = const 0
                x = not a

                stop
            }
        "#,
        r#"

        @0 []
            const 0x1
            const 0x0
            const 0x0
            const 0x0
            const 0x0
            store :0
            store :1
            dup 2
            store :2
            load :1
            load :0
            load :2
            not
            stop
            => []
            (stop)
        "#,
    );
}

#[test]
fn unreachable() {
    assert_lowers_to(
        ScheduleConfig::default(),
        r#"
        fn init:
            entry {
                x = const 3
                stop
            }
            another {
                y = not x
                invalid
            }
        "#,
        r#"

        @0 []
            const 0x3
            stop
            => []
            (stop)
        "#,
    );
}
