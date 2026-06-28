use super::*;

#[test]
fn test_compile_log() {
    assert_diagnostics_and_compile_logs(
        std_project(
            r#"
    import std::option::Some;
    init {
        comptime {
            @compile_log(true);
            @compile_log(42);
            @compile_log("foo");
            @compile_log(u256);
            @compile_log(Some(42));
            @compile_log(struct { id: u256 });
        }
        @evm_stop();
    }
    "#,
        ),
        &[r#"
        error: found compile log statement
         --> main.plk:4:9
          |
        4 |         @compile_log(true);
          |         ^^^^^^^^^^^^^^^^^^
        "#],
        &[
            "true",
            "42",
            r#""foo""#,
            "u256",
            "Option(u256) { inner: 42, is_some: true }",
            "struct@main.plk:9:22",
        ],
    );
}

#[test]
fn test_compile_log_error_suppressed_when_other_errors_exist() {
    assert_diagnostics_and_compile_logs(
        r#"
        init {
            comptime {
                @compile_log(1);
            }
            let x: bool = 5;
            @evm_stop();
        }
        "#,
        &[r#"
        error: mismatched types
         --> main.plk:5:19
          |
        5 |     let x: bool = 5;
          |            ----   ^ expected `bool`, got `u256`
          |            |
          |            `bool` expected because of this
        "#],
        &["1"],
    );
}

#[test]
fn test_compile_log_rejects_runtime_value() {
    assert_diagnostics_and_compile_logs(
        r#"
        init {
            let x: u256 = @evm_caller();
            comptime {
                @compile_log(x);
            }
            @evm_stop();
        }
        "#,
        &[r#"
        error: attempting to evaluate runtime expression in comptime context
         --> main.plk:4:22
          |
        4 |         @compile_log(x);
          |                      ^ runtime expression
        "#],
        &[],
    );
}
