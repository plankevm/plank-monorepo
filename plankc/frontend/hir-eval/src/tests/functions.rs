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
