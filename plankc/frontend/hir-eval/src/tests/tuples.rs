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
            %0 : tuple {u256, bool} = tuple {u256, bool} {
                34,
                false,
            }
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
            %4 : tuple {u256, bool} = tuple {u256, bool} { %2, %3 }
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
            %7 : tuple {Pair, u256} = tuple {Pair, u256} { %5, %6 }
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
          |           ^^^^^^^^^^^^^^^^^^^^^^ type `memptr` of field #1 is runtime only, while type `type` of field #0 is comptime only
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

#[test]
fn test_comptime_tuple_field_count() {
    assert_lowers_to(
        r#"
        const Triple = tuple { u256, bool, u256 };
        const count = @field_count(Triple);
        init {
            let mut x: u256 = count;
            @evm_stop();
        }
        "#,
        r#"
        ==== Functions ====
        ; init
        @fn0() -> never {
            %0 : u256 = 3
            %1 : never = @evm_stop()
        }
        "#,
    );
}

#[test]
fn test_comptime_tuple_field_type() {
    assert_lowers_to(
        r#"
        const Pair = tuple { u256, bool };
        const T0 = @field_type(Pair, 0);
        const T1 = @field_type(Pair, 1);
        init {
            let mut x: T0 = 42;
            let mut y: T1 = true;
            @evm_stop();
        }
        "#,
        r#"
        ==== Functions ====
        ; init
        @fn0() -> never {
            %0 : u256 = 42
            %1 : bool = true
            %2 : never = @evm_stop()
        }
        "#,
    );
}

#[test]
fn test_comptime_tuple_get_field() {
    assert_lowers_to(
        r#"
        const pair = (42, true);
        const val = @get_field(pair, 0);
        init {
            let mut x = val;
            @evm_stop();
        }
        "#,
        r#"
        ==== Functions ====
        ; init
        @fn0() -> never {
            %0 : u256 = 42
            %1 : never = @evm_stop()
        }
        "#,
    );
}

#[test]
fn test_runtime_tuple_get_field() {
    assert_lowers_to(
        r#"
        init {
            let pair = (@evm_calldataload(0), @evm_calldataload(0x20));
            let val = @get_field(pair, 1);
            let mut x: u256 = val;
            @evm_stop();
        }
        "#,
        r#"
        ==== Functions ====
        ; init
        @fn0() -> never {
            %0 : u256 = 0
            %1 : u256 = @evm_calldataload(%0)
            %2 : u256 = 32
            %3 : u256 = @evm_calldataload(%2)
            %4 : tuple {u256, u256} = tuple {u256, u256} { %1, %3 }
            %5 : tuple {u256, u256} = %4
            %6 : u256 = %5.1
            %7 : u256 = %6
            %8 : never = @evm_stop()
        }
        "#,
    );
}

#[test]
fn test_comptime_tuple_set_field() {
    assert_lowers_to(
        r#"
        const pair = (1, 2);
        const pair2 = @set_field(pair, 0, 99);
        const val = @get_field(pair2, 0);
        init {
            let mut x: u256 = val;
            @evm_stop();
        }
        "#,
        r#"
        ==== Functions ====
        ; init
        @fn0() -> never {
            %0 : u256 = 99
            %1 : never = @evm_stop()
        }
        "#,
    );
}

#[test]
fn test_runtime_tuple_set_field() {
    assert_lowers_to(
        r#"
        init {
            let pair = (@evm_calldataload(0), @evm_calldataload(0x20));
            let pair2 = @set_field(pair, 0, 99);
            let mut x: u256 = @get_field(pair2, 0);
            @evm_stop();
        }
        "#,
        r#"
        ==== Functions ====
        ; init
        @fn0() -> never {
            %0 : u256 = 0
            %1 : u256 = @evm_calldataload(%0)
            %2 : u256 = 32
            %3 : u256 = @evm_calldataload(%2)
            %4 : tuple {u256, u256} = tuple {u256, u256} { %1, %3 }
            %5 : tuple {u256, u256} = %4
            %6 : u256 = 99
            %7 : u256 = %5.1
            %8 : tuple {u256, u256} = tuple {u256, u256} { %6, %7 }
            %9 : tuple {u256, u256} = %8
            %10 : u256 = %9.0
            %11 : never = @evm_stop()
        }
        "#,
    );
}

#[test]
fn test_tuple_field_type_out_of_bounds() {
    assert_diagnostics(
        r#"
        const T = @field_type(tuple { u256, bool }, 3);
        init { @evm_stop(); }
        "#,
        &[r#"
        error: field index out of bounds
         --> main.plk:1:45
          |
        1 | const T = @field_type(tuple { u256, bool }, 3);
          |                                             ^ `@field_type`: field index 3 is out of bounds for type with 2 fields
        "#],
    );
}

#[test]
fn test_tuple_get_field_out_of_bounds() {
    assert_diagnostics(
        r#"
        const pair = (1, true);
        const val = @get_field(pair, 3);
        init { @evm_stop(); }
        "#,
        &[r#"
        error: field index out of bounds
         --> main.plk:2:30
          |
        2 | const val = @get_field(pair, 3);
          |                              ^ `@get_field`: field index 3 is out of bounds for type with 2 fields
        "#],
    );
}

#[test]
fn test_tuple_set_field_out_of_bounds() {
    assert_diagnostics(
        r#"
        const pair = (1, true);
        const pair2 = @set_field(pair, 3, 99);
        init { @evm_stop(); }
        "#,
        &[r#"
        error: field index out of bounds
         --> main.plk:2:32
          |
        2 | const pair2 = @set_field(pair, 3, 99);
          |                                ^ `@set_field`: field index 3 is out of bounds for type with 2 fields
        "#],
    );
}

#[test]
fn test_tuple_set_field_non_num_index() {
    assert_diagnostics(
        r#"
        const pair = (1, 2);
        const pair2 = @set_field(pair, false, 99);
        init { @evm_stop(); }
        "#,
        &[r#"
        error: invalid field selector
         --> main.plk:2:32
          |
        2 | const pair2 = @set_field(pair, false, 99);
          |                                ^^^^^ `@set_field` field selector must be `u256`, got `bool`
        "#],
    );
}

#[test]
fn test_tuple_get_field_runtime_index() {
    assert_diagnostics(
        r#"
        init {
            let pair = (1, 2);
            let val = @get_field(pair, @evm_calldataload(0));
            @evm_stop();
        }
        "#,
        &[r#"
        error: expected comptime argument
         --> main.plk:3:15
          |
        3 |     let val = @get_field(pair, @evm_calldataload(0));
          |               ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ `@get_field` requires field selector to be known at comptime
        "#],
    );
}

#[test]
fn test_tuple_set_field_type_mismatch() {
    assert_diagnostics(
        r#"
        const pair = (1, true);
        const pair2 = @set_field(pair, 1, 42);
        init { @evm_stop(); }
        "#,
        &[r#"
        error: mismatched types
         --> main.plk:2:15
          |
        2 | const pair2 = @set_field(pair, 1, 42);
          |               ^^^^^^^^^^^^^^^^^^^^^^^ expected `bool`, got `u256`
        "#],
    );
}

#[test]
fn test_empty_tuple_void_equivalent() {
    assert_lowers_to(
        "
        init {
            let mut x = tuple {} == void;
            @evm_stop();
        }
        ",
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
fn test_empty_tuple_in_diagnostic_is_void() {
    assert_diagnostics(
        "
        init {
            let x: tuple {} = 0;
            @evm_stop();
        }
        ",
        &[r#"
        error: mismatched types
         --> main.plk:2:23
          |
        2 |     let x: tuple {} = 0;
          |            --------   ^ expected `void`, got `u256`
          |            |
          |            `void` expected because of this
          |
          = note: `void` is an alias for `tuple {}`
        "#],
    );
}
