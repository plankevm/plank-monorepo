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
