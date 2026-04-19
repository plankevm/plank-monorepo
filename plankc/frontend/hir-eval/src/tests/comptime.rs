use super::*;

#[test]
fn test_comptime_only_return_caches_per_non_comptime_arg_value() {
    assert_lowers_to(
        r#"
        const f = fn(comptime T: type, x: T) type {
            if eq(x, 0) { T } else { bool }
        };
        init {
            let mut a: f(u256, 0) = 34;
            let mut b: f(u256, 1) = false;
            let mut c: f(u256, 0) = 22;
            evm_stop();
        }
        "#,
        r#"
        ==== Functions ====
        ; init
        @fn0() -> never {
            %0 : u256 = 34
            %1 : bool = false
            %2 : u256 = 22
            %3 : never = evm_stop()
        }
        "#,
    );
}

#[test]
fn test_comptime_evm_builtins() {
    assert_lowers_to(
        r#"
        const add_res = add(10, 7);
        const mul_res = mul(3, 4);
        const sub_res = sub(10, 3);
        const div_res = raw_div(10, 3);
        const mod_res = raw_mod(10, 3);
        const sdiv_res = raw_sdiv(10, 3);
        const smod_res = raw_smod(10, 3);
        const exp_res = exp(2, 10);
        const div_zero = raw_div(5, 0);
        const signext_res = signextend(0, 0x7F);
        const and_res = bitwise_and(0xFF, 0x0F);
        const or_res = bitwise_or(0xF0, 0x0F);
        const xor_res = bitwise_xor(0xFF, 0x0F);
        const byte_res = byte(31, 0x42);
        const shl_res = shl(4, 1);
        const shr_res = shr(1, 16);
        const sar_res = sar(1, 8);
        const lt_res = lt(3, 5);
        const gt_res = gt(5, 3);
        const slt_res = slt(3, 5);
        const sgt_res = sgt(5, 3);
        const eq_res = eq(5, 5);
        const iszero_t = iszero(0);
        const iszero_f = iszero(1);
        const addmod_res = raw_addmod(5, 7, 10);
        const mulmod_res = raw_mulmod(3, 4, 5);
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
            evm_stop();
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
            %26 : never = evm_stop()
        }
        "#,
    );
}

#[test]
fn test_comptime_evm_const_chain() {
    assert_lowers_to(
        r#"
        const a = add(5, 10);
        const b = mul(a, 3);
        init {
            let mut x: u256 = b;
            evm_stop();
        }
        "#,
        r#"
        ==== Functions ====
        ; init
        @fn0() -> never {
            %0 : u256 = 45
            %1 : never = evm_stop()
        }
        "#,
    );
}

#[test]
fn test_comptime_unsupported_evm_builtin() {
    assert_diagnostics(
        r#"
        const x = caller();
        init { evm_stop(); }
        "#,
        &[r#"
        error: builtin not supported at compile time
         --> main.plk:1:11
          |
        1 | const x = caller();
          |           ^^^^^^^^ `caller` cannot be evaluated at compile time
        "#],
    );
}

#[test]
fn test_comptime_evm_wrong_arg_type_in_const() {
    assert_diagnostics(
        r#"
        const y = mul(true, 5);
        init { evm_stop(); }
        "#,
        &[r#"
        error: no valid match for builtin signature
         --> main.plk:1:11
          |
        1 | const y = mul(true, 5);
          |           ^^^^^^^^^^^^ `mul` cannot be called with (bool, u256)
          |
          = note: `mul` accepts (u256, u256)
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
            evm_stop();
        }
        "#,
        r#"
        ==== Functions ====
        ; init
        @fn0() -> never {
            %0 : u256 = 15
            %1 : never = evm_stop()
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
            evm_stop();
        }
        "#,
        r#"
        ==== Functions ====
        ; init
        @fn0() -> never {
            %0 : u256 = 42
            %1 : never = evm_stop()
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
            evm_stop();
        }
        "#,
        r#"
        ==== Functions ====
        ; init
        @fn0() -> never {
            %0 : u256 = 34
            %1 : never = evm_stop()
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
            evm_stop();
        }
        "#,
        r#"
        ==== Functions ====
        ; init
        @fn0() -> never {
            %0 : u256 = 10
            %1 : never = evm_stop()
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
            evm_stop();
        }
        "#,
        r#"
        ==== Functions ====
        ; init
        @fn0() -> never {
            %0 : struct#0@main.plk:3:9 = struct#0 {
                42,
            }
            %1 : never = evm_stop()
        }
        "#,
    );
}

#[test]
fn test_comptime_block_runtime_capture() {
    assert_diagnostics(
        r#"
        init {
            let x = calldataload(0);
            let y = comptime { x };
            evm_stop();
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
            let cond = iszero(calldataload(0));
            let T = if cond { u256 } else { bool };
            evm_stop();
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
            if iszero(n) {
                return a;
            }
            fib_inner(sub(n, 1), b, add(a, b))
        };
        const fib = fn (n: u256) u256 {
            fib_inner(n, 0, 1)
        };

        init {
            let mut f0 = comptime { fib(0) };
            let mut f1 = comptime { fib(1) };
            let mut f10 = comptime { fib(10) };
            let mut f10 = comptime { fib(11) };
            evm_stop();
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
            %4 : never = evm_stop()
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
            evm_stop();
        }
        "#,
        r#"
        ==== Functions ====
        ; init
        @fn0() -> never {
            %0 : u256 = 5
            %1 : never = evm_stop()
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
        init { evm_stop(); }
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

        init { evm_stop(); }
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
           init { evm_stop(); }
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
        init { evm_stop(); }
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
        init { evm_stop(); }
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
            evm_stop();
        }
        "#,
        r#"

        ==== Functions ====
        @fn0(%0: struct#0@main.plk:2:5) -> u256 {
            %1 : struct#0@main.plk:2:5 = %0
            %2 : u256 = %1.1
            ret %2
        }

        @fn1(%0: struct#40@main.plk:2:5) -> u256 {
            %1 : struct#40@main.plk:2:5 = %0
            %2 : u256 = %1.1
            ret %2
        }

        ; init
        @fn2() -> never {
            %0 : struct#0@main.plk:2:5 = struct#0 {
                0,
                34,
            }
            %1 : u256 = call @fn0(%0)
            %2 : struct#40@main.plk:2:5 = struct#40 {
                false,
                33,
            }
            %3 : u256 = call @fn1(%2)
            %4 : never = evm_stop()
        }
        "#,
    );
}

#[test]
fn test_basic_polymorphic_function() {
    assert_lowers_to(
        r#"
        const max = fn (comptime T: type, a: T, b: T) T {
            if T == u256 {
                return if gt(a, b) { a } else { b };
            }
            if T == bool {
                return a or b;
            }
            let _error: void = true;
        };

        init {
            let x = calldataload(0x00);
            let y = calldataload(0x20);
            let mut max_xy = max(u256, x, y);

            let a = false;
            let b = false;
            let mut max_ab = max(bool, a, b);

            evm_stop();
        }
        "#,
        r#"
        ==== Functions ====
        @fn0(%0: u256, %1: u256) -> u256 {
            %2 : u256 = %0
            %3 : u256 = %1
            %4 : bool = gt(%2, %3)
            if %4 {
                %5 : u256 = %0
            } else {
                %5 : u256 = %1
            }
            %6 : u256 = %5
            ret %6
        }

        @fn1(%0: bool, %1: bool) -> bool {
            %2 : void = void_unit
            %3 : void = %2
            %4 : bool = %0
            if %4 {
                %5 : bool = true
            } else {
                %5 : bool = %1
            }
            %6 : bool = %5
            ret %6
        }

        ; init
        @fn2() -> never {
            %0 : u256 = 0
            %1 : u256 = calldataload(%0)
            %2 : u256 = 32
            %3 : u256 = calldataload(%2)
            %4 : u256 = %1
            %5 : u256 = %3
            %6 : u256 = call @fn0(%4, %5)
            %7 : bool = false
            %8 : bool = false
            %9 : bool = call @fn1(%7, %8)
            %10 : never = evm_stop()
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
            add(N, x)
        };

        init {
            let mut x = my_add(ident(4), 4);

            evm_stop();
        }
        "#,
        &[r#"
        error: attempted to pass runtime value as comptime parameter
         --> main.plk:6:24
          |
        2 | const my_add = fn (comptime N: u256, x: u256) u256 {
          |                    ---------------- parameter defined as comptime here
        ...
        6 |     let mut x = my_add(ident(4), 4);
          |                        ^^^^^^^^ runtime argument defined here
          |
        help: you can force compile time evaluation with a `comptime` block
          |
        6 |     let mut x = my_add(comptime { ident(4) }, 4);
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
            add(N, x)
        };

        init {
            let mut x = 3;
            let mut y = comptime {
                my_add(x, 4)
            };

            evm_stop();
        }
        "#,
        &[r#"
        error: attempting to evaluate runtime expression in comptime context
         --> main.plk:7:16
          |
        7 |         my_add(x, 4)
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


            evm_stop();
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
