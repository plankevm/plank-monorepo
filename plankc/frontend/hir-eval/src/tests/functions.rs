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
