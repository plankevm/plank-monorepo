use plank_parser::{PlankInterner, error_report::ParserError};
use plank_test_utils::TestProject;
use plank_values::BigNumInterner;

fn try_lower(source: &str) -> Result<sir_data::EthIRProgram, Vec<ParserError>> {
    let mut interner = PlankInterner::default();
    let project = TestProject::single(source)
        .build(&mut interner)
        .map_err(|collector| collector.errors.into_iter().map(|(_, e)| e).collect::<Vec<_>>())?;

    let mut big_nums = BigNumInterner::default();
    let hir = plank_hir::lower(&project, &mut big_nums);
    let mir = plank_hir_eval::evaluate(&hir);
    let sir = crate::lower(&mir, &big_nums);
    Ok(sir)
}

fn assert_lowers_to(source: &str, expected: &str) {
    let program = match try_lower(source) {
        Ok(p) => p,
        Err(errors) => {
            panic!("Expected no parse errors, got: {}\n{:#?}", errors.len(), errors);
        }
    };
    let actual = sir_data::display_program(&program);
    let expected = plank_test_utils::dedent_preserve_blank_lines(expected);
    pretty_assertions::assert_str_eq!(actual.trim(), expected.trim());
}

#[test]
fn test_simple_set() {
    assert_lowers_to(
        r#"
        init {
            let x = 3;
            evm_stop();
        }

        run {
            let y = false;
            evm_stop();
        }
        "#,
        r#"
        Init: @0
        Run: @1
        Functions:
            fn @0 -> entry @0  (outputs: 0)
            fn @1 -> entry @1  (outputs: 0)

        Basic Blocks:
            @0 {
                $0 = const 0x3
                stop
            }

            @1 {
                $1 = const 0x0
                stop
            }
        "#,
    );
}

#[test]
fn test_evm_builtins() {
    assert_lowers_to(
        r#"
        init {
            let x = 3;
            let y = 4;
            let z = add(3, 4);
            add(3, 4);
            let w = callvalue();
            let a: memptr = malloc_uninit(calldataload(34));
            sstore(x, z);
            evm_stop();
        }
        "#,
        r#"
        Init: @0
        Functions:
            fn @0 -> entry @0  (outputs: 0)

        Basic Blocks:
            @0 {
                $0 = const 0x3
                $1 = const 0x4
                $2 = const 0x3
                $3 = const 0x4
                $4 = add $2 $3
                $5 = const 0x3
                $6 = const 0x4
                $7 = add $5 $6
                $8 = callvalue
                $9 = const 0x22
                $10 = calldataload $9
                $11 = mallocany $10
                $12 = copy $0
                $13 = copy $4
                sstore $12 $13
                stop
            }
        "#,
    );
}

#[test]
fn test_assign() {
    assert_lowers_to(
        r#"
        init {
            let mut x = 3;
            x = 34;
            evm_stop();
        }
        "#,
        r#"
        Init: @0
        Functions:
            fn @0 -> entry @0  (outputs: 0)

        Basic Blocks:
            @0 {
                $0 = const 0x3
                $0 = const 0x22
                stop
            }
        "#,
    );
}

#[test]
fn test_explicit_terminator() {
    assert_lowers_to(
        r#"
        init {
            let ptr = malloc_uninit(0);
            evm_return(ptr, 0);
        }
        "#,
        r#"
        Init: @0
        Functions:
            fn @0 -> entry @0  (outputs: 0)

        Basic Blocks:
            @0 {
                $0 = const 0x0
                $1 = mallocany $0
                $2 = copy $1
                $3 = const 0x0
                return $2 $3
            }
        "#,
    );
}

#[test]
fn test_simple_call() {
    assert_lowers_to(
        r#"
        const dangling = fn () memptr {
            malloc_uninit(0)
        };

        init {
            let ptr = dangling();
            evm_return(ptr, 0);
        }
        "#,
        r#"
        Init: @1
        Functions:
            fn @0 -> entry @0  (outputs: 1)
            fn @1 -> entry @1  (outputs: 0)

        Basic Blocks:
            @0 -> $1 {
                $0 = const 0x0
                $1 = mallocany $0
                iret
            }

            @1 {
                $2 = icall @0
                $3 = copy $2
                $4 = const 0x0
                return $3 $4
            }
        "#,
    );
}

#[test]
fn test_call_with_args() {
    assert_lowers_to(
        r#"
        const safe_add = fn (x: u256, y: u256) u256 {
            let z = add(x, y);
            z
        };

        init {
            let z = safe_add(3, 4);
            evm_stop();
        }
        "#,
        r#"
        Init: @1
        Functions:
            fn @0 -> entry @0  (outputs: 1)
            fn @1 -> entry @1  (outputs: 0)

        Basic Blocks:
            @0 $0 $1 -> $5 {
                $2 = copy $0
                $3 = copy $1
                $4 = add $2 $3
                $5 = copy $4
                iret
            }

            @1 {
                $6 = const 0x3
                $7 = const 0x4
                $8 = icall @0 $6 $7
                stop
            }
        "#,
    );
}

#[test]
fn test_simple_if() {
    assert_lowers_to(
        r#"
        init {
            let x = calldataload(0);
            if slt(x, 0) {
                revert(malloc_uninit(0), 0);
            }
            evm_stop();
        }
        "#,
        r#"
        Init: @0
        Functions:
            fn @0 -> entry @0  (outputs: 0)

        Basic Blocks:
            @0 {
                $0 = const 0x0
                $1 = calldataload $0
                $2 = copy $1
                $3 = const 0x0
                $4 = slt $2 $3
                => $4 ? @1 : @2
            }

            @1 {
                $5 = const 0x0
                $6 = mallocany $5
                $7 = const 0x0
                revert $6 $7
            }

            @2 {
                => @3
            }

            @3 {
                stop
            }
        "#,
    );
}

#[test]
fn test_nested_if_assign() {
    assert_lowers_to(
        r#"
        init {
            let x = calldataload(0);
            let z = if slt(x, 0) {
                0
            } else if lt(x, 237) {
                1
            } else {
                2
            };
            evm_stop();
        }
        "#,
        r#"
        Init: @0
        Functions:
            fn @0 -> entry @0  (outputs: 0)

        Basic Blocks:
            @0 {
                $0 = const 0x0
                $1 = calldataload $0
                $2 = copy $1
                $3 = const 0x0
                $4 = slt $2 $3
                => $4 ? @1 : @2
            }

            @1 {
                $5 = const 0x0
                => @6
            }

            @2 {
                $6 = copy $1
                $7 = const 0xed
                $8 = lt $6 $7
                => $8 ? @3 : @4
            }

            @3 {
                $5 = const 0x1
                => @5
            }

            @4 {
                $5 = const 0x2
                => @5
            }

            @5 {
                => @6
            }

            @6 {
                $9 = copy $5
                stop
            }
        "#,
    );
}

#[test]
fn test_while() {
    assert_lowers_to(
        r#"
        init {
            let mut i = 0;
            while lt(i, 10) {
                i = add(i, 1);
            }
            evm_stop();
        }
        "#,
        r#"
        Init: @0
        Functions:
            fn @0 -> entry @0  (outputs: 0)

        Basic Blocks:
            @0 {
                $0 = const 0x0
                => @1
            }

            @1 {
                $1 = copy $0
                $2 = const 0xa
                $3 = lt $1 $2
                => $3 ? @2 : @3
            }

            @2 {
                $4 = copy $0
                $5 = const 0x1
                $0 = add $4 $5
                => @1
            }

            @3 {
                stop
            }
        "#,
    );
}

#[test]
fn test_struct_lit() {
    assert_lowers_to(
        r#"
        const A = struct { a: u256, b: bool };
        init {
            let mut a = A { a: 3, b: false };
            a = A { a: 2, b: true };
            evm_stop();
        }
        "#,
        r#"
        Init: @0
        Functions:
            fn @0 -> entry @0  (outputs: 0)

        Basic Blocks:
            @0 {
                $0 = const 0x3
                $1 = const 0x0
                $2 = copy $0
                $3 = copy $1
                $4 = const 0x2
                $5 = const 0x1
                $2 = copy $4
                $3 = copy $5
                stop
            }
        "#,
    );
}

#[test]
fn test_struct_field_access() {
    assert_lowers_to(
        r#"
        const A = struct { a: u256, wow: void,  b: bool };
        init {
            let a = A { a: 3, wow: {}, b: false };
            let buf = malloc_uninit(32);
            mstore32(buf, a.a);
            evm_return(buf, 32);
        }
        "#,
        r#"
        Init: @0
        Functions:
            fn @0 -> entry @0  (outputs: 0)

        Basic Blocks:
            @0 {
                $0 = const 0x3
                $1 = const 0x0
                $2 = copy $0
                $3 = copy $1
                $4 = const 0x20
                $5 = mallocany $4
                $6 = copy $5
                $7 = copy $2
                $8 = copy $3
                $9 = copy $7
                mstore256 $6 $9
                $10 = copy $5
                $11 = const 0x20
                return $10 $11
            }
        "#,
    );
}

#[test]
fn test_fn_struct_return() {
    assert_lowers_to(
        r#"
        const Pair = struct { a: u256, b: u256 };
        const swap = fn (x: u256, y: u256) Pair {
            Pair { a: y, b: x }
        };

        init {
            let x = swap(3, 4);
            evm_stop();
        }
        "#,
        r#"
        Init: @1
        Functions:
            fn @0 -> entry @0  (outputs: 2)
            fn @1 -> entry @1  (outputs: 0)

        Basic Blocks:
            @0 $0 $1 -> $4 $5 {
                $2 = copy $1
                $3 = copy $0
                $4 = copy $2
                $5 = copy $3
                iret
            }

            @1 {
                $6 = const 0x3
                $7 = const 0x4
                $8 $9 = icall @0 $6 $7
                stop
            }
        "#,
    );
}

#[test]
fn test_weird_error() {
    assert_lowers_to(
        r#"
        const return_runtime = fn() never {
            let runtime: memptr = malloc_uninit(runtime_length());
            codecopy(runtime, runtime_start_offset(), runtime_length());
            evm_return(runtime, runtime_length());
        };


        const get_balance_slot = fn (owner: u256) u256 {
            let buf = malloc_uninit(32);
            mstore32(buf, owner);
            keccak256(buf, 32)
        };

        init {
            let owner = 34;
            let bal_slot = get_balance_slot(owner);

            return_runtime();
        }


        run {
            evm_stop();
        }
        "#,
        r#"
        Init: @2
        Run: @3
        Functions:
            fn @0 -> entry @0  (outputs: 1)
            fn @1 -> entry @1  (outputs: 0)
            fn @2 -> entry @2  (outputs: 0)
            fn @3 -> entry @3  (outputs: 0)

        Basic Blocks:
            @0 $0 -> $7 {
                $1 = const 0x20
                $2 = mallocany $1
                $3 = copy $2
                $4 = copy $0
                mstore256 $3 $4
                $5 = copy $2
                $6 = const 0x20
                $7 = keccak256 $5 $6
                iret
            }

            @1 {
                $8 = runtime_length
                $9 = mallocany $8
                $10 = copy $9
                $11 = runtime_start_offset
                $12 = runtime_length
                codecopy $10 $11 $12
                $13 = copy $9
                $14 = runtime_length
                return $13 $14
            }

            @2 {
                $15 = const 0x22
                $16 = copy $15
                $17 = icall @0 $16
                icall @1
                invalid
            }

            @3 {
                stop
            }
        "#,
    );
}
