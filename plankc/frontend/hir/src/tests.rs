use crate::{Hir, display::DisplayHir};
use plank_session::Session;
use plank_source::ParsedProject;
use plank_test_utils::{TestProject, dedent_preserve_blank_lines};
use plank_values::ValueInterner;

fn try_lower(source: &str) -> (Hir, ValueInterner, Session, ParsedProject) {
    try_lower_project(source)
}

fn try_lower_project(
    project: impl Into<TestProject>,
) -> (Hir, ValueInterner, Session, ParsedProject) {
    let project = project.into();
    let mut session = Session::new();
    let project = project.build(&mut session);

    let mut big_nums = ValueInterner::new();
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
    render_project_diagnostics(TestProject::root(source))
}

fn format_session_diagnostics(session: &Session) -> String {
    session
        .diagnostics()
        .iter()
        .map(|diagnostic| diagnostic.render_plain(session))
        .collect::<Vec<_>>()
        .join("\n")
}

fn render_project_diagnostics(project: impl Into<TestProject>) -> String {
    let (_hir, _big_nums, session, _project) = try_lower_project(project);
    format_session_diagnostics(&session)
}

#[test]
fn test_basic_init_builtin_calls() {
    assert_lowers_to(
        r#"
        init {
            let a = @evm_calldataload(0x00);
            let b: u256 = @evm_calldataload(0x20);
            let buf = @malloc_uninit(0x20);
            @mstore32(buf, @evm_add(a, b));
            @evm_return(buf, 0x20);
        }
        "#,
        r#"
        ==== Constants ====

        ==== Init ====
        %0 = 0
        %1 = @evm_calldataload(%0)
        %2 = type:u256
        %3 = 32
        %4 : %2 = @evm_calldataload(%3)
        %5 = 32
        %6 = @malloc_uninit(%5)
        %7 = %6
        %8 = %1
        %9 = %4
        %10 = @evm_add(%8, %9)
        eval @mstore32(%7, %10)
        %11 = %6
        %12 = 32
        eval @evm_return(%11, %12)
        "#,
    );
}

#[test]
fn test_eager_fn_lowering() {
    assert_lowers_to(
        r#"
        const f = eager fn(x: u256) u256 { x };
        init { @evm_stop(); }
        "#,
        r#"
        ==== Constants ====
        ConstId(0) ("f") result=LocalId(0) {
            %0 = @fn0
        }

        ==== Functions ====
        eager @fn0(%1: %0) -> %2 {
            preamble:
                %0 = type:u256
                param#0 %1 : %0
                %2 = type:u256
            body:
                ret %1
        }

        ==== Init ====
        eval @evm_stop()
        "#,
    );
}

#[test]
fn test_inline_closure_lowering() {
    assert_lowers_to(
        r#"
        init {
            let halt = fn() never {
                @evm_stop();
            };
            halt();
        }
        run {
            let halt = fn() never {
                @evm_invalid();
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
                %0 = type:never
            body:
                eval @evm_stop()
                ret type:tuple {}
        }
        @fn1() -> %0 {
            preamble:
                %0 = type:never
            body:
                eval @evm_invalid()
                ret type:tuple {}
        }
        @fn2() -> %0 {
            captures: [%0 -> %1]
            preamble:
                %0 = type:never
            body:
                %2 = %1
                eval call %2()
                ret type:tuple {}
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
    let rendered = render_diagnostics(
        r#"
        init { y = 4; }
        "#,
    );
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
fn test_return_in_fn_param_type_expression() {
    let rendered = render_diagnostics(
        r#"
        const f = fn (x: { return 0; u256 }) void {
            @evm_stop();
        };
        init { @evm_stop(); }
        "#,
    );
    let expected = dedent_preserve_blank_lines(
        r#"
        error: return is not allowed outside of function bodies
         --> main.plk:1:20
          |
        1 | const f = fn (x: { return 0; u256 }) void {
          |                    ^^^^^^^^^ not allowed here
        "#,
    );
    pretty_assertions::assert_str_eq!(rendered.trim(), expected.trim());
}

#[test]
fn test_duplicate_any_type_capture() {
    let rendered = render_diagnostics(
        r#"
        const f = fn(a: $T, b: $T) void {};
        init {}
        "#,
    );
    let expected = dedent_preserve_blank_lines(
        r#"
        error: any-type capture conflicts with existing binding
         --> main.plk:1:24
          |
        1 | const f = fn(a: $T, b: $T) void {};
          |                 --     ^^ `T` is already defined
          |                 |
          |                 previous binding here
          |
          = help: use `T` directly to refer to the existing type
        "#,
    );
    pretty_assertions::assert_str_eq!(rendered.trim(), expected.trim());
}

#[test]
fn test_any_type_capture_conflicts_with_comptime_param() {
    let rendered = render_diagnostics(
        r#"
        const f = fn(comptime T: type, value: $T) void {};
        init {}
        "#,
    );
    let expected = dedent_preserve_blank_lines(
        r#"
        error: any-type capture conflicts with existing binding
         --> main.plk:1:39
          |
        1 | const f = fn(comptime T: type, value: $T) void {};
          |                       -               ^^ `T` is already defined
          |                       |
          |                       previous binding here
          |
          = help: use `T` directly to refer to the existing type
        "#,
    );
    pretty_assertions::assert_str_eq!(rendered.trim(), expected.trim());
}

#[test]
fn test_comptime_param_conflicts_with_previous_any_type_capture() {
    let rendered = render_diagnostics(
        r#"
        const f = fn(value: $T, comptime T: type) void {};
        init {}
        "#,
    );
    let expected = dedent_preserve_blank_lines(
        r#"
        error: duplicate function parameter
         --> main.plk:1:34
          |
        1 | const f = fn(value: $T, comptime T: type) void {};
          |                     --           ^ `T` is already defined
          |                     |
          |                     previous parameter here
          |
          = help: choose a different parameter name
        "#,
    );
    pretty_assertions::assert_str_eq!(rendered.trim(), expected.trim());
}

#[test]
fn test_duplicate_comptime_param() {
    let rendered = render_diagnostics(
        r#"
        const f = fn(comptime T: type, comptime T: type) void {};
        init {}
        "#,
    );
    let expected = dedent_preserve_blank_lines(
        r#"
        error: duplicate function parameter
         --> main.plk:1:41
          |
        1 | const f = fn(comptime T: type, comptime T: type) void {};
          |                       -                 ^ `T` is already defined
          |                       |
          |                       previous parameter here
          |
          = help: choose a different parameter name
        "#,
    );
    pretty_assertions::assert_str_eq!(rendered.trim(), expected.trim());
}

#[test]
fn test_duplicate_runtime_param() {
    let rendered = render_diagnostics(
        r#"
        const f = fn(a: u256, a: bool) void {};
        init {}
        "#,
    );
    let expected = dedent_preserve_blank_lines(
        r#"
        error: duplicate function parameter
         --> main.plk:1:23
          |
        1 | const f = fn(a: u256, a: bool) void {};
          |              -        ^ `a` is already defined
          |              |
          |              previous parameter here
          |
          = help: choose a different parameter name
        "#,
    );
    pretty_assertions::assert_str_eq!(rendered.trim(), expected.trim());
}

#[test]
fn test_self_type_outside_method_diagnostic() {
    let rendered = render_diagnostics(
        r#"
        const f = fn(value: Self) void {};
        init {}
        "#,
    );
    let expected = dedent_preserve_blank_lines(
        r#"
        error: `Self` type outside method
         --> main.plk:1:21
          |
        1 | const f = fn(value: Self) void {};
          |                     ^^^^ `Self` is only available in methods
        "#,
    );
    pretty_assertions::assert_str_eq!(rendered.trim(), expected.trim());
}

#[test]
fn test_duplicate_struct_member_name_diagnostic() {
    let rendered = render_diagnostics(
        r#"
        const S = struct { f: u256 fn f() void {} };
        init {}
        "#,
    );
    let expected = dedent_preserve_blank_lines(
        r#"
        error: method name conflicts with field name
         --> main.plk:1:31
          |
        1 | const S = struct { f: u256 fn f() void {} };
          |                    -          ^ method `f` has the same name as a field
          |                    |
          |                    field `f` declared here
        "#,
    );
    pretty_assertions::assert_str_eq!(rendered.trim(), expected.trim());
}

#[test]
fn test_duplicate_method_name_diagnostic() {
    let rendered = render_diagnostics(
        r#"
        const S = struct {
            fn f() void {}
            fn f() void {}
        };
        init {}
        "#,
    );
    let expected = dedent_preserve_blank_lines(
        r#"
        error: duplicate method name
         --> main.plk:3:8
          |
        2 |     fn f() void {}
          |        - previous method `f` declared here
        3 |     fn f() void {}
          |        ^ duplicate method `f`
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
            @evm_stop();
        }
        "#,
        r#"
        ==== Constants ====
        ConstId(0) ("Pair") result=LocalId(0) {
            %1 = type:tuple {}
            %2 = type:u256
            %3 = type:u256
            %0 = struct#0 main.plk:1:14
        }
        ConstId(1) ("swap") result=LocalId(0) {
            %0 = @fn0
        }

        ==== Functions ====
        @fn0(%1: %0, %3: %2) -> %4 {
            preamble:
                %0 = type:u256
                param#0 %1 : %0
                %2 = type:u256
                param#1 %3 : %2
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
        eval @evm_stop()
        "#,
    );
}

#[test]
fn test_struct_method_lowering() {
    assert_lowers_to(
        r#"
        const S = struct {
            fn make() Self { Self }
            fn id(value: Self) Self { value }
        };
        init {}
        "#,
        r#"
        ==== Constants ====
        ConstId(0) ("S") result=LocalId(0) {
            %1 = type:tuple {}
            %0 = struct#0 main.plk:1:11
        }

        ==== Functions ====
        @fn0() -> %1 {
            preamble:
                %1 = %0
            body:
                ret %0
        }
        @fn1(%2: %1) -> %3 {
            preamble:
                %1 = %0
                param#0 %2 : %1
                %3 = %0
            body:
                ret %2
        }

        ==== Structs ====
        @struct0[index: %1] {
            methods: {
                make [Self: %0]: @fn0,
                id [Self: %0]: @fn1
            }
        }

        ==== Init ====
        "#,
    );
}

#[test]
fn test_method_call_lowering() {
    assert_lowers_to(
        r#"
        const S = struct {
            fn make(value: u256, other: bool) Self { Self }
            fn id(value: Self, x: u256) Self { value }
        };
        init {
            S.make(1, true);
            let value: S = S {};
            value.id(2);
        }
        "#,
        r#"
        ==== Constants ====
        ConstId(0) ("S") result=LocalId(0) {
            %1 = type:tuple {}
            %0 = struct#0 main.plk:1:11
        }

        ==== Functions ====
        @fn0(%2: %1, %4: %3) -> %5 {
            preamble:
                %1 = type:u256
                param#0 %2 : %1
                %3 = type:bool
                param#1 %4 : %3
                %5 = %0
            body:
                ret %0
        }
        @fn1(%2: %1, %4: %3) -> %5 {
            preamble:
                %1 = %0
                param#0 %2 : %1
                %3 = type:u256
                param#1 %4 : %3
                %5 = %0
            body:
                ret %2
        }

        ==== Structs ====
        @struct0[index: %1] {
            methods: {
                make [Self: %0]: @fn0,
                id [Self: %0]: @fn1
            }
        }

        ==== Init ====
        %0 = $0
        %1 = 1
        %2 = true
        eval method_call make %0(%1, %2)
        %3 = $0
        %4 = $0
        %5 : %3 = %4 {}
        %6 = %5
        %7 = 2
        eval method_call id %6(%7)
        "#,
    );
}

#[test]
fn test_tuple_type_and_literal() {
    assert_lowers_to(
        r#"
        const Pair = tuple { u256, bool };

        init {
            let pair: Pair = (2, true);
        }
        "#,
        r#"
        ==== Constants ====
        ConstId(0) ("Pair") result=LocalId(0) {
            %1 = type:u256
            %2 = type:bool
            %0 = tuple_type (%1, %2)
        }

        ==== Init ====
        %0 = $0
        %1 = 2
        %2 = true
        %3 : %0 = tuple_value (%1, %2)
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
        %0 [mut]= 1
        %0 := 2
        "#,
    );
}

#[test]
fn test_unresolved_identifier_diagnostic() {
    let rendered = render_diagnostics(
        r#"
        init { x; }
        "#,
    );
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
fn test_duplicated_const_def_should_not_be_lowered_into_hir() {
    let project = TestProject::root(
        r#"
        import m::other::f2;

        const f1 = fn (comptime T: type) void {
            f2;
        };
        init {}
        "#,
    )
    .add_file(
        "other",
        r#"
        const f2 = fn () void { };
        const f2 = fn () void { };
        "#,
    )
    .add_module("m", "");

    let (hir, big_nums, session, _) = try_lower_project(project);

    let rendered = format_session_diagnostics(&session);
    let expected = dedent_preserve_blank_lines(
        r#"
        error: duplicate definition of 'f2'
         --> other.plk:2:1
          |
        1 | const f2 = fn () void { };
          | -------------------------- previously defined here
        2 | const f2 = fn () void { };
          | ^^^^^^^^^^^^^^^^^^^^^^^^^^ 'f2' redefined here
        "#,
    );
    pretty_assertions::assert_str_eq!(rendered.trim(), expected.trim());

    let actual_hir = format!("{}", DisplayHir::new(&hir, &big_nums, &session));
    let expected_hir = dedent_preserve_blank_lines(
        r#"
        ==== Constants ====
        ConstId(0) ("f1") result=LocalId(0) {
            %0 = @fn0
        }
        ConstId(1) ("f2") result=LocalId(0) {
            %0 = @fn2
        }

        ==== Functions ====
        @fn0(comptime %1: %0) -> %2 {
            preamble:
                %0 = type:type
                [comptime] param#0 %1 : %0
                %2 = type:tuple {}
            body:
                eval $1
                ret type:tuple {}
        }
        @fn1() -> %0 {
            preamble:
                %0 = type:tuple {}
            body:
                ret type:tuple {}
        }
        @fn2() -> %0 {
            preamble:
                %0 = type:tuple {}
            body:
                ret type:tuple {}
        }

        ==== Init ====
        "#,
    );
    pretty_assertions::assert_str_eq!(actual_hir.trim(), expected_hir.trim());
}

#[test]
fn test_import_name_collision() {
    let project = TestProject::root(
        r#"
        const x = 1;
        import m::other::x;
        init {}
        "#,
    )
    .add_file(
        "other",
        r#"
        const x = 2;
        "#,
    )
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
    let project = TestProject::root(
        r#"
        const x = 1;
        import m::other::*;
        init {}
        "#,
    )
    .add_file(
        "other",
        r#"
        const x = 2;
        "#,
    )
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
    let project = TestProject::root(
        r#"
        const x = 1;
        import m::other::y as x;
        init {}
        "#,
    )
    .add_file(
        "other",
        r#"
        const y = 2;
        "#,
    )
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
    let project = TestProject::root(
        r#"
        import m::a::x;
        import m::b::x;
        init {}
        "#,
    )
    .add_file(
        "a",
        r#"
        const x = 1;
        "#,
    )
    .add_file(
        "b",
        r#"
        const x = 2;
        "#,
    )
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
    let project = TestProject::root(
        r#"
        import m::other::y;
        init {}
        "#,
    )
    .add_file(
        "other",
        r#"
        const x = 1;
        "#,
    )
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
    let rendered = render_diagnostics(
        r#"
        init { let u256 = 1; }
        "#,
    );
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
    let rendered = render_diagnostics("init { let @evm_add = 1; }");
    let expected = dedent_preserve_blank_lines(
        r#"
        error: unexpected builtin name
         --> main.plk:1:12
          |
        1 | init { let @evm_add = 1; }
          |            ^^^^^^^^ unexpected builtin name, expected one of `mut`, identifier
          |
          = help: `@name` syntax is reserved for builtins and cannot be used as an identifier
        "#,
    );
    pretty_assertions::assert_str_eq!(rendered.trim(), expected.trim());
}

#[test]
fn test_missing_init_block() {
    let rendered = render_diagnostics(
        r#"
        const x = 1;
        "#,
    );
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
fn test_root_run_without_init() {
    let rendered = render_diagnostics(
        r#"
        run {}
        "#,
    );
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
            x = @evm_add;
        }
        "#,
    );
    let expected = dedent_preserve_blank_lines(
        r#"
        error: referencing built-in function as a value
         --> main.plk:3:9
          |
        3 |     x = @evm_add;
          |         ^^^^^^^^ '@evm_add' is a built-in function
          |
          = help: built-in functions must be called directly, wrap in a function if you wish to use it as a first-class value
        "#,
    );
    pretty_assertions::assert_str_eq!(rendered.trim(), expected.trim());
}

#[test]
fn test_unknown_builtin_call() {
    let rendered = render_diagnostics(
        r#"
        init {
            let _ = @skibidi(1, 2);
        }
        "#,
    );
    let expected = dedent_preserve_blank_lines(
        r#"
        error: unknown builtin '@skibidi'
         --> main.plk:2:13
          |
        2 |     let _ = @skibidi(1, 2);
          |             ^^^^^^^^ no built-in function with this name
        "#,
    );
    pretty_assertions::assert_str_eq!(rendered.trim(), expected.trim());
}

#[test]
fn test_unknown_builtin_non_call() {
    let rendered = render_diagnostics(
        r#"
        init {
            let _ = @skibidi;
        }
        "#,
    );
    let expected = dedent_preserve_blank_lines(
        r#"
        error: unknown builtin '@skibidi'
         --> main.plk:2:13
          |
        2 |     let _ = @skibidi;
          |             ^^^^^^^^ no built-in function with this name
        "#,
    );
    pretty_assertions::assert_str_eq!(rendered.trim(), expected.trim());
}

#[test]
fn test_unknown_builtin_call_still_lowers_args() {
    let rendered = render_diagnostics(
        r#"
        init {
            let _ = @nonexistent(@other_unknown(1), foo);
        }
        "#,
    );
    let expected = dedent_preserve_blank_lines(
        r#"
        error: unknown builtin '@other_unknown'
         --> main.plk:2:26
          |
        2 |     let _ = @nonexistent(@other_unknown(1), foo);
          |                          ^^^^^^^^^^^^^^ no built-in function with this name
        error: unresolved identifier 'foo'
         --> main.plk:2:45
          |
        2 |     let _ = @nonexistent(@other_unknown(1), foo);
          |                                             ^^^ not found in this scope
        error: unknown builtin '@nonexistent'
         --> main.plk:2:13
          |
        2 |     let _ = @nonexistent(@other_unknown(1), foo);
          |             ^^^^^^^^^^^^ no built-in function with this name
        "#,
    );
    pretty_assertions::assert_str_eq!(rendered.trim(), expected.trim());
}

#[test]
fn test_at_ident_not_allowed_as_binding() {
    let rendered = render_diagnostics(
        r#"
        init {
            let @skibidi = 1;
        }
        "#,
    );
    let expected = dedent_preserve_blank_lines(
        r#"
        error: unexpected builtin name
         --> main.plk:2:9
          |
        2 |     let @skibidi = 1;
          |         ^^^^^^^^ unexpected builtin name, expected one of `mut`, identifier
          |
          = help: `@name` syntax is reserved for builtins and cannot be used as an identifier
        "#,
    );
    pretty_assertions::assert_str_eq!(rendered.trim(), expected.trim());
}

#[test]
fn test_number_out_of_range() {
    let rendered = render_diagnostics(
        r#"
        init { let x = 0x1FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF; }
        "#,
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
fn test_inline_while_lowering() {
    assert_lowers_to(
        r#"
        init {
            inline while false {
                let x = 1;
            }
        }
        "#,
        r#"
        ==== Constants ====

        ==== Init ====
        [inline] while {
            cond:
                %0 = false
            test %0
            body:
                %1 = 1
        }
        "#,
    );
}

#[test]
fn test_logical_not_literal() {
    assert_lowers_to(
        r#"
        init {
            let x = !true;
            let y = !false;
            @evm_stop();
        }
        "#,
        r#"
        ==== Constants ====

        ==== Init ====
        %0 = true
        %1 = logical_not %0
        %2 = false
        %3 = logical_not %2
        eval @evm_stop()
        "#,
    );
}

#[test]
fn test_logical_not_runtime() {
    assert_lowers_to(
        r#"
        init {
            let c = @evm_calldataload(0);
            let b = @evm_iszero(c);
            let nb = !b;
            @evm_stop();
        }
        "#,
        r#"
        ==== Constants ====

        ==== Init ====
        %0 = 0
        %1 = @evm_calldataload(%0)
        %2 = %1
        %3 = @evm_iszero(%2)
        %4 = %3
        %5 = logical_not %4
        eval @evm_stop()
        "#,
    );
}

#[test]
fn test_and_desugaring() {
    assert_lowers_to(
        r#"
        const slot_good = fn () bool {
            @evm_sstore(0, 0);
            false
        };

        init {
            let a = @evm_iszero(@evm_calldataload(0));
            let c = a and slot_good();
            @evm_stop();
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
                %0 = type:bool
            body:
                %1 = 0
                %2 = 0
                eval @evm_sstore(%1, %2)
                ret false
        }

        ==== Init ====
        %0 = 0
        %1 = @evm_calldataload(%0)
        %2 = @evm_iszero(%1)
        %4 = %2
        %3 <- if %4 {
            %5 = $0
            %3 [br]= call %5()
        } else {
            %3 [br]= false
        }
        %6 = %3
        eval @evm_stop()
        "#,
    );
}

#[test]
fn test_or_desugaring() {
    assert_lowers_to(
        r#"
        init {
            let a = @evm_iszero(@evm_calldataload(0));
            let c = a or {
                @evm_sstore(1, 1);
                false
            };
            @evm_stop();
        }
        "#,
        r#"
        ==== Constants ====

        ==== Init ====
        %0 = 0
        %1 = @evm_calldataload(%0)
        %2 = @evm_iszero(%1)
        %4 = %2
        %3 <- if %4 {
            %3 [br]= true
        } else {
            %5 = 1
            %6 = 1
            eval @evm_sstore(%5, %6)
            %3 [br]= false
        }
        %7 = %3
        eval @evm_stop()
        "#,
    );
}

#[test]
fn test_binary_op_lowering() {
    assert_lowers_to(
        r#"
        init {
            let a = @evm_calldataload(0x00);
            let b = @evm_calldataload(0x20);
            let c = a + b;
            let d = a -/ b;
            let e = a +/ b;
            let f = a </ b;
            let g = a >/ b;
            let h = a *% b;
            let i = a << b;
            @evm_stop();
        }
        "#,
        r#"
        ==== Constants ====

        ==== Init ====
        %0 = 0
        %1 = @evm_calldataload(%0)
        %2 = 32
        %3 = @evm_calldataload(%2)
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
        eval @evm_stop()
        "#,
    );
}

#[test]
fn test_unary_op_lowering() {
    assert_lowers_to(
        r#"
        init {
            let a = @evm_calldataload(0x00);
            let b = -a;
            let c = ~a;
            @evm_stop();
        }
        "#,
        r#"
        ==== Constants ====

        ==== Init ====
        %0 = 0
        %1 = @evm_calldataload(%0)
        %2 = %1
        %3 = (-) %2
        %4 = %1
        %5 = (~) %4
        eval @evm_stop()
        "#,
    );
}

#[test]
fn test_dependent_param_type() {
    assert_lowers_to(
        r#"
        init {
            let f = fn (n: u256, x: n) u256 { x };
            @evm_stop();
        }
        "#,
        r#"
        ==== Constants ====

        ==== Functions ====
        @fn0(%1: %0, %3: %2) -> %4 {
            preamble:
                %0 = type:u256
                param#0 %1 : %0
                %2 = %1
                param#1 %3 : %2
                %4 = type:u256
            body:
                ret %3
        }

        ==== Init ====
        %0 = @fn0
        eval @evm_stop()
        "#,
    );
}

#[test]
fn test_chained_dependent_params_with_comptime() {
    assert_lowers_to(
        r#"
        init {
            let f = fn (comptime n: u256, y: n, z: y) n { z };
            @evm_stop();
        }
        "#,
        r#"
        ==== Constants ====

        ==== Functions ====
        @fn0(comptime %1: %0, %3: %2, %5: %4) -> %6 {
            preamble:
                %0 = type:u256
                [comptime] param#0 %1 : %0
                %2 = %1
                param#1 %3 : %2
                %4 = %3
                param#2 %5 : %4
                %6 = %1
            body:
                ret %5
        }

        ==== Init ====
        %0 = @fn0
        eval @evm_stop()
        "#,
    );
}

#[test]
fn test_self_ref_lower() {
    assert_lowers_to(
        r#"
        const A = {
            let x = 3;
            A
        };
        init { @evm_stop(); }
        "#,
        r#"

        ==== Constants ====
        ConstId(0) ("A") result=LocalId(0) {
            %1 = 3
            %0 = $0
        }

        ==== Init ====
        eval @evm_stop()
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
            @evm_stop();
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
        eval @evm_stop()
        "#,
    );
    pretty_assertions::assert_str_eq!(actual_hir.trim(), expected_hir.trim());
}

#[test]
fn test_builtin_name_without_at_is_valid_identifier() {
    assert_lowers_to(
        r#"
        init {
            let is_struct = 1;
            let field_count = is_struct;
        }
        "#,
        r#"
        ==== Constants ====

        ==== Init ====
        %0 = 1
        %1 = %0
        "#,
    );
}

#[test]
fn test_unresolved_bare_builtin_name_suggests_at() {
    let rendered = render_diagnostics(
        r#"
        init {
            let x = evm_add(1, 2);
        }
        "#,
    );
    let expected = dedent_preserve_blank_lines(
        r#"
        error: unresolved identifier 'evm_add'
         --> main.plk:2:13
          |
        2 |     let x = evm_add(1, 2);
          |             ^^^^^^^ not found in this scope
          |
          = help: if you meant the builtin, use `@evm_add`
        "#,
    );
    pretty_assertions::assert_str_eq!(rendered.trim(), expected.trim());
}

#[test]
fn test_import_group_unresolved_item() {
    let project = TestProject::root(
        r#"
        import m::other::{a, b};
        init { @evm_stop(); }
        "#,
    )
    .add_file(
        "other",
        r#"
        const a = 1;
        "#,
    )
    .add_module("m", "");
    let rendered = render_project_diagnostics(project);
    let expected = dedent_preserve_blank_lines(
        r#"
        error: unresolved import
         --> main.plk:1:22
          |
        1 | import m::other::{a, b};
          |                      ^ 'b' not found in target module
          |
        info: no definition of 'b' found in file
         --> other.plk
        "#,
    );
    pretty_assertions::assert_str_eq!(rendered.trim(), expected.trim());
}

#[test]
fn test_import_group_collision_with_local() {
    let project = TestProject::root(
        r#"
        const x = 1;
        import m::other::{a, b as x};
        init { @evm_stop(); }
        "#,
    )
    .add_file(
        "other",
        r#"
        const a = 1;
        const b = 2;
        "#,
    )
    .add_module("m", "");
    let rendered = render_project_diagnostics(project);
    let expected = dedent_preserve_blank_lines(
        r#"
        error: imported definition collision
         --> main.plk:2:22
          |
        1 | const x = 1;
          | ------------ 'x' previously defined here
        2 | import m::other::{a, b as x};
          |                      ^^^^^^ conflicting import
        "#,
    );
    pretty_assertions::assert_str_eq!(rendered.trim(), expected.trim());
}

#[test]
fn test_import_group_collision_with_other_import() {
    let project = TestProject::root(
        r#"
        import m::a::x;
        import m::b::{y, x};
        init { @evm_stop(); }
        "#,
    )
    .add_file(
        "a",
        r#"
        const x = 1;
        "#,
    )
    .add_file(
        "b",
        r#"
        const y = 2;
        const x = 3;
        "#,
    )
    .add_module("m", "");
    let rendered = render_project_diagnostics(project);
    let expected = dedent_preserve_blank_lines(
        r#"
        error: imported definition collision
         --> main.plk:2:18
          |
        1 | import m::a::x;
          | --------------- 'x' previously imported here
        2 | import m::b::{y, x};
          |                  ^ conflicting import
        "#,
    );
    pretty_assertions::assert_str_eq!(rendered.trim(), expected.trim());
}

#[test]
fn test_import_group_self_collision() {
    let project = TestProject::root(
        r#"
        import m::other::{a as x, b as x};
        init { @evm_stop(); }
        "#,
    )
    .add_file(
        "other",
        r#"
        const a = 1;
        const b = 2;
        "#,
    )
    .add_module("m", "");
    let rendered = render_project_diagnostics(project);
    let expected = dedent_preserve_blank_lines(
        r#"
        error: imported definition collision
         --> main.plk:1:27
          |
        1 | import m::other::{a as x, b as x};
          |                   ------  ^^^^^^ conflicting import
          |                   |
          |                   'x' previously imported here
        "#,
    );
    pretty_assertions::assert_str_eq!(rendered.trim(), expected.trim());
}

#[test]
fn test_explicit_return_in_function() {
    assert_lowers_to(
        r#"
        const add_one = fn (x: u256) u256 {
            return @evm_add(x, 1);
        };
        init { @evm_stop(); }
        "#,
        r#"
        ==== Constants ====
        ConstId(0) ("add_one") result=LocalId(0) {
            %0 = @fn0
        }

        ==== Functions ====
        @fn0(%1: %0) -> %2 {
            preamble:
                %0 = type:u256
                param#0 %1 : %0
                %2 = type:u256
            body:
                %3 = %1
                %4 = 1
                ret @evm_add(%3, %4)
                ret type:tuple {}
        }

        ==== Init ====
        eval @evm_stop()
        "#,
    );
}

#[test]
fn test_return_in_comptime_block() {
    let rendered = render_diagnostics(
        r#"
        const x = fn () void {
            if @evm_calldataload(0) == 0 {
                comptime {
                    return ();
                }
            }
            @evm_revert(0, 0);
        };

        init {
            x();
            @evm_stop();
        }
        "#,
    );
    let expected = dedent_preserve_blank_lines(
        r#"
        error: return is not allowed in comptime blocks
         --> main.plk:4:13
          |
        4 |             return ();
          |             ^^^^^^^^^^ not allowed here
          |
          = help: if the function is already comptime, remove the comptime block
        "#,
    );
    pretty_assertions::assert_str_eq!(rendered.trim(), expected.trim());
}

#[test]
fn test_return_in_function_nested_in_comptime_block() {
    assert_lowers_to(
        r#"
        const outer = fn () void {
            comptime {
                let inner = fn () void {
                    return ();
                };
            }
        };
        init {}
        "#,
        r#"
        ==== Constants ====
        ConstId(0) ("outer") result=LocalId(0) {
            %0 = @fn1
        }

        ==== Functions ====
        @fn0() -> %0 {
            preamble:
                %0 = type:tuple {}
            body:
                ret tuple_value ()
                ret type:tuple {}
        }
        @fn1() -> %0 {
            preamble:
                %0 = type:tuple {}
            body:
                comptime {
                    %2 = @fn0
                    %1 = type:tuple {}
                }
                ret %1
        }

        ==== Init ====
        "#,
    );
}

#[test]
fn test_return_outside_function_body() {
    let source = r#"
        init {
            let a = @evm_add(1, 2);
            return a;
            let b = @evm_add(3, 4);
        }
    "#;

    let (hir, big_nums, session, _project) = try_lower(source);

    let diagnostics = format_session_diagnostics(&session);
    let expected_diagnostics = dedent_preserve_blank_lines(
        r#"
        error: return is not allowed outside of function bodies
         --> main.plk:3:5
          |
        3 |     return a;
          |     ^^^^^^^^^ not allowed here
        "#,
    );
    pretty_assertions::assert_str_eq!(diagnostics.trim(), expected_diagnostics.trim());

    let actual_hir = format!("{}", DisplayHir::new(&hir, &big_nums, &session));
    let expected_hir = dedent_preserve_blank_lines(
        r#"
        ==== Constants ====

        ==== Init ====
        %0 = 1
        %1 = 2
        %2 = @evm_add(%0, %1)
        eval %2
        %3 = 3
        %4 = 4
        %5 = @evm_add(%3, %4)
        "#,
    );
    pretty_assertions::assert_str_eq!(actual_hir.trim(), expected_hir.trim());
}

#[test]
fn test_match_fallback_cannot_shadow_primitive_type() {
    let rendered = render_diagnostics(
        r#"
        init {
            let selector = @evm_calldataload(0);
            let x = match selector {
                else u256 => u256,
            };
            @evm_stop();
        }
        "#,
    );
    let expected = dedent_preserve_blank_lines(
        r#"
        error: shadowing primitive type
         --> main.plk:4:14
          |
        4 |         else u256 => u256,
          |              ^^^^ 'u256' is a primitive type
        "#,
    );
    pretty_assertions::assert_str_eq!(rendered.trim(), expected.trim());
}

#[test]
fn test_multiple_match_else_arms() {
    let (hir, big_nums, session, _project) = try_lower(
        r#"
        init {
            let x = match 0 {
                else => 1,
                else other => other,
            };
        }
        "#,
    );
    let rendered = format_session_diagnostics(&session);
    let expected = dedent_preserve_blank_lines(
        r#"
        error: multiple else arms in match
         --> main.plk:4:9
          |
        3 |         else => 1,
          |         --------- previous else arm
        4 |         else other => other,
          |         ^^^^^^^^^^^^^^^^^^^ duplicate else arm
          |
          = note: a match expression can have only one else arm
        "#,
    );
    pretty_assertions::assert_str_eq!(rendered.trim(), expected.trim());

    let actual_hir = format!("{}", DisplayHir::new(&hir, &big_nums, &session));
    let expected_hir = dedent_preserve_blank_lines(
        r#"
        ==== Constants ====

        ==== Init ====
        %0 = 0
        %2 = <poison>
        %3 = <poison>
        match %0 {
            %2 => {
                %1 [br]= %3
            }
            else => {
                %1 [br]= 1
            }
        }
        %4 = %1
        "#,
    );
    pretty_assertions::assert_str_eq!(actual_hir.trim(), expected_hir.trim());
}

#[test]
fn test_missing_match_else_arm() {
    let rendered = render_diagnostics(
        r#"
        init {
            let x = match 0 {
                1 => 1,
            };
        }
        "#,
    );
    let expected = dedent_preserve_blank_lines(
        r#"
        error: missing else arm in match
         --> main.plk:2:13
          |
        2 |       let x = match 0 {
          |  _____________^
        3 | |         1 => 1,
        4 | |     };
          | |_____^ match expression requires an else arm
        "#,
    );
    pretty_assertions::assert_str_eq!(rendered.trim(), expected.trim());
}

#[test]
fn test_match_case_after_else_arm() {
    let rendered = render_diagnostics(
        r#"
        init {
            let x = match 0 {
                else => 0,
                1 => 1,
            };
        }
        "#,
    );
    let expected = dedent_preserve_blank_lines(
        r#"
        error: else arm must be last
         --> main.plk:4:9
          |
        3 |         else => 0,
          |         --------- else arm starts here
        4 |         1 => 1,
          |         ^^^^^^ this arm appears after else
        "#,
    );
    pretty_assertions::assert_str_eq!(rendered.trim(), expected.trim());
}
