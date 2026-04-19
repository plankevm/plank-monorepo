use super::*;

#[test]
fn test_logical_not_runtime() {
    assert_lowers_to(
        r#"
        init {
            let c = @evm_calldataload(0);
            let b = @evm_iszero(c);
            let nb = !b;
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
            %4 : bool = %3
            %5 : bool = @evm_iszero(%4)
            %6 : never = @evm_stop()
        }
        "#,
    );
}

#[test]
fn test_logical_not_comptime_true() {
    assert_lowers_to(
        r#"
        const x = !true;
        init {
            let mut v: bool = x;
            @evm_stop();
        }
        "#,
        r#"
        ==== Functions ====
        ; init
        @fn0() -> never {
            %0 : bool = false
            %1 : never = @evm_stop()
        }
        "#,
    );
}

#[test]
fn test_logical_not_comptime_false() {
    assert_lowers_to(
        r#"
        const x = !false;
        init {
            let mut v: bool = x;
            @evm_stop();
        }
        "#,
        r#"
        ==== Functions ====
        ; init
        @fn0() -> never {
            %0 : bool = true
            %1 : never = @evm_stop()
        }
        "#,
    );
}

#[test]
fn test_logical_not_in_if_condition() {
    assert_lowers_to(
        r#"
        init {
            let c = @evm_calldataload(0);
            let b = @evm_iszero(c);
            if !b {
                @evm_stop();
            } else {
                @evm_revert(@malloc_uninit(0), 0);
            }
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
            %4 : bool = %3
            %5 : bool = @evm_iszero(%4)
            if %5 {
                %6 : never = @evm_stop()
            } else {
                %7 : u256 = 0
                %8 : memptr = @malloc_uninit(%7)
                %9 : u256 = 0
                %10 : never = @evm_revert(%8, %9)
            }
        }
        "#,
    );
}

#[test]
fn test_logical_not_type_mismatch_runtime() {
    assert_diagnostics(
        r#"
        init {
            let c = @evm_calldataload(0);
            let x = !c;
            @evm_stop();
        }
        "#,
        &[r#"
        error: mismatched types
         --> main.plk:3:14
          |
        3 |     let x = !c;
          |              ^ expected `bool`, got `u256`
        "#],
    );
}

#[test]
fn test_logical_not_type_mismatch_comptime() {
    assert_diagnostics(
        r#"
        const x = !42;
        init { @evm_stop(); }
        "#,
        &[r#"
        error: mismatched types
         --> main.plk:1:12
          |
        1 | const x = !42;
          |            ^^ expected `bool`, got `u256`
        "#],
    );
}

#[test]
fn test_and_comptime_short_circuit_false() {
    assert_lowers_to(
        r#"
        const x = false and true;
        init {
            let mut v: bool = x;
            @evm_stop();
        }
        "#,
        r#"
        ==== Functions ====
        ; init
        @fn0() -> never {
            %0 : bool = false
            %1 : never = @evm_stop()
        }
        "#,
    );
}

#[test]
fn test_and_condition_type_mismatch() {
    assert_diagnostics(
        r#"
        init {
            let c = @evm_calldataload(0);
            let x = c and true;
            @evm_stop();
        }
        "#,
        &[r#"
        error: mismatched types
         --> main.plk:3:13
          |
        3 |     let x = c and true;
          |             ^ expected `bool`, got `u256`
        "#],
    );
}

#[test]
fn test_or_condition_type_mismatch() {
    assert_diagnostics(
        r#"
        init {
            let c = @evm_calldataload(0);
            let x = c or true;
            @evm_stop();
        }
        "#,
        &[r#"
        error: mismatched types
         --> main.plk:3:13
          |
        3 |     let x = c or true;
          |             ^ expected `bool`, got `u256`
        "#],
    );
}
