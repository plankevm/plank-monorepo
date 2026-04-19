use super::*;

#[test]
fn test_preamble_error_per_call_site() {
    assert_diagnostics(
        r#"
        const not_a_type = 42;
        const f = fn() not_a_type { return 0; };
        init {
            f();
            f();
            f();
            @evm_stop();
        }
        "#,
        &[
            r#"
        error: value used as type
         --> main.plk:2:16
          |
        1 | const not_a_type = 42;
          | ---------------------- defined here
        2 | const f = fn() not_a_type { return 0; };
          |                ^^^^^^^^^^ expected type, got value of type `u256`
          |
        note: called here
         --> main.plk:4:5
          |
        4 |     f();
          |     ^^^
        "#,
            r#"
        error: value used as type
         --> main.plk:2:16
          |
        1 | const not_a_type = 42;
          | ---------------------- defined here
        2 | const f = fn() not_a_type { return 0; };
          |                ^^^^^^^^^^ expected type, got value of type `u256`
          |
        note: called here
         --> main.plk:5:5
          |
        5 |     f();
          |     ^^^
        "#,
            r#"
        error: value used as type
         --> main.plk:2:16
          |
        1 | const not_a_type = 42;
          | ---------------------- defined here
        2 | const f = fn() not_a_type { return 0; };
          |                ^^^^^^^^^^ expected type, got value of type `u256`
          |
        note: called here
         --> main.plk:6:5
          |
        6 |     f();
          |     ^^^
        "#,
        ],
    );
}

#[test]
fn test_never_fn_return_type_mismatch_diverges() {
    assert_diagnostics(
        r#"
        const bad_ret = fn() never {
            return 0;
        };
        init {
            comptime {
                bad_ret();
            }
            let x: u256 = false;
            @evm_stop();
        }
        "#,
        &[r#"
        error: mismatched types
         --> main.plk:2:12
          |
        1 | const bad_ret = fn() never {
          |                      ----- `never` expected because of this
        2 |     return 0;
          |            ^ expected `never`, got `u256`
        "#],
    );
}

#[test]
fn test_if_both_branches_never_function_diverges() {
    assert_diagnostics(
        r#"
        const bad_stop = fn() never {
            comptime { @evm_stop(); }
            @evm_stop();
        };
        init {
            let x = @evm_calldataload(0);
            if @evm_eq(x, 0) {
                bad_stop();
            } else {
                bad_stop();
            }
            let y: bool = 0;
            @evm_stop();
        }
        "#,
        &[r#"
        error: builtin not supported at compile time
         --> main.plk:2:16
          |
        2 |     comptime { @evm_stop(); }
          |                ^^^^^^^^^^^ `@evm_stop` cannot be evaluated at compile time
        "#],
    );
}

#[test]
fn test_runtime_never_fn_call_diverges_on_cached_hit() {
    assert_diagnostics(
        r#"
        const bad_stop = fn() never {
            comptime { @evm_stop(); }
            @evm_stop();
        };
        init {
            let x = @evm_calldataload(0);
            if @evm_eq(x, 0) {
                bad_stop();
            } else {
                bad_stop();
                let y: bool = 0;
            }
            @evm_stop();
        }
        "#,
        &[r#"
        error: builtin not supported at compile time
         --> main.plk:2:16
          |
        2 |     comptime { @evm_stop(); }
          |                ^^^^^^^^^^^ `@evm_stop` cannot be evaluated at compile time
        "#],
    );
}

#[test]
fn test_runtime_call_arg_type_mismatch() {
    assert_diagnostics(
        r#"
        init {
            let f = fn(x: u256) never { @evm_stop(); };
            f(false);
        }
        "#,
        &[r#"
        error: mismatched types
         --> main.plk:3:7
          |
        2 |     let f = fn(x: u256) never { @evm_stop(); };
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
        init { @evm_stop(); }
        "#,
        &[r#"
        error: expected function
         --> main.plk:2:11
          |
        1 | const x = 5;
          | ------------ defined here
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
            let f = @evm_calldataload(0);
            f();
            @evm_stop();
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
            @evm_stop();
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
fn test_same_file_not_callable() {
    assert_project_diagnostics(
        r#"
        const x = 5;

        init {
            x();
            @evm_stop();
        }
        "#,
        &[r#"
        error: expected function
         --> main.plk:3:5
          |
        1 | const x = 5;
          | ------------ defined here
        2 | init {
        3 |     x();
          |     ^ `u256` is not callable
        "#],
    );
}

#[test]
fn test_cross_file_not_callable() {
    assert_project_diagnostics(
        TestProject::root(
            "
            import m::other::x;
            init {
                x();
                @evm_stop();
            }
            ",
        )
        .add_file("other", "const x = 5;")
        .add_module("m", ""),
        &[r#"
        error: expected function
         --> main.plk:3:5
          |
        3 |     x();
          |     ^ `u256` is not callable
          |
         ::: other.plk:1:1
          |
        1 | const x = 5;
          | ------------ defined here
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
            @evm_stop();
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
fn test_const_poisoned_never_crashes() {
    assert_diagnostics(
        r#"
        const f = fn() never { @evm_stop(); };
        const x = f();
        init { @evm_stop(); }
        "#,
        &[r#"
        error: builtin not supported at compile time
         --> main.plk:1:24
          |
        1 | const f = fn() never { @evm_stop(); };
          |                        ^^^^^^^^^^^ `@evm_stop` cannot be evaluated at compile time
        "#],
    );
}

#[test]
fn test_comptime_call_arg_count_mismatch() {
    assert_diagnostics(
        r#"
        const f = fn(x: u256) u256 { return x; };
        const r = f(1, 2);
        init { @evm_stop(); }
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
        TestProject::root("import m::other::f;\ninit { f(1, 2); @evm_stop(); }")
            .add_file("other", "const f = fn(x: u256) u256 { return x; };")
            .add_module("m", ""),
        &[r#"
        error: wrong number of arguments
         --> main.plk:2:8
          |
        2 | init { f(1, 2); @evm_stop(); }
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
            @evm_add(true, false);
            @evm_stop();
        }
        "#,
        &[r#"
        error: no valid match for builtin signature
         --> main.plk:2:5
          |
        2 |     @evm_add(true, false);
          |     ^^^^^^^^^^^^^^^^^^^^^ `@evm_add` cannot be called with (bool, bool)
          |
          = note: `@evm_add` accepts (u256, u256), (memptr, u256), (u256, memptr)
        "#],
    );
}

#[test]
fn test_builtin_wrong_arg_count() {
    assert_diagnostics(
        r#"
        init {
            @evm_add(1);
            @evm_stop();
        }
        "#,
        &[r#"
        error: wrong number of arguments
         --> main.plk:2:5
          |
        2 |     @evm_add(1);
          |     ^^^^^^^^^^^ `@evm_add` called with 1 argument, but requires 2
          |
          = note: `@evm_add` accepts (u256, u256), (memptr, u256), (u256, memptr)
        "#],
    );
}

#[test]
fn test_closure_capture_not_comptime() {
    assert_diagnostics(
        r#"
        init {
            let x = @evm_calldataload(0);
            let f = fn() u256 { x };
            @evm_stop();
        }
        "#,
        &[r#"
        error: closure capture must be known at compile time
         --> main.plk:3:25
          |
        2 |     let x = @evm_calldataload(0);
          |             -------------------- defined here
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
            init { @evm_stop(); }
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

#[test]
fn test_import_group_symbols_accessible() {
    assert_project_lowers_to(
        TestProject::root(
            r#"
            import m::other::{f, g as my_g};
            init {
                let x = f(1);
                let y = my_g(2, 3);
                @evm_stop();
            }
        "#,
        )
        .add_file(
            "other",
            r#"
            const f = fn(x: u256) u256 { return x; };
            const g = fn(a: u256, b: u256) u256 { return a; };
            "#,
        )
        .add_module("m", ""),
        r#"
        ==== Functions ====
        @fn0(%0: u256) -> u256 {
            %1 : u256 = %0
            ret %1
        }

        @fn1(%0: u256, %1: u256) -> u256 {
            %2 : u256 = %0
            ret %2
        }

        ; init
        @fn2() -> never {
            %0 : u256 = 1
            %1 : u256 = call @fn0(%0)
            %2 : u256 = 2
            %3 : u256 = 3
            %4 : u256 = call @fn1(%2, %3)
            %5 : never = @evm_stop()
        }
        "#,
    );
}

#[test]
fn test_runtime_recursion_emits_recursion_diagnostic() {
    assert_diagnostics(
        r#"
        const f = fn() never {
            f()
        };
        init {
            f()
        }
        "#,
        &[r#"
        error: runtime recursion not supported
         --> main.plk:2:5
          |
        2 |     f()
          |     ^^^ runtime call that recurses
          |
          = note: recursion is only allowed at compile time to ensure consistent performance and iteration bounds
        "#],
    );
}

#[test]
fn test_runtime_recursion_with_terminator_still_emits_recursion_diagnostic() {
    assert_diagnostics(
        r#"
        const f = fn() never {
            f();
            @evm_stop();
        };
        init {
            f();
            @evm_stop();
        }
        "#,
        &[r#"
        error: runtime recursion not supported
         --> main.plk:2:5
          |
        2 |     f();
          |     ^^^ runtime call that recurses
          |
          = note: recursion is only allowed at compile time to ensure consistent performance and iteration bounds
        "#],
    );
}

#[test]
fn test_nested_preamble_errors_point_at_correct_call_sites() {
    assert_diagnostics(
        r#"
        const bad = 42;
        const inner = fn() bad { return 0; };
        const outer = fn() bad {
            inner();
            return 0;
        };
        init {
            outer();
            @evm_stop();
        }
        "#,
        &[
            r#"
        error: value used as type
         --> main.plk:3:20
          |
        1 | const bad = 42;
          | --------------- defined here
        2 | const inner = fn() bad { return 0; };
        3 | const outer = fn() bad {
          |                    ^^^ expected type, got value of type `u256`
          |
        note: called here
         --> main.plk:8:5
          |
        8 |     outer();
          |     ^^^^^^^
        "#,
            r#"
        error: value used as type
         --> main.plk:2:20
          |
        1 | const bad = 42;
          | --------------- defined here
        2 | const inner = fn() bad { return 0; };
          |                    ^^^ expected type, got value of type `u256`
          |
        note: called here
         --> main.plk:4:5
          |
        4 |     inner();
          |     ^^^^^^^
        "#,
        ],
    );
}

#[test]
fn test_inconsistent_premable() {
    assert_diagnostics(
        r#"
        const even = fn (x: u256) bool { @evm_eq(@evm_mod(x, 2), 0) };

        const not_a_type = {};

        const weird = fn (comptime N: u256) (if even(N) { not_a_type } else { bool }) {
            false
        };

        init {
            let mut fine = weird(3);
            let mut nope = weird(2);

            @evm_stop();
        }
        "#,
        &[r#"
        error: value used as type
         --> main.plk:3:38
          |
        3 | const weird = fn (comptime N: u256) (if even(N) { not_a_type } else { bool }) {
          |                                      ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ expected type, got value of type `void`
          |
        note: called here
         --> main.plk:8:20
          |
        8 |     let mut nope = weird(2);
          |                    ^^^^^^^^
        "#],
    );
}

#[test]
fn test_duplicate_body_error_runtime() {
    assert_diagnostics(
        r#"
        const simple = fn () void {
            let x: bool = 0;
        };


        init {
            simple();
            simple();

            @evm_stop();
        }
        "#,
        &[r#"
        error: mismatched types
         --> main.plk:2:19
          |
        2 |     let x: bool = 0;
          |            ----   ^ expected `bool`, got `u256`
          |            |
          |            `bool` expected because of this
        "#],
    );
}

#[test]
fn test_duplicate_body_error_comptime() {
    assert_diagnostics(
        r#"
        const simple = fn () void {
            let x: bool = 0;
        };


        init {
            comptime {
                simple();
                simple();

            }

            @evm_stop();
        }
        "#,
        &[r#"
        error: mismatched types
         --> main.plk:2:19
          |
        2 |     let x: bool = 0;
          |            ----   ^ expected `bool`, got `u256`
          |            |
          |            `bool` expected because of this
        "#],
    );
}

#[test]
fn test_comptime_calls_cache_correctly() {
    assert_lowers_to(
        r#"
        const fib_inner = fn (n: u256, a: u256, b: u256) u256 {
            if @evm_iszero(n) {
                return a;
            }
            fib_inner(@evm_sub(n, 1), b, @evm_add(a, b))
        };
        const fib = fn (n: u256) u256 {
            fib_inner(n, 0, 1)
        };

        init {
            let mut f0 = comptime { fib(0) };
            let mut f1 = comptime { fib(1) };
            let mut f10 = comptime { fib(10) };
            let mut f10 = comptime { fib(11) };
            let mut f10 = comptime { fib(11) };
            let mut f10 = comptime { fib(11) };
            @evm_stop();
        }
        "#,
        r#"
        ==== Functions ====
        ; init
        @fn0() -> never {
            %0 : u256 = 0
            %1 : u256 = 1
            %2 : u256 = 55
            %3 : u256 = 89
            %4 : u256 = 89
            %5 : u256 = 89
            %6 : never = @evm_stop()
        }
        "#,
    );
}

#[test]
fn test_comptime_diverge_prevents_cascade() {
    assert_diagnostics(
        r#"
        const stop = fn () never { @evm_stop() };

        const a = stop();

        init {
            let _ = a;
            comptime {
                stop();
            }
            let x: u256 = false;

            @evm_stop();
        }
        "#,
        &[r#"
        error: builtin not supported at compile time
         --> main.plk:1:28
          |
        1 | const stop = fn () never { @evm_stop() };
          |                            ^^^^^^^^^^^ `@evm_stop` cannot be evaluated at compile time
        "#],
    );
}

#[test]
fn test_runtime_comptime_only_arg() {
    assert_lowers_to(
        r#"
        const f = fn(x: type, y: u256) u256 { y };
        init {
            f(type, 3);
            f(type, 4);
            f(u256, 5);
            @evm_stop();
        }
        "#,
        r#"
        ==== Functions ====
        @fn0(%0: u256) -> u256 {
            %1 : u256 = %0
            ret %1
        }

        @fn1(%0: u256) -> u256 {
            %1 : u256 = %0
            ret %1
        }

        ; init
        @fn2() -> never {
            %0 : u256 = 3
            %1 : u256 = call @fn0(%0)
            %2 : u256 = 4
            %3 : u256 = call @fn0(%2)
            %4 : u256 = 5
            %5 : u256 = call @fn1(%4)
            %6 : never = @evm_stop()
        }
        "#,
    );
}

#[test]
fn test_comptime_ret_forces_arg_comptime() {
    assert_lowers_to(
        r#"
        const f = fn(comptime T: type, x: u256) type {
            if @evm_eq(x, 0) { T } else { bool }
        };
        init {
            let mut a: f(u256, comptime { 0 }) = 34;
            @evm_stop();
        }
        "#,
        r#"
        ==== Functions ====
        ; init
        @fn0() -> never {
            %0 : u256 = 34
            %1 : never = @evm_stop()
        }
        "#,
    );
}
