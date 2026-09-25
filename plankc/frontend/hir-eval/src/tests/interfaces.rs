use super::*;

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
fn test_concat_custom_interface() {
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
                let encoded = @concat_cbytes(("[", Number(0) { raw: 42 }, Number(1) { raw: 0x1234 }, Number(32) { raw: 1 }, "]"));
                comptime_assert(encoded == "[" hex"340000000000000000000000000000000000000000000000000000000000000001" "]", "custom encoding and nested calls");
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
fn test_concat_invalid_impl_result() {
    assert_diagnostics(
        std_project(
            r#"
        const S = struct { fn impl(comptime T: type) u256 { 1 } };
        const encoded = @concat_cbytes((S {},));
        init { @evm_stop(); }
        "#,
        ),
        &[r#"
        error: invalid interface implementation
         --> main.plk:2:17
          |
        2 | const encoded = @concat_cbytes((S {},));
          |                 ^^^^^^^^^^^^^^^^^^^^^^^ `AsPrimitive` requires `impl` to return a value of type `AsPrimitive`, but it returned `u256`
        "#],
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
        init { @evm_stop(); }
        "#,
        ),
        &[r#"
        error: AsPrimitive byte size exceeds 32 bytes
         --> main.plk:9:17
          |
        9 | const encoded = @concat_cbytes((S {},));
          |                 ^^^^^^^^^^^^^^^^^^^^^^^ `AsPrimitive`: `byte_size` must be at most 32, got 33
        "#],
    );
}

#[test]
fn test_concat_invalid_to_raw_result() {
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
