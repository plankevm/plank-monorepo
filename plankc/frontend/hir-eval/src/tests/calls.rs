use super::*;

#[test]
fn test_runtime_call_arg_type_mismatch() {
    assert_diagnostics(
        r#"
        init {
            let f = fn(x: u256) never { evm_stop(); };
            f(false);
        }
        "#,
        &[r#"
        error: mismatched types
         --> main.plk:3:7
          |
        2 |     let f = fn(x: u256) never { evm_stop(); };
          |                   ---- `u256` expected because of this
        3 |     f(false);
          |       ^^^^^ expected `u256`, got `bool`
        "#],
    );
}

#[test]
fn test_comptime_call_on_non_function() {
    assert_diagnostics(
        r#"
        const x = 5;
        const y = x();
        init { evm_stop(); }
        "#,
        &[r#"
        error: expected function
         --> main.plk:2:11
          |
        2 | const y = x();
          |           ^ `u256` is not callable
        "#],
    );
}

#[test]
fn test_call_target_not_comptime() {
    assert_diagnostics(
        r#"
        init {
            let f = calldataload(0);
            f();
            evm_stop();
        }
        "#,
        &[r#"
        error: call target must be known at compile time
         --> main.plk:3:5
          |
        3 |     f();
          |     ^ not known at compile time
          |
          = note: function calls are statically dispatched
        "#],
    );
}

#[test]
fn test_runtime_call_on_non_function() {
    assert_diagnostics(
        r#"
        init {
            let x = 5;
            x();
            evm_stop();
        }
        "#,
        &[r#"
        error: expected function
         --> main.plk:3:5
          |
        3 |     x();
          |     ^ `u256` is not callable
        "#],
    );
}

#[test]
fn test_runtime_call_arg_count_mismatch() {
    assert_diagnostics(
        r#"
        const foo = fn(x: u256) u256 { return x; };
        init {
            foo(1, 2);
            evm_stop();
        }
        "#,
        &[r#"
        error: wrong number of arguments
         --> main.plk:3:5
          |
        1 | const foo = fn(x: u256) u256 { return x; };
          |               --------- defined with 1 parameter
        2 | init {
        3 |     foo(1, 2);
          |     ^^^^^^^^^ expected 1 argument, got 2
        "#],
    );
}

#[test]
fn test_comptime_call_arg_count_mismatch() {
    assert_diagnostics(
        r#"
        const f = fn(x: u256) u256 { return x; };
        const r = f(1, 2);
        init { evm_stop(); }
        "#,
        &[r#"
        error: wrong number of arguments
         --> main.plk:2:11
          |
        1 | const f = fn(x: u256) u256 { return x; };
          |             --------- defined with 1 parameter
        2 | const r = f(1, 2);
          |           ^^^^^^^ expected 1 argument, got 2
        "#],
    );
}

#[test]
fn test_cross_file_call_arg_count_mismatch() {
    assert_project_diagnostics(
        TestProject::root("import m::other::f;\ninit { f(1, 2); evm_stop(); }")
            .add_file("other", "const f = fn(x: u256) u256 { return x; };")
            .add_module("m", ""),
        &[r#"
        error: wrong number of arguments
         --> main.plk:2:8
          |
        2 | init { f(1, 2); evm_stop(); }
          |        ^^^^^^^ expected 1 argument, got 2
          |
         ::: other.plk:1:13
          |
        1 | const f = fn(x: u256) u256 { return x; };
          |             --------- defined with 1 parameter
        "#],
    );
}

#[test]
fn test_no_matching_builtin_signature() {
    assert_diagnostics(
        r#"
        init {
            add(true, false);
            evm_stop();
        }
        "#,
        &[r#"
        error: no valid match for builtin signature
         --> main.plk:2:5
          |
        2 |     add(true, false);
          |     ^^^^^^^^^^^^^^^^ `add` cannot be called with (bool, bool)
          |
          = note: `add` accepts (u256, u256), (memptr, u256), (u256, memptr)
        "#],
    );
}

#[test]
fn test_builtin_wrong_arg_count() {
    assert_diagnostics(
        r#"
        init {
            add(1);
            evm_stop();
        }
        "#,
        &[r#"
        error: wrong number of arguments
         --> main.plk:2:5
          |
        2 |     add(1);
          |     ^^^^^^ `add` called with 1 argument, but requires 2
          |
          = note: `add` accepts (u256, u256), (memptr, u256), (u256, memptr)
        "#],
    );
}

#[test]
fn test_closure_capture_not_comptime() {
    assert_diagnostics(
        r#"
        init {
            let x = calldataload(0);
            let f = fn() u256 { x };
            evm_stop();
        }
        "#,
        &[r#"
        error: closure capture must be known at compile time
         --> main.plk:3:25
          |
        2 |     let x = calldataload(0);
          |             --------------- defined here
        3 |     let f = fn() u256 { x };
          |                         ^ capture of runtime value
          |
          = note: closures can only capture values known at compile time
        "#],
    );
}

#[test]
fn test_cross_file_type_mismatch() {
    assert_project_diagnostics(
        TestProject::root(
            "
            import m::other::f;
            const y = f(true);
            init { evm_stop(); }
            ",
        )
        .add_file("other", "const f = fn(x: u256) u256 { return x; };")
        .add_module("m", ""),
        &[r#"
        error: mismatched types
         --> main.plk:2:13
          |
        2 | const y = f(true);
          |             ^^^^ expected `u256`, got `bool`
          |
         ::: other.plk:1:17
          |
        1 | const f = fn(x: u256) u256 { return x; };
          |                 ---- `u256` expected because of this
        "#],
    );
}
