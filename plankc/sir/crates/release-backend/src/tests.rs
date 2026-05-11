use std::fmt::Write;

use plank_test_utils::dedent_preserve_blank_lines;
use sir_data::{BlockView, ControlView, EthIRProgram, Idx, Operation};
use sir_parser::EmitConfig;
use sir_passes::{AnalysesStore, ControlFlowGraphInOutBundling};

use super::{
    layouts::{Layout, LayoutMember, LayoutsTracker, build_basic_block_layout_sets},
    op_graph::{OpGraph, OpNodeId, build_graph_simple},
    stack::{ScheduleConfig, StackOps},
};

fn assert_lowers_to(config: ScheduleConfig, source: &str, expected: &str) {
    let source = dedent_preserve_blank_lines(source);
    let program = sir_parser::parse_or_panic(&source, EmitConfig::init_only());
    let actual = format_lowered(&program, config);
    let expected = dedent_preserve_blank_lines(expected);

    pretty_assertions::assert_str_eq!(actual.trim(), expected.trim());
}

fn format_lowered(program: &EthIRProgram, config: ScheduleConfig) -> String {
    let analyses = AnalysesStore::default();
    let lowered = crate::lower(program, &analyses, config);
    let in_out_bundling = ControlFlowGraphInOutBundling::new(program, &analyses);
    let layout_sets = build_basic_block_layout_sets(program, &analyses, &in_out_bundling);
    let layouts = LayoutsTracker::new(program, layout_sets, in_out_bundling);

    let mut out = String::new();
    for (block_id, ops) in lowered {
        let block = program.block(block_id);
        let graph = build_graph_simple(block, &layouts);

        write!(out, "@{block_id} ").unwrap();
        fmt_layout(&mut out, layouts.get_input_layout(block_id), block, LayoutSide::Input);
        writeln!(out).unwrap();

        for op in ops {
            write!(out, "   ").unwrap();
            fmt_stack_op(&mut out, program, block, &graph, op);
            writeln!(out).unwrap();
        }

        write!(out, "   => ").unwrap();
        fmt_layout(&mut out, layouts.get_output_layout(block_id), block, LayoutSide::Output);
        writeln!(out).unwrap();
    }
    out
}

#[derive(Clone, Copy)]
enum LayoutSide {
    Input,
    Output,
}

fn fmt_layout(out: &mut String, layout: &Layout, block: BlockView<'_>, side: LayoutSide) {
    out.push('[');
    for (idx, &member) in layout.members_fifo().iter().enumerate() {
        if idx != 0 {
            out.push_str(", ");
        }
        fmt_layout_member(out, member, block, side);
    }
    out.push(']');
}

fn fmt_layout_member(
    out: &mut String,
    member: LayoutMember,
    block: BlockView<'_>,
    side: LayoutSide,
) {
    match member {
        LayoutMember::ReturnDest => out.push_str("return_dest"),
        LayoutMember::InputOutput(position) => {
            let locals = match side {
                LayoutSide::Input => block.inputs(),
                LayoutSide::Output => block.outputs(),
            };
            let local = locals[position as usize];
            write!(out, "${local}").unwrap();
        }
        LayoutMember::Local(local) => write!(out, "${local}").unwrap(),
    }
}

fn fmt_stack_op(
    out: &mut String,
    program: &EthIRProgram,
    block: BlockView<'_>,
    graph: &OpGraph,
    op: StackOps,
) {
    match op {
        StackOps::Swap(depth) => write!(out, "swap {depth}").unwrap(),
        StackOps::Dup(depth) => write!(out, "dup {depth}").unwrap(),
        StackOps::Pop => out.push_str("pop"),
        StackOps::Op(op) => fmt_graph_op(out, program, block, graph, op),
        StackOps::CallRetPush(operation) => write!(out, "call_ret_push @{operation}").unwrap(),
        StackOps::Exchange(n, m) => write!(out, "exchange {n} {m}").unwrap(),
        StackOps::Store(slot) => write!(out, "store {slot}").unwrap(),
        StackOps::Load(slot) => write!(out, "load {slot}").unwrap(),
    }
}

fn fmt_graph_op(
    out: &mut String,
    program: &EthIRProgram,
    block: BlockView<'_>,
    graph: &OpGraph,
    op: OpNodeId,
) {
    if graph.control_op == Some(op) {
        fmt_control(out, block.control());
        return;
    }

    let op_view = block
        .operations()
        .nth(op.idx())
        .expect("operation graph node should map to a block operation");
    match op_view.op() {
        Operation::SetSmallConst(data) => write!(out, "const {:#x}", data.value).unwrap(),
        Operation::SetLargeConst(data) => {
            write!(out, "large_const {:#x}", program.large_consts[data.value]).unwrap()
        }
        op => out.push_str(op.kind().mnemonic()),
    }
}

fn fmt_control(out: &mut String, control: ControlView<'_>) {
    match control {
        ControlView::LastOpTerminates => out.push_str("<last-op-terminates>"),
        ControlView::InternalReturn => out.push_str("iret"),
        ControlView::ContinuesTo(target) => write!(out, "=> @{target}").unwrap(),
        ControlView::Branches { condition, non_zero_target, zero_target } => {
            write!(out, "=> ${condition} ? @{non_zero_target} : @{zero_target}").unwrap()
        }
        ControlView::Switch(switch) => write!(out, "switch ${}", switch.condition()).unwrap(),
    }
}

#[test]
fn lowers_independent_constants() {
    assert_lowers_to(
        ScheduleConfig::default(),
        r#"
            fn init:
                entry {
                    one = const 1
                    two = const 2
                    stop
                }
        "#,
        r#"
        @0 []
           const 0x1
           const 0x2
           stop
           pop
           pop
           => []
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
           pop
           pop
           pop
           => []
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
                    stop
                }
        "#,
        r#"
        @0 []
           const 0x0
           const 0x7
           pop
           store 0
           load 0
           => [$0]
        @1 [$2]
           dup 0
           => $2 ? @2 : @3
           pop
           => []
        @2 []
           stop
           => []
        @3 []
           stop
           => []
        "#,
    );
}
