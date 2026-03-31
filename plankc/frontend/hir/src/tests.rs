use crate::{BigNumInterner, Hir, display::DisplayHir};
use plank_session::Session;
use plank_source::ParsedProject;
use plank_test_utils::{TestProject, dedent_preserve_blank_lines};

fn try_lower(source: &str) -> (Hir, BigNumInterner, Session, ParsedProject) {
    try_lower_project(TestProject::single(source))
}

fn try_lower_project(project: TestProject) -> (Hir, BigNumInterner, Session, ParsedProject) {
    let mut session = Session::new();
    let project = project.build(&mut session);

    let mut big_nums = BigNumInterner::default();
    let hir = crate::lower(&project, &mut big_nums, &mut session);

    (hir, big_nums, session, project)
}

#[track_caller]
fn assert_lowers_to(source: &str, expected: &str) {
    let (hir, big_nums, session, _project) = try_lower(source);
    assert!(
        session.diagnostics().is_empty(),
        "Expected no diagnostics for valid source, got:\n{:#?}",
        session.diagnostics()
    );
    let actual = format!("{}", DisplayHir::new(&hir, &big_nums, &session));
    let expected = dedent_preserve_blank_lines(expected);

    pretty_assertions::assert_str_eq!(actual.trim(), expected.trim());
}

fn render_diagnostics(source: &str) -> String {
    render_project_diagnostics(TestProject::single(source))
}

fn format_session_diagnostics(session: &Session) -> String {
    session
        .diagnostics()
        .iter()
        .map(|diagnostic| diagnostic.render_plain(session))
        .collect::<Vec<_>>()
        .join("\n")
}

fn render_project_diagnostics(project: TestProject) -> String {
    let (_hir, _big_nums, session, _project) = try_lower_project(project);
    format_session_diagnostics(&session)
}

#[test]
fn test_basic_init_builtin_calls() {
    assert_lowers_to(
        r#"
        init {
            let a = calldataload(0x00);
            let b: u256 = calldataload(0x20);
            let buf = malloc_uninit(0x20);
            mstore32(buf, add(a, b));
            evm_return(buf, 0x20);
        }
        "#,
        r#"
        ==== Constants ====

        ==== Init ====
        %0 = 0
        %1 = calldataload(%0)
        %2 = 32
        %4 = type#1
        %3 : %4 = calldataload(%2)
        %5 = 32
        %6 = malloc_uninit(%5)
        %7 = %6
        %8 = %1
        %9 = %3
        %10 = add(%8, %9)
        eval mstore32(%7, %10)
        %11 = %6
        %12 = 32
        eval evm_return(%11, %12)
        "#,
    );
}

#[test]
fn test_inline_closure_lowering() {
    assert_lowers_to(
        r#"
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
        "#,
        r#"
        ==== Constants ====

        ==== Functions ====
        @fn0() -> %0 {
            preamble:
                %0 = type#6
            body:
                eval evm_stop()
                ret void
        }
        @fn1() -> %0 {
            preamble:
                %0 = type#6
            body:
                eval invalid()
                ret void
        }
        @fn2() -> %0 {
            captures: [%0 -> %1]
            preamble:
                %0 = type#6
            body:
                %2 = %1
                eval call %2()
                ret void
        }

        ==== Init ====
        %0 = @fn0
        %1 = %0
        eval call %1()

        ==== Run ====
        %0 = @fn1
        %1 = @fn2
        %2 = %1
        eval call %2()
        "#,
    );
}

#[test]
fn test_set_undefined() {
    let rendered = render_diagnostics("init { y = 4; }");
    let expected = dedent_preserve_blank_lines(
        r#"
        error: unresolved identifier 'y'
         --> main.plk:1:8
          |
        1 | init { y = 4; }
          |        ^ not found in this scope
        "#,
    );
    pretty_assertions::assert_str_eq!(rendered.trim(), expected.trim());
}

#[test]
fn test_assign_to_immutable_let() {
    let rendered = render_diagnostics(
        r#"
        init {
            let x = 1;
            x = 2;
        }
        "#,
    );
    let expected = dedent_preserve_blank_lines(
        r#"
        error: variable 'x' was not declared mutable
         --> main.plk:3:5
          |
        2 |     let x = 1;
          |         - declared here
        3 |     x = 2;
          |     ^ assignment to immutable variable
          |
          = help: consider declaring it with `let mut`
        "#,
    );
    pretty_assertions::assert_str_eq!(rendered.trim(), expected.trim());
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
        ==== Constants ====
        ConstId(0) ("Pair") result=LocalId(0) {
            %1 = void
            %2 = type#1
            %3 = type#1
            %0 = struct#0 main.plk:1:14
        }
        ConstId(1) ("swap") result=LocalId(0) {
            %0 = @fn0
        }

        ==== Functions ====
        @fn0(%1: %0, %3: %2) -> %4 {
            preamble:
                %0 = type#1
                %2 = type#1
                %4 = $0
            body:
                %5 = $0
                %6 = %3
                %7 = %1
                ret %5 { a: %6, b: %7 }
        }

        ==== Structs ====
        @struct0[index: %1] { a: %2, b: %3 }

        ==== Init ====
        %0 = $1
        %1 = 3
        %2 = 4
        %3 = call %0(%1, %2)
        eval evm_stop()
        "#,
    );
}

#[test]
fn test_assign_to_mutable_let() {
    assert_lowers_to(
        r#"
        init {
            let mut x = 1;
            x = 2;
        }
        "#,
        r#"
        ==== Constants ====

        ==== Init ====
        %0 = 1
        %0 := 2
        "#,
    );
}

#[test]
fn test_unresolved_identifier_diagnostic() {
    let rendered = render_diagnostics("init { x; }");
    let expected = dedent_preserve_blank_lines(
        r#"
        error: unresolved identifier 'x'
         --> main.plk:1:8
          |
        1 | init { x; }
          |        ^ not found in this scope
        "#,
    );
    pretty_assertions::assert_str_eq!(rendered.trim(), expected.trim());
}

#[test]
fn test_multiple_init_blocks() {
    let rendered = render_diagnostics(
        r#"
        init {}
        init {}
        "#,
    );
    let expected = dedent_preserve_blank_lines(
        r#"
        error: multiple init blocks
         --> main.plk:2:1
          |
        1 | init {}
          | ------- previous init block
        2 | init {}
          | ^^^^^^^ duplicate init block
        "#,
    );
    pretty_assertions::assert_str_eq!(rendered.trim(), expected.trim());
}

#[test]
fn test_multiple_run_blocks() {
    let rendered = render_diagnostics(
        r#"
        init {}
        run {}
        run {}
        "#,
    );
    let expected = dedent_preserve_blank_lines(
        r#"
        error: multiple run blocks
         --> main.plk:3:1
          |
        2 | run {}
          | ------ previous run block
        3 | run {}
          | ^^^^^^ duplicate run block
        "#,
    );
    pretty_assertions::assert_str_eq!(rendered.trim(), expected.trim());
}

#[test]
fn test_duplicate_const_def() {
    let rendered = render_diagnostics(
        r#"
        const x = 1;
        const x = 2;
        init {}
        "#,
    );
    let expected = dedent_preserve_blank_lines(
        r#"
        error: duplicate definition of 'x'
         --> main.plk:2:1
          |
        1 | const x = 1;
          | ------------ previously defined here
        2 | const x = 2;
          | ^^^^^^^^^^^^ 'x' redefined here
        "#,
    );
    pretty_assertions::assert_str_eq!(rendered.trim(), expected.trim());
}

#[test]
fn test_init_and_run_outside_entry() {
    let project = TestProject::single("import m::other::*;\ninit {}")
        .add_file(
            "other",
            "
            init {}
            run {}
            ",
        )
        .add_module("m", "");
    let rendered = render_project_diagnostics(project);
    let expected = dedent_preserve_blank_lines(
        r#"
        error: `init` not allowed here
         --> other.plk:1:1
          |
        1 | init {}
          | ^^^^^^^ only the entry file may contain `init`
          |
        note: entry file
         --> main.plk
        error: `run` not allowed here
         --> other.plk:2:1
          |
        2 | run {}
          | ^^^^^^ only the entry file may contain `run`
          |
        note: entry file
         --> main.plk
        "#,
    );
    pretty_assertions::assert_str_eq!(rendered.trim(), expected.trim());
}

#[test]
fn test_import_name_collision() {
    let project = TestProject::single("const x = 1;\nimport m::other::x;\ninit {}")
        .add_file("other", "const x = 2;")
        .add_module("m", "");
    let rendered = render_project_diagnostics(project);
    let expected = dedent_preserve_blank_lines(
        r#"
        error: imported definition collision
         --> main.plk:2:1
          |
        1 | const x = 1;
          | ------------ 'x' previously defined here
        2 | import m::other::x;
          | ^^^^^^^^^^^^^^^^^^^ conflicting import
        "#,
    );
    pretty_assertions::assert_str_eq!(rendered.trim(), expected.trim());
}

#[test]
fn test_glob_import_name_collision() {
    let project = TestProject::single(
        r#"
        const x = 1;
        import m::other::*;
        init {}
        "#,
    )
    .add_file("other", "const x = 2;")
    .add_module("m", "");
    let rendered = render_project_diagnostics(project);
    let expected = dedent_preserve_blank_lines(
        r#"
        error: imported definition collision
         --> main.plk:2:1
          |
        1 | const x = 1;
          | ------------ 'x' previously defined here
        2 | import m::other::*;
          | ^^^^^^^^^^^^^^^^^^^ conflicting import
          |
         ::: other.plk:1:1
          |
        1 | const x = 2;
          | ------------ imported colliding 'x'
        "#,
    );
    pretty_assertions::assert_str_eq!(rendered.trim(), expected.trim());
}

#[test]
fn test_alias_import_collision() {
    let project = TestProject::single("const x = 1;\nimport m::other::y as x;\ninit {}")
        .add_file("other", "const y = 2;")
        .add_module("m", "");
    let rendered = render_project_diagnostics(project);
    let expected = dedent_preserve_blank_lines(
        r#"
        error: imported definition collision
         --> main.plk:2:1
          |
        1 | const x = 1;
          | ------------ 'x' previously defined here
        2 | import m::other::y as x;
          | ^^^^^^^^^^^^^^^^^^^^^^^^ conflicting import
        "#,
    );
    pretty_assertions::assert_str_eq!(rendered.trim(), expected.trim());
}

#[test]
fn test_import_collision_with_previous_import() {
    let project = TestProject::single("import m::a::x;\nimport m::b::x;\ninit {}")
        .add_file("a", "const x = 1;")
        .add_file("b", "const x = 2;")
        .add_module("m", "");
    let rendered = render_project_diagnostics(project);
    let expected = dedent_preserve_blank_lines(
        r#"
        error: imported definition collision
         --> main.plk:2:1
          |
        1 | import m::a::x;
          | --------------- 'x' previously imported here
        2 | import m::b::x;
          | ^^^^^^^^^^^^^^^ conflicting import
        "#,
    );
    pretty_assertions::assert_str_eq!(rendered.trim(), expected.trim());
}

#[test]
fn test_unresolved_import() {
    let project = TestProject::single("import m::other::y;\ninit {}")
        .add_file("other", "const x = 1;")
        .add_module("m", "");
    let rendered = render_project_diagnostics(project);
    let expected = dedent_preserve_blank_lines(
        r#"
        error: unresolved import
         --> main.plk:1:18
          |
        1 | import m::other::y;
          |                  ^ 'y' not found in target module
          |
        info: no definition of 'y' found in file
         --> other.plk
        "#,
    );
    pretty_assertions::assert_str_eq!(rendered.trim(), expected.trim());
}

#[test]
fn test_shadow_primitive_type() {
    let rendered = render_diagnostics("init { let u256 = 1; }");
    let expected = dedent_preserve_blank_lines(
        r#"
        error: shadowing primitive type
         --> main.plk:1:12
          |
        1 | init { let u256 = 1; }
          |            ^^^^ 'u256' is a primitive type
        "#,
    );
    pretty_assertions::assert_str_eq!(rendered.trim(), expected.trim());
}

#[test]
fn test_shadow_builtin() {
    let rendered = render_diagnostics("init { let add = 1; }");
    let expected = dedent_preserve_blank_lines(
        r#"
        error: shadowing built-in function
         --> main.plk:1:12
          |
        1 | init { let add = 1; }
          |            ^^^ 'add' is a built-in function
        "#,
    );
    pretty_assertions::assert_str_eq!(rendered.trim(), expected.trim());
}

#[test]
fn test_missing_init_block() {
    let rendered = render_diagnostics("const x = 1;");
    let expected = dedent_preserve_blank_lines(
        r#"
        error: missing init block
         --> main.plk
          = note: the entry file must contain an init block
        "#,
    );
    pretty_assertions::assert_str_eq!(rendered.trim(), expected.trim());
}

#[test]
fn test_non_call_reference_to_builtin() {
    let rendered = render_diagnostics(
        r#"
        init {
            let mut x = 0;
            x = add;
        }
        "#,
    );
    let expected = dedent_preserve_blank_lines(
        r#"
        error: referencing built-in function as a value
         --> main.plk:3:9
          |
        3 |     x = add;
          |         ^^^ 'add' is a built-in function
          |
          = help: built-in functions must be called directly, wrap in a function if you wish to use it as a first-class value
        "#,
    );
    pretty_assertions::assert_str_eq!(rendered.trim(), expected.trim());
}

#[test]
fn test_number_out_of_range() {
    let rendered = render_diagnostics(
        "init { let x = 0x1FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF; }",
    );
    let expected = dedent_preserve_blank_lines(
        r#"
        error: number literal out of range
         --> main.plk:1:16
          |
        1 | init { let x = 0x1FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF; }
          |                ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ value does not fit in u256
        "#,
    );
    pretty_assertions::assert_str_eq!(rendered.trim(), expected.trim());
}

#[test]
fn test_inline_while_not_yet_supported() {
    let rendered = render_diagnostics(
        r#"
        init {
            inline while true {}
        }
        "#,
    );
    let expected = dedent_preserve_blank_lines(
        r#"
        error: inline while is not yet supported
         --> main.plk:2:5
          |
        2 |     inline while true {}
          |     ^^^^^^^^^^^^^^^^^^^^ not yet supported
        "#,
    );
    pretty_assertions::assert_str_eq!(rendered.trim(), expected.trim());
}

#[test]
fn test_logical_not_literal() {
    assert_lowers_to(
        r#"
        init {
            let x = !true;
            let y = !false;
            evm_stop();
        }
        "#,
        r#"
        ==== Constants ====

        ==== Init ====
        %0 = true
        %1 = logical_not %0
        %2 = false
        %3 = logical_not %2
        eval evm_stop()
        "#,
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
        ==== Constants ====

        ==== Init ====
        %0 = 0
        %1 = calldataload(%0)
        %2 = %1
        %3 = iszero(%2)
        %4 = %3
        %5 = logical_not %4
        eval evm_stop()
        "#,
    );
}

#[test]
fn test_and_desugaring() {
    assert_lowers_to(
        r#"
        const slot_good = fn () bool {
            sstore(0, 0);
            false
        };

        init {
            let a = iszero(calldataload(0));
            let c = a and slot_good();
            evm_stop();
        }
        "#,
        r#"
        ==== Constants ====
        ConstId(0) ("slot_good") result=LocalId(0) {
            %0 = @fn0
        }

        ==== Functions ====
        @fn0() -> %0 {
            preamble:
                %0 = type#2
            body:
                %1 = 0
                %2 = 0
                eval sstore(%1, %2)
                ret false
        }

        ==== Init ====
        %0 = 0
        %1 = calldataload(%0)
        %2 = iszero(%1)
        %4 = %2
        if %4 {
            %5 = $0
            %3 [br]= call %5()
        } else {
            %3 [br]= false
        }
        %6 = %3
        eval evm_stop()
        "#,
    );
}

#[test]
fn test_or_desugaring() {
    assert_lowers_to(
        r#"
        init {
            let a = iszero(calldataload(0));
            let c = a or {
                sstore(1, 1);
                false
            };
            evm_stop();
        }
        "#,
        r#"
        ==== Constants ====

        ==== Init ====
        %0 = 0
        %1 = calldataload(%0)
        %2 = iszero(%1)
        %4 = %2
        if %4 {
            %3 [br]= true
        } else {
            %5 = 1
            %6 = 1
            eval sstore(%5, %6)
            %3 [br]= false
        }
        %7 = %3
        eval evm_stop()
        "#,
    );
}

#[test]
fn test_binary_op_lowering() {
    assert_lowers_to(
        r#"
        init {
            let a = calldataload(0x00);
            let b = calldataload(0x20);
            let c = a + b;
            let d = a -/ b;
            let e = a +/ b;
            let f = a </ b;
            let g = a >/ b;
            let h = a *% b;
            let i = a << b;
            evm_stop();
        }
        "#,
        r#"
        ==== Constants ====

        ==== Init ====
        %0 = 0
        %1 = calldataload(%0)
        %2 = 32
        %3 = calldataload(%2)
        %4 = %1
        %5 = %3
        %6 = (+) %4 %5
        %7 = %1
        %8 = %3
        %9 = (-/) %7 %8
        %10 = %1
        %11 = %3
        %12 = (+/) %10 %11
        %13 = %1
        %14 = %3
        %15 = (</) %13 %14
        %16 = %1
        %17 = %3
        %18 = (>/) %16 %17
        %19 = %1
        %20 = %3
        %21 = (*%) %19 %20
        %22 = %1
        %23 = %3
        %24 = (<<) %22 %23
        eval evm_stop()
        "#,
    );
}

#[test]
fn test_unary_op_lowering() {
    assert_lowers_to(
        r#"
        init {
            let a = calldataload(0x00);
            let b = -a;
            let c = ~a;
            evm_stop();
        }
        "#,
        r#"
        ==== Constants ====

        ==== Init ====
        %0 = 0
        %1 = calldataload(%0)
        %2 = %1
        %3 = (-) %2
        %4 = %1
        %5 = (~) %4
        eval evm_stop()
        "#,
    );
}

#[test]
fn test_lone_slash_not_supported() {
    let (hir, big_nums, session, _project) = try_lower(
        r#"
        init {
            let a = 10;
            let b = a / 2;
            evm_stop();
        }
        "#,
    );

    let rendered = format_session_diagnostics(&session);
    let expected_diag = dedent_preserve_blank_lines(
        r#"
        error: unsupported syntax
         --> main.plk:3:15
          |
        3 |     let b = a / 2;
          |               ^ lone `/` not supported as an operator
          |
          = help: for division rounding towards 0 use `</` (EVM default)
          = help: for division rounding away from 0 use `>/`
          = help: for division rounding towards negative infinity use `-/`
          = help: for division rounding towards positive infinity use `+/`
        "#,
    );
    pretty_assertions::assert_str_eq!(rendered.trim(), expected_diag.trim());

    let actual_hir = format!("{}", DisplayHir::new(&hir, &big_nums, &session));
    let expected_hir = dedent_preserve_blank_lines(
        r#"
        ==== Constants ====

        ==== Init ====
        %0 = 10
        %1 = %0
        %2 = 2
        %3 = (</) %1 %2
        eval evm_stop()
        "#,
    );
    pretty_assertions::assert_str_eq!(actual_hir.trim(), expected_hir.trim());
}
