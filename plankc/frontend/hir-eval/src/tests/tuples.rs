use super::*;

#[test]
fn test_comptime_tuple_literal() {
    assert_lowers_to(
        r#"
        const pair = (34, false);

        init {
            let mut x = pair;
            @evm_stop();
        }
        "#,
        r#"
        ==== Functions ====
        ; init
        @fn0() -> never {
            %0 : tuple {u256, bool} = tuple {u256, bool} (
                34,
                false,
            )
            %1 : never = @evm_stop()
        }
        "#,
    );
}

#[test]
fn test_runtime_tuple_literal() {
    assert_lowers_to(
        r#"
        init {
            let x = @evm_calldataload(0);
            let mut pair = (x, false);
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
            %3 : bool = false
            %4 : tuple {u256, bool} = tuple {u256, bool} ( %2, %3 )
            %5 : never = @evm_stop()
        }
        "#,
    );
}

#[test]
fn test_tuple_with_struct_element() {
    assert_lowers_to(
        r#"
        const Pair = struct { a: u256, b: bool };

        init {
            let x = @evm_calldataload(0);
            let p = Pair { a: x, b: false };
            let mut t = (p, 7);
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
            %3 : bool = false
            %4 : Pair = Pair { %2, %3 }
            %5 : Pair = %4
            %6 : u256 = 7
            %7 : tuple {Pair, u256} = tuple {Pair, u256} ( %5, %6 )
            %8 : never = @evm_stop()
        }
        "#,
    );
}

#[test]
fn test_mixed_tuple_type() {
    assert_diagnostics(
        r#"
        const T = tuple { type, memptr };
        init { @evm_stop(); }
        "#,
        &[r#"
        error: defining uninstantiable type
         --> main.plk:1:11
          |
        1 | const T = tuple { type, memptr };
          |           ^^^^^^^^^^^^^^^^^^^^^^ type 'memptr' of field #1 is runtime only, while type 'type' of field #0 is comptime only
        "#],
    );
}

#[test]
fn test_mixed_comptime_runtime_tuple() {
    assert_diagnostics(
        r#"
        init {
            let x = @evm_calldataload(0);
            let pair = (u256, x);
            @evm_stop();
        }
        "#,
        &[r#"
        error: mixing comptime and runtime data in tuple
         --> main.plk:3:16
          |
        3 |     let pair = (u256, x);
          |                ^----^^-^
          |                ||     |
          |                ||     tuple element not comptime-known
          |                |tuple element is comptime-only
          |                mixed tuple literal
        "#],
    );
}
