use super::*;
use plank_test_utils::TestProject;

const STD_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../std");

fn std_project(source: &str) -> TestProject {
    TestProject::root(source).with_stdlib_dir(STD_DIR)
}

#[test]
fn test_binary_op_not_supported_without_std() {
    assert_diagnostics(
        r#"
        init {
            let a = 1;
            let b = 2;
            let c = a + b;
            @evm_stop();
        }
        "#,
        &[r#"
        error: operator not supported
         --> main.plk:4:13
          |
        4 |     let c = a + b;
          |             ^^^^^ operator '+' is not supported for type `u256`
        "#],
    );
}

#[test]
fn test_unary_bitwise_not_without_std() {
    assert_lowers_to(
        r#"
        init {
            let a = @evm_calldataload(0);
            let b = ~a;
            @evm_sstore(0, b);
            @evm_stop();
        }
        "#,
        r#"
        ==== Functions ====
        ; init
        @fn0() -> never {
            %0 : u256 = 0
            %1 : u256 = @evm_calldataload(%0)
            %2 : u256 = %1
            %3 : u256 = @evm_not(%2)
            %4 : u256 = %3
            %5 : u256 = 0
            %6 : void = @evm_sstore(%5, %4)
            %7 : never = @evm_stop()
        }
        "#,
    );
}

#[test]
fn test_unary_negate_not_supported_without_std() {
    assert_diagnostics(
        r#"
        init {
            let a = 1;
            let b = -a;
            @evm_stop();
        }
        "#,
        &[r#"
        error: operator not supported
         --> main.plk:3:13
          |
        3 |     let b = -a;
          |             ^^ operator '-' is not supported for type `u256`
        "#],
    );
}

#[test]
fn test_runtime_checked_add_with_std() {
    assert_lowers_to(
        std_project(
            r#"
        init {
            let a = @evm_calldataload(0);
            let b = @evm_calldataload(32);
            let c = a + b;
            @evm_stop();
        }
        "#,
        ),
        r#"
        ==== Functions ====
        @fn0() -> never {
            %0 : u256 = 64
            %1 : memptr = @malloc_uninit(%0)
            %2 : memptr = %1
            %3 : u256 = 0
            %4 : memptr = @evm_add(%2, %3)
            %5 : u256 = 0x4e487b71
            %6 : void = @mstore32(%4, %5)
            %7 : memptr = %1
            %8 : u256 = 32
            %9 : memptr = @evm_add(%7, %8)
            %10 : u256 = 17
            %11 : void = @mstore32(%9, %10)
            %12 : memptr = %1
            %13 : u256 = 28
            %14 : memptr = @evm_add(%12, %13)
            %15 : u256 = 36
            %16 : never = @evm_revert(%14, %15)
        }

        @fn1(%0: u256, %1: u256) -> u256 {
            %2 : u256 = %0
            %3 : u256 = %1
            %4 : u256 = @evm_add(%2, %3)
            %5 : u256 = %4
            %6 : u256 = %0
            %7 : bool = @evm_lt(%5, %6)
            if %7 {
                %8 : never = call @fn0()
            } else {
                %9 : void = void_unit
            }
            %10 : void = %9
            %11 : u256 = %4
            ret %11
        }

        ; init
        @fn2() -> never {
            %0 : u256 = 0
            %1 : u256 = @evm_calldataload(%0)
            %2 : u256 = 32
            %3 : u256 = @evm_calldataload(%2)
            %4 : u256 = %1
            %5 : u256 = %3
            %6 : u256 = call @fn1(%4, %5)
            %7 : never = @evm_stop()
        }
        "#,
    );
}

#[test]
fn test_runtime_wrapping_add_with_std() {
    assert_lowers_to(
        std_project(
            r#"
        init {
            let a = @evm_calldataload(0);
            let b = @evm_calldataload(32);
            let c = a +% b;
            @evm_stop();
        }
        "#,
        ),
        r#"
        ==== Functions ====
        ; init
        @fn0() -> never {
            %0 : u256 = 0
            %1 : u256 = @evm_calldataload(%0)
            %2 : u256 = 32
            %3 : u256 = @evm_calldataload(%2)
            %4 : u256 = %1
            %5 : u256 = %3
            %6 : u256 = @evm_add(%4, %5)
            %7 : never = @evm_stop()
        }
        "#,
    );
}

#[test]
fn test_runtime_eq_with_std() {
    assert_lowers_to(
        std_project(
            r#"
        init {
            let a = @evm_calldataload(0);
            let b = @evm_calldataload(32);
            let c = a == b;
            @evm_stop();
        }
        "#,
        ),
        r#"
        ==== Functions ====
        ; init
        @fn0() -> never {
            %0 : u256 = 0
            %1 : u256 = @evm_calldataload(%0)
            %2 : u256 = 32
            %3 : u256 = @evm_calldataload(%2)
            %4 : u256 = %1
            %5 : u256 = %3
            %6 : bool = @evm_eq(%4, %5)
            %7 : never = @evm_stop()
        }
        "#,
    );
}

#[test]
fn test_runtime_bitwise_and_with_std() {
    assert_lowers_to(
        std_project(
            r#"
        init {
            let a = @evm_calldataload(0);
            let b = @evm_calldataload(32);
            let c = a & b;
            @evm_stop();
        }
        "#,
        ),
        r#"
        ==== Functions ====
        ; init
        @fn0() -> never {
            %0 : u256 = 0
            %1 : u256 = @evm_calldataload(%0)
            %2 : u256 = 32
            %3 : u256 = @evm_calldataload(%2)
            %4 : u256 = %1
            %5 : u256 = %3
            %6 : u256 = @evm_and(%4, %5)
            %7 : never = @evm_stop()
        }
        "#,
    );
}

#[test]
fn test_runtime_shift_left_with_std() {
    assert_lowers_to(
        std_project(
            r#"
        init {
            let a = @evm_calldataload(0);
            let b = @evm_calldataload(32);
            let c = a << b;
            @evm_stop();
        }
        "#,
        ),
        r#"
        ==== Functions ====
        ; init
        @fn0() -> never {
            %0 : u256 = 0
            %1 : u256 = @evm_calldataload(%0)
            %2 : u256 = 32
            %3 : u256 = @evm_calldataload(%2)
            %4 : u256 = %1
            %5 : u256 = %3
            %6 : u256 = @evm_shl(%4, %5)
            %7 : never = @evm_stop()
        }
        "#,
    );
}

#[test]
fn test_runtime_negate() {
    assert_lowers_to(
        std_project(
            r#"
        init {
            let a = @evm_calldataload(0);
            let b = -a;
            @evm_stop();
        }
        "#,
        ),
        r#"
        ==== Functions ====
        @fn0(%0: u256) -> u256 {
            %1 : u256 = %0
            %2 : u256 = 0
            %3 : u256 = @evm_sub(%2, %1)
            ret %3
        }

        ; init
        @fn1() -> never {
            %0 : u256 = 0
            %1 : u256 = @evm_calldataload(%0)
            %2 : u256 = %1
            %3 : u256 = call @fn0(%2)
            %4 : never = @evm_stop()
        }
        "#,
    );
}

#[test]
fn test_runtime_not_eq_with_std() {
    assert_lowers_to(
        std_project(
            r#"
        init {
            let a = @evm_calldataload(0);
            let b = @evm_calldataload(32);
            let c = a != b;
            @evm_stop();
        }
        "#,
        ),
        r#"
        ==== Functions ====
        ; init
        @fn0() -> never {
            %0 : u256 = 0
            %1 : u256 = @evm_calldataload(%0)
            %2 : u256 = 32
            %3 : u256 = @evm_calldataload(%2)
            %4 : u256 = %1
            %5 : u256 = %3
            %6 : bool = @evm_eq(%4, %5)
            %7 : bool = @evm_iszero(%6)
            %8 : never = @evm_stop()
        }
        "#,
    );
}

#[test]
fn test_runtime_less_equals_with_std() {
    assert_lowers_to(
        std_project(
            r#"
        init {
            let a = @evm_calldataload(0);
            let b = @evm_calldataload(32);
            let c = a <= b;
            @evm_stop();
        }
        "#,
        ),
        r#"
        ==== Functions ====
        @fn0(%0: u256, %1: u256) -> bool {
            %2 : u256 = %0
            %3 : u256 = %1
            %4 : bool = @evm_gt(%2, %3)
            %5 : bool = @evm_iszero(%4)
            ret %5
        }

        ; init
        @fn1() -> never {
            %0 : u256 = 0
            %1 : u256 = @evm_calldataload(%0)
            %2 : u256 = 32
            %3 : u256 = @evm_calldataload(%2)
            %4 : u256 = %1
            %5 : u256 = %3
            %6 : bool = call @fn0(%4, %5)
            %7 : never = @evm_stop()
        }
        "#,
    );
}

#[test]
fn test_runtime_greater_equals_with_std() {
    assert_lowers_to(
        std_project(
            r#"
        init {
            let a = @evm_calldataload(0);
            let b = @evm_calldataload(32);
            let c = a >= b;
            @evm_stop();
        }
        "#,
        ),
        r#"
        ==== Functions ====
        @fn0(%0: u256, %1: u256) -> bool {
            %2 : u256 = %0
            %3 : u256 = %1
            %4 : bool = @evm_lt(%2, %3)
            %5 : bool = @evm_iszero(%4)
            ret %5
        }

        ; init
        @fn1() -> never {
            %0 : u256 = 0
            %1 : u256 = @evm_calldataload(%0)
            %2 : u256 = 32
            %3 : u256 = @evm_calldataload(%2)
            %4 : u256 = %1
            %5 : u256 = %3
            %6 : bool = call @fn0(%4, %5)
            %7 : never = @evm_stop()
        }
        "#,
    );
}

#[test]
fn test_runtime_bitwise_not_with_std() {
    assert_lowers_to(
        std_project(
            r#"
        init {
            let a = @evm_calldataload(0);
            let b = ~a;
            @evm_sstore(0, b);
            @evm_stop();
        }
        "#,
        ),
        r#"
        ==== Functions ====
        ; init
        @fn0() -> never {
            %0 : u256 = 0
            %1 : u256 = @evm_calldataload(%0)
            %2 : u256 = %1
            %3 : u256 = @evm_not(%2)
            %4 : u256 = %3
            %5 : u256 = 0
            %6 : void = @evm_sstore(%5, %4)
            %7 : never = @evm_stop()
        }
        "#,
    );
}

#[test]
fn test_comptime_checked_add_fold_with_std() {
    assert_lowers_to(
        std_project(
            r#"
        const folded = 3 + 4;
        init {
            @evm_sstore(folded, 0);
            @evm_stop();
        }
        "#,
        ),
        r#"
        ==== Functions ====
        ; init
        @fn0() -> never {
            %0 : u256 = 7
            %1 : u256 = 0
            %2 : void = @evm_sstore(%0, %1)
            %3 : never = @evm_stop()
        }
        "#,
    );
}

#[test]
fn test_comptime_wrapping_add_fold_with_std() {
    assert_lowers_to(
        std_project(
            r#"
        const folded = 5 +% 3;
        init {
            @evm_sstore(folded, 0);
            @evm_stop();
        }
        "#,
        ),
        r#"
        ==== Functions ====
        ; init
        @fn0() -> never {
            %0 : u256 = 8
            %1 : u256 = 0
            %2 : void = @evm_sstore(%0, %1)
            %3 : never = @evm_stop()
        }
        "#,
    );
}

#[test]
fn test_comptime_add_overflow_with_std() {
    assert_project_diagnostics(
        std_project(
            r#"
        const overflow = 0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF + 1;
        init {
            @evm_stop();
        }
        "#,
        ),
        &[r#"
        error: arithmetic overflow
         --> main.plk:1:18
          |
        1 | const overflow = 0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF + 1;
          |                  ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ '+' overflow at compile time
        "#],
    );
}

#[test]
fn test_comptime_sub_overflow_with_std() {
    assert_project_diagnostics(
        std_project(
            r#"
        const underflow = 0 - 1;
        init {
            @evm_stop();
        }
        "#,
        ),
        &[r#"
        error: arithmetic underflow
         --> main.plk:1:19
          |
        1 | const underflow = 0 - 1;
          |                   ^^^^^ '-' underflow at compile time
        "#],
    );
}

#[test]
fn test_comptime_mul_overflow_with_std() {
    assert_project_diagnostics(
        std_project(
            r#"
        const overflow = 0x8000000000000000000000000000000000000000000000000000000000000000 * 2;
        init {
            @evm_stop();
        }
        "#,
        ),
        &[r#"
        error: arithmetic overflow
         --> main.plk:1:18
          |
        1 | const overflow = 0x8000000000000000000000000000000000000000000000000000000000000000 * 2;
          |                  ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ '*' overflow at compile time
        "#],
    );
}

#[test]
fn test_comptime_mod_by_zero_with_std() {
    assert_project_diagnostics(
        std_project(
            r#"
        const bad = 5 % 0;
        init {
            @evm_stop();
        }
        "#,
        ),
        &[r#"
        error: modulo by zero
         --> main.plk:1:13
          |
        1 | const bad = 5 % 0;
          |             ^^^^^ '%' modulo by zero at compile time
          |
          = info: for EVM behavior where modulo by zero returns 0, use `@evm_mod`
        "#],
    );
}

#[test]
fn test_comptime_div_up_by_zero_with_std() {
    assert_project_diagnostics(
        std_project(
            r#"
        const bad = 5 +/ 0;
        init {
            @evm_stop();
        }
        "#,
        ),
        &[r#"
        error: division by zero
         --> main.plk:1:13
          |
        1 | const bad = 5 +/ 0;
          |             ^^^^^^ '+/' division by zero at compile time
          |
          = info: for EVM behavior where division by zero returns 0, use `@evm_div` or `@evm_sdiv`, note that the rounding direction may differ
        "#],
    );
}

#[test]
fn test_comptime_div_down_by_zero_with_std() {
    assert_project_diagnostics(
        std_project(
            r#"
        const bad = 5 -/ 0;
        init {
            @evm_stop();
        }
        "#,
        ),
        &[r#"
        error: division by zero
         --> main.plk:1:13
          |
        1 | const bad = 5 -/ 0;
          |             ^^^^^^ '-/' division by zero at compile time
          |
          = info: for EVM behavior where division by zero returns 0, use `@evm_div` or `@evm_sdiv`, note that the rounding direction may differ
        "#],
    );
}

#[test]
fn test_equals_type_equality() {
    assert_lowers_to(
        r#"
        const eq = u256 == u256;
        init {
            if eq {
                @evm_sstore(0, 1);
            } else {
                @evm_sstore(0, 2);
            }
            @evm_stop();
        }
        "#,
        r#"
        ==== Functions ====
        ; init
        @fn0() -> never {
            %0 : u256 = 0
            %1 : u256 = 1
            %2 : void = @evm_sstore(%0, %1)
            %3 : void = void_unit
            %4 : void = %3
            %5 : never = @evm_stop()
        }
        "#,
    );
}

#[test]
fn test_operator_type_mismatch() {
    assert_project_diagnostics(
        std_project(
            r#"
        init {
            let a = 1;
            let b = true;
            let c = a + b;
            @evm_stop();
        }
        "#,
        ),
        &[r#"
        error: mismatched types
         --> main.plk:4:13
          |
        4 |     let c = a + b;
          |             ^^^^^ expected `u256`, got `bool`
        "#],
    );
}

#[test]
fn test_type_inequality() {
    assert_lowers_to(
        std_project(
            r#"
        init {
            let mut x = bool != void;
            @evm_stop();
        }
        "#,
        ),
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
fn test_std_operator_not_a_function() {
    assert_project_diagnostics(
        TestProject::root(
            r#"
        init {
            @evm_stop();
        }
        "#,
        )
        .with_core_ops(
            r#"
const checked_sub = fn (x: u256, y: u256) u256 { x };
const checked_mul = fn (x: u256, y: u256) u256 { x };
const checked_mod = fn (x: u256, y: u256) u256 { x };
const checked_div_up = fn (x: u256, y: u256) u256 { x };
const checked_div_down = fn (x: u256, y: u256) u256 { x };
const greater_equals = fn (x: u256, y: u256) bool { true };
const less_equals = fn (x: u256, y: u256) bool { true };
const neg_u256 = fn (x: u256) u256 { x };
const checked_add = 42;
        "#,
        ),
        &[r#"
        error: invalid standard library operator
         --> __core_ops.plk:9:1
          |
        9 | const checked_add = 42;
          | ^^^^^^^^^^^^^^^^^^^^^^^ `checked_add` is not a function
        "#],
    );
}

#[test]
fn test_std_operator_missing() {
    assert_project_diagnostics(
        TestProject::root(
            r#"
        init {
            @evm_stop();
        }
        "#,
        )
        .with_core_ops(
            r#"
const checked_sub = fn (x: u256, y: u256) u256 { x };
const checked_mul = fn (x: u256, y: u256) u256 { x };
const checked_mod = fn (x: u256, y: u256) u256 { x };
const checked_div_up = fn (x: u256, y: u256) u256 { x };
const checked_div_down = fn (x: u256, y: u256) u256 { x };
const less_equals = fn (x: u256, y: u256) bool { true };
const neg_u256 = fn (x: u256) u256 { x };
        "#,
        ),
        &[
            r#"
        error: failed to resolve core operation handler `checked_add`
         --> __core_ops.plk
        "#,
            r#"
        error: failed to resolve core operation handler `greater_equals`
         --> __core_ops.plk
        "#,
        ],
    );
}

#[test]
fn test_bool_runtime_inequality() {
    assert_lowers_to(
        std_project(
            r#"
        init {
            let mut x = false;
            let mut y = true;
            let mut z = x != y;
            @evm_stop();
        }
        "#,
        ),
        r#"
        ==== Functions ====
        ; init
        @fn0() -> never {
            %0 : bool = false
            %1 : bool = true
            %2 : bool = %0
            %3 : bool = %1
            %4 : bool = @evm_xor(%2, %3)
            %5 : never = @evm_stop()
        }
        "#,
    );
}

#[test]
fn test_bool_runtime_equality() {
    assert_lowers_to(
        std_project(
            r#"
        init {
            let mut x = false;
            let mut y = true;
            if x == y {
                @evm_stop();
            } else {
                @evm_invalid();
            }
        }
        "#,
        ),
        r#"
        ==== Functions ====
        ; init
        @fn0() -> never {
            %0 : bool = false
            %1 : bool = true
            %2 : bool = %0
            %3 : bool = %1
            %4 : bool = @evm_eq(%2, %3)
            if %4 {
                %5 : never = @evm_stop()
            } else {
                %6 : never = @evm_invalid()
            }
        }
        "#,
    );
}

#[test]
fn test_bool_comptime_equality() {
    assert_lowers_to(
        std_project(
            r#"
        init {
            let mut x = true == true;
            @evm_stop();
        }
        "#,
        ),
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
fn test_memptr_equality() {
    assert_lowers_to(
        std_project(
            r#"
        init {
            let p1 = @malloc_uninit(0);
            let p2 = p1;
            if p1 == p2 {
                @evm_invalid();
            }
            @evm_stop();
        }
        "#,
        ),
        r#"
        ==== Functions ====
        ; init
        @fn0() -> never {
            %0 : u256 = 0
            %1 : memptr = @malloc_uninit(%0)
            %2 : memptr = %1
            %3 : memptr = %1
            %4 : memptr = %2
            %5 : bool = @evm_eq(%3, %4)
            if %5 {
                %6 : never = @evm_invalid()
            } else {
                %7 : void = void_unit
            }
            %8 : void = %7
            %9 : never = @evm_stop()
        }
        "#,
    );
}

#[test]
fn test_memptr_add_not_supported() {
    assert_project_diagnostics(
        TestProject::root(
            r#"
        init {
            let a = @malloc_uninit(32);
            let b = @malloc_uninit(32);
            let c = a + b;
            @evm_stop();
        }
        "#,
        ),
        &[r#"
        error: operator not supported
         --> main.plk:4:13
          |
        4 |     let c = a + b;
          |             ^^^^^ operator '+' is not supported for type `memptr`
          |
          = help: only wrapping operators `+%` and `-%` are supported for `memptr`
        "#],
    );
}

#[test]
fn test_memptr_sub_not_supported() {
    assert_project_diagnostics(
        TestProject::root(
            r#"
        init {
            let a = @malloc_uninit(32);
            let b = @malloc_uninit(32);
            let c = a - b;
            @evm_stop();
        }
        "#,
        ),
        &[r#"
        error: operator not supported
         --> main.plk:4:13
          |
        4 |     let c = a - b;
          |             ^^^^^ operator '-' is not supported for type `memptr`
          |
          = help: only wrapping operators `+%` and `-%` are supported for `memptr`
        "#],
    );
}

#[test]
fn test_operator_precedence() {
    assert_lowers_to(
        std_project(
            r#"
        const a = fn () u256 { @evm_sload(0) };
        const b = fn () u256 { @evm_sload(1) };
        const c = fn () u256 { @evm_sload(2) };
        const d = fn () u256 { @evm_sload(3) };

        init {
            let x = a() * b() + (c() +% c()) -/ d();
            @evm_stop();
        }
        "#,
        ),
        r#"
        ==== Functions ====
        @fn0() -> u256 {
            %0 : u256 = 0
            %1 : u256 = @evm_sload(%0)
            ret %1
        }

        @fn1() -> u256 {
            %0 : u256 = 1
            %1 : u256 = @evm_sload(%0)
            ret %1
        }

        @fn2() -> never {
            %0 : u256 = 64
            %1 : memptr = @malloc_uninit(%0)
            %2 : memptr = %1
            %3 : u256 = 0
            %4 : memptr = @evm_add(%2, %3)
            %5 : u256 = 0x4e487b71
            %6 : void = @mstore32(%4, %5)
            %7 : memptr = %1
            %8 : u256 = 32
            %9 : memptr = @evm_add(%7, %8)
            %10 : u256 = 17
            %11 : void = @mstore32(%9, %10)
            %12 : memptr = %1
            %13 : u256 = 28
            %14 : memptr = @evm_add(%12, %13)
            %15 : u256 = 36
            %16 : never = @evm_revert(%14, %15)
        }

        @fn3(%0: u256, %1: u256) -> u256 {
            %2 : u256 = %0
            %3 : u256 = %1
            %4 : u256 = @evm_mul(%2, %3)
            %5 : u256 = %4
            %6 : u256 = %0
            %7 : u256 = @evm_div(%5, %6)
            %8 : u256 = %1
            %9 : bool = @evm_eq(%7, %8)
            %10 : bool = @evm_iszero(%9)
            %11 : u256 = %0
            %12 : u256 = 0
            %13 : bool = @evm_gt(%11, %12)
            %14 : bool = @evm_and(%10, %13)
            if %14 {
                %15 : never = call @fn2()
            } else {
                %16 : void = void_unit
            }
            %17 : void = %16
            %18 : u256 = %4
            ret %18
        }

        @fn4() -> u256 {
            %0 : u256 = 2
            %1 : u256 = @evm_sload(%0)
            ret %1
        }

        @fn5() -> u256 {
            %0 : u256 = 3
            %1 : u256 = @evm_sload(%0)
            ret %1
        }

        @fn6() -> never {
            %0 : u256 = 64
            %1 : memptr = @malloc_uninit(%0)
            %2 : memptr = %1
            %3 : u256 = 0
            %4 : memptr = @evm_add(%2, %3)
            %5 : u256 = 0x4e487b71
            %6 : void = @mstore32(%4, %5)
            %7 : memptr = %1
            %8 : u256 = 32
            %9 : memptr = @evm_add(%7, %8)
            %10 : u256 = 18
            %11 : void = @mstore32(%9, %10)
            %12 : memptr = %1
            %13 : u256 = 28
            %14 : memptr = @evm_add(%12, %13)
            %15 : u256 = 36
            %16 : never = @evm_revert(%14, %15)
        }

        @fn7(%0: u256, %1: u256) -> u256 {
            %2 : u256 = %1
            %3 : u256 = 0
            %4 : bool = @evm_eq(%2, %3)
            if %4 {
                %5 : never = call @fn6()
            } else {
                %6 : void = void_unit
            }
            %7 : void = %6
            %8 : u256 = %0
            %9 : u256 = %1
            %10 : u256 = @evm_div(%8, %9)
            ret %10
        }

        @fn8(%0: u256, %1: u256) -> u256 {
            %2 : u256 = %0
            %3 : u256 = %1
            %4 : u256 = @evm_add(%2, %3)
            %5 : u256 = %4
            %6 : u256 = %0
            %7 : bool = @evm_lt(%5, %6)
            if %7 {
                %8 : never = call @fn2()
            } else {
                %9 : void = void_unit
            }
            %10 : void = %9
            %11 : u256 = %4
            ret %11
        }

        ; init
        @fn9() -> never {
            %0 : u256 = call @fn0()
            %1 : u256 = call @fn1()
            %2 : u256 = call @fn3(%0, %1)
            %3 : u256 = call @fn4()
            %4 : u256 = call @fn4()
            %5 : u256 = @evm_add(%3, %4)
            %6 : u256 = call @fn5()
            %7 : u256 = call @fn7(%5, %6)
            %8 : u256 = call @fn8(%2, %7)
            %9 : never = @evm_stop()
        }
        "#,
    );
}

#[test]
fn test_bitwise_bool() {
    assert_lowers_to(
        std_project(
            r#"
        init {
            let mut x = false ^ false;
            let mut x = true ^ false;
            let mut x = true | false;
            let mut x = false | false;
            let mut x = false & false;
            let mut x = true & false;
            let mut x = true & true;
            @evm_stop();
        }

        "#,
        ),
        r#"
        ==== Functions ====
        ; init
        @fn0() -> never {
            %0 : bool = false
            %1 : bool = true
            %2 : bool = true
            %3 : bool = false
            %4 : bool = false
            %5 : bool = false
            %6 : bool = true
            %7 : never = @evm_stop()
        }
        "#,
    );
}
