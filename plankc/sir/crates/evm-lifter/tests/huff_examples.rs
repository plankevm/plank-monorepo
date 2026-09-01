use plank_core as _;
use plank_test_utils::dedent_preserve_indent;
use sir_data as _;
use sir_evm_lifter::{
    cfg::build_provisional_cfg, classify::classify, decode, icall::infer_internal_calls,
    lower::lower_to_sir, ownership::analyze_ownership, primitive_blocks::build_primitive_blocks,
    ssa::build_ssa, verify::verify,
};
use sir_passes as _;
use thiserror as _;

fn fixture(source: &str) -> Vec<u8> {
    alloy_primitives::hex::decode(source.trim()).unwrap()
}

fn assert_snapshot(actual: &str, expected: &str) {
    pretty_assertions::assert_str_eq!(
        dedent_preserve_indent(actual),
        dedent_preserve_indent(expected),
    );
}

#[test]
fn unreachable_icall_shape_remains_data() {
    let bytecode = alloy_primitives::hex::decode("006006565b005b56").unwrap();
    let decoded = decode(&bytecode).unwrap();
    let primitive = build_primitive_blocks(&decoded);
    let inference = infer_internal_calls(&decoded, &primitive);
    let cfg = build_provisional_cfg(&decoded, &inference);
    let ownership = analyze_ownership(&inference, &cfg).unwrap();
    let verification = verify(&decoded, &inference, &cfg, &ownership).unwrap();
    let classified = classify(&decoded, &inference, &ownership);

    let actual = format!(
        "== calls ==\n{}\n== ownership ==\n{}\n== verification ==\n{}\n== classification ==\n{}",
        inference.display(&decoded),
        ownership.display(&inference),
        verification.display(&ownership),
        classified.display(&decoded),
    );
    let expected = r#"
        == calls ==
        functions:
        blocks:
            @0 pc=[0x0,0x1)
                #0 00000000: STOP
            @1 pc=[0x1,0x4)
                #1 00000001: PUSH1
                #2 00000003: JUMP
            @2 pc=[0x4,0x6)
                #3 00000004: JUMPDEST
                #4 00000005: STOP
            @3 pc=[0x6,0x8)
                #5 00000006: JUMPDEST
                #6 00000007: JUMP ; iret candidate

        == ownership ==
        functions:
            f0: Root entry=@0
        blocks:
            @0 [0x0,0x1) owner=f0
            @1 [0x1,0x4) data
            @2 [0x4,0x6) data
            @3 [0x6,0x8) data

        == verification ==
        f0 Root Root
            @0: []

        == classification ==
        .d0 [0x1,0x8) 0x6006565b005b56
    "#;
    assert_snapshot(&actual, expected);
}

#[test]
fn shared_return_epilogue_is_duplicated_per_function() {
    let bytecode =
        alloy_primitives::hex::decode("6005600d565b600b6011565b005b6015565b6015565b5f9056")
            .unwrap();
    let decoded = decode(&bytecode).unwrap();
    let primitive = build_primitive_blocks(&decoded);
    let inference = infer_internal_calls(&decoded, &primitive);
    let cfg = build_provisional_cfg(&decoded, &inference);
    let ownership = analyze_ownership(&inference, &cfg).unwrap();
    let verification = verify(&decoded, &inference, &cfg, &ownership).unwrap();
    let classified = classify(&decoded, &inference, &ownership);
    let ssa = build_ssa(&decoded, &inference, &cfg, &ownership, &verification).unwrap();
    let lifted =
        lower_to_sir(&decoded, &classified, &ssa, verification.postorder(), ownership.root())
            .unwrap();

    let actual = format!(
        "== ownership ==\n{}\n== verification ==\n{}\n== SSA ==\n{}\n== SIR ==\n{}",
        ownership.display(&inference),
        verification.display(&ownership),
        ssa,
        lifted.program,
    );
    let expected = r#"
        == ownership ==
        functions:
            f0: Returning entry=@1
            f1: Returning entry=@2
            f2: Root entry=@0
        blocks:
            @0 [0x0,0xd) owner=f2
            @1 [0xd,0x11) owner=f0
            @2 [0x11,0x15) owner=f1
            @3 [0x15,0x19) duplicated in f0 f1

        == verification ==
        f0 Returning Returning(ReturningArity { physical: StackIO { inputs: 1, outputs: 1 }, return_input: 0 })
            @1: [input0..]
            @3: [input0..]
        f1 Returning Returning(ReturningArity { physical: StackIO { inputs: 1, outputs: 1 }, return_input: 0 })
            @2: [input0..]
            @3: [input0..]
        f2 Root Root
            @0: []
        return pushes:
            f2 call #2 <- push #0
            f2 call #6 <- push #4

        == SSA ==
        fn f0 Returning inputs=0 outputs=1 entry=%1
            %1 ; @1
                => %3
            %3 -> v2 ; @3
                v2 = const 0x0
                => iret
        fn f1 Returning inputs=0 outputs=1 entry=%2
            %2 ; @2
                => %4
            %4 -> v3 ; @3
                v3 = const 0x0
                => iret
        fn f2 Root inputs=0 outputs=0 entry=%0
            %0 ; @0
                v0 = icall f0
                v1 = icall f1
                STOP
                => terminates

        == SIR ==
        fn init:
            bb4 {
                v2 = icall @f0
                v3 = icall @f1
                stop
            }
        fn f0:
            bb0 {
                => @bb1
            }
            bb1 -> v0 {
                v0 = const 0
                iret
            }
        fn f1:
            bb2 {
                => @bb3
            }
            bb3 -> v1 {
                v1 = const 0
                iret
            }
    "#;
    assert_snapshot(&actual, expected);

    let mut provenance_contexts = std::collections::BTreeMap::new();
    for (block, &source) in lifted.provenance.blocks.enumerate_idx() {
        let Some(source) = source else { continue };
        provenance_contexts
            .entry(source)
            .or_insert_with(std::collections::BTreeSet::new)
            .insert(lifted.provenance.block_functions[block]);
    }
    assert_eq!(provenance_contexts.values().filter(|contexts| contexts.len() > 1).count(), 1,);
}

#[test]
fn small_huff_pipeline() {
    let bytecode = fixture(include_str!("fixtures/small.hex"));
    let decoded = decode(&bytecode).unwrap();
    let primitive = build_primitive_blocks(&decoded);
    let inference = infer_internal_calls(&decoded, &primitive);
    let cfg = build_provisional_cfg(&decoded, &inference);
    let ownership = analyze_ownership(&inference, &cfg).unwrap();
    let verification = verify(&decoded, &inference, &cfg, &ownership).unwrap();
    let classified = classify(&decoded, &inference, &ownership);
    let ssa = build_ssa(&decoded, &inference, &cfg, &ownership, &verification).unwrap();
    let lifted =
        lower_to_sir(&decoded, &classified, &ssa, verification.postorder(), ownership.root())
            .unwrap();

    let actual = format!(
        "== primitive ==\n{}\n== cfg ==\n{}\n== ownership ==\n{}\n== classification ==\n{}\n== SSA ==\n{}\n== SIR ==\n{}",
        primitive.display(&decoded),
        cfg.display(&decoded, &inference),
        ownership.display(&inference),
        classified.display(&decoded),
        ssa,
        lifted.program,
    );
    let expected = r#"
        == primitive ==
        @0 pc=[0x0,0x3) io=(0, 0)
            #0 00000000: PUSH1 0x06
            #1 00000002: JUMP ; direct=0x6 from #0
        @1 pc=[0x3,0x4) io=(0, 0)
            #2 00000003: STOP
        @2 pc=[0x4,0x5) io=(0, 0)
            #3 00000004: STOP
        @3 pc=[0x5,0x6) io=(0, 0)
            #4 00000005: STOP
        @4 pc=[0x6,0x7) io=(0, 0)
            #5 00000006: JUMPDEST

        == cfg ==
        @0 [0x0,0x3) => goto @4 ; last=#1 pc=0x2
        @1 [0x3,0x4) => terminates ; last=#2 pc=0x3
        @2 [0x4,0x5) => terminates ; last=#3 pc=0x4
        @3 [0x5,0x6) => terminates ; last=#4 pc=0x5
        @4 [0x6,0x7) => end-of-code ; last=#5 pc=0x6

        == ownership ==
        functions:
            f0: Root entry=@0
        blocks:
            @0 [0x0,0x3) owner=f0
            @1 [0x3,0x4) data
            @2 [0x4,0x5) data
            @3 [0x5,0x6) data
            @4 [0x6,0x7) owner=f0

        == classification ==
        .d0 [0x3,0x6) 0x000000

        == SSA ==
        fn f0 Root inputs=0 outputs=0 entry=%0
            %0 ; @0
                => %1
            %1 ; @4
                stop [synthetic]
                => terminates

        == SIR ==
        fn init:
            bb0 {
                => @bb1
            }
            bb1 {
                stop
            }
    "#;
    assert_snapshot(&actual, expected);
}

#[test]
fn many_calls_huff_pipeline() {
    let bytecode = fixture(include_str!("fixtures/many_calls.hex"));
    let decoded = decode(&bytecode).unwrap();
    let primitive = build_primitive_blocks(&decoded);
    let inference = infer_internal_calls(&decoded, &primitive);
    let cfg = build_provisional_cfg(&decoded, &inference);
    let ownership = analyze_ownership(&inference, &cfg).unwrap();
    let verification = verify(&decoded, &inference, &cfg, &ownership).unwrap();
    let classified = classify(&decoded, &inference, &ownership);
    let ssa = build_ssa(&decoded, &inference, &cfg, &ownership, &verification).unwrap();
    let lifted =
        lower_to_sir(&decoded, &classified, &ssa, verification.postorder(), ownership.root())
            .unwrap();

    let actual = format!(
        "== calls ==\n{}\n== cfg ==\n{}\n== ownership ==\n{}\n== verification ==\n{}\n== SSA ==\n{}\n== SIR ==\n{}",
        inference.display(&decoded),
        cfg.display(&decoded, &inference),
        ownership.display(&inference),
        verification.display(&ownership),
        ssa,
        lifted.program,
    );
    let expected = r#"
        == calls ==
        functions:
            f0: entry=0x17
        blocks:
            @0 pc=[0x0,0x16)
                #0 00000000: PUSH1
                #1 00000002: PUSH1
                #2 00000004: PUSH1
                #3 00000006: PUSH1
                #4 00000008: JUMP ; call f0 return=0x9
                #5 00000009: JUMPDEST
                #6 0000000a: PUSH1
                #7 0000000c: PUSH1
                #8 0000000e: PUSH1
                #9 00000010: JUMP ; call f0 return=0x11
                #10 00000011: JUMPDEST
                #11 00000012: PUSH0
                #12 00000013: PUSH1
                #13 00000015: JUMPI
            @1 pc=[0x16,0x17)
                #14 00000016: STOP
            @2 pc=[0x17,0x1c)
                #15 00000017: JUMPDEST
                #16 00000018: SWAP2
                #17 00000019: ADD
                #18 0000001a: SWAP1
                #19 0000001b: JUMP ; iret candidate
            @3 pc=[0x1c,0x1e)
                #20 0000001c: JUMPDEST
                #21 0000001d: INVALID

        == cfg ==
        @0 [0x0,0x16) call(f0 -> 0x17) call(f0 -> 0x17) => branch @3 else @1 ; last=#13 pc=0x15
        @1 [0x16,0x17) => terminates ; last=#14 pc=0x16
        @2 [0x17,0x1c) => iret ; last=#19 pc=0x1b
        @3 [0x1c,0x1e) => terminates ; last=#21 pc=0x1d

        == ownership ==
        functions:
            f0: Returning entry=@2
            f1: Root entry=@0
        blocks:
            @0 [0x0,0x16) owner=f1
            @1 [0x16,0x17) owner=f1
            @2 [0x17,0x1c) owner=f0
            @3 [0x1c,0x1e) owner=f1

        == verification ==
        f0 Returning Returning(ReturningArity { physical: StackIO { inputs: 3, outputs: 1 }, return_input: 0 })
            @2: [input0..]
        f1 Root Root
            @0: []
            @1: [{CallResult { call: InstructionId(9), output: 0 }}]
            @3: [{CallResult { call: InstructionId(9), output: 0 }}]
        return pushes:
            f1 call #4 <- push #2
            f1 call #9 <- push #7

        == SSA ==
        fn f0 Returning inputs=2 outputs=1 entry=%2
            %2 v7 v8 -> v9 ; @2
                v9 = ADD v8 v7
                => iret
        fn f1 Root inputs=0 outputs=0 entry=%0
            %0 -> v4 ; @0
                v0 = const 0x2
                v1 = const 0x4
                v2 = icall f0 v1 v0
                v3 = const 0x5
                v4 = icall f0 v3 v2
                v5 = const 0x0
                => v5 ? %3 : %1
            %1 v6 ; @1
                STOP
                => terminates
            %3 v10 ; @3
                INVALID
                => terminates

        == SIR ==
        fn init:
            bb1 -> v7 {
                v3 = const 2
                v4 = const 4
                v5 = icall @f0 v4 v3
                v6 = const 5
                v7 = icall @f0 v6 v5
                v8 = const 0
                => v8 ? @bb3 : @bb2
            }
            bb2 v9 {
                stop
            }
            bb3 v10 {
                invalid
            }
        fn f0:
            bb0 v0 v1 -> v2 {
                v2 = add v1 v0
                iret
            }
    "#;
    assert_snapshot(&actual, expected);
}

#[test]
fn depth_two_icall_huff_reaches_expected_unsupported_opcode() {
    let bytecode = fixture(include_str!("fixtures/depth2_icall.hex"));
    let decoded = decode(&bytecode).unwrap();
    let primitive = build_primitive_blocks(&decoded);
    let inference = infer_internal_calls(&decoded, &primitive);
    let cfg = build_provisional_cfg(&decoded, &inference);
    let ownership = analyze_ownership(&inference, &cfg).unwrap();
    let verification = verify(&decoded, &inference, &cfg, &ownership).unwrap();
    let classified = classify(&decoded, &inference, &ownership);
    let ssa = build_ssa(&decoded, &inference, &cfg, &ownership, &verification).unwrap();

    let actual =
        format!("== verification ==\n{}\n== SSA ==\n{}", verification.display(&ownership), ssa,);
    let expected = r#"
        == verification ==
        f0 Returning Returning(ReturningArity { physical: StackIO { inputs: 3, outputs: 1 }, return_input: 1 })
            @1: [input0..]
            @2: [{FunctionInput(1)}, {InstructionResult { instruction: InstructionId(23), output: 0 }}, input3..]
            @3: [{FunctionInput(1)}, {InstructionResult { instruction: InstructionId(23), output: 0 }}, input3..]
        f1 Returning Returning(ReturningArity { physical: StackIO { inputs: 4, outputs: 1 }, return_input: 3 })
            @4: [input0..]
        f2 Root Root
            @0: []
        return pushes:
            f1 call #38 <- push #35
            f1 call #43 <- push #40
            f2 call #11 <- push #3
            f2 call #14 <- push #2

        == SSA ==
        fn f0 Returning inputs=2 outputs=1 entry=%1
            %1 v13 v14 -> v15 ; @1
                v15 = ADD v14 v13
                v16 = GT v14 v15
                => v16 ? %3 : %2
            %2 v17 -> v17 ; @2
                => iret
            %3 v18 ; @3
                v19 = const 0x0
                v20 = const 0x0
                REVERT v20 v19
                => terminates
        fn f1 Returning inputs=3 outputs=1 entry=%4
            %4 v21 v22 v23 -> v25 ; @4
                v24 = icall f0 v21 v22
                v25 = icall f0 v24 v23
                => iret
        fn f2 Root inputs=0 outputs=0 entry=%0
            %0 ; @0
                v0 = const 0x60
                v1 = CALLDATALOAD v0
                v2 = const 0x0
                v3 = CALLDATALOAD v2
                v4 = const 0x20
                v5 = CALLDATALOAD v4
                v6 = const 0x40
                v7 = CALLDATALOAD v6
                v8 = icall f1 v7 v5 v3
                v9 = icall f0 v8 v1
                v10 = const 0x0
                MSTORE v10 v9
                v11 = MSIZE
                v12 = const 0x0
                RETURN v12 v11
                => terminates
    "#;
    assert_snapshot(&actual, expected);

    let error =
        lower_to_sir(&decoded, &classified, &ssa, verification.postorder(), ownership.root())
            .unwrap_err();
    pretty_assertions::assert_str_eq!(error.to_string(), "unsupported reachable opcode MSIZE");
}
