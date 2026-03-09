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
        @fn0() -> never {
            %0 : u256 = 32
            %1 : memptr = malloc_uninit(%0)
            %2 : memptr = %1
            %3 : u256 = 5
            %4 : void = mstore32(%2, %3)
            %5 : memptr = %1
            %6 : u256 = 32
            %7 : never = evm_return(%5, %6)
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

#[test]
#[should_panic(
    expected = "not yet implemented: diagnostic: entry point must have an explicit terminator"
)]
fn test_run_missing_termination() {
    let _ = try_lower(
        "
            init {
                evm_stop();
            }
            run {
                let x = 5;
            }
        ",
    );
}

#[test]
#[should_panic(expected = "return type mismatch")]
fn test_never_fn_missing_termination() {
    let _ = try_lower(
        "
            init {
                let halt = fn() never {
                    let x = 5;
                };
                halt();
            }
        ",
    );
}

#[test]
fn test_init_run_with_never_fn() {
    assert_lowers_to(
        "
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
        ",
        "
        ==== Functions ====
        @fn0() -> never {
            %0 : never = evm_stop()
        }

        ; init
        @fn1() -> never {
            %0 : never = call @fn0()
        }

        @fn2() -> never {
            %0 : never = invalid()
        }

        @fn3() -> never {
            %0 : never = call @fn2()
        }

        ; run
        @fn4() -> never {
            %0 : never = call @fn3()
        }
        ",
    );
}

#[test]
fn test_diverging_block_middle() {
    assert_lowers_to(
        r#"
        init {
            evm_stop();
            let x = 42;
        }
        "#,
        r#"
        ==== Functions ====
        ; init
        @fn0() -> never {
            %0 : never = evm_stop()
        }
        "#,
    );
}

#[test]
fn test_builtin_call_with_never_arg() {
    assert_lowers_to(
        r#"
        init {
            let halt = fn() never {
                evm_stop();
            };
            mstore32(malloc_uninit(0x20), halt());
        }
        "#,
        r#"
        ==== Functions ====
        @fn0() -> never {
            %0 : never = evm_stop()
        }

        ; init
        @fn1() -> never {
            %0 : u256 = 32
            %1 : memptr = malloc_uninit(%0)
            %2 : never = call @fn0()
        }
        "#,
    );
}

#[test]
fn test_if_mixed_never_and_value_branches() {
    assert_lowers_to(
        r#"
        init {
            let c = calldataload(0);
            let x = if iszero(c) {
                evm_stop()
            } else {
                42
            };
            evm_stop();
        }
        "#,
        r#"
        ==== Functions ====
        ; init
        @fn0() -> never {
            %0 : u256 = 0
            %1 : u256 = calldataload(%0)
            %2 : u256 = %1
            %3 : bool = iszero(%2)
            if %3 {
                %4 : u256 = evm_stop()
            } else {
                %4 : u256 = 42
            }
            %5 : u256 = %4
            %6 : never = evm_stop()
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
        ==== Functions ====
        @fn0(%0: u256, %1: u256) -> struct#0 {
            %2 : u256 = %1
            %3 : u256 = %0
            %4 : struct#0 = struct#0 { %2, %3 }
            ret %4
        }

        ; init
        @fn1() -> never {
            %0 : u256 = 3
            %1 : u256 = 4
            %2 : struct#0 = call @fn0(%0, %1)
            %3 : never = evm_stop()
        }
        "#,
    );
}
