use super::*;

#[test]
fn test_type_annotation_type_mismatch() {
    assert_diagnostics(
        "
        init {
            let x: u256 = false;
            @evm_stop();
        }
        ",
        &[r#"
        error: mismatched types
         --> main.plk:2:19
          |
        2 |     let x: u256 = false;
          |            ----   ^^^^^ expected `u256`, got `bool`
          |            |
          |            `u256` expected because of this
        "#],
    );
}

#[test]
fn test_if_two_branches_type_mismatch() {
    assert_diagnostics(
        "
        init {
            let c = @evm_calldataload(0);
            let x = if @evm_slt(c, 0)  {
                334
            } else {
                false
            };
            @evm_stop();
        }
        ",
        &[r#"
            error: `if` and `else` have incompatible types
             --> main.plk:6:9
              |
            4 |         334
              |         --- `u256` expected because of this
            5 |     } else {
            6 |         false
              |         ^^^^^ expected `u256`, got `bool`
        "#],
    );
}

#[test]
fn test_if_three_branches_type_mismatch() {
    assert_diagnostics(
        "
        init {
            let c = @evm_calldataload(0);
            let x = if @evm_slt(c, 0) {
                3
            } else if @evm_eq(c, 34) {
                false
            } else {
                true
            };
            @evm_stop();
        }
        ",
        &[r#"
            error: `if` and `else` have incompatible types
             --> main.plk:6:9
              |
            4 |         3
              |         - `u256` expected because of this
            5 |     } else if @evm_eq(c, 34) {
            6 |         false
              |         ^^^^^ expected `u256`, got `bool`
            "#],
    );
}

#[test]
fn test_if_type_mismatch() {
    assert_diagnostics(
        "
        init {
            let c = @evm_calldataload(0);
            let x: u256 = if @evm_slt(c, 0)  {
                true
            } else {
                false
            };
            @evm_stop();
        }
        ",
        &[r#"
            error: mismatched types
             --> main.plk:3:19
              |
            3 |       let x: u256 = if @evm_slt(c, 0)  {
              |  ____________----___^
              | |            |
              | |            `u256` expected because of this
            4 | |         true
            5 | |     } else {
            6 | |         false
            7 | |     };
              | |_____^ expected `u256`, got `bool`
            "#],
    );
}

#[test]
fn test_assign_type_mismatch() {
    assert_diagnostics(
        r#"
        init {
            let mut x = 1;
            x = false;
            @evm_stop();
        }
        "#,
        &[r#"
        error: mismatched types
         --> main.plk:3:9
          |
        2 |     let mut x = 1;
          |                 - `u256` expected because of this
        3 |     x = false;
          |         ^^^^^ expected `u256`, got `bool`
        "#],
    );
}

#[test]
fn test_runtime_fn_return_type_not_type() {
    assert_diagnostics(
        r#"
        const forty_two = 42;
        init {
            let f = fn() forty_two { return 1; };
            f();
            @evm_stop();
        }
        "#,
        &[r#"
        error: value used as type
         --> main.plk:3:18
          |
        1 | const forty_two = 42;
          | --------------------- defined here
        2 | init {
        3 |     let f = fn() forty_two { return 1; };
          |                  ^^^^^^^^^ expected type, got value of type `u256`
          |
        note: called here
         --> main.plk:4:5
          |
        4 |     f();
          |     ^^^
        "#],
    );
}

#[test]
fn test_comptime_assign_type_mismatch() {
    assert_diagnostics(
        r#"
        const f = fn() u256 {
            let mut x = 1;
            x = false;
            return x;
        };
        const r = f();
        init { @evm_stop(); }
        "#,
        &[r#"
        error: mismatched types
         --> main.plk:3:9
          |
        2 |     let mut x = 1;
          |                 - `u256` expected because of this
        3 |     x = false;
          |         ^^^^^ expected `u256`, got `bool`
        "#],
    );
}

#[test]
fn test_comptime_call_arg_type_mismatch() {
    assert_diagnostics(
        r#"
        const f = fn(x: u256) u256 { return x; };
        const r = f(false);
        init { @evm_stop(); }
        "#,
        &[r#"
        error: mismatched types
         --> main.plk:2:13
          |
        1 | const f = fn(x: u256) u256 { return x; };
          |                 ---- `u256` expected because of this
        2 | const r = f(false);
          |             ^^^^^ expected `u256`, got `bool`
        "#],
    );
}

#[test]
fn test_runtime_return_type_mismatch() {
    assert_diagnostics(
        r#"
        init {
            let f = fn() u256 { return false; };
            f();
            @evm_stop();
        }
        "#,
        &[r#"
        error: mismatched types
         --> main.plk:2:32
          |
        2 |     let f = fn() u256 { return false; };
          |                  ----          ^^^^^ expected `u256`, got `bool`
          |                  |
          |                  `u256` expected because of this
        "#],
    );
}

#[test]
fn test_comptime_if_condition_not_bool() {
    assert_diagnostics(
        r#"
        init {
            comptime {
                if 42 {
                    @evm_add(3, 4);
                } else {
                    @evm_iszero(34);
                }
            }
            @evm_stop();
        }
        "#,
        &[r#"
        error: mismatched types
         --> main.plk:3:12
          |
        3 |         if 42 {
          |            ^^ expected `bool`, got `u256`
        "#],
    );
}

#[test]
fn test_runtime_if_condition_comptime_not_bool() {
    assert_diagnostics(
        "
        init {
            if 42 { @evm_stop(); } else { @evm_stop(); }
        }
        ",
        &[r#"
        error: mismatched types
         --> main.plk:2:8
          |
        2 |     if 42 { @evm_stop(); } else { @evm_stop(); }
          |        ^^ expected `bool`, got `u256`
        "#],
    );
}

#[test]
fn test_runtime_if_condition_runtime_not_bool() {
    assert_diagnostics(
        "
        init {
            let c = @evm_calldataload(0);
            if c { @evm_stop(); } else { @evm_stop(); }
        }
        ",
        &[r#"
        error: mismatched types
         --> main.plk:3:8
          |
        3 |     if c { @evm_stop(); } else { @evm_stop(); }
          |        ^ expected `bool`, got `u256`
        "#],
    );
}

#[test]
fn test_runtime_while_condition_not_bool() {
    assert_diagnostics(
        "
        init {
            let c = @evm_calldataload(0);
            while c { }
            @evm_stop();
        }
        ",
        &[r#"
        error: mismatched types
         --> main.plk:3:11
          |
        3 |     while c { }
          |           ^ expected `bool`, got `u256`
        "#],
    );
}

#[test]
fn test_diagnostic_renders_struct_name() {
    assert_diagnostics(
        r#"
        const Pair = struct { a: u256, b: bool };
        init {
            let x: Pair = 42;
            @evm_stop();
        }
        "#,
        &[r#"
        error: mismatched types
         --> main.plk:3:19
          |
        3 |     let x: Pair = 42;
          |            ----   ^^ expected `Pair`, got `u256`
          |            |
          |            `Pair` expected because of this
        "#],
    );
}

#[test]
fn test_diagnostic_renders_generic_struct_name() {
    assert_diagnostics(
        r#"
        const Box = fn (comptime T: type) type {
            struct T { value: T }
        };

        init {
            let x: Box(u256) = Box(bool) { value: true };
            @evm_stop();
        }
        "#,
        &[r#"
        error: mismatched types
         --> main.plk:5:24
          |
        5 |     let x: Box(u256) = Box(bool) { value: true };
          |            ---------   ^^^^^^^^^^^^^^^^^^^^^^^^^ expected `Box(u256)`, got `Box(bool)`
          |            |
          |            `Box(u256)` expected because of this
        "#],
    );
}

#[test]
fn test_const_alias_preserves_generic_struct_name() {
    assert_diagnostics(
        r#"
        const Box = fn (comptime T: type) type {
            struct T { value: T }
        };
        const Alias = Box(u256);

        init {
            let x: Alias = 42;
            @evm_stop();
        }
        "#,
        &[r#"
        error: mismatched types
         --> main.plk:6:20
          |
        6 |     let x: Alias = 42;
          |            -----   ^^ expected `Box(u256)`, got `u256`
          |            |
          |            `Box(u256)` expected because of this
        "#],
    );
}

#[test]
fn test_parameterized_name_for_deduped_struct_uses_first_specialization() {
    assert_diagnostics(
        r#"
        const Phantom = fn (comptime T: type) type {
            struct { value: u256 }
        };

        init {
            let a: Phantom(u256) = Phantom(u256) { value: 1 };
            let b: Phantom(bool) = 42;
            @evm_stop();
        }
        "#,
        &[r#"
        error: mismatched types
         --> main.plk:6:28
          |
        6 |     let b: Phantom(bool) = 42;
          |            -------------   ^^ expected `Phantom(u256)`, got `u256`
          |            |
          |            `Phantom(u256)` expected because of this
        "#],
    );
}

#[test]
fn test_identity_type_function_does_not_rename_struct() {
    assert_diagnostics(
        r#"
        const id = fn (comptime T: type) type { T };
        init {
            let T = struct { a: u256 };
            let x: id(T) = 42;
            @evm_stop();
        }
        "#,
        &[r#"
        error: mismatched types
         --> main.plk:4:20
          |
        4 |     let x: id(T) = 42;
          |            -----   ^^ expected `struct#0@main.plk:3:13`, got `u256`
          |            |
          |            `struct#0@main.plk:3:13` expected because of this
        "#],
    );
}

#[test]
fn test_identity_type_function_preserves_named_struct() {
    assert_diagnostics(
        r#"
        const id = fn (comptime T: type) type { T };
        const Pair = struct { a: u256 };

        init {
            let x: id(Pair) = 42;
            @evm_stop();
        }
        "#,
        &[r#"
        error: mismatched types
         --> main.plk:4:23
          |
        4 |     let x: id(Pair) = 42;
          |            --------   ^^ expected `Pair`, got `u256`
          |            |
          |            `Pair` expected because of this
        "#],
    );
}

#[test]
fn test_type_annotation_not_comptime() {
    assert_diagnostics(
        "
        init {
            let T = @evm_calldataload(0);
            let x: T = 5;
            @evm_stop();
        }
        ",
        &[r#"
        error: type must be known at compile time
         --> main.plk:3:12
          |
        3 |     let x: T = 5;
          |            ^ not known at compile time
        "#],
    );
}
