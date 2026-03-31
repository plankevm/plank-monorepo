use sir_parser::{EmitConfig, parse_or_panic};
use sir_passes::AnalysesStore;

use crate::ir_to_bytecode_spill_count;

/// 18 calldataload values in a single block exceed the EVM stack depth of 16,
/// requiring spilling. Verifies that spill stores are emitted and the program
/// compiles successfully.
#[test]
fn spilling_triggered_for_deep_stack() {
    let ir = parse_or_panic(
        r#"
        fn init:
            init_entry {
                c0 = const 0
                runtime_off = runtime_start_offset
                runtime_len = runtime_length
                buf = malloc runtime_len
                codecopy buf runtime_off runtime_len
                return buf runtime_len
            }

        fn main:
            main_entry {
                c0 = const 0
                c32 = const 32
                c64 = const 64
                c96 = const 96
                c128 = const 128
                c160 = const 160
                c192 = const 192
                c224 = const 224
                c256 = const 256
                c288 = const 288
                c320 = const 320
                c352 = const 352
                c384 = const 384
                c416 = const 416
                c448 = const 448
                c480 = const 480
                c512 = const 512
                c544 = const 544

                v0 = calldataload c0
                v1 = calldataload c32
                v2 = calldataload c64
                v3 = calldataload c96
                v4 = calldataload c128
                v5 = calldataload c160
                v6 = calldataload c192
                v7 = calldataload c224
                v8 = calldataload c256
                v9 = calldataload c288
                v10 = calldataload c320
                v11 = calldataload c352
                v12 = calldataload c384
                v13 = calldataload c416
                v14 = calldataload c448
                v15 = calldataload c480
                v16 = calldataload c512
                v17 = calldataload c544

                s0 = add v0 v1
                s1 = add s0 v2
                s2 = add s1 v3
                s3 = add s2 v4
                s4 = add s3 v5
                s5 = add s4 v6
                s6 = add s5 v7
                s7 = add s6 v8
                s8 = add s7 v9
                s9 = add s8 v10
                s10 = add s9 v11
                s11 = add s10 v12
                s12 = add s11 v13
                s13 = add s12 v14
                s14 = add s13 v15
                s15 = add s14 v16
                result = add s15 v17

                out = sallocany 32
                mstore256 out result
                return out c32
            }
        "#,
        EmitConfig::default(),
    );

    let store = AnalysesStore::default();
    let mut bytecode = Vec::new();
    let spill_count = ir_to_bytecode_spill_count(&ir, &store, &mut bytecode);
    assert!(!bytecode.is_empty());
    assert!(spill_count > 0, "expected spilling to occur for deep stack program");
}

/// A program that stays within stack depth 16 should not trigger any spills.
#[test]
fn no_spilling_for_shallow_stack() {
    let ir = parse_or_panic(
        r#"
        fn init:
            init_entry {
                c0 = const 0
                c32 = const 32
                a = calldataload c0
                b = calldataload c32
                result = add a b
                buf = sallocany 32
                mstore256 buf result
                return buf c32
            }
        "#,
        EmitConfig::init_only(),
    );

    let store = AnalysesStore::default();
    let mut bytecode = Vec::new();
    let spill_count = ir_to_bytecode_spill_count(&ir, &store, &mut bytecode);
    assert!(!bytecode.is_empty());
    assert_eq!(spill_count, 0, "shallow stack program should not spill");
}
