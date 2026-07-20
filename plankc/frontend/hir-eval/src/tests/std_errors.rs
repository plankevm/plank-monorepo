use super::*;

#[test]
fn test_runtime_slice_rejects_cbytes_elements() {
    assert_diagnostics(
        std_project(
            r#"
            import std::option::None;
            import std::regions::memory;
            import std::slice::Slice;

            const InvalidSlice = Slice(memory, cbytes, None(u256));

            init {
                @evm_stop();
            }
            "#,
        ),
        &[r#"
        error: type cannot be embedded as a runtime slice element
          --> std/slice.plk:19:9
           |
        19 |         @compile_error("type cannot be embedded as a runtime slice element");
           |         ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ custom compile error triggered here
        "#],
    );
}

#[test]
fn test_runtime_slice_rejects_nested_cbytes_elements() {
    assert_diagnostics(
        std_project(
            r#"
            import std::option::None;
            import std::regions::memory;
            import std::slice::Slice;

            const Dynamic = struct { data: cbytes };
            const InvalidSlice = Slice(memory, Dynamic, None(u256));

            init {
                @evm_stop();
            }
            "#,
        ),
        &[r#"
        error: type cannot be embedded as a runtime slice element
          --> std/slice.plk:19:9
           |
        19 |         @compile_error("type cannot be embedded as a runtime slice element");
           |         ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ custom compile error triggered here
        "#],
    );
}

#[test]
fn test_ctime_slice_rejects_cbytes_elements() {
    assert_diagnostics(
        std_project(
            r#"
            import std::option::None;
            import std::regions::ctime;
            import std::slice::Slice;

            const InvalidSlice = Slice(ctime, cbytes, None(u256));

            init {
                @evm_stop();
            }
            "#,
        ),
        &[r#"
        error: type cannot be embedded as a CSlice element
          --> std/slice.plk:39:9
           |
        39 |         @compile_error("type cannot be embedded as a CSlice element");
           |         ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ custom compile error triggered here
        "#],
    );
}

#[test]
fn test_ctime_slice_rejects_nested_cbytes_elements() {
    assert_diagnostics(
        std_project(
            r#"
            import std::option::None;
            import std::regions::ctime;
            import std::slice::Slice;

            const Dynamic = struct { data: cbytes };
            const InvalidSlice = Slice(ctime, Dynamic, None(u256));

            init {
                @evm_stop();
            }
            "#,
        ),
        &[r#"
        error: type cannot be embedded as a CSlice element
          --> std/slice.plk:39:9
           |
        39 |         @compile_error("type cannot be embedded as a CSlice element");
           |         ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ custom compile error triggered here
        "#],
    );
}

#[test]
fn test_cslice_rejects_runtime_index() {
    assert_diagnostics(
        std_project(
            r#"
            import std::option::None;
            import std::slice::{CSlice, get};

            const VALUES = CSlice(u256, None(u256)) {
                data: @concat_cbytes((1, 2)),
                len: 2
            };

            init {
                let index = @evm_calldataload(0);
                get(VALUES, index);
                @evm_stop();
            }
            "#,
        ),
        &[r#"
        error: CSlice access must be evaluated at compile time
          --> std/slice.plk:69:9
           |
        69 |         @compile_error("CSlice access must be evaluated at compile time");
           |         ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ custom compile error triggered here
        "#],
    );
}
