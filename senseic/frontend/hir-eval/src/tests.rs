use sensei_hir::BigNumInterner;
use sensei_mir::{Mir, display::DisplayMir};
use sensei_parser::{PlankInterner, error_report::ParserError};
use sensei_test_utils::{TestProject, dedent_preserve_blank_lines};

fn try_lower(source: &str) -> Result<(Mir, BigNumInterner, PlankInterner), Vec<ParserError>> {
    let mut interner = PlankInterner::default();
    let project = TestProject::single(source)
        .build(&mut interner)
        .map_err(|collector| collector.errors.into_iter().map(|(_, e)| e).collect::<Vec<_>>())?;

    let mut big_nums = BigNumInterner::default();
    let hir = sensei_hir::lower(&project, &mut big_nums);
    let mir = crate::evaluate(&hir);

    Ok((mir, big_nums, interner))
}

fn assert_lowers_to(source: &str, expected: &str) {
    let (mir, big_nums, _interner) = match try_lower(source) {
        Ok(values) => values,
        Err(errors) => {
            panic!("Expected no parse errors, got: {}\n{:#?}", errors.len(), errors);
        }
    };
    let actual = format!("{}", DisplayMir::new(&mir, &big_nums));
    let expected = dedent_preserve_blank_lines(expected);

    pretty_assertions::assert_str_eq!(actual.trim(), expected.trim());
}

#[test]
fn test_simple_malloc_mstore_return() {
    assert_lowers_to(
        r#"
        init {
            let buf = malloc_uninit(0x20);
            mstore32(buf, 0x05);
            evm_return(buf, 0x20);
        }
        "#,
        r#"
        ==== Functions ====
        ; init
        @fn0() -> void {
            %0 : u256 = 32
            %1 : u256 = %0
            %2 : memptr = malloc_uninit(%1)
            %3 : memptr = %2
            %4 : memptr = %3
            %5 : u256 = 5
            %6 : u256 = %5
            %7 : void = mstore32(%4, %6)
            %8 : memptr = %3
            %9 : u256 = 32
            %10 : u256 = %9
            %11 : void = evm_return(%8, %10)
        }
        "#,
    );
}

#[test]
#[should_panic(expected = "type mismatch in AssertType")]
fn test_type_annotation_type_mismatch() {
    let _ = try_lower(
        "
        init {
            let x: u256 = false;
        }
        ",
    );
}

#[test]
#[should_panic(expected = "not yet implemented: diagnostic: type mismatch on set")]
fn test_if_branches_type_mismatch() {
    let _ = try_lower(
        "
        init {
            let c = calldataload(0);
            let x = if slt(c, 0)  {
                3
            } else {
                false
            };
        }
        ",
    );
}

#[test]
#[should_panic(expected = "not yet implemented: diagnostic: type mismatch in AssertType")]
fn test_if_type_mismatch() {
    let _ = try_lower(
        "
        init {
            let c = calldataload(0);
            let x: u256 = if slt(c, 0)  {
                true
            } else {
                false
            };
        }
        ",
    );
}
