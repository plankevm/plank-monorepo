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
        error: Slice: type cannot be embedded as a runtime slice element
         --> std/error.plk:8:5
          |
        8 |     @compile_error(@concat_cbytes((caller, ": ", message)));
          |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ custom compile error triggered here
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
        error: Slice: type cannot be embedded as a runtime slice element
         --> std/error.plk:8:5
          |
        8 |     @compile_error(@concat_cbytes((caller, ": ", message)));
          |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ custom compile error triggered here
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
          --> std/slice.plk:40:9
           |
        40 |         @compile_error("type cannot be embedded as a CSlice element");
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
        error: new: cannot construct `code` or `ctime` slices in runtime context; use `new_comptime`
          --> std/slice.plk:70:13
           |
        70 |             @compile_error("new: cannot construct `code` or `ctime` slices in runtime context; use `new_comptime`");
           |             ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ custom compile error triggered here
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
        error: new: `calldata` not supported because calldata is read-only
          --> std/slice.plk:65:9
           |
        65 |         @compile_error("new: `calldata` not supported because calldata is read-only");
           |         ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ custom compile error triggered here
        "#],
    );
}

#[test]
fn test_slice_new_ctime_rejects_runtime_values() {
    assert_diagnostics(
        std_project(
            r#"
            import std::regions::ctime;
            import std::slice::new;

            init {
                let value = @evm_calldataload(0);
                new(ctime, (value,));
                @evm_stop();
            }
            "#,
        ),
        &[r#"
        error: runtime argument to function with comptime-only return type
         --> main.plk:6:16
          |
        6 |     new(ctime, (value,));
          |     -----------^^^^^^^^-
          |     |          |
          |     |          runtime argument here
          |     function called here
          |
          = note: functions with comptime-only return types require all arguments to be known at compile time
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
        error: new: slice constructor elements must all have the same type
         --> std/error.plk:8:5
          |
        8 |     @compile_error(@concat_cbytes((caller, ": ", message)));
          |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ custom compile error triggered here
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
        error: new: cannot infer a slice element type from an empty tuple; use `empty`
         --> std/error.plk:8:5
          |
        8 |     @compile_error(@concat_cbytes((caller, ": ", message)));
          |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ custom compile error triggered here
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
           --> std/slice.plk:109:9
            |
        109 |         @compile_error("CSlice access must be evaluated at compile time");
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
           --> std/slice.plk:142:9
            |
        142 | ...   @compile_error("set mutates backing data in place; calldata and code are immutable, and cbytes-backed slices do not support in-place mutation");
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
        error: replace: index is out of bounds
           --> std/slice.plk:156:9
            |
        156 |         @compile_error("replace: index is out of bounds");
            |         ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ custom compile error triggered here
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
        error: replace: requires a ctime slice
           --> std/slice.plk:152:9
            |
        152 |         @compile_error("replace: requires a ctime slice");
            |         ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ custom compile error triggered here
        "#],
    );
}
