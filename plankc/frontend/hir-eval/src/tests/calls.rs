use super::*;
use crate::quota::DEFAULT_COMPTIME_BRANCH_QUOTA;

#[test]
fn test_captured_value_propagates_through_nested_functions() {
    assert_lowers_to(
        r#"
        const Make = fn(comptime value: u256) u256 {
            let middle = fn() u256 {
                let unrelated = 11;
                let inner = fn() u256 { value };
                inner()
            };
            middle()
        };
        const RESULT = Make(7);

        init {
            let input = @evm_calldataload(0);
            let observed = @evm_add(input, RESULT);
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
            %3 : u256 = 7
            %4 : u256 = @evm_add(%2, %3)
            %5 : never = @evm_stop()
        }
        "#,
    );
}

#[test]
fn test_eager_fn_folds_only_with_all_comptime_inputs() {
    assert_lowers_to(
        r#"
        const probe = eager fn(x: u256) bool { @in_comptime() };
        const zero = eager fn() bool { @in_comptime() };

        init {
            let mut folded = probe(7);
            let mut folded_zero = zero();
            let input = @evm_calldataload(0);
            let mut runtime = probe(input);
            @evm_stop();
        }
        "#,
        r#"
        ==== Functions ====
        @fn0(%0: u256) -> bool {
            %1 : bool = false
            ret %1
        }

        ; init
        @fn1() -> never {
            %0 : bool = true
            %1 : bool = true
            %2 : u256 = 0
            %3 : u256 = @evm_calldataload(%2)
            %4 : u256 = %3
            %5 : bool = call @fn0(%4)
            %6 : never = @evm_stop()
        }
        "#,
    );
}

#[test]
fn test_eager_type_helper_drives_comptime_if_without_explicit_comptime_block() {
    assert_lowers_to(
        r#"
        const is_u256 = eager fn(comptime T: type) bool { T == u256 };

        init {
            if is_u256(u256) {
                @evm_stop();
            } else {
                @evm_invalid();
            }
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
         --> main.plk:4:5
          |
        1 | const x = 5;
          | ------------ defined here
        ...
        4 |     x();
          |     ^ `u256` is not callable
        "#],
    );
}

#[test]
fn test_cross_file_not_callable() {
    assert_project_diagnostics(
        TestProject::root(
            "
            use m::other::x;
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
fn test_non_never_comptime_function_preserves_nested_never_divergence() {
    assert_diagnostics(
        r#"
        const stop = fn() never {
            @evm_stop();
        };
        const f = fn() u256 {
            stop();
        };
        init {
            comptime { f(); }
            let y: bool = 0;
            @evm_stop();
        }
        "#,
        &[r#"
        error: builtin not supported at compile time
         --> main.plk:2:5
          |
        2 |     @evm_stop();
          |     ^^^^^^^^^^^ `@evm_stop` cannot be evaluated at compile time
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
        TestProject::root("use m::other::f;\ninit { f(1, 2); @evm_stop(); }")
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
          = note: `@evm_add` accepts (u256, u256)
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
          = note: `@evm_add` accepts (u256, u256)
        "#],
    );
}

#[test]
fn test_nested_closure_capture_not_comptime() {
    assert_diagnostics(
        r#"
        init {
            let x = @evm_calldataload(0);
            let middle = fn() void {
                let inner = fn() u256 { x };
            };
            @evm_stop();
        }
        "#,
        &[r#"
        error: closure capture must be known at compile time
         --> main.plk:4:33
          |
        2 |     let x = @evm_calldataload(0);
          |             -------------------- defined here
        3 |     let middle = fn() void {
        4 |         let inner = fn() u256 { x };
          |                                 ^ capture of runtime value
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
            use m::other::f;
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
          --> main.plk:5:38
           |
         5 | const weird = fn (comptime N: u256) (if even(N) { not_a_type } else { bool }) {
           |                                      ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ expected type, got value of type `void`
           |
        note: called here
          --> main.plk:11:20
           |
        11 |     let mut nope = weird(2);
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
fn test_cached_non_never_poison_does_not_diverge() {
    assert_diagnostics(
        r#"
        const bad = fn() u256 { return @uninit(never); };

        const warm = bad();

        init {
            comptime {
                bad();
                let after: u256 = false;
            }
            @evm_stop();
        }
        "#,
        &[
            r#"
            error: cannot create uninitialized value
             --> main.plk:1:32
              |
            1 | const bad = fn() u256 { return @uninit(never); };
              |                                ^^^^^^^^^^^^^^ type `never` cannot be uninitialized
              |
              = help: @uninit only supports types that do not contain never or function
            "#,
            r#"
            error: mismatched types
             --> main.plk:8:27
              |
            8 |         let after: u256 = false;
              |                    ----   ^^^^^ expected `u256`, got `bool`
              |                    |
              |                    `u256` expected because of this
            "#,
        ],
    );
}

#[test]
fn test_poisoned_arg_to_never_builtin_does_not_report_missing_terminator() {
    assert_diagnostics(
        r#"
        init {
            let bob = (struct {}) {};
            @evm_return(@malloc_uninit(0), bob.missing);
        }
        "#,
        &[r#"
        error: unknown field
         --> main.plk:3:36
          |
        3 |     @evm_return(@malloc_uninit(0), bob.missing);
          |                                    ^^^^^^^^^^^ `struct@main.plk:2:16` has no field `missing`
        "#],
    );
}

#[test]
fn test_comptime_poisoned_arg_to_never_builtin_reports_unsupported_builtin() {
    assert_diagnostics(
        r#"
        const Bob = struct {};
        const bob = Bob {};
        const x = @evm_return(bob.missing, 0);

        init { @evm_stop(); }
        "#,
        &[
            r#"
            error: unknown field
             --> main.plk:3:23
              |
            3 | const x = @evm_return(bob.missing, 0);
              |                       ^^^^^^^^^^^ `Bob` has no field `missing`
            "#,
            r#"
            error: builtin not supported at compile time
             --> main.plk:3:11
              |
            3 | const x = @evm_return(bob.missing, 0);
              |           ^^^^^^^^^^^^^^^^^^^^^^^^^^^ `@evm_return` cannot be evaluated at compile time
            "#,
        ],
    );
}

#[test]
fn test_poisoned_arg_to_non_never_builtin_reports_missing_terminator() {
    assert_diagnostics(
        r#"
        init {
            let bob = (struct {}) {};
            @evm_sstore(0, bob.missing);
        }
        "#,
        &[
            r#"
            error: unknown field
             --> main.plk:3:20
              |
            3 |     @evm_sstore(0, bob.missing);
              |                    ^^^^^^^^^^^ `struct@main.plk:2:16` has no field `missing`
            "#,
            r#"
            error: entry point must end with explicit terminator
             --> main.plk:1:1
              |
            1 | / init {
            2 | |     let bob = (struct {}) {};
            3 | |     @evm_sstore(0, bob.missing);
            4 | | }
              | |_^ execution may reach end of entry point
              |
              = help: entry points must end with a terminating `never` expression (e.g. `@evm_stop()`, `@evm_revert(...)`, `@evm_invalid()`)
            "#,
        ],
    );
}

#[test]
fn test_if_arm_mismatch_into_never_call_prevents_cascade() {
    assert_diagnostics(
        std_project(
            r#"
        const sink = fn(x: u256) never { @evm_stop(); };
        const f = fn() never {
            let c = @evm_calldataload(0);
            let v = if c == 0 {
                1
            } else {
                false
            };
            sink(v);
        };
        init {
            f();
        }
        "#,
        ),
        &[r#"
        error: incompatible branch types
         --> main.plk:7:9
          |
        5 |         1
          |         - `u256` expected because of this
        6 |     } else {
        7 |         false
          |         ^^^^^ expected `u256`, got `bool`
        "#],
    );
}

#[test]
fn test_if_arm_mismatch_into_non_never_call_preserves_poison() {
    assert_diagnostics(
        std_project(
            r#"
        const sink = fn(x: u256) u256 { x };
        const f = fn() void {
            let c = @evm_calldataload(0);
            let v = if c == 0 {
                1
            } else {
                false
            };
            sink(v);
            let bad: u256 = false;
        };
        init {
            f();
            @evm_stop();
        }
        "#,
        ),
        &[
            r#"
            error: incompatible branch types
             --> main.plk:7:9
              |
            5 |         1
              |         - `u256` expected because of this
            6 |     } else {
            7 |         false
              |         ^^^^^ expected `u256`, got `bool`
            "#,
            r#"
            error: mismatched types
              --> main.plk:10:21
               |
            10 |     let bad: u256 = false;
               |              ----   ^^^^^ expected `u256`, got `bool`
               |              |
               |              `u256` expected because of this
            "#,
        ],
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

#[test]
fn test_non_recursive_comptime_calls_do_not_consume_caller_quota() {
    assert_eq!(1000, DEFAULT_COMPTIME_BRANCH_QUOTA);
    assert_diagnostics(
        std_project(
            r#"
        const f = fn(x: u256) u256 { x };

        init {
            let mut x: u256 = comptime {
                let mut i = 0;
                while i < 1000 {
                    f(1);
                    i = i + 1;
                }
                f(2);
                0
            };
            @evm_stop();
        }
        "#,
        ),
        &[],
    );
}

#[test]
fn test_cached_comptime_function_body_quota_replays_in_fresh_child() {
    assert_eq!(1000, DEFAULT_COMPTIME_BRANCH_QUOTA);
    assert_diagnostics(
        std_project(
            r#"
        const consume_3_branches = fn() u256 {
            let mut i = 0;
            while i < 2 {
                i = i + 1;
            }
            i
        };

        init {
            let mut warm: u256 = comptime { consume_3_branches() };
            let mut x: u256 = comptime {
                let mut i = 0;
                while i < 996 {
                    i = i + 1;
                }
                consume_3_branches();
                0
            };
            @evm_stop();
        }
        "#,
        ),
        &[],
    );
}

#[test]
fn test_runtime_function_body_uses_fresh_quota_without_spending_caller_quota() {
    assert_eq!(1000, DEFAULT_COMPTIME_BRANCH_QUOTA);
    assert_diagnostics(
        std_project(
            r#"
        const spend_1000 = fn() void {
            comptime {
                let mut i = 0;
                while i < 1000 {
                    i = i + 1;
                }
            }
        };

        init {
            let mut before: u256 = comptime {
                let mut i = 0;
                while i < 500 {
                    i = i + 1;
                }
                i
            };
            spend_1000();
            let mut after: u256 = comptime {
                let mut i = 0;
                while i < 500 {
                    i = i + 1;
                }
                i
            };
            @evm_stop();
        }
        "#,
        ),
        &[],
    );
}

#[test]
fn test_runtime_function_body_inherits_limit_without_raising_caller_limit() {
    assert_eq!(1000, DEFAULT_COMPTIME_BRANCH_QUOTA);
    assert_diagnostics(
        std_project(
            r#"
        const spend_and_raise = fn() void {
            comptime {
                let mut i = 0;
                while i < 1200 {
                    i = i + 1;
                }
                @set_eval_branch_quota(2000);
            }
        };

        const identity = fn(x: u256) u256 { x };

        init {
            @set_eval_branch_quota(1500);
            spend_and_raise();
            let mut x: u256 = comptime {
                let mut i = 0;
                while i < 1500 {
                    i = i + 1;
                }
                while i < 1501 {
                    i = i + 1;
                }
                i
            };
            @evm_stop();
        }
        "#,
        ),
        &[r#"
        error: comptime branch quota exhausted
          --> main.plk:21:15
           |
        21 |         while i < 1501 {
           |               ^^^^^^^^^ evaluating this loop exceeded the comptime branch quota
           |
           = note: current eval branch quota is 1500
        note: comptime evaluation began here
          --> main.plk:13:1
           |
        13 | / init {
        14 | |     @set_eval_branch_quota(1500);
        15 | |     spend_and_raise();
        16 | |     let mut x: u256 = comptime {
        ...  |
        26 | |     @evm_stop();
        27 | | }
           | |_^
        "#],
    );
}

#[test]
fn test_runtime_lowering_quota_exhaustion_is_retryable() {
    assert_eq!(1000, DEFAULT_COMPTIME_BRANCH_QUOTA);
    let (mir, big_nums, session) = try_lower(std_project(
        r#"
        const f = fn() void {
            comptime {
                let mut i = 0;
                while i < 1001 {
                    i = i + 1;
                }
            }
            @evm_sstore(0, 0);
        };

        init {
            f();
        }

        run {
            @set_eval_branch_quota(3000);
            f();
            @evm_stop();
        }
        "#,
    ));
    plank_test_utils::assert_diagnostics(
        session.diagnostics(),
        &session,
        &[r#"
        error: comptime branch quota exhausted
          --> main.plk:4:15
           |
         4 |         while i < 1001 {
           |               ^^^^^^^^^ evaluating this loop exceeded the comptime branch quota
           |
           = note: current eval branch quota is 1000
        note: comptime evaluation began here
          --> main.plk:12:5
           |
        12 |     f();
           |     ^^^
        "#],
    );
    let actual = format!("{}", DisplayMir::new(&mir, &big_nums, &session));
    assert_eq!(
        actual,
        r#"==== Functions ====
; init
@fn0() -> never {
}

@fn1() -> void {
    %0 : u256 = 0
    %1 : u256 = 0
    %2 : void = @evm_sstore(%0, %1)
    %3 : void = ()
    ret %3
}

; run
@fn2() -> never {
    %0 : void = call @fn1()
    %1 : never = @evm_stop()
}

"#
    );
}

#[test]
fn test_runtime_lowering_recursion_poison_is_not_marked_retryable() {
    assert_eq!(1000, DEFAULT_COMPTIME_BRANCH_QUOTA);
    let (_mir, _big_nums, session) = try_lower(std_project(
        r#"
        const f = fn() void {
            f();
            comptime {
                let mut i = 0;
                while i < 1001 {
                    i = i + 1;
                }
            }
        };

        init {
            f();
            @evm_stop();
        }
        "#,
    ));
    plank_test_utils::assert_diagnostics(
        session.diagnostics(),
        &session,
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
fn test_nested_runtime_function_preamble_uses_child_quota() {
    assert_eq!(1000, DEFAULT_COMPTIME_BRANCH_QUOTA);
    assert_diagnostics(
        std_project(
            r#"
        const f = fn(comptime N: u256) comptime {
            let mut i = 0;
            while i < 500 {
                i = i + 1;
            }
            void
        } {
            if N == 1 {
                comptime {
                    let mut i = 0;
                    while i < 501 {
                        i = i + 1;
                    }
                }
            }
        };

        init {
            f(0);
            comptime {
                let mut i = 0;
                while i < 501 { i = i + 1; }
            }
            f(1);

            @evm_stop();
        }
        "#,
        ),
        &[r#"
        error: comptime branch quota exhausted
          --> main.plk:11:19
           |
        11 |             while i < 501 {
           |                   ^^^^^^^^ evaluating this loop exceeded the comptime branch quota
           |
           = note: current eval branch quota is 1000
        note: comptime evaluation began here
          --> main.plk:24:5
           |
        24 |     f(1);
           |     ^^^^
        "#],
    );
}

#[test]
fn test_nested_runtime_function_preamble_quota_raise_does_not_reach_outer_caller() {
    assert_eq!(1000, DEFAULT_COMPTIME_BRANCH_QUOTA);
    assert_diagnostics(
        std_project(
            r#"
        const a = fn() void {
            b();
        };

        const b = fn() comptime {
            @set_eval_branch_quota(1200);
            void
        } {
            a();
        };

        const identity = fn(x: u256) u256 { x };

        init {
            a();
            let mut x: u256 = comptime {
                let mut i = 0;
                while i < 1100 {
                    i = i + 1;
                }
                identity(0)
            };
            @evm_stop();
        }
        "#,
        ),
        &[
            r#"
        error: runtime recursion not supported
         --> main.plk:9:5
          |
        9 |     a();
          |     ^^^ runtime call that recurses
          |
          = note: recursion is only allowed at compile time to ensure consistent performance and iteration bounds
        "#,
            r#"
        error: comptime branch quota exhausted
          --> main.plk:18:15
           |
        18 |         while i < 1100 {
           |               ^^^^^^^^^ evaluating this loop exceeded the comptime branch quota
           |
           = note: current eval branch quota is 1000
        note: comptime evaluation began here
          --> main.plk:14:1
           |
        14 | / init {
        15 | |     a();
        16 | |     let mut x: u256 = comptime {
        17 | |         let mut i = 0;
        ...  |
        23 | |     @evm_stop();
        24 | | }
           | |_^
        "#,
        ],
    );
}

#[test]
fn test_cached_comptime_function_quota_raise_stays_in_child() {
    assert_eq!(1000, DEFAULT_COMPTIME_BRANCH_QUOTA);
    assert_diagnostics(
        std_project(
            r#"
        const raise_quota = fn() u256 {
            @set_eval_branch_quota(1001);
            20
        };

        const identity = fn(x: u256) u256 { x };

        init {
            let mut warm: u256 = comptime { raise_quota() };
            let mut x: u256 = comptime {
                let mut i = 0;
                while i < 998 {
                    i = i + 1;
                }
                raise_quota();
                identity(1);
                identity(2);
                while i < 1001 {
                    i = i + 1;
                }
                i
            };
            @evm_stop();
        }
        "#,
        ),
        &[r#"
        error: comptime branch quota exhausted
          --> main.plk:18:15
           |
        18 |         while i < 1001 {
           |               ^^^^^^^^^ evaluating this loop exceeded the comptime branch quota
           |
           = note: current eval branch quota is 1000
        note: comptime evaluation began here
          --> main.plk:8:1
           |
         8 | / init {
         9 | |     let mut warm: u256 = comptime { raise_quota() };
        10 | |     let mut x: u256 = comptime {
        11 | |         let mut i = 0;
        ...  |
        23 | |     @evm_stop();
        24 | | }
           | |_^
        "#],
    );
}

#[test]
fn test_comptime_function_preamble_quota_exhaustion_reports_call_site() {
    assert_eq!(1000, DEFAULT_COMPTIME_BRANCH_QUOTA);
    assert_diagnostics(
        std_project(
            r#"
        const f = fn() comptime {
            let mut i = 0;
            while i < 1001 {
                i = i + 1;
            }
            u256
        } { 0 };

        init {
            let mut x: u256 = comptime { f() };
            @evm_stop();
        }
        "#,
        ),
        &[r#"
        error: comptime branch quota exhausted
          --> main.plk:3:11
           |
         3 |     while i < 1001 {
           |           ^^^^^^^^^ evaluating this loop exceeded the comptime branch quota
           |
           = note: current eval branch quota is 1000
        note: comptime evaluation began here
          --> main.plk:10:34
           |
        10 |     let mut x: u256 = comptime { f() };
           |                                  ^^^
        note: called here
          --> main.plk:10:34
           |
        10 |     let mut x: u256 = comptime { f() };
           |                                  ^^^
        "#],
    );
}

#[test]
fn test_runtime_context_comptime_call_body_gets_full_child_quota() {
    assert_eq!(1000, DEFAULT_COMPTIME_BRANCH_QUOTA);
    assert_diagnostics(
        std_project(
            r#"
        const f = fn() type {
            let mut i = 0;
            while i < 1000 {
                i = i + 1;
            }
            u256
        };

        init {
            let mut x: f() = 0;
            @evm_stop();
        }
        "#,
        ),
        &[],
    );
}

#[test]
fn test_runtime_forced_comptime_call_does_not_spend_root_quota() {
    assert_eq!(1000, DEFAULT_COMPTIME_BRANCH_QUOTA);
    assert_diagnostics(
        std_project(
            r#"
        const f = fn() type { u256 };

        init {
            let mut warm: u256 = comptime {
                let mut i = 0;
                while i < 1000 {
                    i = i + 1;
                }
                i
            };
            let mut x: f() = 0;
            @evm_stop();
        }
        "#,
        ),
        &[],
    );
}

#[test]
fn test_descendant_quota_raise_does_not_raise_suspended_ancestor_limit() {
    assert_eq!(1000, DEFAULT_COMPTIME_BRANCH_QUOTA);
    assert_diagnostics(
        std_project(
            r#"
        const recurse = fn(n: u256) u256 {
            if n == 1 { @set_eval_branch_quota(2000); }
            recurse(n + 1)
        };

        init {
            comptime { recurse(0); }
            @evm_stop();
        }
        "#,
        ),
        &[r#"
        error: comptime branch quota exhausted
         --> main.plk:3:5
          |
        3 |     recurse(n + 1)
          |     ^^^^^^^^^^^^^^ evaluating this call exceeded the comptime branch quota
          |
          = note: current eval branch quota is 1000
        note: comptime evaluation began here
         --> main.plk:7:16
          |
        7 |     comptime { recurse(0); }
          |                ^^^^^^^^^^
        "#],
    );
}

#[test]
fn test_all_same_definition_ancestors_retain_recursive_charges() {
    assert_eq!(1000, DEFAULT_COMPTIME_BRANCH_QUOTA);
    assert_diagnostics(
        std_project(
            r#"
        const recurse = fn(n: u256) u256 {
            if n < 2 { recurse(n + 1); }
            if n == 0 {
                let mut i = 0;
                while i < 999 { i = i + 1; }
            }
            n
        };

        init {
            comptime { recurse(0); }
            @evm_stop();
        }
        "#,
        ),
        &[r#"
        error: comptime branch quota exhausted
          --> main.plk:5:15
           |
         5 |         while i < 999 { i = i + 1; }
           |               ^^^^^^^^ evaluating this loop exceeded the comptime branch quota
           |
           = note: current eval branch quota is 1000
        note: comptime evaluation began here
          --> main.plk:11:16
           |
        11 |     comptime { recurse(0); }
           |                ^^^^^^^^^^
        "#],
    );
}

#[test]
fn test_immediate_same_definition_ancestor_retains_recursive_charge() {
    assert_eq!(1000, DEFAULT_COMPTIME_BRANCH_QUOTA);
    assert_diagnostics(
        std_project(
            r#"
        const recurse = fn(n: u256) u256 {
            if n < 2 { recurse(n + 1); }
            if n == 1 {
                let mut i = 0;
                while i < 1000 { i = i + 1; }
            }
            n
        };

        init {
            comptime { recurse(0); }
            @evm_stop();
        }
        "#,
        ),
        &[r#"
        error: comptime branch quota exhausted
         --> main.plk:5:15
          |
        5 |         while i < 1000 { i = i + 1; }
          |               ^^^^^^^^^ evaluating this loop exceeded the comptime branch quota
          |
          = note: current eval branch quota is 1000
        note: comptime evaluation began here
         --> main.plk:2:16
          |
        2 |     if n < 2 { recurse(n + 1); }
          |                ^^^^^^^^^^^^^^
        "#],
    );
}

#[test]
fn test_comptime_recursion_with_changing_arguments_exhausts_quota() {
    assert_eq!(1000, DEFAULT_COMPTIME_BRANCH_QUOTA);
    assert_diagnostics(
        std_project(
            r#"
        const recurse = fn(n: u256) u256 {
            recurse(n + 1)
        };

        init {
            comptime { recurse(0); }
            @evm_stop();
        }
        "#,
        ),
        &[r#"
        error: comptime branch quota exhausted
         --> main.plk:2:5
          |
        2 |     recurse(n + 1)
          |     ^^^^^^^^^^^^^^ evaluating this call exceeded the comptime branch quota
          |
          = note: current eval branch quota is 1000
        note: comptime evaluation began here
         --> main.plk:6:16
          |
        6 |     comptime { recurse(0); }
          |                ^^^^^^^^^^
        "#],
    );
}

#[test]
fn test_mutual_changing_specialization_recursion_exhausts_quota() {
    assert_eq!(1000, DEFAULT_COMPTIME_BRANCH_QUOTA);
    assert_diagnostics(
        std_project(
            r#"
        const a = fn(n: u256) u256 { b(n) };
        const b = fn(n: u256) u256 { a(n + 1) };

        init {
            comptime { a(0); }
            @evm_stop();
        }
        "#,
        ),
        &[r#"
        error: comptime branch quota exhausted
         --> main.plk:2:30
          |
        2 | const b = fn(n: u256) u256 { a(n + 1) };
          |                              ^^^^^^^^ evaluating this call exceeded the comptime branch quota
          |
          = note: current eval branch quota is 1000
        note: comptime evaluation began here
         --> main.plk:5:16
          |
        5 |     comptime { a(0); }
          |                ^^^^
        "#],
    );
}

#[test]
fn test_mixed_runtime_comptime_specialization_recursion_exhausts_quota() {
    assert_eq!(1000, DEFAULT_COMPTIME_BRANCH_QUOTA);
    assert_diagnostics(
        std_project(
            r#"
        const stupid = fn(comptime N: u256, x: u256) u256 {
            stupid(N + 1, x)
        };

        init {
            stupid(0, @evm_calldataload(0));
            @evm_invalid();
        }
        "#,
        ),
        &[r#"
        error: comptime branch quota exhausted
         --> main.plk:2:5
          |
        2 |     stupid(N + 1, x)
          |     ^^^^^^^^^^^^^^^^ evaluating this call exceeded the comptime branch quota
          |
          = note: current eval branch quota is 1000
        note: comptime evaluation began here
         --> main.plk:6:5
          |
        6 |     stupid(0, @evm_calldataload(0));
          |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
        "#],
    );
}
