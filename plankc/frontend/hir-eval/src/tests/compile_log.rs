use super::*;

#[test]
fn test_compile_log() {
    assert_compile_logs(
        std_project(
            r#"
            import std::option::Some;

            init {
                comptime {
                    let computed = @evm_add(20, 22);
                    @compile_log(true);
                    @compile_log(42);
                    @compile_log("foo");
                    @compile_log(computed);
                    @compile_log(u256);
                    @compile_log(struct { id: u256 });
                }
                @evm_stop();
            }
            "#,
        ),
        &["true", "42", r#""foo""#, "42", "u256", "struct@main.plk:11:22"],
    );
}

#[test]
fn test_compile_log_usage_emits_error_diagnostic() {
    assert_diagnostics(
        r#"
        init {
            comptime {
                @compile_log(1);
                @compile_log(2);
            }
            @evm_stop();
        }
        "#,
        &[r#"
        error: found compile log
         --> main.plk:3:9
          |
        3 |         @compile_log(1);
          |         ^^^^^^^^^^^^^^^
        "#],
    );
}
