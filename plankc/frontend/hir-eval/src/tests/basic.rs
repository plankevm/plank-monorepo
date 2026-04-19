use super::*;

#[test]
fn test_simple_malloc_mstore_return() {
    assert_lowers_to(
        r#"
        init {
            let buf = @malloc_uninit(0x20);
            @mstore32(buf, 0x05);
            @evm_return(buf, 0x20);
        }
        "#,
        r#"
        ==== Functions ====
        ; init
        @fn0() -> never {
            %0 : u256 = 32
            %1 : memptr = @malloc_uninit(%0)
            %2 : memptr = %1
            %3 : u256 = 5
            %4 : void = @mstore32(%2, %3)
            %5 : memptr = %1
            %6 : u256 = 32
            %7 : never = @evm_return(%5, %6)
        }
        "#,
    );
}

#[test]
fn test_no_else_if_as_expr() {
    assert_lowers_to(
        "
        init {
            let cond = @evm_calldataload(0);
            let y = if @evm_iszero(cond) {
                @evm_revert(@malloc_uninit(0), 0);
            } else if @evm_gt(cond, 2) {
                @evm_sstore(3, 4);
            };
            @evm_stop();
        }
        ",
        r#"
        ==== Functions ====
        ; init
        @fn0() -> never {
            %0 : u256 = 0
            %1 : u256 = @evm_calldataload(%0)
            %2 : u256 = %1
            %3 : bool = @evm_iszero(%2)
            if %3 {
                %4 : u256 = 0
                %5 : memptr = @malloc_uninit(%4)
                %6 : u256 = 0
                %7 : never = @evm_revert(%5, %6)
            } else {
                %8 : u256 = %1
                %9 : u256 = 2
                %10 : bool = @evm_gt(%8, %9)
                if %10 {
                    %11 : u256 = 3
                    %12 : u256 = 4
                    %13 : void = @evm_sstore(%11, %12)
                    %14 : void = void_unit
                } else {
                    %14 : void = void_unit
                }
            }
            %15 : void = %14
            %16 : never = @evm_stop()
        }
        "#,
    );
}

#[test]
fn test_comptime_if_condition_folds_in_runtime() {
    assert_lowers_to(
        "
        init {
            let cond = false;
            if cond {
                @evm_revert(@malloc_uninit(0), 0);
            } else {
                @evm_sstore(3, 4);
            }
            @evm_stop();
        }
        ",
        r#"
        ==== Functions ====
        ; init
        @fn0() -> never {
            %0 : u256 = 3
            %1 : u256 = 4
            %2 : void = @evm_sstore(%0, %1)
            %3 : void = void_unit
            %4 : void = %3
            %5 : never = @evm_stop()
        }
        "#,
    );
}

#[test]
fn test_if_three_branches() {
    assert_lowers_to(
        "
        init {
            let c = @evm_calldataload(0);
            let x = if @evm_slt(c, 0)  {
                334
            } else if @evm_iszero(c) {
                333
            } else {
                0
            };
            @evm_stop();
        }
        ",
        r#"
        ==== Functions ====
        ; init
        @fn0() -> never {
            %0 : u256 = 0
            %1 : u256 = @evm_calldataload(%0)
            %2 : u256 = %1
            %3 : u256 = 0
            %4 : bool = @evm_slt(%2, %3)
            if %4 {
                %5 : u256 = 334
            } else {
                %6 : u256 = %1
                %7 : bool = @evm_iszero(%6)
                if %7 {
                    %5 : u256 = 333
                } else {
                    %5 : u256 = 0
                }
            }
            %8 : u256 = %5
            %9 : never = @evm_stop()
        }
        "#,
    );
}

#[test]
fn test_run_missing_termination() {
    assert_diagnostics(
        "
        init {
            @evm_stop();
        }
        run {
            let x = 5;
        }
        ",
        &[r#"
        error: entry point must end with explicit terminator
         --> main.plk:4:1
          |
        4 | / run {
        5 | |     let x = 5;
        6 | | }
          | |_^ execution may reach end of entry point
          |
          = help: entry points must end with a terminating `never` expression (e.g. `@evm_stop()`, `@evm_revert(...)`, `@evm_invalid()`)
        "#],
    );
}

#[test]
fn test_never_fn_missing_termination() {
    assert_diagnostics(
        "
        init {
            let halt = fn() never {
                let x = 5;
            };
            halt();
        }
        ",
        &[r#"
        error: mismatched types
         --> main.plk:2:27
          |
        2 |       let halt = fn() never {
          |  _____________________-----_^
          | |                     |
          | |                     `never` expected because of this
        3 | |         let x = 5;
        4 | |     };
          | |_____^ expected `never`, got `void`
        "#],
    );
}

#[test]
fn test_init_run_with_never_fn() {
    assert_lowers_to(
        "
        init {
            let halt = fn() never {
                @evm_stop();
            };
            halt();
        }
        run {
            let halt = fn() never {
                @evm_invalid();
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
            %0 : never = @evm_stop()
        }

        ; init
        @fn1() -> never {
            %0 : never = call @fn0()
        }

        @fn2() -> never {
            %0 : never = @evm_invalid()
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
            @evm_stop();
            let x = 42;
        }
        "#,
        r#"
        ==== Functions ====
        ; init
        @fn0() -> never {
            %0 : never = @evm_stop()
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
                @evm_stop();
            };
            @mstore32(@malloc_uninit(0x20), halt());
        }
        "#,
        r#"
        ==== Functions ====
        @fn0() -> never {
            %0 : never = @evm_stop()
        }

        ; init
        @fn1() -> never {
            %0 : u256 = 32
            %1 : memptr = @malloc_uninit(%0)
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
            let c = @evm_calldataload(0);
            let x = if @evm_iszero(c) {
                @evm_stop()
            } else {
                42
            };
            @evm_stop();
        }
        "#,
        r#"
        ==== Functions ====
        ; init
        @fn0() -> never {
            %0 : u256 = 0
            %1 : u256 = @evm_calldataload(%0)
            %2 : u256 = %1
            %3 : bool = @evm_iszero(%2)
            if %3 {
                %4 : never = @evm_stop()
            } else {
                %5 : u256 = 42
            }
            %6 : u256 = %5
            %7 : never = @evm_stop()
        }
        "#,
    );
}

#[test]
fn test_init_missing_termination() {
    assert_diagnostics(
        "
        init {
            @evm_sstore(0, 1);
        }
        ",
        &[r#"
        error: entry point must end with explicit terminator
         --> main.plk:1:1
          |
        1 | / init {
        2 | |     @evm_sstore(0, 1);
        3 | | }
          | |_^ execution may reach end of entry point
          |
          = help: entry points must end with a terminating `never` expression (e.g. `@evm_stop()`, `@evm_revert(...)`, `@evm_invalid()`)
        "#],
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
            @evm_stop();
        }
        "#,
        r#"
        ==== Functions ====
        @fn0(%0: u256, %1: u256) -> Pair {
            %2 : u256 = %1
            %3 : u256 = %0
            %4 : Pair = Pair { %2, %3 }
            ret %4
        }

        ; init
        @fn1() -> never {
            %0 : u256 = 3
            %1 : u256 = 4
            %2 : Pair = call @fn0(%0, %1)
            %3 : never = @evm_stop()
        }
        "#,
    );
}
