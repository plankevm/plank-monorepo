use crate::OptLevel;
use plank_session::Session;
use plank_test_utils::TestProject;
use plank_values::ValueInterner;

fn lower_ir(source: &str) -> String {
    let mut session = Session::new();
    let project = TestProject::root(source).build(&mut session);
    let mut values = ValueInterner::new();
    let hir = plank_hir::lower(&project, &mut values, &mut session);
    let mir = plank_hir_eval::evaluate(&hir, project.core_ops_source, &mut values, &mut session);
    assert!(!session.has_errors());
    crate::emit_ir(&mir, &values, &session, OptLevel::O0).unwrap()
}

fn lower_bytecode(source: &str) -> Vec<u8> {
    let mut session = Session::new();
    let project = TestProject::root(source).build(&mut session);
    let mut values = ValueInterner::new();
    let hir = plank_hir::lower(&project, &mut values, &mut session);
    let mir = plank_hir_eval::evaluate(&hir, project.core_ops_source, &mut values, &mut session);
    assert!(!session.has_errors());
    crate::emit_bytecode(&mir, &values, &session, OptLevel::O0).unwrap()
}

#[track_caller]
fn assert_contains(haystack: &str, needle: &str) {
    assert!(haystack.contains(needle), "expected IR to contain `{needle}`\n\n{haystack}");
}

#[track_caller]
fn assert_lowers_to_ir(source: &str, expected: &str) {
    let actual = lower_ir(source);
    let expected = plank_test_utils::dedent_preserve_blank_lines(expected);
    pretty_assertions::assert_str_eq!(actual.trim(), expected.trim());
}

#[test]
fn lowers_simple_init_stop_to_exact_ir() {
    assert_lowers_to_ir(
        r#"
        init {
            @evm_stop();
        }
        "#,
        r#"
        target = "evm-ethereum-osaka"

        func public %init() {
            block0:
                evm_stop;
        }


        object @Contract {
            section init {
                entry %init;
            }
        }
        "#,
    );
}

#[test]
fn lowers_builtin_dataflow_to_exact_ir() {
    assert_lowers_to_ir(
        r#"
        init {
            let value = @evm_calldataload(0);
            let sum = @evm_add(value, 1);
            @evm_sstore(2, sum);
            @evm_stop();
        }
        "#,
        r#"
        target = "evm-ethereum-osaka"

        func public %init() {
            block0:
                v1.i256 = evm_calldata_load 0.i256;
                v3.i256 = add v1 1.i256;
                evm_sstore 2.i256 v3;
                evm_stop;
        }


        object @Contract {
            section init {
                entry %init;
            }
        }
        "#,
    );
}

#[test]
fn lowers_call_with_args_to_exact_ir() {
    assert_lowers_to_ir(
        r#"
        const safe_add = fn (x: u256, y: u256) u256 {
            let z = @evm_add(x, y);
            z
        };

        init {
            let ptr = @malloc_uninit(32);
            let z = safe_add(3, 4);
            @mstore32(ptr, z);
            @evm_stop();
        }
        "#,
        r#"
        target = "evm-ethereum-osaka"

        func private %fn_0(v0.i256, v1.i256) -> i256 {
            block0:
                v2.i256 = add v0 v1;
                return v2;
        }

        func public %init() {
            block0:
                v1.*i8 = evm_malloc 32.i256;
                v2.i256 = ptr_to_int v1 i256;
                v5.i256 = call %fn_0 3.i256 4.i256;
                mstore v2 v5 i256;
                evm_stop;
        }


        object @Contract {
            section init {
                entry %init;
            }
        }
        "#,
    );
}

#[test]
fn lowers_terminal_if_to_exact_ir() {
    assert_lowers_to_ir(
        r#"
        init {
            let value = @evm_calldataload(0);
            if @evm_gt(value, 0) {
                @evm_stop();
            } else {
                @evm_revert(@malloc_uninit(0), 0);
            }
        }
        "#,
        r#"
        target = "evm-ethereum-osaka"

        func public %init() {
            block0:
                v1.i256 = evm_calldata_load 0.i256;
                v2.i1 = gt v1 0.i256;
                br v2 block2 block1;

            block1:
                v3.*i8 = evm_malloc 0.i256;
                v4.i256 = ptr_to_int v3 i256;
                evm_revert v4 0.i256;

            block2:
                evm_stop;
        }


        object @Contract {
            section init {
                entry %init;
            }
        }
        "#,
    );
}

#[test]
fn lowers_runtime_object_to_exact_ir() {
    assert_lowers_to_ir(
        r#"
        init {
            let ptr = @malloc_uninit(32);
            @mstore32(ptr, @runtime_start_offset());
            @evm_return(ptr, @runtime_length());
        }

        run {
            @evm_stop();
        }
        "#,
        r#"
        target = "evm-ethereum-osaka"

        func public %init() {
            block0:
                v1.*i8 = evm_malloc 32.i256;
                v2.i256 = ptr_to_int v1 i256;
                v3.i256 = sym_addr &runtime;
                mstore v2 v3 i256;
                v4.i256 = sym_size &runtime;
                evm_return v2 v4;
        }

        func public %run() {
            block0:
                evm_stop;
        }


        object @Contract {
            section runtime {
                entry %run;
            }
            section init {
                entry %init;
                embed .runtime as &runtime;
            }
        }
        "#,
    );
}

#[test]
fn lowers_aggregate_access_to_exact_ir() {
    assert_lowers_to_ir(
        r#"
        const Pair = struct { a: u256, empty: void, b: u256 };

        init {
            let pair = Pair { a: @evm_calldataload(0), empty: {}, b: @evm_calldataload(32) };
            let ptr = @malloc_uninit(32);
            @mstore32(ptr, pair.b);
            @evm_return(ptr, 32);
        }
        "#,
        r#"
        target = "evm-ethereum-osaka"

        type @struct_0 = {i256, unit, i256};

        func public %init() {
            block0:
                v1.i256 = evm_calldata_load 0.i256;
                v3.i256 = evm_calldata_load 32.i256;
                v5.@struct_0 = insert_value undef.@struct_0 0.i256 v1;
                v7.@struct_0 = insert_value v5 2.i256 v3;
                v8.*i8 = evm_malloc 32.i256;
                v9.i256 = ptr_to_int v8 i256;
                v10.i256 = extract_value v7 2.i256;
                mstore v9 v10 i256;
                evm_return v9 32.i256;
        }


        object @Contract {
            section init {
                entry %init;
            }
        }
        "#,
    );
}

#[test]
fn lowers_zero_runtime_struct_to_exact_ir() {
    assert_lowers_to_ir(
        r#"
        const Empty = struct {};

        init {
            let empty = Empty {};
            let copy = empty;
            @evm_stop();
        }
        "#,
        r#"
        target = "evm-ethereum-osaka"

        func public %init() {
            block0:
                evm_stop;
        }


        object @Contract {
            section init {
                entry %init;
            }
        }
        "#,
    );
}

#[test]
fn lowers_signextend_and_partial_memory_helpers() {
    let ir = lower_ir(
        r#"
        init {
            let ptr = @malloc_uninit(32);
            let byte = @evm_calldataload(0);
            let value = @evm_calldataload(32);
            let signed = @evm_signextend(byte, value);
            @mstore1(ptr, signed);
            let loaded = @mload3(ptr);
            @mstore2(ptr, loaded);
            @evm_stop();
        }
        "#,
    );

    assert_contains(&ir, "evm_signextend");
    assert_contains(&ir, "mstore8");
    assert_contains(&ir, "mload");
    assert_contains(&ir, "shr");
    assert_contains(&ir, "mstore");
}

#[test]
fn lowers_runtime_introspection_symbols() {
    let ir = lower_ir(
        r#"
        init {
            let ptr = @malloc_uninit(96);
            @mstore32(ptr, @runtime_start_offset());
            @mstore32(@evm_add(ptr, 32), @runtime_length());
            @mstore32(@evm_add(ptr, 64), @init_end_offset());
            @evm_return(ptr, @runtime_length());
        }

        run {
            @evm_stop();
        }
        "#,
    );

    assert_contains(&ir, "sym_addr &runtime");
    assert_contains(&ir, "sym_size &runtime");
    assert_contains(&ir, "sym_size .");
}

#[test]
fn compiles_runtime_length_used_from_runtime_section() {
    let source = r#"
        const len = fn () u256 {
            @runtime_length()
        };

        init {
            let runtime = @malloc_uninit(@runtime_length());
            @evm_codecopy(runtime, @runtime_start_offset(), @runtime_length());
            @evm_return(runtime, @runtime_length());
        }

        run {
            let ptr = @malloc_uninit(32);
            @mstore32(ptr, len());
            @evm_return(ptr, 32);
        }
        "#;

    assert_lowers_to_ir(
        source,
        r#"
        target = "evm-ethereum-osaka"

        func public %init() {
            block0:
                v0.i256 = sym_size &runtime;
                v1.*i8 = evm_malloc v0;
                v2.i256 = ptr_to_int v1 i256;
                v3.i256 = sym_addr &runtime;
                v4.i256 = sym_size &runtime;
                evm_code_copy v2 v3 v4;
                v5.i256 = sym_size &runtime;
                evm_return v2 v5;
        }

        func private %fn_1() -> i256 {
            block0:
                v0.i256 = sym_size .;
                return v0;
        }

        func public %run() {
            block0:
                v1.*i8 = evm_malloc 32.i256;
                v2.i256 = ptr_to_int v1 i256;
                v3.i256 = call %fn_1;
                mstore v2 v3 i256;
                evm_return v2 32.i256;
        }


        object @Contract {
            section runtime {
                entry %run;
            }
            section init {
                entry %init;
                embed .runtime as &runtime;
            }
        }
        "#,
    );
    assert!(!lower_bytecode(source).is_empty());
}

#[test]
fn lowers_runtime_tuple_field_builtins() {
    let bytecode = lower_bytecode(
        r#"
        init {
            let pair = (@evm_calldataload(0), @evm_calldataload(0x20));
            let pair2 = @set_field(pair, 0, 99);
            let x = @get_field(pair2, 0);
            @evm_sstore(0, x);
            @evm_stop();
        }
        "#,
    );
    assert!(!bytecode.is_empty());
}

#[test]
fn duplicates_runtime_introspection_helpers_per_section() {
    let source = r#"
        const len = fn () u256 {
            @runtime_length()
        };

        init {
            let runtime = @malloc_uninit(len());
            @evm_codecopy(runtime, @runtime_start_offset(), len());
            @evm_return(runtime, len());
        }

        run {
            let ptr = @malloc_uninit(32);
            @mstore32(ptr, len());
            @evm_return(ptr, 32);
        }
        "#;

    assert_lowers_to_ir(
        source,
        r#"
        target = "evm-ethereum-osaka"

        func private %fn_0() -> i256 {
            block0:
                v0.i256 = sym_size &runtime;
                return v0;
        }

        func private %fn_0_runtime() -> i256 {
            block0:
                v0.i256 = sym_size .;
                return v0;
        }

        func public %init() {
            block0:
                v0.i256 = call %fn_0;
                v1.*i8 = evm_malloc v0;
                v2.i256 = ptr_to_int v1 i256;
                v3.i256 = sym_addr &runtime;
                v4.i256 = call %fn_0;
                evm_code_copy v2 v3 v4;
                v5.i256 = call %fn_0;
                evm_return v2 v5;
        }

        func public %run() {
            block0:
                v1.*i8 = evm_malloc 32.i256;
                v2.i256 = ptr_to_int v1 i256;
                v3.i256 = call %fn_0_runtime;
                mstore v2 v3 i256;
                evm_return v2 32.i256;
        }


        object @Contract {
            section runtime {
                entry %run;
            }
            section init {
                entry %init;
                embed .runtime as &runtime;
            }
        }
        "#,
    );
    assert!(!lower_bytecode(source).is_empty());
}

#[test]
fn lowers_bool_builtins_as_i1_without_zext() {
    let ir = lower_ir(
        r#"
        init {
            let value = @evm_calldataload(0);
            if @evm_gt(value, 0) {
                @evm_stop();
            } else {
                @evm_stop();
            }
        }
        "#,
    );

    assert_contains(&ir, " = gt ");
    assert!(!ir.contains(" = zext "), "bool lowering should not widen through zext\n\n{ir}");
    assert!(
        !ir.contains(" = ne "),
        "bool branch conditions should stay i1 instead of comparing a word to zero\n\n{ir}"
    );
    assert!(!ir.contains("evm_invalid"), "terminal if should not emit unreachable invalid\n\n{ir}");
}

#[test]
fn lowers_structs_as_sonatina_aggregates() {
    let ir = lower_ir(
        r#"
        const Pair = struct { a: u256, b: u256 };

        init {
            let pair = Pair { a: @evm_calldataload(0), b: @evm_calldataload(32) };
            let pair_copy = pair;
            let ptr = @malloc_uninit(32);
            @mstore32(ptr, pair_copy.b);
            @evm_return(ptr, 32);
        }
        "#,
    );

    assert_contains(&ir, "insert_value");
    assert_contains(&ir, "extract_value");
    assert!(ir.contains("type @struct_"), "expected a declared aggregate type\n\n{ir}");
}

#[test]
fn lowers_zero_runtime_structs_without_aggregate_values() {
    let ir = lower_ir(
        r#"
        const Empty = struct {};

        init {
            let empty = Empty {};
            let copy = empty;
            @evm_stop();
        }
        "#,
    );

    assert!(
        !ir.contains("type @struct_"),
        "zero-runtime struct should not declare an aggregate type\n\n{ir}"
    );
    assert!(
        !ir.contains("insert_value"),
        "zero-runtime struct should not construct an aggregate\n\n{ir}"
    );
}

#[test]
fn lowers_zeroed_allocation_with_calldatacopy() {
    let ir = lower_ir(
        r#"
        init {
            let zeroed = @malloc_zeroed(32);
            @malloc_uninit(32);
            @mstore32(zeroed, 1);
            @evm_stop();
        }
        "#,
    );

    assert_contains(&ir, "evm_malloc");
    assert_contains(&ir, "evm_calldata_size");
    assert_contains(&ir, "evm_calldata_copy");
}

#[test]
fn emits_bytecode_for_simple_program() {
    let bytecode = lower_bytecode(
        r#"
        init {
            @evm_stop();
        }
        "#,
    );

    assert!(!bytecode.is_empty());
}

#[test]
fn lowers_data_offset_to_const_global() {
    let ir = lower_ir(
        r#"
        init {
            let first = @data_offset("hello");
            let second = @data_offset("hello");
            let other = @data_offset(hex"00ff");
            @evm_stop();
        }
        "#,
    );

    assert_contains(&ir, "sym_addr $cbytes_0");
    assert_contains(&ir, "sym_addr $cbytes_1");
    assert_contains(&ir, "$cbytes_0 =");
    assert_contains(&ir, "$cbytes_1 =");
    assert!(!ir.contains("$cbytes_2"), "identical literals must share one global\n\n{ir}");
}

#[test]
fn lowers_sliced_data_offset_to_sym_addr_plus_start() {
    let ir = lower_ir(
        r#"
        init {
            let offset = @data_offset(@slice_cbytes("hello" hex"00ff", 2, 6));
            @evm_stop();
        }
        "#,
    );

    assert_contains(&ir, "sym_addr $cbytes_0");
    assert_contains(&ir, "add v0 2.i256");
}

#[test]
fn lowers_data_offset_of_concat_to_const_global() {
    assert_lowers_to_ir(
        r#"
        const VERY_LONG =
            hex"0000000000000000000000000000000000000000000000000000000000000000"
            hex"000102030405060708090a0b0c0d0e0f00112233445566778899aabbccddeeff";

        init {
            let offset = @data_offset(
                @concat_cbytes((@slice_cbytes(VERY_LONG, 33, 64), "a"))
            );
            @evm_stop();
        }
        "#,
        r#"
        target = "evm-ethereum-osaka"

        global private const [i8; 32] $cbytes_0 = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 0, 17, 34, 51, 68, 85, 102, 119, -120, -103, -86, -69, -52, -35, -18, -1, 97];

        func public %init() {
            block0:
                v0.i256 = sym_addr $cbytes_0;
                evm_stop;
        }


        object @Contract {
            section init {
                entry %init;
            }
        }
        "#,
    );
}

#[test]
fn lowers_data_offset_of_lone_concat_to_const_global() {
    assert_lowers_to_ir(
        r#"
        const VERY_LONG =
            hex"0000000000000000000000000000000000000000000000000000000000000000"
            hex"000102030405060708090a0b0c0d0e0f00112233445566778899aabbccddeeff";

        init {
            let offset = @data_offset(
                @concat_cbytes((@slice_cbytes(VERY_LONG, 33, 64),))
            );
            @evm_stop();
        }
        "#,
        r#"
        target = "evm-ethereum-osaka"

        global private const [i8; 31] $cbytes_0 = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 0, 17, 34, 51, 68, 85, 102, 119, -120, -103, -86, -69, -52, -35, -18, -1];

        func public %init() {
            block0:
                v0.i256 = sym_addr $cbytes_0;
                evm_stop;
        }


        object @Contract {
            section init {
                entry %init;
            }
        }
        "#,
    );
}

#[test]
fn emits_bytecode_for_data_offset() {
    let bytecode = lower_bytecode(
        r#"
        init {
            let ptr = @malloc_uninit(32);
            @evm_codecopy(ptr, @data_offset("hello"), "hello".length);
            @evm_return(ptr, 32);
        }
        "#,
    );

    let hello = b"hello";
    assert!(
        bytecode.windows(hello.len()).any(|w| w == hello),
        "expected data bytes embedded in code: {bytecode:02x?}"
    );
}
