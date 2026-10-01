use super::*;

#[test]
fn test_implemented_interface_is_resolved_once() {
    assert_diagnostics_and_compile_logs(
        std_project(
            r#"
            use std::core::interfaces::AsPrimitive;
            const S = struct {
                raw: u256,
                fn to_raw(self: Self) u256 { self.raw }
                fn from_raw(raw: u256) Self { Self { raw: raw } }
                fn impl(comptime T: type) (comptime {
                    @compile_log("resolve impl");
                    AsPrimitive
                }) {
                    AsPrimitive { byte_size: 1, to_raw: Self.to_raw, unchecked_from_raw: Self.from_raw }
                }
            };
            const encoded = @concat_cbytes((S { raw: 1 }, S { raw: 2 }));
            init { @evm_stop(); }
            "#,
        ),
        &[r#"
        error: found compile log statement
         --> main.plk:7:9
          |
        7 |         @compile_log("resolve impl");
          |         ^^^^^^^^^^^^^^^^^^^^^^^^^^^^
        "#],
        &["\"resolve impl\""],
    );
}

#[test]
fn test_unimplemented_interface_is_resolved_once() {
    assert_diagnostics_and_compile_logs(
        std_project(
            r#"
            const S = struct {
                fn impl(comptime T: type) (comptime {
                    @compile_log("unsupported");
                    void
                }) {}
            };
            const encoded = @concat_cbytes((S {}, S {}));
            init { @evm_stop(); }
            "#,
        ),
        &[r#"
        error: interface not implemented
         --> main.plk:7:17
          |
        7 | const encoded = @concat_cbytes((S {}, S {}));
          |                 ^^^^^^^^^^^^^^^^^^^^^^^^^^^^ `S` does not implement `AsPrimitive`
        "#],
        &["\"unsupported\""],
    );
}

#[test]
fn test_poisoned_interface_is_resolved_once() {
    assert_diagnostics_and_compile_logs(
        std_project(
            r#"
            use std::core::interfaces::AsPrimitive;
            const S = struct {
                fn impl(comptime T: type) (comptime {
                    @compile_log("resolve impl");
                    AsPrimitive
                }) {
                    @compile_error("broken impl");
                }
            };
            const encoded = @concat_cbytes((S {}, S {}));
            init { @evm_stop(); }
            "#,
        ),
        &[r#"
        error: broken impl
         --> main.plk:7:9
          |
        7 |         @compile_error("broken impl");
          |         ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ custom compile error triggered here
        "#],
        &["\"resolve impl\""],
    );
}

#[test]
fn test_concat_struct_without_std() {
    assert_diagnostics(
        r#"
        const S = struct {};
        const encoded = @concat_cbytes((S {},));
        init { @evm_stop(); }
        "#,
        &[r#"
        error: invalid cbytes concat element
         --> main.plk:2:17
          |
        2 | const encoded = @concat_cbytes((S {},));
          |                 ^^^^^^^^^^^^^^^^^^^^^^^ `@concat_cbytes` tuple elements must be `u256`, `cbytes`, or implement `AsPrimitive`, got `S`
        "#],
    );
}

#[test]
fn test_invalid_interface_definition() {
    assert_diagnostics(
        std_project(
            r#"
            init { @evm_stop(); }
            "#,
        )
        .add_file(
            "std/core/interfaces",
            r#"
            const AsPrimitive = struct {
                byte_size: bool,
                to_raw: function,
            };
        "#,
        ),
        &[
            r#"
            error: invalid interface definition
             --> std/core/interfaces.plk:2:5
              |
            2 |     byte_size: bool,
              |     ^^^^^^^^^^^^^^^ `AsPrimitive` field `byte_size` must have type `u256`, got `bool`
            "#,
            r#"
            error: invalid interface definition
             --> std/core/interfaces.plk:1:21
              |
            1 |   const AsPrimitive = struct {
              |  _____________________^
            2 | |     byte_size: bool,
            3 | |     to_raw: function,
            4 | | };
              | |_^ `AsPrimitive` is missing required field `unchecked_from_raw: function`
            "#,
        ],
    );
}

#[test]
fn test_std_without_interfaces() {
    assert_diagnostics(
        std_project(
            r#"
            const S = struct {};
            const encoded = @concat_cbytes((S {},));
            const repeated = @concat_cbytes((S {},));
            init { @evm_stop(); }
            "#,
        )
        .add_file(
            "std/core/interfaces",
            r#"
            const Other = struct {};
        "#,
        ),
        &[r#"
        error: failed to resolve standard library interface `AsPrimitive`
         --> std/core/interfaces.plk
        "#],
    );
}

#[test]
fn test_std_as_primitive_not_a_struct() {
    assert_diagnostics(
        std_project(
            r#"
            const S = struct {};
            const encoded = @concat_cbytes((S {},));
            const repeated = @concat_cbytes((S {},));
            init { @evm_stop(); }
            "#,
        )
        .add_file(
            "std/core/interfaces",
            r#"
            const AsPrimitive = 5;
        "#,
        ),
        &[r#"
        error: invalid standard library interface
         --> std/core/interfaces.plk:1:1
          |
        1 | const AsPrimitive = 5;
          | ^^^^^^^^^^^^^^^^^^^^^^ `AsPrimitive` is not a struct type
        "#],
    );
}

#[test]
fn test_concat_as_primitive() {
    assert_lowers_to(
        std_project(
            r#"
        use std::core::uint::u64;
        use std::core::addr::addr;
        use std::error::comptime_assert;

        init {
            comptime {
                let number = u64.unchecked_from_raw(34);
                let address = addr.unchecked_from_raw(0x1234567890abcdef1234567890abcdef12345678);
                let encoded = @concat_cbytes(("[", number, address, "]"));
                comptime_assert(encoded == "[" hex"00000000000000221234567890abcdef1234567890abcdef12345678" "]", "primitive encoding");
            };
            @evm_stop();
        }
        "#,
        ),
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
fn test_concat_custom_as_primitive() {
    assert_lowers_to(
        std_project(
            r#"
        use std::core::interfaces::{AsPrimitive, supports};
        use std::error::comptime_assert;

        const Number = fn(comptime bytes: u256) type {
            struct {
                raw: u256,
                fn to_raw(self: Self) u256 {
                    let encoded = @concat_cbytes((self.raw,));
                    @padded_read_cbytes(encoded, 0)
                }
                fn from_raw(raw: u256) Self { Self { raw: raw } }
                fn impl(comptime T: type) supports(T, (AsPrimitive,)) {
                    if T == AsPrimitive {
                        AsPrimitive { byte_size: bytes, to_raw: Self.to_raw, unchecked_from_raw: Self.from_raw }
                    } else { {} }
                }
            }
        };
        init {
            comptime {
                let max_raw = 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff;
                let encoded = @concat_cbytes(("[", Number(0) { raw: 0 }, Number(1) { raw: 0xff }, Number(32) { raw: max_raw }, "]"));
                comptime_assert(encoded == "[" hex"ff" hex"ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff" "]", "custom encoding and nested calls");
            };
            @evm_stop();
        }
        "#,
        ),
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
fn test_concat_interface_not_implemented() {
    assert_diagnostics(
        std_project(
            r#"
        const Missing = struct {};
        const Unsupported = struct { fn impl(comptime T: type) void {} };
        const first = @concat_cbytes((Missing {},));
        const second = @concat_cbytes((Unsupported {},));
        init { @evm_stop(); }
        "#,
        ),
        &[
            r#"
            error: interface not implemented
             --> main.plk:3:15
              |
            3 | const first = @concat_cbytes((Missing {},));
              |               ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ `Missing` does not implement `AsPrimitive`
            "#,
            r#"
            error: interface not implemented
             --> main.plk:4:16
              |
            4 | const second = @concat_cbytes((Unsupported {},));
              |                ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ `Unsupported` does not implement `AsPrimitive`
            "#,
        ],
    );
}

#[test]
fn test_concat_impl_wrong_return_type() {
    assert_diagnostics(
        std_project(
            r#"
        const S = struct { fn impl(comptime T: type) u256 { 1 } };
        const encoded = @concat_cbytes((S {},));
        const repeated = @concat_cbytes((S {},));
        init { @evm_stop(); }
        "#,
        ),
        &[
            r#"
            error: invalid interface implementation
             --> main.plk:2:17
              |
            2 | const encoded = @concat_cbytes((S {},));
              |                 ^^^^^^^^^^^^^^^^^^^^^^^ `AsPrimitive` requires `impl` to return a value of type `AsPrimitive`, but it returned `u256`
            "#,
            r#"
            error: invalid interface implementation
             --> main.plk:3:18
              |
            3 | const repeated = @concat_cbytes((S {},));
              |                  ^^^^^^^^^^^^^^^^^^^^^^^ `AsPrimitive` requires `impl` to return a value of type `AsPrimitive`, but it returned `u256`
            "#,
        ],
    );
}

#[test]
fn test_concat_invalid_byte_size() {
    assert_diagnostics(
        std_project(
            r#"
        use std::core::interfaces::AsPrimitive;
        const S = struct {
            fn to_raw(self: Self) u256 { 0 }
            fn from_raw(raw: u256) Self { Self {} }
            fn impl(comptime T: type) AsPrimitive {
                AsPrimitive { byte_size: 33, to_raw: Self.to_raw, unchecked_from_raw: Self.from_raw }
            }
        };
        const encoded = @concat_cbytes((S {},));
        const repeated = @concat_cbytes((S {},));
        init { @evm_stop(); }
        "#,
        ),
        &[
            r#"
            error: invalid interface implementation
             --> main.plk:9:17
              |
            9 | const encoded = @concat_cbytes((S {},));
              |                 ^^^^^^^^^^^^^^^^^^^^^^^ `AsPrimitive` requires `byte_size` to be at most 32, but it is 33
            "#,
            r#"
            error: invalid interface implementation
              --> main.plk:10:18
               |
            10 | const repeated = @concat_cbytes((S {},));
               |                  ^^^^^^^^^^^^^^^^^^^^^^^ `AsPrimitive` requires `byte_size` to be at most 32, but it is 33
            "#,
        ],
    );
}

#[test]
fn test_concat_to_raw_wrong_return_type() {
    assert_diagnostics(
        std_project(
            r#"
        use std::core::interfaces::AsPrimitive;
        const S = struct {
            fn to_raw(self: Self) bool { true }
            fn from_raw(raw: u256) Self { Self {} }
            fn impl(comptime T: type) AsPrimitive {
                AsPrimitive { byte_size: 1, to_raw: Self.to_raw, unchecked_from_raw: Self.from_raw }
            }
        };
        const encoded = @concat_cbytes((S {},));
        init { @evm_stop(); }
        "#,
        ),
        &[r#"
        error: invalid interface implementation
         --> main.plk:9:17
          |
        9 | const encoded = @concat_cbytes((S {},));
          |                 ^^^^^^^^^^^^^^^^^^^^^^^ `AsPrimitive` requires `to_raw` to return a value of type `u256`, but it returned `bool`
        "#],
    );
}

#[test]
fn test_concat_to_raw_exceeds_byte_size() {
    assert_diagnostics(
        std_project(
            r#"
            use std::core::interfaces::AsPrimitive;
            const Number = fn(comptime bytes: u256) type {
                struct {
                    raw: u256,
                    fn to_raw(self: Self) u256 { self.raw }
                    fn from_raw(raw: u256) Self { Self { raw: raw } }
                    fn impl(comptime T: type) AsPrimitive {
                        AsPrimitive { byte_size: bytes, to_raw: Self.to_raw, unchecked_from_raw: Self.from_raw }
                    }
                }
            };
            const valid = @concat_cbytes((Number(1) { raw: 255 },));
            const oversized = @concat_cbytes((Number(1) { raw: 256 },));
            const zero_width = @concat_cbytes((Number(0) { raw: 1 },));
            init { @evm_stop(); }
            "#,
        ),
        &[
            r#"
            error: `AsPrimitive` value exceeds declared byte size
              --> main.plk:13:19
               |
            13 | const oversized = @concat_cbytes((Number(1) { raw: 256 },));
               |                   ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ `to_raw` returned 256, which does not fit in 1 byte
            "#,
            r#"
            error: `AsPrimitive` value exceeds declared byte size
              --> main.plk:14:20
               |
            14 | const zero_width = @concat_cbytes((Number(0) { raw: 1 },));
               |                    ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ `to_raw` returned 1, which does not fit in 0 bytes
            "#,
        ],
    );
}
