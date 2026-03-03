use crate::{BigNumInterner, Hir, display::DisplayHir};
use sensei_parser::{
    PlankInterner,
    error_report::{ErrorCollector, ParserError},
    lexer::Lexed,
    parser::parse,
};
use sensei_test_utils::{dedent_preserve_blank_lines, dedent_preserve_indent};

fn try_lower(source: &str) -> Result<(Hir, BigNumInterner, PlankInterner), Vec<ParserError>> {
    let source = dedent_preserve_indent(source);

    let lexed = Lexed::lex(&source);
    let mut interner = PlankInterner::default();
    let mut diagnostics = ErrorCollector::default();
    let cst = parse(&lexed, &mut interner, &mut diagnostics);

    if !diagnostics.errors.is_empty() {
        return Err(diagnostics.errors);
    }

    assert!(
        diagnostics.errors.is_empty(),
        "Expected no parse errors, but found {}:\n{:#?}",
        diagnostics.errors.len(),
        diagnostics.errors
    );

    let mut big_nums = BigNumInterner::default();
    let hir = crate::lower(&cst, &mut big_nums);

    Ok((hir, big_nums, interner))
}

fn assert_lowers_to(source: &str, expected: &str) {
    let (hir, big_nums, interner) = match try_lower(source) {
        Ok(values) => values,
        Err(errors) => {
            panic!("Expected no parse errors, got: {}\n{:#?}", errors.len(), errors);
        }
    };
    let actual = format!("{}", DisplayHir::new(&hir, &big_nums, &interner));
    let expected = dedent_preserve_blank_lines(expected);

    pretty_assertions::assert_str_eq!(actual.trim(), expected.trim());
}

#[test]
fn test_basic_init_builtin_calls() {
    assert_lowers_to(
        r#"
        init {
            let a = calldataload(0x00);
            let b: u256 = calldataload(0x20);
            let buf = malloc_uninit(0x20);
            mstore32(buf, add(a, b));
            evm_return(buf, 0x20);
        }
        "#,
        r#"
        ==== Constants ====

        ==== Init ====
        %0 = 0
        %1 = calldataload(%0)
        %2 = type#1
        %3 = 32
        %4 = calldataload(%3)
        assert_type %4 : %2
        %5 = 32
        %6 = malloc_uninit(%5)
        %7 = %6
        %8 = %1
        %9 = %4
        %10 = add(%8, %9)
        eval mstore32(%7, %10)
        %11 = %6
        %12 = 32
        eval evm_return(%11, %12)
        "#,
    );
}

#[test]
#[should_panic(expected = "unresolved assignment target")]
fn test_set_undefined() {
    let _ = try_lower(
        "
        init {
            y = 4;
        }
        ",
    );
}
