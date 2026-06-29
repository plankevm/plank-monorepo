use super::*;

#[test]
fn test_fn_name() {
    assert_lowers_to(
        r#"
        const transfer = fn() void {};
        const named_ok = @fn_name(transfer) == "transfer";
        const anonymous_ok = @fn_name(fn() void {}) == "";

        init {
            let mut a: bool = named_ok;
            let mut b: bool = anonymous_ok;
            @evm_stop();
        }
        "#,
        r#"
        ==== Functions ====
        ; init
        @fn0() -> never {
            %0 : bool = true
            %1 : bool = true
            %2 : never = @evm_stop()
        }
        "#,
    );
}

#[test]
fn test_fn_name_alias_returns_original_name() {
    assert_lowers_to(
        r#"
        const transfer = fn() void {};
        const alias = transfer;
        const alias_ok = @fn_name(alias) == "transfer";

        init {
            let mut a: bool = alias_ok;
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
fn test_fn_name_expects_function() {
    assert_diagnostics(
        r#"
        const bad = @fn_name(42);
        init { @evm_stop(); }
        "#,
        &[r#"
        error: expected function argument
         --> main.plk:1:22
          |
        1 | const bad = @fn_name(42);
          |                      ^^ `@fn_name` expects a function argument, got a value of type `u256`
        "#],
    );
}

#[test]
fn test_get_comptime_param_count() {
    assert_lowers_to(
        r#"
        const runtime_only = fn(x: u256, y: bool) void {};
        const explicit_comptime = fn(comptime x: u256, y: bool) void {};
        const runtime_any = fn(x: $T) void {};
        const comptime_any = fn(comptime x: $T) void {};
        const mixed = fn(comptime T: type, x: T, y: $U, comptime z: $V) void {};

        const runtime_only_ok = @get_comptime_param_count(runtime_only) == 0;
        const explicit_comptime_ok = @get_comptime_param_count(explicit_comptime) == 1;
        const runtime_any_ok = @get_comptime_param_count(runtime_any) == 1;
        const comptime_any_ok = @get_comptime_param_count(comptime_any) == 1;
        const mixed_ok = @get_comptime_param_count(mixed) == 3;

        init {
            let mut a: bool = runtime_only_ok;
            let mut b: bool = explicit_comptime_ok;
            let mut c: bool = runtime_any_ok;
            let mut d: bool = comptime_any_ok;
            let mut e: bool = mixed_ok;
            @evm_stop();
        }
        "#,
        r#"
        ==== Functions ====
        ; init
        @fn0() -> never {
            %0 : bool = true
            %1 : bool = true
            %2 : bool = true
            %3 : bool = true
            %4 : bool = true
            %5 : never = @evm_stop()
        }
        "#,
    );
}

#[test]
fn test_get_comptime_param_count_expects_function() {
    assert_diagnostics(
        r#"
        const bad = @get_comptime_param_count(42);
        init { @evm_stop(); }
        "#,
        &[r#"
        error: expected function argument
         --> main.plk:1:39
          |
        1 | const bad = @get_comptime_param_count(42);
          |                                       ^^ `@get_comptime_param_count` expects a function argument, got a value of type `u256`
        "#],
    );
}

#[test]
fn test_function_signature_introspection() {
    assert_lowers_to(
        r#"
        const simple = fn(x: u256, y: bool) u256 { x };
        const generic = fn(x: $T) T { x };
        const mixed = fn(comptime T: type, x: T, comptime n: u256, y: $U) U { y };

        const simple_sig_ok = @get_runtime_signature(simple, ()) == tuple { u256, bool };
        const simple_return_ok = @get_return_type(simple, ()) == u256;
        const generic_sig_ok = @get_runtime_signature(generic, (bool,)) == tuple { bool };
        const generic_return_ok = @get_return_type(generic, (bool,)) == bool;
        const mixed_sig_ok = @get_runtime_signature(mixed, (u256, 7, bool)) == tuple { u256, bool };
        const mixed_return_ok = @get_return_type(mixed, (u256, 7, bool)) == bool;

        init {
            let mut a: bool = simple_sig_ok;
            let mut b: bool = simple_return_ok;
            let mut c: bool = generic_sig_ok;
            let mut d: bool = generic_return_ok;
            let mut e: bool = mixed_sig_ok;
            let mut f: bool = mixed_return_ok;
            @evm_stop();
        }
        "#,
        r#"
        ==== Functions ====
        ; init
        @fn0() -> never {
            %0 : bool = true
            %1 : bool = true
            %2 : bool = true
            %3 : bool = true
            %4 : bool = true
            %5 : bool = true
            %6 : never = @evm_stop()
        }
        "#,
    );
}

#[test]
fn test_function_introspection_comptime_args_mismatch() {
    assert_diagnostics(
        r#"
        const f = fn(comptime T: type, x: T, y: $U) U { y };
        const bad = @get_return_type(f, (u256,));
        init { @evm_stop(); }
        "#,
        &[r#"
        error: `@get_return_type` arguments mismatch
         --> main.plk:2:33
          |
        2 | const bad = @get_return_type(f, (u256,));
          |                                 ^^^^^^^ function `f` expects 2 comptime argument values, but 1 comptime argument value was supplied
        "#],
    );
}

#[test]
fn test_function_introspection_comptime_args_must_be_tuple() {
    assert_diagnostics(
        r#"
        const f = fn(x: u256) u256 { x };
        const bad = @get_runtime_signature(f, 42);
        init { @evm_stop(); }
        "#,
        &[r#"
        error: expected tuple argument
         --> main.plk:2:39
          |
        2 | const bad = @get_runtime_signature(f, 42);
          |                                       ^^ `@get_runtime_signature` expects comptime_args to be a tuple, got a value of type `u256`
        "#],
    );
}

#[test]
fn test_function_introspection_any_param_requires_type_arg() {
    assert_diagnostics(
        r#"
        const f = fn(x: $T, y: $U) U { y };
        const bad = @get_runtime_signature(f, (u256, 42));
        init { @evm_stop(); }
        "#,
        &[r#"
        error: mismatched types
         --> main.plk:2:39
          |
        2 | const bad = @get_runtime_signature(f, (u256, 42));
          |                                       ^^^^^^^^^^ expected `type`, got `u256`
        "#],
    );
}

#[test]
fn test_function_introspection_comptime_param_type_mismatch() {
    assert_diagnostics(
        r#"
        const f = fn(comptime n: u256) void {};
        const bad = @get_runtime_signature(f, (false,));
        init { @evm_stop(); }
        "#,
        &[r#"
        error: mismatched types
         --> main.plk:2:39
          |
        1 | const f = fn(comptime n: u256) void {};
          |                          ---- `u256` expected because of this
        2 | const bad = @get_runtime_signature(f, (false,));
          |                                       ^^^^^^^^ expected `u256`, got `bool`
        "#],
    );
}

#[test]
fn test_call_builtin_comptime() {
    assert_lowers_to(
        r#"
        const id = fn(x: $T) T { x };
        const mixed = fn(comptime T: type, x: T, comptime n: u256, y: $U) U { y };

        const id_ok = @call(id, (u256,), (34,)) == 34;
        const mixed_ok = @call(mixed, (u256, 7, bool), (34, false)) == false;

        init {
            let mut a: bool = id_ok;
            let mut b: bool = mixed_ok;
            @evm_stop();
        }
        "#,
        r#"
        ==== Functions ====
        ; init
        @fn0() -> never {
            %0 : bool = true
            %1 : bool = true
            %2 : never = @evm_stop()
        }
        "#,
    );
}

#[test]
fn test_call_builtin_runtime() {
    assert_lowers_to(
        r#"
        const f = fn(x: u256, y: bool) u256 { x };

        init {
            let x = @evm_calldataload(0);
            let y = @call(f, (), (x, false));
            let mut z: u256 = y;
            @evm_stop();
        }
        "#,
        r#"
        ==== Functions ====
        @fn0(%0: u256, %1: bool) -> u256 {
            %2 : u256 = %0
            ret %2
        }

        ; init
        @fn1() -> never {
            %0 : u256 = 0
            %1 : u256 = @evm_calldataload(%0)
            %2 : u256 = %1
            %3 : bool = false
            %4 : tuple {u256, bool} = tuple {u256, bool} { %2, %3 }
            %5 : u256 = %4.0
            %6 : bool = %4.1
            %7 : u256 = call @fn0(%5, %6)
            %8 : u256 = %7
            %9 : never = @evm_stop()
        }
        "#,
    );
}

#[test]
fn test_call_builtin_runtime_generic() {
    assert_lowers_to(
        r#"
        const id = fn(x: $T) T { x };

        init {
            let x = @evm_calldataload(0);
            let y = @call(id, (u256,), (x,));
            let mut z: u256 = y;
            @evm_stop();
        }
        "#,
        r#"
        ==== Functions ====
        @fn0(%0: u256) -> u256 {
            %1 : u256 = %0
            ret %1
        }

        ; init
        @fn1() -> never {
            %0 : u256 = 0
            %1 : u256 = @evm_calldataload(%0)
            %2 : u256 = %1
            %3 : tuple {u256} = tuple {u256} { %2 }
            %4 : u256 = %3.0
            %5 : u256 = call @fn0(%4)
            %6 : u256 = %5
            %7 : never = @evm_stop()
        }
        "#,
    );
}

#[test]
fn test_call_builtin_runtime_mixed_comptime_args() {
    assert_lowers_to(
        r#"
        const mixed = fn(comptime T: type, x: T, comptime n: u256, y: $U) U { y };

        init {
            let x = @evm_calldataload(0);
            let y = @call(mixed, (u256, 7, bool), (x, false));
            let mut z: bool = y;
            @evm_stop();
        }
        "#,
        r#"
        ==== Functions ====
        @fn0(%0: u256, %1: bool) -> bool {
            %2 : bool = %1
            ret %2
        }

        ; init
        @fn1() -> never {
            %0 : u256 = 0
            %1 : u256 = @evm_calldataload(%0)
            %2 : u256 = %1
            %3 : bool = false
            %4 : tuple {u256, bool} = tuple {u256, bool} { %2, %3 }
            %5 : u256 = %4.0
            %6 : bool = %4.1
            %7 : bool = call @fn0(%5, %6)
            %8 : bool = %7
            %9 : never = @evm_stop()
        }
        "#,
    );
}

#[test]
fn test_call_builtin_runtime_args_type_mismatch() {
    assert_diagnostics(
        r#"
        const f = fn(x: u256) u256 { x };
        const bad = @call(f, (), (false,));
        init { @evm_stop(); }
        "#,
        &[r#"
        error: mismatched types
         --> main.plk:2:26
          |
        1 | const f = fn(x: u256) u256 { x };
          |                 ---- `u256` expected because of this
        2 | const bad = @call(f, (), (false,));
          |                          ^^^^^^^^ expected `u256`, got `bool`
        "#],
    );
}

#[test]
fn test_call_builtin_runtime_any_type_arg_mismatch() {
    assert_diagnostics(
        r#"
        const id = fn(x: $T) T { x };
        const bad = @call(id, (u256,), (false,));
        init { @evm_stop(); }
        "#,
        &[r#"
        error: mismatched types
         --> main.plk:2:32
          |
        2 | const bad = @call(id, (u256,), (false,));
          |                       -------  ^^^^^^^^ expected `u256`, got `bool`
          |                       |
          |                       `u256` expected because of this
        "#],
    );
}

#[test]
fn test_call_builtin_runtime_args_must_be_tuple() {
    assert_diagnostics(
        r#"
        const f = fn(x: u256) u256 { x };
        const bad = @call(f, (), false);
        init { @evm_stop(); }
        "#,
        &[r#"
        error: expected tuple argument
         --> main.plk:2:26
          |
        2 | const bad = @call(f, (), false);
          |                          ^^^^^ `@call` expects runtime_args to be a tuple, got a value of type `bool`
        "#],
    );
}

#[test]
fn test_call_builtin_comptime_args_must_be_tuple() {
    assert_diagnostics(
        r#"
        const f = fn(x: u256) u256 { x };
        const bad = @call(f, 42, (0,));
        init { @evm_stop(); }
        "#,
        &[r#"
        error: expected tuple argument
         --> main.plk:2:22
          |
        2 | const bad = @call(f, 42, (0,));
          |                      ^^ `@call` expects comptime_args to be a tuple, got a value of type `u256`
        "#],
    );
}
