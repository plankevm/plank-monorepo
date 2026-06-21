use super::*;
use crate::quota::DEFAULT_COMPTIME_BRANCH_QUOTA;
use sha2::Digest;

#[test]
fn test_comptime_only_return_caches_per_non_comptime_arg_value() {
    assert_lowers_to(
        r#"
        const f = fn(comptime T: type, x: T) type {
            if @evm_eq(x, 0) { T } else { bool }
        };
        init {
            let mut a: f(u256, 0) = 34;
            let mut b: f(u256, 1) = false;
            let mut c: f(u256, 0) = 22;
            @evm_stop();
        }
        "#,
        r#"
        ==== Functions ====
        ; init
        @fn0() -> never {
            %0 : u256 = 34
            %1 : bool = false
            %2 : u256 = 22
            %3 : never = @evm_stop()
        }
        "#,
    );
}

#[test]
fn test_comptime_evm_builtins() {
    assert_lowers_to(
        r#"
        const add_res = @evm_add(10, 7);
        const mul_res = @evm_mul(3, 4);
        const sub_res = @evm_sub(10, 3);
        const div_res = @evm_div(10, 3);
        const mod_res = @evm_mod(10, 3);
        const sdiv_res = @evm_sdiv(10, 3);
        const smod_res = @evm_smod(10, 3);
        const exp_res = @evm_exp(2, 10);
        const div_zero = @evm_div(5, 0);
        const signext_res = @evm_signextend(0, 0x7F);
        const and_res = @evm_and(0xFF, 0x0F);
        const or_res = @evm_or(0xF0, 0x0F);
        const xor_res = @evm_xor(0xFF, 0x0F);
        const byte_res = @evm_byte(31, 0x42);
        const shl_res = @evm_shl(4, 1);
        const shr_res = @evm_shr(1, 16);
        const sar_res = @evm_sar(1, 8);
        const lt_res = @evm_lt(3, 5);
        const gt_res = @evm_gt(5, 3);
        const slt_res = @evm_slt(3, 5);
        const sgt_res = @evm_sgt(5, 3);
        const eq_res = @evm_eq(5, 5);
        const iszero_t = @evm_iszero(0);
        const iszero_f = @evm_iszero(1);
        const addmod_res = @evm_addmod(5, 7, 10);
        const mulmod_res = @evm_mulmod(3, 4, 5);
        init {
            let mut a: u256 = add_res;
            let mut b: u256 = mul_res;
            let mut c: u256 = sub_res;
            let mut d: u256 = div_res;
            let mut e: u256 = mod_res;
            let mut f: u256 = sdiv_res;
            let mut g: u256 = smod_res;
            let mut h: u256 = exp_res;
            let mut i: u256 = div_zero;
            let mut j: u256 = signext_res;
            let mut k: u256 = and_res;
            let mut l: u256 = or_res;
            let mut m: u256 = xor_res;
            let mut n: u256 = byte_res;
            let mut o: u256 = shl_res;
            let mut p: u256 = shr_res;
            let mut q: u256 = sar_res;
            let mut r: bool = lt_res;
            let mut s: bool = gt_res;
            let mut t: bool = slt_res;
            let mut u: bool = sgt_res;
            let mut v: bool = eq_res;
            let mut w: bool = iszero_t;
            let mut x: bool = iszero_f;
            let mut y: u256 = addmod_res;
            let mut z: u256 = mulmod_res;
            @evm_stop();
        }
        "#,
        r#"
        ==== Functions ====
        ; init
        @fn0() -> never {
            %0 : u256 = 17
            %1 : u256 = 12
            %2 : u256 = 7
            %3 : u256 = 3
            %4 : u256 = 1
            %5 : u256 = 3
            %6 : u256 = 1
            %7 : u256 = 1024
            %8 : u256 = 0
            %9 : u256 = 127
            %10 : u256 = 15
            %11 : u256 = 255
            %12 : u256 = 240
            %13 : u256 = 66
            %14 : u256 = 16
            %15 : u256 = 8
            %16 : u256 = 4
            %17 : bool = true
            %18 : bool = true
            %19 : bool = true
            %20 : bool = true
            %21 : bool = true
            %22 : bool = true
            %23 : bool = false
            %24 : u256 = 2
            %25 : u256 = 2
            %26 : never = @evm_stop()
        }
        "#,
    );
}

#[test]
fn test_comptime_evm_const_chain() {
    assert_lowers_to(
        r#"
        const a = @evm_add(5, 10);
        const b = @evm_mul(a, 3);
        init {
            let mut x: u256 = b;
            @evm_stop();
        }
        "#,
        r#"
        ==== Functions ====
        ; init
        @fn0() -> never {
            %0 : u256 = 45
            %1 : never = @evm_stop()
        }
        "#,
    );
}

#[test]
fn test_comptime_unsupported_evm_builtin() {
    assert_diagnostics(
        r#"
        const x = @evm_caller();
        init { @evm_stop(); }
        "#,
        &[r#"
        error: builtin not supported at compile time
         --> main.plk:1:11
          |
        1 | const x = @evm_caller();
          |           ^^^^^^^^^^^^^ `@evm_caller` cannot be evaluated at compile time
        "#],
    );
}

#[test]
fn test_compile_error_builtin() {
    assert_diagnostics(
        r#"
        const x = @compile_error("custom failure");
        init { @evm_stop(); }
        "#,
        &[r#"
        error: custom failure
         --> main.plk:1:11
          |
        1 | const x = @compile_error("custom failure");
          |           ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ custom compile error triggered here
        "#],
    );
}

#[test]
fn test_compile_error_escaped_message() {
    assert_diagnostics(
        r#"
        const x = @compile_error("quote: \" slash: \\");
        init { @evm_stop(); }
        "#,
        &[r#"
        error: quote: " slash: \
         --> main.plk:1:11
          |
        1 | const x = @compile_error("quote: \" slash: \\");
          |           ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ custom compile error triggered here
        "#],
    );
}

#[test]
fn test_compile_error_accepts_cbytes_const() {
    assert_diagnostics(
        r#"
        const msg = "from const";
        const x = @compile_error(msg);
        init { @evm_stop(); }
        "#,
        &[r#"
        error: from const
         --> main.plk:2:11
          |
        2 | const x = @compile_error(msg);
          |           ^^^^^^^^^^^^^^^^^^^ custom compile error triggered here
        "#],
    );
}

#[test]
fn test_compile_error_accepts_cbytes_let() {
    assert_diagnostics(
        r#"
        init {
            let msg = "from let";
            @compile_error(msg);
        }
        "#,
        &[r#"
        error: from let
         --> main.plk:3:5
          |
        3 |     @compile_error(msg);
          |     ^^^^^^^^^^^^^^^^^^^ custom compile error triggered here
        "#],
    );
}

#[test]
fn test_compile_error_accepts_hex_cbytes() {
    assert_diagnostics(
        r#"
        const x = @compile_error(hex"686578206661696c757265");
        init { @evm_stop(); }
        "#,
        &[r#"
        error: hex failure
         --> main.plk:1:11
          |
        1 | const x = @compile_error(hex"686578206661696c757265");
          |           ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ custom compile error triggered here
        "#],
    );
}

#[test]
fn test_compile_error_accepts_non_utf8_cbytes() {
    assert_diagnostics(
        r#"
        const x = @compile_error(hex"ff");
        init { @evm_stop(); }
        "#,
        &[r#"
        error: �
         --> main.plk:1:11
          |
        1 | const x = @compile_error(hex"ff");
          |           ^^^^^^^^^^^^^^^^^^^^^^^ custom compile error triggered here
        "#],
    );
}

#[test]
fn test_runtime_cbytes() {
    assert_diagnostics(
        r#"
        init {
            let mut x = "";
            @evm_stop();
        }
        "#,
        &[r#"
        error: use of comptime-only value at runtime
         --> main.plk:2:17
          |
        2 |     let mut x = "";
          |                 ^^ reference to comptime-only value
        "#],
    );
}

#[test]
fn test_compile_error_requires_string_literal() {
    assert_diagnostics(
        r#"
        const x = @compile_error(1);
        init { @evm_stop(); }
        "#,
        &[r#"
        error: no valid match for builtin signature
         --> main.plk:1:11
          |
        1 | const x = @compile_error(1);
          |           ^^^^^^^^^^^^^^^^^ `@compile_error` cannot be called with (u256)
          |
          = note: `@compile_error` accepts (cbytes)
        "#],
    );
}

#[test]
fn test_comptime_cbytes_literals() {
    assert_lowers_to(
        r#"
        const same = "hello" == "hello";
        const different = "hello" != "world";
        const empty = "" == "";
        const empty_different = "" != "x";
        const escaped = "\x5cq" == "\\q";
        const hex_equal = "abc" == hex"616263";
        const hex_different = "abc" != hex"616264";
        const arbitrary_bytes = hex"00ff" == hex"00ff";
        init {
            let mut a: bool = same;
            let mut b: bool = different;
            let mut c: bool = empty;
            let mut d: bool = empty_different;
            let mut e: bool = escaped;
            let mut f: bool = hex_equal;
            let mut g: bool = hex_different;
            let mut h: bool = arbitrary_bytes;
            @evm_stop();
        }
        "#,
        r#"
        ==== Functions ====
        ; init
        @fn0() -> never {
            %0 : bool = true
            %1 : bool = true
            %2 : bool = true
            %3 : bool = true
            %4 : bool = true
            %5 : bool = true
            %6 : bool = true
            %7 : bool = true
            %8 : never = @evm_stop()
        }
        "#,
    );
}

#[test]
fn test_comptime_evm_wrong_arg_type_in_const() {
    assert_diagnostics(
        r#"
        const y = @evm_mul(true, 5);
        init { @evm_stop(); }
        "#,
        &[r#"
        error: no valid match for builtin signature
         --> main.plk:1:11
          |
        1 | const y = @evm_mul(true, 5);
          |           ^^^^^^^^^^^^^^^^^ `@evm_mul` cannot be called with (bool, u256)
          |
          = note: `@evm_mul` accepts (u256, u256)
        "#],
    );
}

#[test]
fn test_comptime_block_multi_statement() {
    assert_lowers_to(
        r#"
        init {
            let y = 15;
            let mut x: u256 = comptime {
                let mut a = 10;
                let b = 20;
                a = y;
                a
            };
            @evm_stop();
        }
        "#,
        r#"
        ==== Functions ====
        ; init
        @fn0() -> never {
            %0 : u256 = 15
            %1 : never = @evm_stop()
        }
        "#,
    );
}

#[test]
fn test_comptime_block_with_const_ref() {
    assert_lowers_to(
        r#"
        const N = 42;
        init {
            let mut x: u256 = comptime { N };
            @evm_stop();
        }
        "#,
        r#"
        ==== Functions ====
        ; init
        @fn0() -> never {
            %0 : u256 = 42
            %1 : never = @evm_stop()
        }
        "#,
    );
}

#[test]
fn test_out_of_order_const_ref() {
    assert_lowers_to(
        r#"
        const B = comptime { A };
        const A = 34;
        init {
            let mut x: u256 = comptime { B };
            @evm_stop();
        }
        "#,
        r#"
        ==== Functions ====
        ; init
        @fn0() -> never {
            %0 : u256 = 34
            %1 : never = @evm_stop()
        }
        "#,
    );
}

#[test]
fn test_comptime_block_nested_const() {
    assert_lowers_to(
        r#"
        const A = 10;
        const B = comptime { A };
        init {
            let mut x: u256 = comptime { B };
            @evm_stop();
        }
        "#,
        r#"
        ==== Functions ====
        ; init
        @fn0() -> never {
            %0 : u256 = 10
            %1 : never = @evm_stop()
        }
        "#,
    );
}

#[test]
fn test_comptime_block_struct_type() {
    assert_lowers_to(
        r#"
        init {
            let T = comptime {
                struct { x: u256 }
            };
            let mut val = T { x: 42 };
            @evm_stop();
        }
        "#,
        r#"
        ==== Functions ====
        ; init
        @fn0() -> never {
            %0 : struct@main.plk:3:9 = struct@main.plk:3:9 {
                42,
            }
            %1 : never = @evm_stop()
        }
        "#,
    );
}

#[test]
fn test_comptime_block_runtime_capture() {
    assert_diagnostics(
        r#"
        init {
            let x = @evm_calldataload(0);
            let y = comptime { x };
            @evm_stop();
        }
        "#,
        &[r#"
        error: attempting to evaluate runtime expression in comptime context
         --> main.plk:3:24
          |
        3 |     let y = comptime { x };
          |                        ^ runtime expression
        "#],
    );
}

#[test]
fn test_comptime_expr_runtime_dep() {
    assert_diagnostics(
        r#"
        init {
            let cond = @evm_iszero(@evm_calldataload(0));
            let T = if cond { u256 } else { bool };
            @evm_stop();
        }
        "#,
        &[
            r#"
        error: use of comptime-only value at runtime
         --> main.plk:3:23
          |
        3 |     let T = if cond { u256 } else { bool };
          |                       ^^^^ reference to comptime-only value
        "#,
            r#"
        error: use of comptime-only value at runtime
         --> main.plk:3:37
          |
        3 |     let T = if cond { u256 } else { bool };
          |                                     ^^^^ reference to comptime-only value
        "#,
        ],
    );
}

#[test]
fn test_comptime_recursion() {
    assert_lowers_to(
        r#"
        const fib_inner = fn (n: u256, a: u256, b: u256) u256 {
            if @evm_iszero(n) {
                return a;
            }
            fib_inner(@evm_sub(n, 1), b, @evm_add(a, b))
        };
        const fib = fn (n: u256) u256 {
            fib_inner(n, 0, 1)
        };

        init {
            let mut f0 = comptime { fib(0) };
            let mut f1 = comptime { fib(1) };
            let mut f10 = comptime { fib(10) };
            let mut f10 = comptime { fib(11) };
            @evm_stop();
        }
        "#,
        r#"
        ==== Functions ====
        ; init
        @fn0() -> never {
            %0 : u256 = 0
            %1 : u256 = 1
            %2 : u256 = 55
            %3 : u256 = 89
            %4 : never = @evm_stop()
        }
        "#,
    );
}

#[test]
fn test_comptime_block_type_result() {
    assert_lowers_to(
        r#"
        init {
            let mut x: comptime { u256 } = 5;
            @evm_stop();
        }
        "#,
        r#"
        ==== Functions ====
        ; init
        @fn0() -> never {
            %0 : u256 = 5
            %1 : never = @evm_stop()
        }
        "#,
    );
}

#[test]
fn test_comptime_param_type_not_type() {
    assert_diagnostics(
        r#"
        const forty_two = 42;
        const f = fn(x: forty_two) u256 { return x; };
        const r = f(1);
        init { @evm_stop(); }
        "#,
        &[r#"
        error: value used as type
         --> main.plk:2:17
          |
        1 | const forty_two = 42;
          | --------------------- defined here
        2 | const f = fn(x: forty_two) u256 { return x; };
          |                 ^^^^^^^^^ expected type, got value of type `u256`
          |
        note: called here
         --> main.plk:3:11
          |
        3 | const r = f(1);
          |           ^^^^
        "#],
    );
}

#[test]
fn test_const_self_cycle() {
    assert_diagnostics(
        r#"
        const A = {
            let x = 67;
            A
        };

        init { @evm_stop(); }
        "#,
        &[r#"
        error: cycle in constant evaluation
         --> main.plk:1:1
          |
        1 | / const A = {
        2 | |     let x = 67;
        3 | |     A
        4 | | };
          | |__^ `A` depends on itself
        "#],
    );
}

#[test]
fn test_const_mutual_cycle() {
    assert_diagnostics(
        r#"
           const A = B;
           const B = A;
           init { @evm_stop(); }
           "#,
        &[r#"
        error: cycle in constant evaluation
         --> main.plk:1:1
          |
        1 | const A = B;
          | ^^^^^^^^^^^^ `A` depends on itself
        "#],
    );
}

#[test]
fn test_const_with_type_error_does_not_panic() {
    assert_diagnostics(
        r#"
        const x = {
            let a: bool = 5;
            a
        };
        init { @evm_stop(); }
        "#,
        &[r#"
        error: mismatched types
         --> main.plk:2:19
          |
        2 |     let a: bool = 5;
          |            ----   ^ expected `bool`, got `u256`
          |            |
          |            `bool` expected because of this
        "#],
    );
}

#[test]
fn test_const_with_poisoned_control_flow() {
    assert_diagnostics(
        r#"
        const x = {
            if 34 { 1 } else { 2 }
        };
        init { @evm_stop(); }
        "#,
        &[r#"
        error: mismatched types
         --> main.plk:2:8
          |
        2 |     if 34 { 1 } else { 2 }
          |        ^^ expected `bool`, got `u256`
        "#],
    );
}

#[test]
fn test_comptime_params_monomorphize_uniquely_at_runtime() {
    assert_lowers_to(
        r#"
        const Gen = fn (comptime T: type) type {
            struct {
                inner: T,
                len: u256
            }
        };

        const get_len = fn (comptime T: type, arr: Gen(T)) u256 {
            arr.len
        };

        init {
            let mut x = get_len(u256, comptime { Gen(u256) } {
                inner: 0,
                len: 34
            });
            let mut y = get_len(bool, comptime { Gen(bool) } {
                inner: false,
                len: 33
            });
            @evm_stop();
        }
        "#,
        r#"

        ==== Functions ====
        @fn0(%0: Gen(u256)) -> u256 {
            %1 : Gen(u256) = %0
            %2 : u256 = %1.1
            ret %2
        }

        @fn1(%0: Gen(bool)) -> u256 {
            %1 : Gen(bool) = %0
            %2 : u256 = %1.1
            ret %2
        }

        ; init
        @fn2() -> never {
            %0 : Gen(u256) = Gen(u256) {
                0,
                34,
            }
            %1 : u256 = call @fn0(%0)
            %2 : Gen(bool) = Gen(bool) {
                false,
                33,
            }
            %3 : u256 = call @fn1(%2)
            %4 : never = @evm_stop()
        }
        "#,
    );
}

#[test]
fn comptime_arg_in_runtime_does_not_monomorphize() {
    assert_lowers_to(
        r#"
        const meta_add = fn (x: u256, y: u256) u256 {
            @evm_add(x, y)
        };

        init {
            let mut x = 3;
            let mut y = 4;
            let z1 = meta_add(x, y);
            let z2 = meta_add(3, y);
            @evm_stop();
        }
        "#,
        r#"
        ==== Functions ====
        @fn0(%0: u256, %1: u256) -> u256 {
            %2 : u256 = %0
            %3 : u256 = %1
            %4 : u256 = @evm_add(%2, %3)
            ret %4
        }

        ; init
        @fn1() -> never {
            %0 : u256 = 3
            %1 : u256 = 4
            %2 : u256 = %0
            %3 : u256 = %1
            %4 : u256 = call @fn0(%2, %3)
            %5 : u256 = %1
            %6 : u256 = 3
            %7 : u256 = call @fn0(%6, %5)
            %8 : never = @evm_stop()
        }
        "#,
    );
}

#[test]
fn comptime_any_parameter() {
    assert_lowers_to(
        r#"
        const meta_mul = fn (comptime x: $T, comptime y: T) T {
            if T == bool {
                x and y
            } else if T == u256 {
                @evm_mul(x, y)
            }
        };

        init {
            let mut x = comptime { meta_mul(true, true) };
            let mut x = comptime { meta_mul(true, false) };
            let mut x = comptime { meta_mul(3, 21) };
            @evm_stop();
        }
        "#,
        r#"
        ==== Functions ====
        ; init
        @fn0() -> never {
            %0 : bool = true
            %1 : bool = false
            %2 : u256 = 63
            %3 : never = @evm_stop()
        }
        "#,
    );
}

#[test]
fn test_any_type_params_monomorphize_uniquely_at_runtime() {
    assert_lowers_to(
        r#"
        const Gen = fn (comptime T: type) type {
            struct {
                inner: T,
                len: u256
            }
        };

        const get_len = fn (arr: $T) @field_type(T, 0) {
            arr.inner
        };

        const captured_type = fn(value: $T) type { T };
        const same_type = fn(x: $T, y: T) T { y };
        const comptime_value = fn(comptime value: $T) T { value };

        init {
            let mut x = get_len(comptime { Gen(u256) } {
                inner: 0,
                len: 34
            });
            let mut y = get_len(comptime { Gen(bool) } {
                inner: false,
                len: 33
            });
            let mut a: captured_type(1) = 2;
            let mut b = same_type(3, 4);
            let mut c = comptime_value(5);
            @evm_stop();
        }
        "#,
        r#"

        ==== Functions ====
        @fn0(%0: Gen(u256)) -> u256 {
            %1 : Gen(u256) = %0
            %2 : u256 = %1.0
            ret %2
        }

        @fn1(%0: Gen(bool)) -> bool {
            %1 : Gen(bool) = %0
            %2 : bool = %1.0
            ret %2
        }

        @fn2(%0: u256, %1: u256) -> u256 {
            %2 : u256 = %1
            ret %2
        }

        @fn3() -> u256 {
            %0 : u256 = 5
            ret %0
        }

        ; init
        @fn4() -> never {
            %0 : Gen(u256) = Gen(u256) {
                0,
                34,
            }
            %1 : u256 = call @fn0(%0)
            %2 : Gen(bool) = Gen(bool) {
                false,
                33,
            }
            %3 : bool = call @fn1(%2)
            %4 : u256 = 2
            %5 : u256 = 3
            %6 : u256 = 4
            %7 : u256 = call @fn2(%5, %6)
            %8 : u256 = call @fn3()
            %9 : never = @evm_stop()
        }
        "#,
    );
}

#[test]
fn test_poisoned_any_type_arg_does_not_panic() {
    assert_diagnostics(
        r#"
        const f = fn (value: $T) void {};

        init {
            f(missing);
            @evm_stop();
        }
        "#,
        &[r#"
        error: unresolved identifier 'missing'
         --> main.plk:4:7
          |
        4 |     f(missing);
          |       ^^^^^^^ not found in this scope
        "#],
    );
}

#[test]
fn test_any_type_capture_used_by_later_param_type_mismatch() {
    assert_diagnostics(
        r#"
        const f = fn (x: $T, y: T) T { y };

        init {
            f(1, false);
            @evm_stop();
        }
        "#,
        &[r#"
        error: mismatched types
         --> main.plk:4:10
          |
        1 | const f = fn (x: $T, y: T) T { y };
          |                         - `u256` expected because of this
        ...
        4 |     f(1, false);
          |          ^^^^^ expected `u256`, got `bool`
        "#],
    );
}

#[test]
fn test_basic_polymorphic_function() {
    assert_lowers_to(
        r#"
        const max = fn (comptime T: type, a: T, b: T) T {
            if T == u256 {
                return if @evm_gt(a, b) { a } else { b };
            }
            if T == bool {
                return a or b;
            }
            let _error: void = true;
        };

        init {
            let x = @evm_calldataload(0x00);
            let y = @evm_calldataload(0x20);
            let mut max_xy = max(u256, x, y);

            let a = false;
            let b = false;
            let mut max_ab = max(bool, a, b);

            @evm_stop();
        }
        "#,
        r#"
        ==== Functions ====
        @fn0(%0: u256, %1: u256) -> u256 {
            %2 : u256 = %0
            %3 : u256 = %1
            %4 : bool = @evm_gt(%2, %3)
            if %4 {
                %5 : u256 = %0
            } else {
                %5 : u256 = %1
            }
            %6 : u256 = %5
            ret %6
        }

        @fn1(%0: bool, %1: bool) -> bool {
            %2 : bool = %0
            if %2 {
                %3 : bool = true
            } else {
                %3 : bool = %1
            }
            %4 : bool = %3
            ret %4
        }

        ; init
        @fn2() -> never {
            %0 : u256 = 0
            %1 : u256 = @evm_calldataload(%0)
            %2 : u256 = 32
            %3 : u256 = @evm_calldataload(%2)
            %4 : u256 = %1
            %5 : u256 = %3
            %6 : u256 = call @fn0(%4, %5)
            %7 : bool = false
            %8 : bool = false
            %9 : bool = call @fn1(%7, %8)
            %10 : never = @evm_stop()
        }
        "#,
    );
}

#[test]
fn test_comptime_param_not_eager() {
    assert_diagnostics(
        r#"
        const ident = fn (x: u256) u256 { x };

        const my_add = fn (comptime N: u256, x: u256) u256 {
            @evm_add(N, x)
        };

        init {
            let mut x = my_add(ident(4), 4);

            @evm_stop();
        }
        "#,
        &[r#"
        error: attempted to pass runtime value as comptime parameter
         --> main.plk:8:24
          |
        3 | const my_add = fn (comptime N: u256, x: u256) u256 {
          |                    ---------------- parameter defined as comptime here
        ...
        8 |     let mut x = my_add(ident(4), 4);
          |                        ^^^^^^^^ runtime argument defined here
          |
        help: you can force compile time evaluation with a `comptime` block
          |
        8 |     let mut x = my_add(comptime { ident(4) }, 4);
          |                        ++++++++++          +
          = note: this only works if the expression is not fundamentally runtime
        "#],
    );
}

#[test]
fn test_comptime_call_comptime_param_runtime() {
    assert_diagnostics(
        r#"
        const my_add = fn (comptime N: u256, x: u256) u256 {
            @evm_add(N, x)
        };

        init {
            let mut x = 3;
            let mut y = comptime {
                my_add(x, 4)
            };

            @evm_stop();
        }
        "#,
        &[r#"
        error: attempting to evaluate runtime expression in comptime context
         --> main.plk:8:16
          |
        8 |         my_add(x, 4)
          |                ^ runtime expression
        "#],
    );
}

#[test]
fn test_comptime_infinite_recursion_diagnostic() {
    assert_diagnostics(
        r#"
        const bomb = fn (x: u256) u256 { bomb(x) };

        init {
            comptime {
                bomb(67_67);
            }


            @evm_stop();
        }
        "#,
        &[r#"
        error: infinite comptime recursion detected
         --> main.plk:1:34
          |
        1 | const bomb = fn (x: u256) u256 { bomb(x) };
          |                                  ^^^^^^^ call that recurses with identical arguments
        "#],
    );
}

#[test]
fn test_comptime_is_struct_expects_type() {
    assert_diagnostics(
        r#"
        const x = @is_struct(42);
        init { @evm_stop(); }
        "#,
        &[r#"
        error: expected type argument
         --> main.plk:1:11
          |
        1 | const x = @is_struct(42);
          |           ^^^^^^^^^^^^^^ `@is_struct` expects a type argument, got a value of type `u256`
        "#],
    );
}

#[test]
fn test_comptime_is_struct() {
    assert_lowers_to(
        r#"
        const Pair = struct { a: u256, b: bool };
        const yes = @is_struct(Pair);
        const no = @is_struct(u256);
        const tuple_no = @is_struct(tuple { u256, bool });
        init {
            let mut x: bool = yes;
            let mut y: bool = no;
            let mut z: bool = tuple_no;
            @evm_stop();
        }
        "#,
        r#"
        ==== Functions ====
        ; init
        @fn0() -> never {
            %0 : bool = true
            %1 : bool = false
            %2 : bool = false
            %3 : never = @evm_stop()
        }
        "#,
    );
}

#[test]
fn test_comptime_is_tuple_expects_type() {
    assert_diagnostics(
        r#"
        const x = @is_tuple(42);
        init { @evm_stop(); }
        "#,
        &[r#"
        error: expected type argument
         --> main.plk:1:11
          |
        1 | const x = @is_tuple(42);
          |           ^^^^^^^^^^^^^ `@is_tuple` expects a type argument, got a value of type `u256`
        "#],
    );
}

#[test]
fn test_comptime_is_tuple() {
    assert_lowers_to(
        r#"
        const Pair = struct { a: u256, b: bool };
        const yes = @is_tuple(tuple { u256, bool });
        const struct_no = @is_tuple(Pair);
        const primitive_no = @is_tuple(u256);
        init {
            let mut x: bool = yes;
            let mut y: bool = struct_no;
            let mut z: bool = primitive_no;
            @evm_stop();
        }
        "#,
        r#"
        ==== Functions ====
        ; init
        @fn0() -> never {
            %0 : bool = true
            %1 : bool = false
            %2 : bool = false
            %3 : never = @evm_stop()
        }
        "#,
    );
}

#[test]
fn test_comptime_type_index_expects_type() {
    assert_diagnostics(
        r#"
        const x = @type_index(42);
        init { @evm_stop(); }
        "#,
        &[r#"
        error: expected type argument
         --> main.plk:1:11
          |
        1 | const x = @type_index(42);
          |           ^^^^^^^^^^^^^^^ `@type_index` expects a type argument, got a value of type `u256`
        "#],
    );
}

#[test]
fn test_comptime_type_index_expects_struct() {
    assert_diagnostics(
        r#"
        const x = @type_index(u256);
        init { @evm_stop(); }
        "#,
        &[r#"
        error: unexpected type kind
         --> main.plk:1:11
          |
        1 | const x = @type_index(u256);
          |           ^^^^^^^^^^^^^^^^^ `@type_index` expects a struct type, got `u256`
        "#],
    );
}

#[test]
fn test_comptime_type_index() {
    assert_lowers_to(
        r#"
        const Numeric = struct 42 { a: u256 };
        const Flagged = struct true { a: u256 };
        const numeric = @type_index(Numeric);
        const flagged = @type_index(Flagged);
        init {
            let mut x: u256 = numeric;
            let mut y: bool = flagged;
            @evm_stop();
        }
        "#,
        r#"
        ==== Functions ====
        ; init
        @fn0() -> never {
            %0 : u256 = 42
            %1 : bool = true
            %2 : never = @evm_stop()
        }
        "#,
    );
}

#[test]
fn test_comptime_type_index_default_is_void() {
    assert_lowers_to(
        r#"
        const Default = struct { a: u256 };
        const index = @type_index(Default);
        const check: void = index;
        init { @evm_stop(); }
        "#,
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
fn test_comptime_field_count_expects_type() {
    assert_diagnostics(
        r#"
        const x = @field_count(true);
        init { @evm_stop(); }
        "#,
        &[r#"
        error: expected type argument
         --> main.plk:1:11
          |
        1 | const x = @field_count(true);
          |           ^^^^^^^^^^^^^^^^^^ `@field_count` expects a type argument, got a value of type `bool`
        "#],
    );
}

#[test]
fn test_comptime_field_count_expects_struct() {
    assert_diagnostics(
        r#"
        const x = @field_count(u256);
        init { @evm_stop(); }
        "#,
        &[r#"
        error: unexpected type kind
         --> main.plk:1:11
          |
        1 | const x = @field_count(u256);
          |           ^^^^^^^^^^^^^^^^^^ `@field_count` expects a struct or tuple type, got `u256`
        "#],
    );
}

#[test]
fn test_comptime_field_count() {
    assert_lowers_to(
        r#"
        const Triple = struct { a: u256, b: bool, c: u256 };
        const count = @field_count(Triple);
        init {
            let mut x: u256 = count;
            @evm_stop();
        }
        "#,
        r#"
        ==== Functions ====
        ; init
        @fn0() -> never {
            %0 : u256 = 3
            %1 : never = @evm_stop()
        }
        "#,
    );
}

#[test]
fn test_comptime_field_type() {
    assert_lowers_to(
        r#"
        const Pair = struct { a: u256, b: bool };
        const T0 = @field_type(Pair, 0);
        const T1 = @field_type(Pair, 1);
        init {
            let mut x: T0 = 42;
            let mut y: T1 = true;
            @evm_stop();
        }
        "#,
        r#"
        ==== Functions ====
        ; init
        @fn0() -> never {
            %0 : u256 = 42
            %1 : bool = true
            %2 : never = @evm_stop()
        }
        "#,
    );
}

#[test]
fn test_comptime_field_type_expects_struct() {
    assert_diagnostics(
        r#"
        const T = @field_type(u256, 0);
        init { @evm_stop(); }
        "#,
        &[r#"
        error: unexpected type kind
         --> main.plk:1:11
          |
        1 | const T = @field_type(u256, 0);
          |           ^^^^^^^^^^^^^^^^^^^^ `@field_type` expects a struct or tuple type, got `u256`
        "#],
    );
}

#[test]
fn test_comptime_get_field() {
    assert_lowers_to(
        r#"
        const Pair = struct { a: u256, b: bool };
        const p = Pair { a: 42, b: true };
        const val = @get_field(p, 0);
        const val_by_name = @get_field(p, "a");
        init {
            let mut x = val;
            let mut y = val_by_name;
            @evm_stop();
        }
        "#,
        r#"
        ==== Functions ====
        ; init
        @fn0() -> never {
            %0 : u256 = 42
            %1 : u256 = 42
            %2 : never = @evm_stop()
        }
        "#,
    );
}

#[test]
fn test_runtime_get_field() {
    assert_lowers_to(
        r#"
        const Pair = struct { a: u256, b: u256 };
        init {
            let s = Pair { a: @evm_calldataload(0), b: @evm_calldataload(0x20) };
            let val = @get_field(s, 1);
            let val_by_name = @get_field(s, "b");
            let mut x: u256 = val;
            let mut y: u256 = val_by_name;
            @evm_stop();
        }
        "#,
        r#"
        ==== Functions ====
        ; init
        @fn0() -> never {
            %0 : u256 = 0
            %1 : u256 = @evm_calldataload(%0)
            %2 : u256 = 32
            %3 : u256 = @evm_calldataload(%2)
            %4 : Pair = Pair { %1, %3 }
            %5 : Pair = %4
            %6 : u256 = %5.1
            %7 : Pair = %4
            %8 : u256 = %7.1
            %9 : u256 = %6
            %10 : u256 = %8
            %11 : never = @evm_stop()
        }
        "#,
    );
}

#[test]
fn test_comptime_get_field_out_of_bounds() {
    assert_diagnostics(
        r#"
        const S = struct { a: u256 };
        const s = S { a: 1 };
        const val = @get_field(s, 3);
        init { @evm_stop(); }
        "#,
        &[r#"
        error: field index out of bounds
         --> main.plk:3:27
          |
        3 | const val = @get_field(s, 3);
          |                           ^ `@get_field`: field index 3 is out of bounds for type with 1 field
        "#],
    );
}

#[test]
fn test_comptime_get_field_index_overflow() {
    assert_diagnostics(
        r#"
        const S = struct { a: u256 };
        const s = S { a: 1 };
        const val = @get_field(s, 0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF);
        init { @evm_stop(); }
        "#,
        &[r#"
        error: field index out of bounds
         --> main.plk:3:27
          |
        3 | const val = @get_field(s, 0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF);
          |                           ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ `@get_field`: field index 115792089237316195423570985008687907853269984665640564039457584007913129639935 is out of bounds for type with 1 field
        "#],
    );
}

#[test]
fn test_comptime_get_field_runtime_index() {
    assert_diagnostics(
        r#"
        const S = struct { a: u256 };
        init {
            let s = S { a: 1 };
            let val = @get_field(s, @evm_calldataload(0));
            @evm_stop();
        }
        "#,
        &[r#"
        error: expected comptime argument
         --> main.plk:4:15
          |
        4 |     let val = @get_field(s, @evm_calldataload(0));
          |               ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ `@get_field` requires field selector to be known at comptime
        "#],
    );
}

#[test]
fn test_get_field_non_struct_instance() {
    assert_diagnostics(
        r#"
        init {
            let x: u256 = @evm_calldataload(0);
            let val = @get_field(x, 0);
            @evm_stop();
        }
        "#,
        &[r#"
        error: unexpected type kind
         --> main.plk:3:15
          |
        3 |     let val = @get_field(x, 0);
          |               ^^^^^^^^^^^^^^^^ `@get_field` expects a struct or tuple type, got `u256`
        "#],
    );
}

#[test]
fn test_set_field_non_num_index() {
    assert_diagnostics(
        r#"
        const Pair = struct { a: u256, b: u256 };
        const p = Pair { a: 1, b: 2 };
        const p2 = @set_field(p, false, 99);
        const val = p2.a;
        init {
            let mut x: u256 = val;
            @evm_stop();
        }
        "#,
        &[r#"
        error: invalid field selector
         --> main.plk:3:26
          |
        3 | const p2 = @set_field(p, false, 99);
          |                          ^^^^^ `@set_field` field selector must be `u256` or `cbytes`, got `bool`
        "#],
    );
}

#[test]
fn test_comptime_set_field() {
    assert_lowers_to(
        r#"
        const Pair = struct { a: u256, b: u256 };
        const p = Pair { a: 1, b: 2 };
        const p2 = @set_field(p, 0, 99);
        const val = p2.a;
        const p3 = @set_field(p, "a", 99);
        const val_by_name = p3.a;
        init {
            let mut x: u256 = val;
            let mut y: u256 = val_by_name;
            @evm_stop();
        }
        "#,
        r#"
        ==== Functions ====
        ; init
        @fn0() -> never {
            %0 : u256 = 99
            %1 : u256 = 99
            %2 : never = @evm_stop()
        }
        "#,
    );
}

#[test]
fn test_runtime_set_field() {
    assert_lowers_to(
        r#"
        const Pair = struct { a: u256, b: u256 };
        init {
            let s = Pair { a: @evm_calldataload(0), b: @evm_calldataload(0x20) };
            let s2 = @set_field(s, 0, 99);
            let s3 = @set_field(s, "a", 99);
            let mut x: u256 = s2.a;
            let mut y: u256 = s3.a;
            @evm_stop();
        }
        "#,
        r#"
        ==== Functions ====
        ; init
        @fn0() -> never {
            %0 : u256 = 0
            %1 : u256 = @evm_calldataload(%0)
            %2 : u256 = 32
            %3 : u256 = @evm_calldataload(%2)
            %4 : Pair = Pair { %1, %3 }
            %5 : Pair = %4
            %6 : u256 = 99
            %7 : u256 = %5.1
            %8 : Pair = Pair { %6, %7 }
            %9 : Pair = %4
            %10 : u256 = 99
            %11 : u256 = %9.1
            %12 : Pair = Pair { %10, %11 }
            %13 : Pair = %8
            %14 : u256 = %13.0
            %15 : Pair = %12
            %16 : u256 = %15.0
            %17 : never = @evm_stop()
        }
        "#,
    );
}

#[test]
fn test_set_field_comptime_struct_runtime_value() {
    assert_lowers_to(
        r#"
        const Pair = struct { a: u256, b: u256 };
        const p = Pair { a: 1, b: 2 };
        init {
            let val: u256 = @evm_calldataload(0);
            let p2 = @set_field(p, 0, val);
            let mut x: u256 = p2.a;
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
            %3 : Pair = Pair {
                1,
                2,
            }
            %4 : u256 = %3.1
            %5 : Pair = Pair { %2, %4 }
            %6 : Pair = %5
            %7 : u256 = %6.0
            %8 : never = @evm_stop()
        }
        "#,
    );
}

#[test]
fn test_comptime_set_field_type_mismatch() {
    assert_diagnostics(
        r#"
        const Pair = struct { a: u256, b: bool };
        const p = Pair { a: 1, b: true };
        const p2 = @set_field(p, 1, 42);
        init { @evm_stop(); }
        "#,
        &[r#"
        error: mismatched types
         --> main.plk:3:29
          |
        1 | const Pair = struct { a: u256, b: bool };
          |                                ------- `bool` expected because of this
        2 | const p = Pair { a: 1, b: true };
        3 | const p2 = @set_field(p, 1, 42);
          |                             ^^ expected `bool`, got `u256`
        "#],
    );
}

#[test]
fn test_get_field_comptime_only_field_flows_to_comptime_use() {
    assert_lowers_to(
        r#"
        const Wrapper = struct { t: type, n: u256 };
        const w = Wrapper { t: u256, n: 7 };
        init {
            let t = @get_field(w, 0);
            let s = @is_struct(t);
            let mut x: bool = s;
            @evm_stop();
        }
        "#,
        r#"
        ==== Functions ====
        ; init
        @fn0() -> never {
            %0 : bool = false
            %1 : never = @evm_stop()
        }
        "#,
    );
}

#[test]
fn test_set_field_comptime_only_struct_runtime_value() {
    assert_diagnostics(
        r#"
        const Wrapper = struct { t: type, n: u256 };
        const w = Wrapper { t: u256, n: 7 };
        init {
            let v: u256 = @evm_calldataload(0);
            let w2 = @set_field(w, 1, v);
            @evm_stop();
        }
        "#,
        &[r#"
        error: mixing comptime and runtime data in compound type
         --> main.plk:5:31
          |
        1 | const Wrapper = struct { t: type, n: u256 };
          |                 --------------------------- `Wrapper` is a comptime-only type
        ...
        5 |     let w2 = @set_field(w, 1, v);
          |                               ^ this value is only known at runtime
        "#],
    );
}

#[test]
fn test_uninit_struct_runtime_set_field() {
    assert_lowers_to(
        r#"
        const Pair = struct { a: u256, b: u256 };
        const p = @uninit(Pair);
        init {
            let val: u256 = @evm_calldataload(0);
            let p2 = @set_field(p, 0, val);
            let mut a: u256 = p2.a;
            let mut b: u256 = p2.b;
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
            %3 : Pair = Pair {
                0,
                0,
            }
            %4 : u256 = %3.1
            %5 : Pair = Pair { %2, %4 }
            %6 : Pair = %5
            %7 : u256 = %6.0
            %8 : Pair = %5
            %9 : u256 = %8.1
            %10 : never = @evm_stop()
        }
        "#,
    );
}

#[test]
fn test_in_comptime_builtin() {
    assert_lowers_to(
        r#"
        const const_comptime = @in_comptime();

        const simple_func = fn () bool { @in_comptime() };

        init {
            let mut a = @in_comptime();
            let mut b = comptime { @in_comptime() };
            let mut c = comptime { simple_func() };
            let mut d = { simple_func() };

            @evm_stop();
        }
        "#,
        r#"
        ==== Functions ====
        @fn0() -> bool {
            %0 : bool = false
            ret %0
        }

        ; init
        @fn1() -> never {
            %0 : bool = false
            %1 : bool = true
            %2 : bool = true
            %3 : bool = call @fn0()
            %4 : never = @evm_stop()
        }
        "#,
    );
}

#[test]
fn test_uninit_invalid_type() {
    assert_diagnostics(
        r#"
        const x = @uninit(never);
        init { @evm_stop(); }
        "#,
        &[r#"
        error: cannot create uninitialized value
         --> main.plk:1:11
          |
        1 | const x = @uninit(never);
          |           ^^^^^^^^^^^^^^ type `never` cannot be uninitialized
          |
          = help: @uninit only supports types that do not contain never or function
        "#],
    );
}

#[test]
fn test_uninit_tuple() {
    assert_lowers_to(
        r#"
        const t = @uninit(tuple { u256, bool });

        init {
            let mut x = t;
            @evm_stop();
        }
        "#,
        r#"
        ==== Functions ====
        ; init
        @fn0() -> never {
            %0 : tuple {u256, bool} = tuple {u256, bool} {
                0,
                false,
            }
            %1 : never = @evm_stop()
        }
        "#,
    );
}

#[test]
fn test_uninit_type_spilled_to_runtime() {
    assert_diagnostics(
        r#"
        const t = @uninit(type);
        init {
            let mut x = t;
            @evm_stop();
        }
        "#,
        &[r#"
        error: use of comptime-only value at runtime
         --> main.plk:3:17
          |
        3 |     let mut x = t;
          |                 ^ reference to comptime-only value
        "#],
    );
}

#[test]
fn test_uninit_struct_with_function_field() {
    assert_diagnostics(
        r#"
        const Bad = struct { a: u256, b: function };
        const x = @uninit(Bad);
        init { @evm_stop(); }
        "#,
        &[r#"
        error: struct contains field that cannot be uninitialized
         --> main.plk:2:11
          |
        1 | const Bad = struct { a: u256, b: function };
          |                               ----------- type `function` cannot be uninitialized
        2 | const x = @uninit(Bad);
          |           ^^^^^^^^^^^^ cannot use @uninit on this struct
          |
          = help: @uninit only supports types that do not contain never or function
        "#],
    );
}

#[test]
fn test_uninit_struct_with_invalid_tuple_field() {
    assert_diagnostics(
        r#"
        const Bad = struct { a: u256, b: tuple { function } };
        const x = @uninit(Bad);
        init { @evm_stop(); }
        "#,
        &[r#"
        error: struct contains field that cannot be uninitialized
         --> main.plk:2:11
          |
        1 | const Bad = struct { a: u256, b: tuple { function } };
          |                               --------------------- type `tuple {function}` cannot be uninitialized
        2 | const x = @uninit(Bad);
          |           ^^^^^^^^^^^^ cannot use @uninit on this struct
          |
          = help: @uninit only supports types that do not contain never or function
        "#],
    );
}

#[test]
fn test_uninit_memptr_in_comptime() {
    assert_diagnostics(
        r#"
        const x = @uninit(memptr);
        init { @evm_stop(); }
        "#,
        &[r#"
        error: cannot use @uninit on memptr type at comptime
         --> main.plk:1:11
          |
        1 | const x = @uninit(memptr);
          |           ^^^^^^^^^^^^^^^ memptr requires runtime allocation
        "#],
    );
}

#[test]
fn test_uninit_type_direct_runtime_scope_is_comptime_value() {
    assert_lowers_to(
        r#"
        init {
            let x = @uninit(type);
            @evm_stop();
        }
        "#,
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
fn test_uninit_cbytes_direct_runtime_scope_is_comptime_value() {
    assert_lowers_to(
        r#"
        init {
            let x = @uninit(cbytes);
            @evm_stop();
        }
        "#,
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
fn test_uninit_struct_with_comptime_only_field_direct_runtime_scope_is_comptime_value() {
    assert_lowers_to(
        r#"
        const Wrapper = struct { t: type, n: u256 };
        init {
            let x = @uninit(Wrapper);
            @evm_stop();
        }
        "#,
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
fn test_uninit_struct_with_memptr_and_invalid_field_reports_invalid_field() {
    assert_diagnostics(
        r#"
        const Bad = struct { ptr: memptr, f: never };

        const x = @uninit(Bad);
        init { @evm_stop(); }
        "#,
        &[r#"
        error: struct contains field that cannot be uninitialized
         --> main.plk:3:11
          |
        1 | const Bad = struct { ptr: memptr, f: never };
          |                                   -------- type `never` cannot be uninitialized
        2 |
        3 | const x = @uninit(Bad);
          |           ^^^^^^^^^^^^ cannot use @uninit on this struct
          |
          = help: @uninit only supports types that do not contain never or function
        "#],
    );
}

#[test]
fn test_uninit_struct_reports_all_invalid_fields() {
    assert_diagnostics(
        r#"
        const Bad = struct { f: function, g: function };
        const x = @uninit(Bad);
        init { @evm_stop(); }
        "#,
        &[r#"
        error: struct contains field that cannot be uninitialized
         --> main.plk:2:11
          |
        1 | const Bad = struct { f: function, g: function };
          |                      ----------- type `function` cannot be uninitialized
        2 | const x = @uninit(Bad);
          |           ^^^^^^^^^^^^ cannot use @uninit on this struct
          |
          = help: @uninit only supports types that do not contain never or function
        "#],
    );
}

#[test]
fn test_merged_cbytes_literals() {
    assert_lowers_to(
        r#"
        const merged = "abc" "123" hex"01ab" == "abc123\x01\xab";
        init {
            let mut a: bool = merged;
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
fn test_cbytes_dot_length_attribute() {
    assert_lowers_to(
        r#"
        const len_plain = "hello".length;
        const len_empty = "".length;
        const len_escaped = "a\x00b\n".length;
        const len_merged = ("abc" "123" hex"01ab").length;
        init {
            let mut a: u256 = len_plain;
            let mut b: u256 = len_empty;
            let mut c: u256 = len_escaped;
            let mut d: u256 = len_merged;
            @evm_stop();
        }
        "#,
        r#"
        ==== Functions ====
        ; init
        @fn0() -> never {
            %0 : u256 = 5
            %1 : u256 = 0
            %2 : u256 = 4
            %3 : u256 = 8
            %4 : never = @evm_stop()
        }
        "#,
    );
}

#[test]
fn test_cbytes_unknown_attribute() {
    assert_diagnostics(
        r#"
        const bad = "hello".foo;
        init { @evm_stop(); }
        "#,
        &[r#"
        error: unknown cbytes attribute
         --> main.plk:1:13
          |
        1 | const bad = "hello".foo;
          |             ^^^^^^^^^^^ `cbytes` has no attribute `foo`
          |
          = help: available attribute: `.length`
        "#],
    );
}

#[test]
fn test_slice_cbytes_builtin() {
    assert_lowers_to(
        r#"
        const basic = @slice_cbytes("hello", 1, 3) == "el";
        const len_of_slice = @slice_cbytes("hello", 1, 4).length;
        const nested = @slice_cbytes(@slice_cbytes("hello", 1, 4), 1, 2) == "l";
        const full = @slice_cbytes("hello", 0, 5) == "hello";
        const empty = @slice_cbytes("hello", 2, 2) == "";
        init {
            let mut a: bool = basic;
            let mut b: u256 = len_of_slice;
            let mut c: bool = nested;
            let mut d: bool = full;
            let mut e: bool = empty;
            @evm_stop();
        }
        "#,
        r#"
        ==== Functions ====
        ; init
        @fn0() -> never {
            %0 : bool = true
            %1 : u256 = 3
            %2 : bool = true
            %3 : bool = true
            %4 : bool = true
            %5 : never = @evm_stop()
        }
        "#,
    );
}

#[test]
fn test_slice_cbytes_start_after_end() {
    assert_diagnostics(
        r#"
        const bad = @slice_cbytes("hello", 3, 1);
        init {
            @evm_stop();
        }
        "#,
        &[r#"
        error: bytes slice out of bounds
         --> main.plk:1:13
          |
        1 | const bad = @slice_cbytes("hello", 3, 1);
          |             ^^^^^^^^^^^^^^^^^^^^^^^^^^^^ requested range 3..1 of bytes with length 5
          |
          = note: requires `start <= end` and `end <= bytes.length`
        "#],
    );
}

#[test]
fn test_slice_cbytes_end_past_len() {
    assert_diagnostics(
        r#"
        const bad = @slice_cbytes("hello", 2, 6);
        init {
            @evm_stop();
        }
        "#,
        &[r#"
        error: bytes slice out of bounds
         --> main.plk:1:13
          |
        1 | const bad = @slice_cbytes("hello", 2, 6);
          |             ^^^^^^^^^^^^^^^^^^^^^^^^^^^^ requested range 2..6 of bytes with length 5
          |
          = note: requires `start <= end` and `end <= bytes.length`
        "#],
    );
}

#[test]
fn test_slice_cbytes_runtime_bound() {
    assert_diagnostics(
        r#"
        init {
            let n = @evm_calldataload(0);
            let s = @slice_cbytes("hello", n, 3);
            @evm_stop();
        }
        "#,
        &[r#"
        error: expected comptime argument
         --> main.plk:3:13
          |
        3 |     let s = @slice_cbytes("hello", n, 3);
          |             ^^^^^^^^^^^^^^^^^^^^^^^^^^^^ `@slice_cbytes` requires slice start to be known at comptime
        "#],
    );
}

#[test]
fn test_padded_read_cbytes_without_padding() {
    assert_lowers_to(
        r#"
        const read_matches = @padded_read_cbytes(
            hex"0000000000000000000000000000000000000000000000000000000000000000"
            hex"0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20"
            hex"ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff",
            33,
        ) == 0x02030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20ff;
        init {
            let mut a: bool = read_matches;
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
fn test_padded_read_cbytes_with_padding() {
    assert_lowers_to(
        r#"
        const read_matches = @padded_read_cbytes(hex"010203", 1)
            == 0x0203000000000000000000000000000000000000000000000000000000000000;
        init {
            let mut a: bool = read_matches;
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
fn test_padded_read_cbytes_offset_past_len() {
    assert_diagnostics(
        r#"
        const bad = @padded_read_cbytes("hi", 3);
        init {
            @evm_stop();
        }
        "#,
        &[r#"
        error: cbytes read offset out of bounds
         --> main.plk:1:13
          |
        1 | const bad = @padded_read_cbytes("hi", 3);
          |             ^^^^^^^^^^^^^^^^^^^^^^^^^^^^ offset 3 is outside `cbytes` with length 2
          |
          = note: offset must be within `0..=bytes.length`
        "#],
    );
}

#[test]
fn test_concat_cbytes_empty_tuple() {
    assert_lowers_to(
        r#"
        const concat_matches = @concat_cbytes(()) == "";
        init {
            let mut a: bool = concat_matches;
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
fn test_concat_cbytes_mixed_elements() {
    assert_lowers_to(
        r#"
        const concat_matches = @concat_cbytes(("a", 1, hex"ff"))
            == "a" hex"0000000000000000000000000000000000000000000000000000000000000001ff";
        init {
            let mut a: bool = concat_matches;
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
fn test_concat_cbytes_uses_visible_slice() {
    assert_lowers_to(
        r#"
        const concat_matches = @concat_cbytes((@slice_cbytes("hello", 1, 4), "!")) == "ell!";
        init {
            let mut a: bool = concat_matches;
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
fn test_concat_cbytes_requires_tuple() {
    assert_diagnostics(
        r#"
        const bad = @concat_cbytes("hello");
        init {
            @evm_stop();
        }
        "#,
        &[r#"
        error: invalid cbytes concat argument
         --> main.plk:1:13
          |
        1 | const bad = @concat_cbytes("hello");
          |             ^^^^^^^^^^^^^^^^^^^^^^^ `@concat_cbytes` expects a tuple, got `cbytes`
        "#],
    );
}

#[test]
fn test_concat_cbytes_rejects_invalid_element() {
    assert_diagnostics(
        r#"
        const bad = @concat_cbytes(("hello", true));
        init {
            @evm_stop();
        }
        "#,
        &[r#"
        error: invalid cbytes concat element
         --> main.plk:1:13
          |
        1 | const bad = @concat_cbytes(("hello", true));
          |             ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ `@concat_cbytes` tuple elements must be `u256` or `cbytes`, got `bool`
        "#],
    );
}

#[test]
fn test_concat_cbytes_rejects_runtime_tuple_element() {
    assert_diagnostics(
        r#"
        init {
            let n = @evm_calldataload(0);
            let bad = @concat_cbytes(("hello", n));
            @evm_stop();
        }
        "#,
        &[r#"
        error: mixing comptime and runtime data in tuple
         --> main.plk:3:30
          |
        3 |     let bad = @concat_cbytes(("hello", n));
          |                              ^-------^^-^
          |                              ||        |
          |                              ||        tuple element not comptime-known
          |                              |tuple element is comptime-only
          |                              mixed tuple literal
        "#],
    );
}

#[test]
fn test_data_offset_of_slice_cbytes() {
    assert_lowers_to(
        r#"
        init {
            let offset = @data_offset(@slice_cbytes("hello" hex"00ff", 2, 6));
            @evm_stop();
        }
        "#,
        r#"
        ==== Functions ====
        ; init
        @fn0() -> never {
            %0 : u256 = data_offset(hex"68656c6c6f00ff") + 2
            %1 : never = @evm_stop()
        }
        "#,
    );
}

#[test]
fn test_keccak256_cbytes_builtin() {
    assert_eq!(
        alloy_primitives::keccak256(b"abc"),
        alloy_primitives::b256!(
            "0x4e03657aea45a94fc7d47ba826c8d667c0d1e6e33a64a036ec44f58fa12d6c45"
        ),
    );
    assert_eq!(
        alloy_primitives::keccak256(b""),
        alloy_primitives::b256!(
            "0xc5d2460186f7233c927e7db2dcc703c0e500b653ca82273b7bfad8045d85a470"
        ),
    );
    assert_lowers_to(
        r#"
        const abc_hash_matches = @keccak256_cbytes("abc")
            == 0x4e03657aea45a94fc7d47ba826c8d667c0d1e6e33a64a036ec44f58fa12d6c45;
        const empty_hash_matches = @keccak256_cbytes("")
            == 0xc5d2460186f7233c927e7db2dcc703c0e500b653ca82273b7bfad8045d85a470;
        init {
            let mut a: bool = abc_hash_matches;
            let mut b: bool = empty_hash_matches;
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
fn test_keccak256_cbytes_of_slice() {
    assert_eq!(
        alloy_primitives::keccak256(b"ell"),
        alloy_primitives::b256!(
            "0x0dd666b403ddf2d5833ea7c8306cfc8d62ee1052f2da09d8c290aac4d3085b43"
        ),
    );
    assert_lowers_to(
        r#"
        const ell_hash_matches = @keccak256_cbytes(@slice_cbytes("hello", 1, 4))
            == 0x0dd666b403ddf2d5833ea7c8306cfc8d62ee1052f2da09d8c290aac4d3085b43;
        init {
            let mut a: bool = ell_hash_matches;
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
fn test_sha256_cbytes_builtin() {
    let abc_hash: [u8; 32] = sha2::Sha256::digest(b"abc").into();
    assert_eq!(
        abc_hash,
        alloy_primitives::b256!(
            "0xba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        )
        .0,
    );
    let empty_hash: [u8; 32] = sha2::Sha256::digest(b"").into();
    assert_eq!(
        empty_hash,
        alloy_primitives::b256!(
            "0xe3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        )
        .0,
    );
    assert_lowers_to(
        r#"
        const abc_hash_matches = @sha256_cbytes("abc")
            == 0xba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad;
        const empty_hash_matches = @sha256_cbytes("")
            == 0xe3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855;
        init {
            let mut a: bool = abc_hash_matches;
            let mut b: bool = empty_hash_matches;
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
fn test_sha256_cbytes_of_slice() {
    let ell_hash: [u8; 32] = sha2::Sha256::digest(b"ell").into();
    assert_eq!(
        ell_hash,
        alloy_primitives::b256!(
            "0xbaea96500997ff5cd6cfd26592a978d6b73d480b4ad33d002499cf0041ac9996"
        )
        .0,
    );
    assert_lowers_to(
        r#"
        const ell_hash_matches = @sha256_cbytes(@slice_cbytes("hello", 1, 4))
            == 0xbaea96500997ff5cd6cfd26592a978d6b73d480b4ad33d002499cf0041ac9996;
        init {
            let mut a: bool = ell_hash_matches;
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
fn test_data_offset_runtime() {
    assert_lowers_to(
        r#"
        init {
            let offset = @data_offset("hi" hex"00ff");
            @evm_stop();
        }
        "#,
        r#"
        ==== Functions ====
        ; init
        @fn0() -> never {
            %0 : u256 = data_offset(hex"686900ff")
            %1 : never = @evm_stop()
        }
        "#,
    );
}

#[test]
fn test_data_offset_in_comptime() {
    assert_diagnostics(
        r#"
        const offset = @data_offset("hello");
        init {
            @evm_stop();
        }
        "#,
        &[r#"
        error: builtin `@data_offset` not supported at compile time
         --> main.plk:1:16
          |
        1 | const offset = @data_offset("hello");
          |                ^^^^^^^^^^^^^^^^^^^^^ `@data_offset` produces a runtime-only value and cannot be evaluated at compile time
        "#],
    );
}

#[test]
fn test_set_eval_branch_quota_runtime_arg() {
    assert_diagnostics(
        r#"
        init {
            let n = @evm_calldataload(0);
            let x = comptime { @set_eval_branch_quota(n); 1 };
            @evm_stop();
        }
        "#,
        &[r#"
        error: attempting to evaluate runtime expression in comptime context
         --> main.plk:3:47
          |
        3 |     let x = comptime { @set_eval_branch_quota(n); 1 };
          |                                               ^ runtime expression
        "#],
    );
}

#[test]
fn test_set_eval_branch_quota_wrong_arg_type() {
    assert_diagnostics(
        r#"
        const x = comptime { @set_eval_branch_quota(true); 1 };
        init { @evm_stop(); }
        "#,
        &[r#"
        error: no valid match for builtin signature
         --> main.plk:1:22
          |
        1 | const x = comptime { @set_eval_branch_quota(true); 1 };
          |                      ^^^^^^^^^^^^^^^^^^^^^^^^^^^^ `@set_eval_branch_quota` cannot be called with (bool)
          |
          = note: `@set_eval_branch_quota` accepts (u256)
        "#],
    );
}

#[test]
fn test_set_eval_branch_quota_too_large() {
    assert_diagnostics(
        r#"
        const x = comptime { @set_eval_branch_quota(4294967296); 1 };
        init { @evm_stop(); }
        "#,
        &[r#"
        error: eval branch quota is too large
         --> main.plk:1:45
          |
        1 | const x = comptime { @set_eval_branch_quota(4294967296); 1 };
          |                                             ^^^^^^^^^^ quota must fit in u32
          |
          = note: maximum supported quota is 4294967295
        "#],
    );
}

#[test]
fn test_comptime_nested_while_and_if_evaluates_loop() {
    assert_lowers_to(
        std_project(
            r#"
        init {
            let mut x: u256 = comptime {
                let mut outer = 0;
                let mut total = 0;
                while outer < 3 {
                    let mut inner = 0;
                    while inner < 2 {
                        if inner == 0 {
                            let add = outer + 1;
                            total = total + add;
                        } else {
                            let add = outer + 2;
                            total = total + add;
                        }
                        inner = inner + 1;
                    }
                    outer = outer + 1;
                }
                total
            };
            @evm_stop();
        }
        "#,
        ),
        r#"
        ==== Functions ====
        ; init
        @fn0() -> never {
            %0 : u256 = 15
            %1 : never = @evm_stop()
        }
        "#,
    );
}

#[test]
fn test_nested_comptime_block_shares_caller_quota() {
    assert_eq!(1000, DEFAULT_COMPTIME_BRANCH_QUOTA);
    assert_diagnostics(
        std_project(
            r#"
        const x = comptime {
            let mut i = 0;
            while i < 1000 {
                i = i + 1;
            }
            comptime {
                let mut j = 0;
                while j == 0 {
                    j = 1;
                }
                j
            }
        };
        init { @evm_stop(); }
        "#,
        ),
        &[r#"
        error: comptime branch quota exhausted
          --> main.plk:8:15
           |
         8 |         while j == 0 {
           |               ^^^^^^^ evaluating this loop exceeded the comptime branch quota
           |
           = note: current eval branch quota is 1000
        note: comptime evaluation began here
          --> main.plk:1:1
           |
         1 | / const x = comptime {
         2 | |     let mut i = 0;
         3 | |     while i < 1000 {
         4 | |         i = i + 1;
        ...  |
        13 | | };
           | |__^
        "#],
    );
}

#[test]
fn test_cached_const_references_do_not_consume_quota() {
    assert_eq!(1000, DEFAULT_COMPTIME_BRANCH_QUOTA);
    assert_lowers_to(
        std_project(
            r#"
        const Cached = comptime {
            let mut i = 0;
            while i < 1000 {
                i = i + 1;
            }
            i
        };

        const x = comptime {
            Cached;
            Cached
        };

        init {
            let mut y: u256 = x;
            @evm_stop();
        }
        "#,
        ),
        r#"
        ==== Functions ====
        ; init
        @fn0() -> never {
            %0 : u256 = 1000
            %1 : never = @evm_stop()
        }
        "#,
    );
}

#[test]
fn test_cached_const_does_not_increase_caller_quota() {
    assert_eq!(1000, DEFAULT_COMPTIME_BRANCH_QUOTA);
    assert_eq!(2000, DEFAULT_COMPTIME_BRANCH_QUOTA * 2);
    assert_diagnostics(
        std_project(
            r#"
        const F = comptime {
            @set_eval_branch_quota(2000);
            let mut i = 0;
            while i < 2000 {
                i = i + 1;
            }
            i
        };

        init {
            let mut warm: u256 = comptime { F };
            let mut x: u256 = comptime {
                let start = F;
                let mut i = 0;
                while i < 2000 {
                    i = i + 1;
                }
                start + i
            };
            @evm_stop();
        }
        "#,
        ),
        &[r#"
        error: comptime branch quota exhausted
          --> main.plk:15:15
           |
        15 |         while i < 2000 {
           |               ^^^^^^^^^ evaluating this loop exceeded the comptime branch quota
           |
           = note: current eval branch quota is 1000
        note: comptime evaluation began here
          --> main.plk:10:1
           |
        10 | / init {
        11 | |     let mut warm: u256 = comptime { F };
        12 | |     let mut x: u256 = comptime {
        13 | |         let start = F;
        ...  |
        20 | |     @evm_stop();
        21 | | }
           | |_^
        "#],
    );
}

#[test]
fn test_const_eval_uses_fresh_quota() {
    assert_eq!(1000, DEFAULT_COMPTIME_BRANCH_QUOTA);
    assert_eq!(1001, DEFAULT_COMPTIME_BRANCH_QUOTA + 1);
    assert_diagnostics(
        std_project(
            r#"
        const C = comptime {
            let mut i = 0;
            while i < 1001 {
                i = i + 1;
            }
            i
        };

        init {
            let mut warm: u256 = comptime {
                @set_eval_branch_quota(1001);
                C
            };
            @evm_stop();
        }
        "#,
        ),
        &[r#"
        error: comptime branch quota exhausted
         --> main.plk:3:11
          |
        3 |     while i < 1001 {
          |           ^^^^^^^^^ evaluating this loop exceeded the comptime branch quota
          |
          = note: current eval branch quota is 1000
        note: comptime evaluation began here
         --> main.plk:1:1
          |
        1 | / const C = comptime {
        2 | |     let mut i = 0;
        3 | |     while i < 1001 {
        4 | |         i = i + 1;
        5 | |     }
        6 | |     i
        7 | | };
          | |__^
        "#],
    );
}

#[test]
fn test_referenced_const_quota_exhaustion_emits_once() {
    assert_eq!(1000, DEFAULT_COMPTIME_BRANCH_QUOTA);
    assert_eq!(1001, DEFAULT_COMPTIME_BRANCH_QUOTA + 1);
    assert_diagnostics(
        std_project(
            r#"
        const C = comptime {
            let mut i = 0;
            while i < 1001 {
                i = i + 1;
            }
            i
        };

        init {
            let mut x: u256 = C;
            @evm_stop();
        }
        "#,
        ),
        &[r#"
        error: comptime branch quota exhausted
         --> main.plk:3:11
          |
        3 |     while i < 1001 {
          |           ^^^^^^^^^ evaluating this loop exceeded the comptime branch quota
          |
          = note: current eval branch quota is 1000
        note: comptime evaluation began here
         --> main.plk:1:1
          |
        1 | / const C = comptime {
        2 | |     let mut i = 0;
        3 | |     while i < 1001 {
        4 | |         i = i + 1;
        5 | |     }
        6 | |     i
        7 | | };
          | |__^
        "#],
    );
}

#[test]
fn test_unused_const_comptime_while_branch_quota_exhausted() {
    assert_eq!(1000, DEFAULT_COMPTIME_BRANCH_QUOTA);
    assert_diagnostics(
        r#"
        const Bad = comptime {
            while true {}
        };
        init { @evm_stop(); }
        "#,
        &[r#"
        error: comptime branch quota exhausted
         --> main.plk:2:11
          |
        2 |     while true {}
          |           ^^^^ evaluating this loop exceeded the comptime branch quota
          |
          = note: current eval branch quota is 1000
        note: comptime evaluation began here
         --> main.plk:1:1
          |
        1 | / const Bad = comptime {
        2 | |     while true {}
        3 | | };
          | |__^
        "#],
    );
}

#[test]
fn scoped_set_eval_in_branch() {
    assert_lowers_to(
        r#"
        init {
            let mut cond = false;
            if cond {
                @set_eval_branch_quota(3);
                comptime {
                    let x = 3;
                }
            } else {

            }

            @evm_stop();
        }
        "#,
        r#"
        ==== Functions ====
        ; init
        @fn0() -> never {
            %0 : bool = false
            %1 : bool = %0
            if %1 {
                %2 : void = void_unit
            } else {
                %2 : void = void_unit
            }
            %3 : void = %2
            %4 : never = @evm_stop()
        }
        "#,
    );
}
