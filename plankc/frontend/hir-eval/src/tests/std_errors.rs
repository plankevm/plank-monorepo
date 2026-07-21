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
fn test_slice_new_rejects_code_region() {
    assert_diagnostics(
        std_project(
            r#"
            import std::regions::code;
            import std::slice::new;

            init {
                new(code, (1, 2));
                @evm_stop();
            }
            "#,
        ),
        &[r#"
        error: `new` cannot construct code slices; use `new_code` to embed compile-time values in code
          --> std/slice.plk:64:9
           |
        64 |         @compile_error("`new` cannot construct code slices; use `new_code` to embed compile-time values in code");
           |         ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ custom compile error triggered here
        "#],
    );
}

#[test]
fn test_slice_new_rejects_calldata_region() {
    assert_diagnostics(
        std_project(
            r#"
            import std::regions::calldata;
            import std::slice::new;

            init {
                new(calldata, (1, 2));
                @evm_stop();
            }
            "#,
        ),
        &[r#"
        error: `new` does not support calldata because calldata is read-only
          --> std/slice.plk:67:9
           |
        67 |         @compile_error("`new` does not support calldata because calldata is read-only");
           |         ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ custom compile error triggered here
        "#],
    );
}

#[test]
fn test_slice_new_rejects_heterogeneous_tuple() {
    assert_diagnostics(
        std_project(
            r#"
            import std::regions::memory;
            import std::slice::new;

            init {
                new(memory, (1, true));
                @evm_stop();
            }
            "#,
        ),
        &[r#"
        error: slice constructor elements must all have the same type
           --> std/slice.plk:211:13
            |
        211 |             @compile_error("slice constructor elements must all have the same type");
            |             ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ custom compile error triggered here
            |
        note: called here
           --> main.plk:5:5
            |
          5 |     new(memory, (1, true));
            |     ^^^^^^^^^^^^^^^^^^^^^^
        "#],
    );
}

#[test]
fn test_slice_new_rejects_empty_tuple() {
    assert_diagnostics(
        std_project(
            r#"
            import std::regions::memory;
            import std::slice::new;

            init {
                new(memory, ());
                @evm_stop();
            }
            "#,
        ),
        &[r#"
        error: cannot infer a slice element type from an empty tuple; use `empty`
           --> std/slice.plk:204:9
            |
        204 |         @compile_error("cannot infer a slice element type from an empty tuple; use `empty`");
            |         ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ custom compile error triggered here
            |
        note: called here
           --> main.plk:5:5
            |
          5 |     new(memory, ());
            |     ^^^^^^^^^^^^^^^
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
           --> std/slice.plk:101:9
            |
        101 |         @compile_error("CSlice access must be evaluated at compile time");
            |         ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ custom compile error triggered here
        "#],
    );
}

#[test]
fn test_slice_set_rejects_non_memory_region() {
    assert_diagnostics(
        std_project(
            r#"
            import std::option::Some;
            import std::regions::code;
            import std::slice::{Slice, set};

            init {
                let slice = Slice(code, u256, comptime { Some(1) }) { ptr: 0 };
                set(slice, 0, 1);
                @evm_stop();
            }
            "#,
        ),
        &[r#"
        error: set mutates backing data in place; calldata and code are immutable, and cbytes-backed slices do not support in-place mutation
           --> std/slice.plk:134:9
            |
        134 | ...   @compile_error("set mutates backing data in place; calldata and code are immutable, and cbytes-backed slices do not support in-place mutation");
            |       ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ custom compile error triggered here
        "#],
    );
}

#[test]
fn test_slice_replace_rejects_out_of_bounds_index() {
    assert_diagnostics(
        std_project(
            r#"
            import std::option::Some;
            import std::regions::ctime;
            import std::slice::{Slice, replace};

            const VALUES = Slice(ctime, u256, Some(2)) {
                data: @concat_cbytes((1, 2))
            };
            const INVALID = replace(VALUES, 2, 3);

            init {
                @evm_stop();
            }
            "#,
        ),
        &[r#"
        error: replace index is out of bounds
           --> std/slice.plk:148:9
            |
        148 |         @compile_error("replace index is out of bounds");
            |         ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ custom compile error triggered here
        "#],
    );
}

#[test]
fn test_slice_replace_rejects_non_ctime_region() {
    assert_diagnostics(
        std_project(
            r#"
            import std::option::Some;
            import std::regions::memory;
            import std::slice::{Slice, replace};

            init {
                let ptr = @evm_calldataload(0);
                let slice = Slice(memory, u256, comptime { Some(1) }) { ptr: ptr };
                replace(slice, 0, 1);
                @evm_stop();
            }
            "#,
        ),
        &[r#"
        error: replace requires a ctime slice
           --> std/slice.plk:144:9
            |
        144 |         @compile_error("replace requires a ctime slice");
            |         ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ custom compile error triggered here
        "#],
    );
}
