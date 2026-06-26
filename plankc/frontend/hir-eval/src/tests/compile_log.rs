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

/// `@compile_log` is variadic: every argument is formatted and joined with a single space
/// into one logged line.
#[test]
fn test_compile_log_multiple_values_joined_with_space() {
    assert_compile_logs(
        r#"
        init {
            comptime {
                @compile_log(1, true, "three");
            }
            @evm_stop();
        }
        "#,
        &[r#"1 true "three""#],
    );
}

/// A `@compile_log` with no arguments still records an (empty) log line.
#[test]
fn test_compile_log_no_args() {
    assert_compile_logs(
        r#"
        init {
            comptime {
                @compile_log();
            }
            @evm_stop();
        }
        "#,
        &[""],
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
        error: found compile log statement
         --> main.plk:3:9
          |
        3 |         @compile_log(1);
          |         ^^^^^^^^^^^^^^^
        "#],
    );
}

/// All logs accumulate in order, but the umbrella error is emitted once, anchored at the
/// first call site.
#[test]
fn test_compile_log_accumulates_across_sites() {
    assert_compile_logs(
        r#"
        init {
            comptime {
                @compile_log(1);
                @compile_log(2);
            }
            @evm_stop();
        }
        "#,
        &["1", "2"],
    );
}

/// When another error already exists, the logs are still recorded (and printed by the
/// driver) but the umbrella "found compile log statement" error is suppressed.
#[test]
fn test_compile_log_error_suppressed_when_other_errors_exist() {
    let (_, _, session) = try_lower(
        r#"
        init {
            comptime {
                @compile_log(1);
            }
            let x: bool = 5;
            @evm_stop();
        }
        "#,
    );
    assert!(session.has_errors(), "the unrelated type error should be present");
    let has_umbrella = session
        .diagnostics()
        .iter()
        .any(|d| d.render_plain(&session).contains("found compile log statement"));
    assert!(!has_umbrella, "umbrella error must be suppressed when other errors exist");
    assert!(!session.compile_logs().is_empty(), "the log should still be recorded");
}

/// Logging a value that is only known at runtime is rejected: `@compile_log` requires a
/// comptime-known operand, and nothing is recorded when the operand cannot be evaluated.
#[test]
fn test_compile_log_rejects_runtime_value() {
    let (_, _, session) = try_lower(
        r#"
        init {
            let x: u256 = @evm_caller();
            comptime {
                @compile_log(x);
            }
            @evm_stop();
        }
        "#,
    );
    assert!(session.has_errors(), "expected an error for runtime value in @compile_log");
    assert!(
        session.compile_logs().is_empty(),
        "no compile log should be recorded for a rejected runtime value",
    );
}
