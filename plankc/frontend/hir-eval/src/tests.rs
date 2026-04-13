use plank_mir::{Mir, display::DisplayMir};
use plank_session::Session;
use plank_test_utils::{TestProject, dedent_preserve_blank_lines};
use plank_values::ValueInterner;

fn try_lower(project: impl Into<TestProject>) -> (Mir, ValueInterner, Session) {
    let project = project.into();
    let mut session = Session::new();
    let project = project.build(&mut session);

    let mut big_nums = ValueInterner::new();
    let hir = plank_hir::lower(&project, &mut big_nums, &mut session);
    let mir = crate::evaluate(&hir, &mut big_nums, &mut session);

    (mir, big_nums, session)
}

fn assert_lowers_to(source: &str, expected: &str) {
    let (mir, big_nums, session) = try_lower(source);

    if session.has_errors() {
        let diags: Vec<String> =
            session.diagnostics().iter().map(|d| d.render_plain(&session)).collect();
        panic!("expected no diagnostics but got {}:\n{}", diags.len(), diags.join("\n---\n"));
    }

    let actual = format!("{}", DisplayMir::new(&mir, &big_nums, &session));
    let expected = dedent_preserve_blank_lines(expected);

    pretty_assertions::assert_str_eq!(actual.trim(), expected.trim());
}

fn render_project_diagnostics(test_project: TestProject) -> Vec<String> {
    let (_, _, session) = try_lower(test_project);
    session.diagnostics().iter().map(|d| d.render_plain(&session)).collect()
}

#[track_caller]
fn assert_diagnostics(source: &str, expected: &[&str]) {
    assert_project_diagnostics(TestProject::root(source), expected)
}

#[track_caller]
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
fn test_no_else_if_as_expr() {
    assert_lowers_to(
        "
        init {
            let cond = calldataload(0);
            let y = if iszero(cond) {
                revert(malloc_uninit(0), 0);
            } else if gt(cond, 2) {
                sstore(3, 4);
            };
            evm_stop();
        }
        ",
        r#"
        ==== Functions ====
        ; init
        @fn0() -> never {
            %0 : u256 = 0
            %1 : u256 = calldataload(%0)
            %2 : u256 = %1
            %3 : bool = iszero(%2)
            if %3 {
                %4 : u256 = 0
                %5 : memptr = malloc_uninit(%4)
                %6 : u256 = 0
                %7 : never = revert(%5, %6)
            } else {
                %8 : u256 = %1
                %9 : u256 = 2
                %10 : bool = gt(%8, %9)
                if %10 {
                    %11 : u256 = 3
                    %12 : u256 = 4
                    %13 : void = sstore(%11, %12)
                    %14 : void = void_unit
                } else {
                    %14 : void = void_unit
                }
            }
            %15 : void = %14
            %16 : never = evm_stop()
        }
        "#,
    );
}

#[test]
fn test_comptime_if_condition_folds_in_runtime() {
    assert_lowers_to(
        "
        init {
            let cond = false;
            if cond {
                revert(malloc_uninit(0), 0);
            } else {
                sstore(3, 4);
            }
            evm_stop();
        }
        ",
        r#"
        ==== Functions ====
        ; init
        @fn0() -> never {
            %0 : u256 = 3
            %1 : u256 = 4
            %2 : void = sstore(%0, %1)
            %3 : void = void_unit
            %4 : void = %3
            %5 : never = evm_stop()
        }
        "#,
    );
}

#[test]
fn test_if_three_branches() {
    assert_lowers_to(
        "
        init {
            let c = calldataload(0);
            let x = if slt(c, 0)  {
                334
            } else if iszero(c) {
                333
            } else {
                0
            };
            evm_stop();
        }
        ",
        r#"
        ==== Functions ====
        ; init
        @fn0() -> never {
            %0 : u256 = 0
            %1 : u256 = calldataload(%0)
            %2 : u256 = %1
            %3 : u256 = 0
            %4 : bool = slt(%2, %3)
            if %4 {
                %5 : u256 = 334
            } else {
                %6 : u256 = %1
                %7 : bool = iszero(%6)
                if %7 {
                    %5 : u256 = 333
                } else {
                    %5 : u256 = 0
                }
            }
            %8 : u256 = %5
            %9 : never = evm_stop()
        }
        "#,
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
        &[r#"
            error: `if` and `else` have incompatible types
             --> main.plk:6:9
              |
            4 |         3
              |         - `u256` expected because of this
            5 |     } else if eq(c, 34) {
            6 |         false
              |         ^^^^^ expected `u256`, got `bool`
            "#],
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
fn test_run_missing_termination() {
    assert_diagnostics(
        "
        init {
            evm_stop();
        }
        run {
            let x = 5;
        }
        ",
        &[r#"
        error: entry point must end with explicit terminator
         --> main.plk:4:1
          |
        4 | / run {
        5 | |     let x = 5;
        6 | | }
          | |_^ execution may reach end of entry point
          |
          = help: entry points must end with a terminating `never` expression (e.g. `evm_stop()`, `revert(...)`, `invalid()`)
        "#],
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
                %4 : never = evm_stop()
            } else {
                %5 : u256 = 42
            }
            %6 : u256 = %5
            %7 : never = evm_stop()
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
            let mut y: u256 = x.a;
            let mut z: bool = x.b;

            let mut p = Pair { a: 49, b: true };
            let mut pa = p.a;
            let mut pb = p.b;

            evm_stop();
        }
        "#,
        r#"
        ==== Functions ====
        ; init
        @fn0() -> never {
            %0 : u256 = 34
            %1 : bool = false
            %2 : Pair = struct#7 {
                49,
                true,
            }
            %3 : Pair = %2
            %4 : u256 = %3.0
            %5 : Pair = %2
            %6 : bool = %5.1
            %7 : never = evm_stop()
        }
        "#,
    );
}

#[test]
fn test_invalid_field_access() {
    assert_diagnostics(
        r#"
        const Pair = struct { a: u256, b: bool };

        init {
            let x = Pair { b: false, a : 34 };
            let y: u256 = x.hey;
            evm_stop();
        }
        "#,
        &[r#"
        error: unknown field
         --> main.plk:4:19
          |
        4 |     let y: u256 = x.hey;
          |                   ^^^^^ `Pair` has no field `hey`
        "#],
    );
}

#[test]
fn test_comptime_invalid_field_access() {
    assert_diagnostics(
        r#"
        const Pair = struct { a: u256, b: bool };
        const p = Pair { a: 42, b: false };
        const x = p.hey;

        init {
            evm_stop();
        }
        "#,
        &[r#"
        error: unknown field
         --> main.plk:3:11
          |
        3 | const x = p.hey;
          |           ^^^^^ `Pair` has no field `hey`
        "#],
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
            let mut x: u256 = a_val;
            let mut y: bool = b_val;
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
fn test_comptime_struct_missing_field() {
    assert_diagnostics(
        r#"
        const Pair = struct { a: u256, b: bool };
        const my_pair = Pair { a: 42 };

        init {
            evm_stop();
        }
        "#,
        &[r#"
        error: missing field
         --> main.plk:2:17
          |
        2 | const my_pair = Pair { a: 42 };
          |                 ^^^^^^^^^^^^^^ missing field `b` in `Pair`
        "#],
    );
}

#[test]
fn test_comptime_struct_unknown_field() {
    assert_diagnostics(
        r#"
        const Pair = struct { a: u256, b: bool };
        const my_pair = Pair { a: 42, c: true, b: false };

        init {
            evm_stop();
        }
        "#,
        &[r#"
        error: unexpected field
         --> main.plk:2:31
          |
        2 | const my_pair = Pair { a: 42, c: true, b: false };
          |                               ^ `Pair` has no field `c`
        "#],
    );
}

#[test]
fn test_comptime_struct_duplicate_field() {
    assert_diagnostics(
        r#"
        const Pair = struct { a: u256, b: bool };
        const my_pair = Pair { a: 42, a: 99, b: false };

        init {
            evm_stop();
        }
        "#,
        &[r#"
        error: duplicate field
         --> main.plk:2:31
          |
        2 | const my_pair = Pair { a: 42, a: 99, b: false };
          |                        -      ^ `a` assigned more than once
          |                        |
          |                        first assigned here
        "#],
    );
}

#[test]
fn test_comptime_struct_unknown_and_missing() {
    assert_diagnostics(
        r#"
        const Pair = struct { a: u256, b: bool };
        const my_pair = Pair { a: 42, c: true };

        init {
            evm_stop();
        }
        "#,
        &[
            r#"
            error: unexpected field
             --> main.plk:2:31
              |
            2 | const my_pair = Pair { a: 42, c: true };
              |                               ^ `Pair` has no field `c`
            "#,
            r#"
            error: missing field
             --> main.plk:2:17
              |
            2 | const my_pair = Pair { a: 42, c: true };
              |                 ^^^^^^^^^^^^^^^^^^^^^^^ missing field `b` in `Pair`
            "#,
        ],
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
        error: incorrect type for struct field
         --> main.plk:2:27
          |
        2 | const my_pair = Pair { a: false, b: false };
          |                           ^^^^^ field `a` expects `u256`, got `bool`
        "#],
    );
}

#[test]
fn test_mixed_comptime_runtime_struct() {
    assert_diagnostics(
        r#"
        const Wrapper = struct { t: type, n: u256 };
        init {
            let x = calldataload(0);
            let w = Wrapper { t: u256, n: x,
                c: 34
            };
            evm_stop();
        }
        "#,
        &[
            r#"
            error: unexpected field
             --> main.plk:5:9
              |
            5 |         c: 34
              |         ^ `Wrapper` has no field `c`
            "#,
            r#"
            error: mixing comptime and runtime data in struct
             --> main.plk:4:13
              |
            4 |       let w = Wrapper { t: u256, n: x,
              |               ^         -        - `n` not comptime known
              |               |         |
              |  _____________|         `t` is comptime only
              | |
            5 | |         c: 34
            6 | |     };
              | |_____^ mixed struct literal
            "#,
        ],
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
        error: type must be known at compile time
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
fn test_runtime_struct_def_type_index_not_comptime() {
    assert_diagnostics(
        r#"
        init {
            let T = calldataload(0);
            let S = struct T { x: u256 };
            evm_stop();
        }
        "#,
        &[r#"
        error: struct definition requires compile-time values
         --> main.plk:3:20
          |
        3 |     let S = struct T { x: u256 };
          |                    ^ type index is not known at compile time
        "#],
    );
}

#[test]
fn test_runtime_struct_def_field_type_not_comptime() {
    assert_diagnostics(
        r#"
        init {
            let T = calldataload(0);
            let S = struct { x: T };
            evm_stop();
        }
        "#,
        &[r#"
        error: type must be known at compile time
         --> main.plk:3:25
          |
        3 |     let S = struct { x: T };
          |                         ^ not known at compile time
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
        init {
            comptime {
                if 42 {
                    add(3, 4);
                } else {
                    iszero(34);
                }
            }
            evm_stop();
        }
        "#,
        &[r#"
        error: mismatched types
         --> main.plk:3:12
          |
        3 |         if 42 {
          |            ^^ expected `bool`, got `u256`
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
        error: incorrect type for struct field
         --> main.plk:3:23
          |
        3 |     let x = Pair { a: false, b: false };
          |                       ^^^^^ field `a` expects `u256`, got `bool`
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
        2 |     let f = fn(x: u256) never { evm_stop(); };
          |                   ---- `u256` expected because of this
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
fn test_runtime_struct_lit_unknown_field() {
    assert_diagnostics(
        r#"
        const Pair = struct { a: u256, b: bool };
        init {
            let x = Pair { a: 42, c: true, b: false };
            evm_stop();
        }
        "#,
        &[r#"
        error: unexpected field
         --> main.plk:3:27
          |
        3 |     let x = Pair { a: 42, c: true, b: false };
          |                           ^ `Pair` has no field `c`
        "#],
    );
}

#[test]
fn test_runtime_struct_lit_duplicate_field() {
    assert_diagnostics(
        r#"
        const Pair = struct { a: u256, b: bool };
        init {
            let x = Pair { a: 42, a: 99, b: false };
            evm_stop();
        }
        "#,
        &[r#"
        error: duplicate field
         --> main.plk:3:27
          |
        3 |     let x = Pair { a: 42, a: 99, b: false };
          |                    -      ^ `a` assigned more than once
          |                    |
          |                    first assigned here
        "#],
    );
}

#[test]
fn test_runtime_struct_lit_missing_field() {
    assert_diagnostics(
        r#"
        const Pair = struct { a: u256, b: bool };
        init {
            let x = Pair { a: 42 };
            evm_stop();
        }
        "#,
        &[r#"
        error: missing field
         --> main.plk:3:13
          |
        3 |     let x = Pair { a: 42 };
          |             ^^^^^^^^^^^^^^ missing field `b` in `Pair`
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
        TestProject::root("import m::other::f;\ninit { f(1, 2); evm_stop(); }")
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
          |             --------------- defined here
        3 |     let f = fn() u256 { x };
          |                         ^ capture of runtime value
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
            let mut v: bool = x;
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
            let mut v: bool = x;
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
            let mut v: bool = x;
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
            %0 : struct@main.plk:3:9 = struct#7 {
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
        error: use of comptime only value at runtime
         --> main.plk:3:23
          |
        3 |     let T = if cond { u256 } else { bool };
          |                       ^^^^ reference to comptime only value
          |
          = info: `let mut` definitions and mutable assignments require runtime-compatible values
        "#,
            r#"
        error: use of comptime only value at runtime
         --> main.plk:3:37
          |
        3 |     let T = if cond { u256 } else { bool };
          |                                     ^^^^ reference to comptime only value
          |
          = info: `let mut` definitions and mutable assignments require runtime-compatible values
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
fn test_struct_def_duplicate_field() {
    assert_diagnostics(
        r#"
        const S = struct { x: u256, x: bool };
        init { evm_stop(); }
        "#,
        &[r#"
        error: duplicate field name in struct definition
         --> main.plk:1:29
          |
        1 | const S = struct { x: u256, x: bool };
          |                    -        ^ `x` assigned more than once
          |                    |
          |                    first assigned here
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
