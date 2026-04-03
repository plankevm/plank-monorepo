use plank_mir::{Mir, display::DisplayMir};
use plank_session::Session;
use plank_test_utils::{TestProject, dedent_preserve_blank_lines};
use plank_values::BigNumInterner;

fn try_lower(source: &str) -> (Mir, BigNumInterner, Session) {
    try_lower_project(TestProject::single(source))
}

fn try_lower_project(project: TestProject) -> (Mir, BigNumInterner, Session) {
    let mut session = Session::new();
    let project = project.build(&mut session);

    let mut big_nums = BigNumInterner::default();
    let hir = plank_hir::lower(&project, &mut big_nums, &mut session);
    let mir = crate::evaluate(&hir, &mut big_nums, &mut session);

    (mir, big_nums, session)
}

fn assert_lowers_to(source: &str, expected: &str) {
    let (mir, big_nums, session) = try_lower(source);
    let actual = format!("{}", DisplayMir::new(&mir, &big_nums, &session));
    let expected = dedent_preserve_blank_lines(expected);

    pretty_assertions::assert_str_eq!(actual.trim(), expected.trim());
}

fn render_project_diagnostics(test_project: TestProject) -> Vec<String> {
    let (_, _, session) = try_lower_project(test_project);
    session.diagnostics().iter().map(|d| d.render_plain(&session)).collect()
}

fn assert_diagnostics(source: &str, expected: &[&str]) {
    assert_project_diagnostics(TestProject::single(source), expected)
}

fn assert_project_diagnostics(test_project: TestProject, expected: &[&str]) {
    let actual = render_project_diagnostics(test_project);
    let expected: Vec<String> =
        expected.iter().map(|s| dedent_preserve_blank_lines(s).trim().to_string()).collect();
    let actual: Vec<String> = actual.iter().map(|s| s.trim().to_string()).collect();

    let actual_joined = actual.join("\n\n---\n\n");
    let expected_joined = expected.join("\n\n---\n\n");
    let message = if actual.len() != expected.len() {
        format!("length mismatch: {} != {}", actual.len(), expected.len())
    } else {
        "".to_string()
    };
    pretty_assertions::assert_str_eq!(actual_joined, expected_joined, "{}", message);
}

#[test]
fn test_simple_malloc_mstore_return() {
    assert_lowers_to(
        r#"
        init {
            let buf = malloc_uninit(0x20);
            mstore32(buf, 0x05);
            evm_return(buf, 0x20);
        }
        "#,
        r#"
        ==== Functions ====
        ; init
        @fn0() -> never {
            %0 : u256 = 32
            %1 : memptr = malloc_uninit(%0)
            %2 : memptr = %1
            %3 : u256 = 5
            %4 : void = mstore32(%2, %3)
            %5 : memptr = %1
            %6 : u256 = 32
            %7 : never = evm_return(%5, %6)
        }
        "#,
    );
}

#[test]
fn test_type_annotation_type_mismatch() {
    assert_diagnostics(
        "
        init {
            let x: u256 = false;
            evm_stop();
        }
        ",
        &[r#"
        error: mismatched types
         --> main.plk:2:19
          |
        2 |     let x: u256 = false;
          |            ----   ^^^^^ expected `u256`, got `bool`
          |            |
          |            `u256` expected because of this
        "#],
    );
}

#[test]
fn test_if_two_branches_type_mismatch() {
    assert_diagnostics(
        "
        init {
            let c = calldataload(0);
            let x = if slt(c, 0)  {
                334
            } else {
                false
            };
            evm_stop();
        }
        ",
        &[r#"
            error: `if` and `else` have incompatible types
             --> main.plk:6:9
              |
            4 |         334
              |         --- `u256` expected because of this
            5 |     } else {
            6 |         false
              |         ^^^^^ expected `u256`, got `bool`
        "#],
    );
}

#[test]
fn test_if_three_branches_type_mismatch() {
    assert_diagnostics(
        "
        init {
            let c = calldataload(0);
            let x = if slt(c, 0) {
                3
            } else if eq(c, 34) {
                false
            } else {
                true
            };
            evm_stop();
        }
        ",
        &[
            r#"
                error: `if` and `else` have incompatible types
                 --> main.plk:6:9
                  |
                4 |         3
                  |         - `u256` expected because of this
                5 |     } else if eq(c, 34) {
                6 |         false
                  |         ^^^^^ expected `u256`, got `bool`
            "#,
            r#"
                error: `if` and `else` have incompatible types
                 --> main.plk:8:9
                  |
                4 |         3
                  |         - `u256` expected because of this
                ...
                8 |         true
                  |         ^^^^ expected `u256`, got `bool`
            "#,
        ],
    );
}

#[test]
fn test_if_type_mismatch() {
    assert_diagnostics(
        "
        init {
            let c = calldataload(0);
            let x: u256 = if slt(c, 0)  {
                true
            } else {
                false
            };
            evm_stop();
        }
        ",
        &[r#"
            error: mismatched types
             --> main.plk:3:19
              |
            3 |       let x: u256 = if slt(c, 0)  {
              |  ____________----___^
              | |            |
              | |            `u256` expected because of this
            4 | |         true
            5 | |     } else {
            6 | |         false
            7 | |     };
              | |_____^ expected `u256`, got `bool`
            "#],
    );
}

#[test]
#[should_panic(
    expected = "not yet implemented: diagnostic: entry point must have an explicit terminator"
)]
fn test_run_missing_termination() {
    let _ = try_lower(
        "
            init {
                evm_stop();
            }
            run {
                let x = 5;
            }
        ",
    );
}

#[test]
fn test_never_fn_missing_termination() {
    assert_diagnostics(
        "
        init {
            let halt = fn() never {
                let x = 5;
            };
            halt();
        }
        ",
        &[r#"
        error: mismatched types
         --> main.plk:2:27
          |
        2 |       let halt = fn() never {
          |  _____________________-----_^
          | |                     |
          | |                     `never` expected because of this
        3 | |         let x = 5;
        4 | |     };
          | |_____^ expected `never`, got `void`
        "#],
    );
}

#[test]
fn test_init_run_with_never_fn() {
    assert_lowers_to(
        "
        init {
            let halt = fn() never {
                evm_stop();
            };
            halt();
        }
        run {
            let halt = fn() never {
                invalid();
            };
            let abort = fn() never {
                halt();
            };
            abort();
        }
        ",
        "
        ==== Functions ====
        @fn0() -> never {
            %0 : never = evm_stop()
        }

        ; init
        @fn1() -> never {
            %0 : never = call @fn0()
        }

        @fn2() -> never {
            %0 : never = invalid()
        }

        @fn3() -> never {
            %0 : never = call @fn2()
        }

        ; run
        @fn4() -> never {
            %0 : never = call @fn3()
        }
        ",
    );
}

#[test]
fn test_diverging_block_middle() {
    assert_lowers_to(
        r#"
        init {
            evm_stop();
            let x = 42;
        }
        "#,
        r#"
        ==== Functions ====
        ; init
        @fn0() -> never {
            %0 : never = evm_stop()
        }
        "#,
    );
}

#[test]
fn test_builtin_call_with_never_arg() {
    assert_lowers_to(
        r#"
        init {
            let halt = fn() never {
                evm_stop();
            };
            mstore32(malloc_uninit(0x20), halt());
        }
        "#,
        r#"
        ==== Functions ====
        @fn0() -> never {
            %0 : never = evm_stop()
        }

        ; init
        @fn1() -> never {
            %0 : u256 = 32
            %1 : memptr = malloc_uninit(%0)
            %2 : never = call @fn0()
        }
        "#,
    );
}

#[test]
fn test_if_mixed_never_and_value_branches() {
    assert_lowers_to(
        r#"
        init {
            let c = calldataload(0);
            let x = if iszero(c) {
                evm_stop()
            } else {
                42
            };
            evm_stop();
        }
        "#,
        r#"
        ==== Functions ====
        ; init
        @fn0() -> never {
            %0 : u256 = 0
            %1 : u256 = calldataload(%0)
            %2 : u256 = %1
            %3 : bool = iszero(%2)
            if %3 {
                %4 : u256 = evm_stop()
            } else {
                %4 : u256 = 42
            }
            %5 : u256 = %4
            %6 : never = evm_stop()
        }
        "#,
    );
}

#[test]
fn test_fn_struct_return() {
    assert_lowers_to(
        r#"
        const Pair = struct { a: u256, b: u256 };
        const swap = fn (x: u256, y: u256) Pair {
            Pair { a: y, b: x }
        };

        init {
            let x = swap(3, 4);
            evm_stop();
        }
        "#,
        r#"
        ==== Functions ====
        @fn0(%0: u256, %1: u256) -> Pair {
            %2 : u256 = %1
            %3 : u256 = %0
            %4 : Pair = Pair { %2, %3 }
            ret %4
        }

        ; init
        @fn1() -> never {
            %0 : u256 = 3
            %1 : u256 = 4
            %2 : Pair = call @fn0(%0, %1)
            %3 : never = evm_stop()
        }
        "#,
    );
}

#[test]
fn test_struct_field_access() {
    assert_lowers_to(
        r#"
        const Pair = struct { a: u256, b: bool };

        init {
            let x = Pair { b: false, a : 34 };
            let y: u256 = x.a;
            let z: bool = x.b;

            let p = Pair { a: 49, b: true };
            let pa = p.a;
            let pb = p.b;

            evm_stop();
        }
        "#,
        r#"
        ==== Functions ====
        ; init
        @fn0() -> never {
            %0 : bool = false
            %1 : u256 = 34
            %2 : Pair = Pair { %1, %0 }
            %3 : Pair = %2
            %4 : u256 = %3.0
            %5 : Pair = %2
            %6 : bool = %5.1
            %7 : u256 = 49
            %8 : bool = true
            %9 : Pair = Pair { %7, %8 }
            %10 : Pair = %9
            %11 : u256 = %10.0
            %12 : Pair = %9
            %13 : bool = %12.1
            %14 : never = evm_stop()
        }
        "#,
    );
}

#[test]
#[should_panic(expected = "not yet implemented: diagnostic: access undefined attribute")]
fn test_invalid_field_access() {
    let _ = try_lower(
        r#"
        const Pair = struct { a: u256, b: bool };

        init {
            let x = Pair { b: false, a : 34 };
            let y: u256 = x.hey;
            evm_stop();
        }
        "#,
    );
}

#[test]
fn test_comptime_struct_field_ordering() {
    assert_lowers_to(
        r#"
        const Pair = struct { a: u256, b: bool };
        const my_pair = Pair { b: true, a: 42 };
        const a_val = my_pair.a;
        const b_val = my_pair.b;

        init {
            let x: u256 = a_val;
            let y: bool = b_val;
            evm_stop();
        }
        "#,
        r#"
        ==== Functions ====
        ; init
        @fn0() -> never {
            %0 : u256 = 42
            %1 : bool = true
            %2 : never = evm_stop()
        }
        "#,
    );
}

#[test]
#[should_panic(expected = "not yet implemented: diagnostic: literal missing struct field")]
fn test_comptime_struct_missing_field() {
    let _ = try_lower(
        r#"
        const Pair = struct { a: u256, b: bool };
        const my_pair = Pair { a: 42 };

        init {
            evm_stop();
        }
        "#,
    );
}

#[test]
#[should_panic(expected = "not yet implemented: diagnostic: duplicate struct field assignment")]
fn test_comptime_struct_duplicate_field() {
    let _ = try_lower(
        r#"
        const Pair = struct { a: u256, b: bool };
        const my_pair = Pair { a: 42, a: 99, b: false };

        init {
            evm_stop();
        }
        "#,
    );
}

#[test]
fn test_assign_type_mismatch() {
    assert_diagnostics(
        r#"
        init {
            let mut x = 1;
            x = false;
            evm_stop();
        }
        "#,
        &[r#"
        error: mismatched types
         --> main.plk:3:9
          |
        2 |     let mut x = 1;
          |                 - `u256` expected because of this
        3 |     x = false;
          |         ^^^^^ expected `u256`, got `bool`
        "#],
    );
}

#[test]
fn test_comptime_struct_field_type_mismatch() {
    assert_diagnostics(
        r#"
        const Pair = struct { a: u256, b: bool };
        const my_pair = Pair { a: false, b: false };

        init {
            evm_stop();
        }
        "#,
        &[r#"
        error: mismatched types
         --> main.plk:2:27
          |
        2 | const my_pair = Pair { a: false, b: false };
          |                           ^^^^^ expected `u256`, got `bool`
        "#],
    );
}

#[test]
fn test_comptime_struct_def_field_not_type() {
    assert_diagnostics(
        r#"
        const S = struct { x: 42 };
        init { evm_stop(); }
        "#,
        &[r#"
        error: value used as type
         --> main.plk:1:23
          |
        1 | const S = struct { x: 42 };
          |                       ^^ expected type, got value of type `u256`
        "#],
    );
}

#[test]
fn test_comptime_struct_lit_type_not_type() {
    assert_diagnostics(
        r#"
        const T = 42;
        const x = T { };
        init { evm_stop(); }
        "#,
        &[r#"
        error: value used as type
         --> main.plk:2:11
          |
        2 | const x = T { };
          |           ^ expected type, got value of type `u256`
        "#],
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
        2 | const f = fn(x: forty_two) u256 { return x; };
          |                 ^^^^^^^^^ expected type, got value of type `u256`
        "#],
    );
}

#[test]
fn test_struct_lit_value_as_type_in_init() {
    assert_diagnostics(
        r#"
        const T = 42;
        init {
            let x = T { };
            evm_stop();
        }
        "#,
        &[r#"
        error: value used as type
         --> main.plk:3:13
          |
        3 |     let x = T { };
          |             ^ expected type, got value of type `u256`
        "#],
    );
}

#[test]
fn test_struct_type_not_comptime_known() {
    assert_diagnostics(
        r#"
        init {
            let T = calldataload(0);
            let x = T { };
            evm_stop();
        }
        "#,
        &[r#"
        error: struct type must be known at compile time
         --> main.plk:3:13
          |
        3 |     let x = T { };
          |             ^ not known at compile time
        "#],
    );
}

#[test]
fn test_runtime_struct_def_field_not_type() {
    assert_diagnostics(
        r#"
        init {
            let S = struct { x: 42 };
            evm_stop();
        }
        "#,
        &[r#"
        error: value used as type
         --> main.plk:2:25
          |
        2 |     let S = struct { x: 42 };
          |                         ^^ expected type, got value of type `u256`
        "#],
    );
}

#[test]
fn test_runtime_fn_return_type_not_type() {
    assert_diagnostics(
        r#"
        const forty_two = 42;
        init {
            let f = fn() forty_two { return 1; };
            f();
            evm_stop();
        }
        "#,
        &[r#"
        error: value used as type
         --> main.plk:3:18
          |
        3 |     let f = fn() forty_two { return 1; };
          |                  ^^^^^^^^^ expected type, got value of type `u256`
        "#],
    );
}

#[test]
fn test_comptime_assign_type_mismatch() {
    assert_diagnostics(
        r#"
        const f = fn() u256 {
            let mut x = 1;
            x = false;
            return x;
        };
        const r = f();
        init { evm_stop(); }
        "#,
        &[r#"
        error: mismatched types
         --> main.plk:3:9
          |
        2 |     let mut x = 1;
          |                 - `u256` expected because of this
        3 |     x = false;
          |         ^^^^^ expected `u256`, got `bool`
        "#],
    );
}

#[test]
fn test_comptime_call_arg_type_mismatch() {
    assert_diagnostics(
        r#"
        const f = fn(x: u256) u256 { return x; };
        const r = f(false);
        init { evm_stop(); }
        "#,
        &[r#"
        error: mismatched types
         --> main.plk:2:13
          |
        1 | const f = fn(x: u256) u256 { return x; };
          |                 ---- `u256` expected because of this
        2 | const r = f(false);
          |             ^^^^^ expected `u256`, got `bool`
        "#],
    );
}

#[test]
fn test_runtime_return_type_mismatch() {
    assert_diagnostics(
        r#"
        init {
            let f = fn() u256 { return false; };
            f();
            evm_stop();
        }
        "#,
        &[r#"
        error: mismatched types
         --> main.plk:2:32
          |
        2 |     let f = fn() u256 { return false; };
          |                  ----          ^^^^^ expected `u256`, got `bool`
          |                  |
          |                  `u256` expected because of this
        "#],
    );
}

#[test]
fn test_comptime_if_condition_not_bool() {
    assert_diagnostics(
        r#"
        const f = fn() u256 {
            if 42 { return 1; } else { return 2; }
        };
        const r = f();
        init { evm_stop(); }
        "#,
        &[r#"
        error: mismatched types
         --> main.plk:2:8
          |
        2 |     if 42 { return 1; } else { return 2; }
          |        ^^ expected `bool`, got `u256`
        "#],
    );
}

#[test]
fn test_runtime_struct_lit_field_type_mismatch() {
    assert_diagnostics(
        r#"
        const Pair = struct { a: u256, b: bool };
        init {
            let x = Pair { a: false, b: false };
            evm_stop();
        }
        "#,
        &[r#"
        error: mismatched types
         --> main.plk:3:23
          |
        3 |     let x = Pair { a: false, b: false };
          |                       ^^^^^ expected `u256`, got `bool`
        "#],
    );
}

#[test]
fn test_runtime_call_arg_type_mismatch() {
    assert_diagnostics(
        r#"
        init {
            let f = fn(x: u256) never { evm_stop(); };
            f(false);
        }
        "#,
        &[r#"
        error: mismatched types
         --> main.plk:3:7
          |
        3 |     f(false);
          |       ^^^^^ expected `u256`, got `bool`
        "#],
    );
}

#[test]
fn test_runtime_if_condition_comptime_not_bool() {
    assert_diagnostics(
        "
        init {
            if 42 { evm_stop(); } else { evm_stop(); }
        }
        ",
        &[r#"
        error: mismatched types
         --> main.plk:2:8
          |
        2 |     if 42 { evm_stop(); } else { evm_stop(); }
          |        ^^ expected `bool`, got `u256`
        "#],
    );
}

#[test]
fn test_runtime_if_condition_runtime_not_bool() {
    assert_diagnostics(
        "
        init {
            let c = calldataload(0);
            if c { evm_stop(); } else { evm_stop(); }
        }
        ",
        &[r#"
        error: mismatched types
         --> main.plk:3:8
          |
        3 |     if c { evm_stop(); } else { evm_stop(); }
          |        ^ expected `bool`, got `u256`
        "#],
    );
}

#[test]
fn test_runtime_while_condition_not_bool() {
    assert_diagnostics(
        "
        init {
            let c = calldataload(0);
            while c { }
            evm_stop();
        }
        ",
        &[r#"
        error: mismatched types
         --> main.plk:3:11
          |
        3 |     while c { }
          |           ^ expected `bool`, got `u256`
        "#],
    );
}

#[test]
fn test_comptime_struct_lit_not_a_struct() {
    assert_diagnostics(
        r#"
        const x = u256 { };
        init { evm_stop(); }
        "#,
        &[r#"
        error: expected struct type
         --> main.plk:1:11
          |
        1 | const x = u256 { };
          |           ^^^^ `u256` is not a struct type
        "#],
    );
}

#[test]
fn test_runtime_struct_lit_not_a_struct() {
    assert_diagnostics(
        r#"
        init {
            let x = u256 { };
            evm_stop();
        }
        "#,
        &[r#"
        error: expected struct type
         --> main.plk:2:13
          |
        2 |     let x = u256 { };
          |             ^^^^ `u256` is not a struct type
        "#],
    );
}

#[test]
fn test_comptime_member_on_non_struct() {
    assert_diagnostics(
        r#"
        const x: u256 = 5;
        const y = x.foo;
        init { evm_stop(); }
        "#,
        &[r#"
        error: no fields on type
         --> main.plk:2:11
          |
        2 | const y = x.foo;
          |           ^ value of type `u256` is not a struct type
        "#],
    );
}

#[test]
fn test_runtime_member_on_non_struct() {
    assert_diagnostics(
        r#"
        init {
            let x: u256 = calldataload(0);
            let y = x.foo;
            evm_stop();
        }
        "#,
        &[r#"
        error: no fields on type
         --> main.plk:3:13
          |
        3 |     let y = x.foo;
          |             ^ value of type `u256` is not a struct type
        "#],
    );
}

#[test]
fn test_comptime_call_on_non_function() {
    assert_diagnostics(
        r#"
        const x = 5;
        const y = x();
        init { evm_stop(); }
        "#,
        &[r#"
        error: expected function
         --> main.plk:2:11
          |
        2 | const y = x();
          |           ^ `u256` is not callable
        "#],
    );
}

#[test]
fn test_diagnostic_renders_struct_name() {
    assert_diagnostics(
        r#"
        const Pair = struct { a: u256, b: bool };
        init {
            let x: Pair = 42;
            evm_stop();
        }
        "#,
        &[r#"
        error: mismatched types
         --> main.plk:3:19
          |
        3 |     let x: Pair = 42;
          |            ----   ^^ expected `Pair`, got `u256`
          |            |
          |            `Pair` expected because of this
        "#],
    );
}

#[test]
fn test_call_target_not_comptime() {
    assert_diagnostics(
        r#"
        init {
            let f = calldataload(0);
            f();
            evm_stop();
        }
        "#,
        &[r#"
        error: call target must be known at compile time
         --> main.plk:3:5
          |
        3 |     f();
          |     ^ not known at compile time
          |
          = note: function calls are statically dispatched
        "#],
    );
}

#[test]
fn test_runtime_call_on_non_function() {
    assert_diagnostics(
        r#"
        init {
            let x = 5;
            x();
            evm_stop();
        }
        "#,
        &[r#"
        error: expected function
         --> main.plk:3:5
          |
        3 |     x();
          |     ^ `u256` is not callable
        "#],
    );
}

#[test]
fn test_runtime_call_arg_count_mismatch() {
    assert_diagnostics(
        r#"
        const foo = fn(x: u256) u256 { return x; };
        init {
            foo(1, 2);
            evm_stop();
        }
        "#,
        &[r#"
        error: wrong number of arguments
         --> main.plk:3:5
          |
        1 | const foo = fn(x: u256) u256 { return x; };
          |               --------- defined with 1 parameter
        2 | init {
        3 |     foo(1, 2);
          |     ^^^^^^^^^ expected 1 argument, got 2
        "#],
    );
}

#[test]
fn test_comptime_call_arg_count_mismatch() {
    assert_diagnostics(
        r#"
        const f = fn(x: u256) u256 { return x; };
        const r = f(1, 2);
        init { evm_stop(); }
        "#,
        &[r#"
        error: wrong number of arguments
         --> main.plk:2:11
          |
        1 | const f = fn(x: u256) u256 { return x; };
          |             --------- defined with 1 parameter
        2 | const r = f(1, 2);
          |           ^^^^^^^ expected 1 argument, got 2
        "#],
    );
}

#[test]
fn test_cross_file_call_arg_count_mismatch() {
    assert_project_diagnostics(
        TestProject::single("import m::other::f;\ninit { f(1, 2); evm_stop(); }")
            .add_file("other", "const f = fn(x: u256) u256 { return x; };")
            .add_module("m", ""),
        &[r#"
        error: wrong number of arguments
         --> main.plk:2:8
          |
        2 | init { f(1, 2); evm_stop(); }
          |        ^^^^^^^ expected 1 argument, got 2
          |
         ::: other.plk:1:13
          |
        1 | const f = fn(x: u256) u256 { return x; };
          |             --------- defined with 1 parameter
        "#],
    );
}

#[test]
fn test_no_matching_builtin_signature() {
    assert_diagnostics(
        r#"
        init {
            add(true, false);
            evm_stop();
        }
        "#,
        &[r#"
        error: no valid match for builtin signature
         --> main.plk:2:5
          |
        2 |     add(true, false);
          |     ^^^^^^^^^^^^^^^^ `add` cannot be called with (bool, bool)
          |
          = note: `add` accepts (u256, u256), (memptr, u256), (u256, memptr)
        "#],
    );
}

#[test]
fn test_builtin_wrong_arg_count() {
    assert_diagnostics(
        r#"
        init {
            add(1);
            evm_stop();
        }
        "#,
        &[r#"
        error: wrong number of arguments
         --> main.plk:2:5
          |
        2 |     add(1);
          |     ^^^^^^ `add` called with 1 argument, but requires 2
          |
          = note: `add` accepts (u256, u256), (memptr, u256), (u256, memptr)
        "#],
    );
}

#[test]
fn test_closure_capture_not_comptime() {
    assert_diagnostics(
        r#"
        init {
            let x = calldataload(0);
            let f = fn() u256 { x };
            evm_stop();
        }
        "#,
        &[r#"
        error: closure capture must be known at compile time
         --> main.plk:3:25
          |
        2 |     let x = calldataload(0);
          |             --------------- not known at compile time
        3 |     let f = fn() u256 { x };
          |                         ^ captures a runtime value
          |
          = note: closures can only capture values known at compile time
        "#],
    );
}

#[test]
fn test_logical_not_runtime() {
    assert_lowers_to(
        r#"
        init {
            let c = calldataload(0);
            let b = iszero(c);
            let nb = !b;
            evm_stop();
        }
        "#,
        r#"
        ==== Functions ====
        ; init
        @fn0() -> never {
            %0 : u256 = 0
            %1 : u256 = calldataload(%0)
            %2 : u256 = %1
            %3 : bool = iszero(%2)
            %4 : bool = %3
            %5 : bool = iszero(%4)
            %6 : never = evm_stop()
        }
        "#,
    );
}

#[test]
fn test_logical_not_comptime_true() {
    assert_lowers_to(
        r#"
        const x = !true;
        init {
            let v: bool = x;
            evm_stop();
        }
        "#,
        r#"
        ==== Functions ====
        ; init
        @fn0() -> never {
            %0 : bool = false
            %1 : never = evm_stop()
        }
        "#,
    );
}

#[test]
fn test_logical_not_comptime_false() {
    assert_lowers_to(
        r#"
        const x = !false;
        init {
            let v: bool = x;
            evm_stop();
        }
        "#,
        r#"
        ==== Functions ====
        ; init
        @fn0() -> never {
            %0 : bool = true
            %1 : never = evm_stop()
        }
        "#,
    );
}

#[test]
fn test_logical_not_in_if_condition() {
    assert_lowers_to(
        r#"
        init {
            let c = calldataload(0);
            let b = iszero(c);
            if !b {
                evm_stop();
            } else {
                revert(malloc_uninit(0), 0);
            }
        }
        "#,
        r#"
        ==== Functions ====
        ; init
        @fn0() -> never {
            %0 : u256 = 0
            %1 : u256 = calldataload(%0)
            %2 : u256 = %1
            %3 : bool = iszero(%2)
            %4 : bool = %3
            %5 : bool = iszero(%4)
            if %5 {
                %6 : never = evm_stop()
            } else {
                %7 : u256 = 0
                %8 : memptr = malloc_uninit(%7)
                %9 : u256 = 0
                %10 : never = revert(%8, %9)
            }
        }
        "#,
    );
}

#[test]
fn test_logical_not_type_mismatch_runtime() {
    assert_diagnostics(
        r#"
        init {
            let c = calldataload(0);
            let x = !c;
            evm_stop();
        }
        "#,
        &[r#"
        error: mismatched types
         --> main.plk:3:14
          |
        3 |     let x = !c;
          |              ^ expected `bool`, got `u256`
        "#],
    );
}

#[test]
fn test_logical_not_type_mismatch_comptime() {
    assert_diagnostics(
        r#"
        const x = !42;
        init { evm_stop(); }
        "#,
        &[r#"
        error: mismatched types
         --> main.plk:1:12
          |
        1 | const x = !42;
          |            ^^ expected `bool`, got `u256`
        "#],
    );
}

#[test]
fn test_and_comptime_short_circuit_false() {
    assert_lowers_to(
        r#"
        const x = false and true;
        init {
            let v: bool = x;
            evm_stop();
        }
        "#,
        r#"
        ==== Functions ====
        ; init
        @fn0() -> never {
            %0 : bool = false
            %1 : never = evm_stop()
        }
        "#,
    );
}

#[test]
fn test_and_condition_type_mismatch() {
    assert_diagnostics(
        r#"
        init {
            let c = calldataload(0);
            let x = c and true;
            evm_stop();
        }
        "#,
        &[r#"
        error: mismatched types
         --> main.plk:3:13
          |
        3 |     let x = c and true;
          |             ^ expected `bool`, got `u256`
        "#],
    );
}

#[test]
fn test_or_condition_type_mismatch() {
    assert_diagnostics(
        r#"
        init {
            let c = calldataload(0);
            let x = c or true;
            evm_stop();
        }
        "#,
        &[r#"
        error: mismatched types
         --> main.plk:3:13
          |
        3 |     let x = c or true;
          |             ^ expected `bool`, got `u256`
        "#],
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
            let a: u256 = add_res;
            let b: u256 = mul_res;
            let c: u256 = sub_res;
            let d: u256 = div_res;
            let e: u256 = mod_res;
            let f: u256 = sdiv_res;
            let g: u256 = smod_res;
            let h: u256 = exp_res;
            let i: u256 = div_zero;
            let j: u256 = signext_res;
            let k: u256 = and_res;
            let l: u256 = or_res;
            let m: u256 = xor_res;
            let n: u256 = byte_res;
            let o: u256 = shl_res;
            let p: u256 = shr_res;
            let q: u256 = sar_res;
            let r: bool = lt_res;
            let s: bool = gt_res;
            let t: bool = slt_res;
            let u: bool = sgt_res;
            let v: bool = eq_res;
            let w: bool = iszero_t;
            let x: bool = iszero_f;
            let y: u256 = addmod_res;
            let z: u256 = mulmod_res;
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
            let x: u256 = b;
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
        error: comptime evaluation not supported
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
            let x: u256 = comptime {
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
            %1 : u256 = 15
            %2 : never = evm_stop()
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
            let x: u256 = comptime { N };
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
            let x: u256 = comptime { B };
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
            let x: u256 = comptime { B };
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
            let val = T { x: 42 };
            evm_stop();
        }
        "#,
        r#"
        ==== Functions ====
        ; init
        @fn0() -> never {
            %0 : u256 = 42
            %1 : struct@3:9 = struct@3:9 { %0 }
            %2 : never = evm_stop()
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
        error: comptime block capture must be known at compile time
         --> main.plk:3:24
          |
        3 |     let y = comptime { x };
          |                        ^ not known at compile time
          |
          = note: comptime blocks can only reference values known at compile time
        "#],
    );
}

#[test]
fn test_comptime_block_type_result() {
    assert_lowers_to(
        r#"
        init {
            let x: comptime { u256 } = 5;
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
