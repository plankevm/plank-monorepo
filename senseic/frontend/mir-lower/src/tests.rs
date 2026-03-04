use sensei_parser::{
    PlankInterner,
    error_report::{ErrorCollector, ParserError},
    lexer::Lexed,
    parser::parse,
};
use sensei_test_utils::dedent_preserve_indent;
use sensei_values::BigNumInterner;

fn try_lower(source: &str) -> Result<sir_data::EthIRProgram, Vec<ParserError>> {
    let source = dedent_preserve_indent(source);
    let lexed = Lexed::lex(&source);
    let mut interner = PlankInterner::default();
    let mut diagnostics = ErrorCollector::default();
    let cst = parse(&lexed, &mut interner, &mut diagnostics);

    if !diagnostics.errors.is_empty() {
        return Err(diagnostics.errors);
    }

    let mut big_nums = BigNumInterner::default();
    let hir = sensei_hir::lower(&cst, &mut big_nums);
    let mir = sensei_hir_eval::evaluate(&hir);
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
    let expected = sensei_test_utils::dedent_preserve_blank_lines(expected);
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
                $1 = copy $0
                stop
            }

            @1 {
                $2 = const 0x0
                $3 = copy $2
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
                $1 = copy $0
                $2 = const 0x4
                $3 = copy $2
                $4 = const 0x3
                $5 = copy $4
                $6 = const 0x4
                $7 = copy $6
                $8 = add $5 $7
                $9 = copy $8
                $10 = const 0x3
                $11 = copy $10
                $12 = const 0x4
                $13 = copy $12
                $14 = add $11 $13
                $15 = callvalue
                $16 = copy $15
                $17 = const 0x22
                $18 = copy $17
                $19 = calldataload $18
                $20 = copy $19
                $21 = mallocany $20
                $22 = copy $21
                $23 = copy $1
                $24 = copy $9
                sstore $23 $24
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
                $1 = copy $0
                $2 = const 0x22
                $1 = copy $2
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
                $1 = copy $0
                $2 = mallocany $1
                $3 = copy $2
                $4 = copy $3
                $5 = const 0x0
                $6 = copy $5
                return $4 $6
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
            @0 -> $2 {
                $0 = const 0x0
                $1 = copy $0
                $2 = mallocany $1
                iret
            }

            @1 {
                $3 = icall @0
                $4 = copy $3
                $5 = copy $4
                $6 = const 0x0
                $7 = copy $6
                return $5 $7
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
                $7 = copy $6
                $8 = const 0x4
                $9 = copy $8
                $10 = icall @0 $7 $9
                $11 = copy $10
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
                $1 = copy $0
                $2 = calldataload $1
                $3 = copy $2
                $4 = copy $3
                $5 = const 0x0
                $6 = copy $5
                $7 = slt $4 $6
                $8 = copy $7
                => $8 ? @1 : @2
            }

            @1 {
                $9 = const 0x0
                $10 = copy $9
                $11 = mallocany $10
                $12 = copy $11
                $13 = const 0x0
                $14 = copy $13
                revert $12 $14
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
                $1 = copy $0
                $2 = calldataload $1
                $3 = copy $2
                $4 = copy $3
                $5 = const 0x0
                $6 = copy $5
                $7 = slt $4 $6
                $8 = copy $7
                => $8 ? @1 : @2
            }

            @1 {
                $9 = const 0x0
                $10 = copy $9
                => @6
            }

            @2 {
                $11 = copy $3
                $12 = const 0xed
                $13 = copy $12
                $14 = lt $11 $13
                $15 = copy $14
                => $15 ? @3 : @4
            }

            @3 {
                $16 = const 0x1
                $10 = copy $16
                => @5
            }

            @4 {
                $17 = const 0x2
                $10 = copy $17
                => @5
            }

            @5 {
                => @6
            }

            @6 {
                $18 = copy $10
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
                $1 = copy $0
                => @1
            }

            @1 {
                $2 = copy $1
                $3 = const 0xa
                $4 = copy $3
                $5 = lt $2 $4
                $6 = copy $5
                => $6 ? @2 : @3
            }

            @2 {
                $7 = copy $1
                $8 = const 0x1
                $9 = copy $8
                $10 = add $7 $9
                $1 = copy $10
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
                $1 = copy $0
                $2 = const 0x0
                $3 = copy $2
                $4 = copy $1
                $5 = copy $3
                $6 = copy $4
                $7 = copy $5
                $8 = const 0x2
                $9 = copy $8
                $10 = const 0x1
                $11 = copy $10
                $12 = copy $9
                $13 = copy $11
                $6 = copy $12
                $7 = copy $13
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
            let x = a.a;

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
                $1 = copy $0
                $2 = const 0x0
                $3 = copy $2
                $4 = copy $1
                $5 = copy $3
                $6 = copy $4
                $7 = copy $5
                $8 = copy $6
                $9 = copy $7
                $10 = copy $8
                $11 = copy $10
                stop
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
                $7 = copy $6
                $8 = const 0x4
                $9 = copy $8
                $10 $11 = icall @0 $7 $9
                $12 = copy $10
                $13 = copy $11
                stop
            }
        "#,
    );
}

#[test]
fn test_weird_error() {
    assert_lowers_to(
        r#"
        const return_runtime = fn() void {
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

            evm_stop();
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
            @0 $0 -> $10 {
                $1 = const 0x20
                $2 = copy $1
                $3 = mallocany $2
                $4 = copy $3
                $5 = copy $4
                $6 = copy $0
                mstore256 $5 $6
                $7 = copy $4
                $8 = const 0x20
                $9 = copy $8
                $10 = keccak256 $7 $9
                iret
            }

            @1 {
                $11 = runtime_length
                $12 = copy $11
                $13 = mallocany $12
                $14 = copy $13
                $15 = copy $14
                $16 = runtime_start_offset
                $17 = copy $16
                $18 = runtime_length
                $19 = copy $18
                codecopy $15 $17 $19
                $20 = copy $14
                $21 = runtime_length
                $22 = copy $21
                return $20 $22
            }

            @2 {
                $23 = const 0x22
                $24 = copy $23
                $25 = copy $24
                $26 = icall @0 $25
                $27 = copy $26
                icall @1
                stop
            }

            @3 {
                stop
            }
        "#,
    );
}
