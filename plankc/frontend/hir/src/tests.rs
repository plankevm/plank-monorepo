use crate::{BigNumInterner, Hir, display::DisplayHir};
use plank_parser::{PlankInterner, error_report::ParserError};
use plank_test_utils::{TestProject, dedent_preserve_blank_lines};

fn try_lower(source: &str) -> Result<(Hir, BigNumInterner, PlankInterner), Vec<ParserError>> {
    let mut interner = PlankInterner::default();
    let project = TestProject::single(source)
        .build(&mut interner)
        .map_err(|collector| collector.errors.into_iter().map(|(_, e)| e).collect::<Vec<_>>())?;

    let mut big_nums = BigNumInterner::default();
    let hir = crate::lower(&project, &mut big_nums);

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
fn test_inline_closure_lowering() {
    assert_lowers_to(
        r#"
        init {
            let halt = fn() never {
                evm_stop();
            };
            halt();
        }
        run {
            let halt = fn() never {
                invalid();
            };
            let abort = fn() never {
                halt();
            };
            abort();
        }
        "#,
        r#"
        ==== Constants ====

        ==== Functions ====
        @fn0() -> %0 {
            preamble:
                %0 = type#6
            body:
                eval evm_stop()
                ret void
        }
        @fn1() -> %0 {
            preamble:
                %0 = type#6
            body:
                eval invalid()
                ret void
        }
        @fn2() -> %0 {
            captures: [%0 -> %1]
            preamble:
                %0 = type#6
            body:
                %2 = %1
                eval call %2()
                ret void
        }

        ==== Init ====
        %0 = @fn0
        %1 = %0
        eval call %1()

        ==== Run ====
        %0 = @fn1
        %1 = @fn2
        %2 = %1
        eval call %2()
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

#[test]
#[should_panic(expected = "assignment to immutable variable")]
fn test_assign_to_immutable_let() {
    let _ = try_lower(
        "
        init {
            let x = 1;
            x = 2;
        }
        ",
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
        ==== Constants ====
        ConstId(0) ("Pair") result=LocalId(0) {
            %1 = void
            %2 = type#1
            %3 = type#1
            %0 = @struct0
        }
        ConstId(1) ("swap") result=LocalId(0) {
            %0 = @fn0
        }

        ==== Functions ====
        @fn0(%1: %0, %3: %2) -> %4 {
            preamble:
                %0 = type#1
                %2 = type#1
                %4 = $0
            body:
                %5 = $0
                %6 = %3
                %7 = %1
                ret %5 { a: %6, b: %7 }
        }

        ==== Structs ====
        @struct0[index: %1] { a: %2, b: %3 }

        ==== Init ====
        %0 = $1
        %1 = 3
        %2 = 4
        %3 = call %0(%1, %2)
        eval evm_stop()
        "#,
    );
}

#[test]
fn test_assign_to_mutable_let() {
    assert_lowers_to(
        r#"
        init {
            let mut x = 1;
            x = 2;
        }
        "#,
        r#"
        ==== Constants ====

        ==== Init ====
        %0 = 1
        %0 := 2
        "#,
    );
}
