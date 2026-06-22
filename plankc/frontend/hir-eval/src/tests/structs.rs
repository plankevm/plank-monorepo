use super::*;

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

            @evm_stop();
        }
        "#,
        r#"
        ==== Functions ====
        ; init
        @fn0() -> never {
            %0 : u256 = 34
            %1 : bool = false
            %2 : Pair = Pair {
                49,
                true,
            }
            %3 : Pair = %2
            %4 : u256 = %3.0
            %5 : Pair = %2
            %6 : bool = %5.1
            %7 : never = @evm_stop()
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
            @evm_stop();
        }
        "#,
        &[r#"
        error: unknown field
         --> main.plk:5:19
          |
        5 |     let y: u256 = x.hey;
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
            @evm_stop();
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
fn test_has_name_kind_plain_struct() {
    assert_lowers_to(
        r#"
        const Pair = struct { a: u256 };
        const plain = @has_plain_name(Pair);
        const parameterized = @has_parameterized_name(Pair);
        init {
            let mut x: bool = plain;
            let mut y: bool = parameterized;
            @evm_stop();
        }
        "#,
        r#"
        ==== Functions ====
        ; init
        @fn0() -> never {
            %0 : bool = true
            %1 : bool = false
            %2 : never = @evm_stop()
        }
        "#,
    );
}

#[test]
fn test_has_name_kind_parameterized_struct() {
    assert_lowers_to(
        r#"
        const Box = fn (comptime T: type) type {
            struct { value: T }
        };
        const BoxU256 = Box(u256);
        const plain = @has_plain_name(BoxU256);
        const parameterized = @has_parameterized_name(BoxU256);
        init {
            let mut x: bool = plain;
            let mut y: bool = parameterized;
            @evm_stop();
        }
        "#,
        r#"
        ==== Functions ====
        ; init
        @fn0() -> never {
            %0 : bool = false
            %1 : bool = true
            %2 : never = @evm_stop()
        }
        "#,
    );
}

#[test]
fn test_has_name_kind_anonymous_struct() {
    assert_lowers_to(
        r#"
        const plain = @has_plain_name(struct { a: u256 });
        const parameterized = @has_parameterized_name(struct { a: u256 });
        init {
            let mut x: bool = plain;
            let mut y: bool = parameterized;
            @evm_stop();
        }
        "#,
        r#"
        ==== Functions ====
        ; init
        @fn0() -> never {
            %0 : bool = false
            %1 : bool = false
            %2 : never = @evm_stop()
        }
        "#,
    );
}

#[test]
fn test_has_name_kind_expects_struct() {
    assert_diagnostics(
        r#"
        const x = @has_plain_name(u256);
        const y = @has_parameterized_name(tuple { u256 });
        init { @evm_stop(); }
        "#,
        &[
            r#"
        error: unexpected type kind
         --> main.plk:1:11
          |
        1 | const x = @has_plain_name(u256);
          |           ^^^^^^^^^^^^^^^^^^^^^ `@has_plain_name` expects a struct type, got `u256`
        "#,
            r#"
        error: unexpected type kind
         --> main.plk:2:11
          |
        2 | const y = @has_parameterized_name(tuple { u256 });
          |           ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ `@has_parameterized_name` expects a struct type, got `tuple {u256}`
        "#,
        ],
    );
}

#[test]
fn test_type_name() {
    assert_lowers_to(
        r#"
        const Pair = struct { a: u256 };
        const Box = fn (comptime T: type) type {
            struct { value: T }
        };
        const BoxU256 = Box(u256);

        const plain_ok = @type_name(Pair) == "Pair";
        const parameterized_ok = @type_name(BoxU256) == "Box(u256)";
        const anonymous = @type_name(struct { a: u256 });
        const anonymous_ok = anonymous == "struct@main.plk:9:30";

        init {
            let mut x: bool = plain_ok;
            let mut y: bool = parameterized_ok;
            let mut z: bool = anonymous_ok;
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
            %3 : never = @evm_stop()
        }
        "#,
    );
}

#[test]
fn test_field_name() {
    assert_lowers_to(
        r#"
        const Pair = struct { a: u256, b: bool };
        const first_ok = @field_name(Pair, 0) == "a";
        const second_ok = @field_name(Pair, 1) == "b";

        init {
            let mut x: bool = first_ok;
            let mut y: bool = second_ok;
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
fn test_field_name_out_of_bounds() {
    assert_diagnostics(
        r#"
        const Pair = struct { a: u256 };
        const bad = @field_name(Pair, 1);
        init { @evm_stop(); }
        "#,
        &[r#"
        error: field index out of bounds
         --> main.plk:2:31
          |
        2 | const bad = @field_name(Pair, 1);
          |                               ^ `@field_name`: field index 1 is out of bounds for type with 1 field
        "#],
    );
}

#[test]
fn test_field_index() {
    assert_lowers_to(
        r#"
        const Pair = struct { a: u256, b: bool };
        const first_ok = @field_index(Pair, "a") == 0;
        const second_ok = @field_index(Pair, "b") == 1;
        const missing_ok = @field_index(Pair, "missing") == 2;
        const sliced_ok = @field_index(Pair, @slice_cbytes("xa", 1, 2)) == 0;

        init {
            let mut x: bool = first_ok;
            let mut y: bool = second_ok;
            let mut z: bool = missing_ok;
            let mut w: bool = sliced_ok;
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
            %4 : never = @evm_stop()
        }
        "#,
    );
}

#[test]
fn test_struct_name_builtins_expect_struct() {
    assert_diagnostics(
        r#"
        const x = @type_name(u256);
        const y = @field_name(tuple { u256 }, 0);
        const z = @field_index(u256, "a");
        init { @evm_stop(); }
        "#,
        &[
            r#"
        error: unexpected type kind
         --> main.plk:1:11
          |
        1 | const x = @type_name(u256);
          |           ^^^^^^^^^^^^^^^^ `@type_name` expects a struct type, got `u256`
        "#,
            r#"
        error: unexpected type kind
         --> main.plk:2:11
          |
        2 | const y = @field_name(tuple { u256 }, 0);
          |           ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ `@field_name` expects a struct type, got `tuple {u256}`
        "#,
            r#"
        error: unexpected type kind
         --> main.plk:3:11
          |
        3 | const z = @field_index(u256, "a");
          |           ^^^^^^^^^^^^^^^^^^^^^^^ `@field_index` expects a struct type, got `u256`
        "#,
        ],
    );
}

#[test]
fn test_get_field_unknown_name_selector() {
    assert_diagnostics(
        r#"
        const Pair = struct { a: u256 };
        const p = Pair { a: 1 };
        const bad = @get_field(p, "missing");
        init { @evm_stop(); }
        "#,
        &[r#"
        error: unknown field
         --> main.plk:3:27
          |
        3 | const bad = @get_field(p, "missing");
          |                           ^^^^^^^^^ `@get_field`: `Pair` has no field named "missing"
        "#],
    );
}

#[test]
fn test_set_field_unknown_name_selector() {
    assert_diagnostics(
        r#"
        const Pair = struct { a: u256 };
        const p = Pair { a: 1 };
        const bad = @set_field(p, "missing", 1);
        init { @evm_stop(); }
        "#,
        &[r#"
        error: unknown field
         --> main.plk:3:27
          |
        3 | const bad = @set_field(p, "missing", 1);
          |                           ^^^^^^^^^ `@set_field`: `Pair` has no field named "missing"
        "#],
    );
}

#[test]
fn test_tuple_field_name_selector_rejected() {
    assert_diagnostics(
        r#"
        const pair = (1, 2);
        const bad = @get_field(pair, "a");
        init { @evm_stop(); }
        "#,
        &[r#"
        error: invalid field selector
         --> main.plk:2:30
          |
        2 | const bad = @get_field(pair, "a");
          |                              ^^^ `@get_field` field selector must be `u256`, got `cbytes`
        "#],
    );
}

#[test]
fn test_get_field_invalid_selector_type() {
    assert_diagnostics(
        r#"
        const Pair = struct { a: u256 };
        const p = Pair { a: 1 };
        const bad = @get_field(p, false);
        init { @evm_stop(); }
        "#,
        &[r#"
        error: invalid field selector
         --> main.plk:3:27
          |
        3 | const bad = @get_field(p, false);
          |                           ^^^^^ `@get_field` field selector must be `u256` or `cbytes`, got `bool`
        "#],
    );
}

#[test]
fn test_comptime_struct_missing_field() {
    assert_diagnostics(
        r#"
        const Pair = struct { a: u256, b: bool };
        const my_pair = Pair { a: 42 };

        init {
            @evm_stop();
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
            @evm_stop();
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
            @evm_stop();
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
            @evm_stop();
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
fn test_comptime_struct_field_type_mismatch() {
    assert_diagnostics(
        r#"
        const Pair = struct { a: u256, b: bool };
        const my_pair = Pair { a: false, b: false };

        init {
            @evm_stop();
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
fn test_mixed_struct_type() {
    assert_diagnostics(
        r#"
        const S = struct { t: type, p: memptr };
        init { @evm_stop(); }
        "#,
        &[r#"
        error: defining uninstantiable type
         --> main.plk:1:11
          |
        1 | const S = struct { t: type, p: memptr };
          |           ^^^^^^^^^-------^^---------^^
          |                    |        |
          |                    |        type `memptr` is runtime only
          |                    type `type` is comptime only
        "#],
    );
}

#[test]
fn test_mixed_comptime_runtime_struct() {
    assert_diagnostics(
        r#"
        const Wrapper = struct { t: type, n: u256 };
        init {
            let x = @evm_calldataload(0);
            let w = Wrapper { t: u256, n: x,
                c: 34
            };
            @evm_stop();
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
              |               ^         -        - `n` not comptime-known
              |               |         |
              |  _____________|         `t` is comptime-only
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
        init { @evm_stop(); }
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
        init { @evm_stop(); }
        "#,
        &[r#"
        error: value used as type
         --> main.plk:2:11
          |
        1 | const T = 42;
          | ------------- defined here
        2 | const x = T { };
          |           ^ expected type, got value of type `u256`
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
            @evm_stop();
        }
        "#,
        &[r#"
        error: value used as type
         --> main.plk:3:13
          |
        1 | const T = 42;
          | ------------- defined here
        2 | init {
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
            let T = @evm_calldataload(0);
            let x = T { };
            @evm_stop();
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
            @evm_stop();
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
            let T = @evm_calldataload(0);
            let S = struct T { x: u256 };
            @evm_stop();
        }
        "#,
        &[r#"
        error: attempting to evaluate runtime expression in comptime context
         --> main.plk:3:20
          |
        3 |     let S = struct T { x: u256 };
          |                    ^ runtime expression
        "#],
    );
}

#[test]
fn test_runtime_struct_def_field_type_not_comptime() {
    assert_diagnostics(
        r#"
        init {
            let T = @evm_calldataload(0);
            let S = struct { x: T };
            @evm_stop();
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
fn test_runtime_struct_lit_field_type_mismatch() {
    assert_diagnostics(
        r#"
        const Pair = struct { a: u256, b: bool };
        init {
            let x = Pair { a: false, b: false };
            @evm_stop();
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
fn test_comptime_struct_lit_not_a_struct() {
    assert_diagnostics(
        r#"
        const x = u256 { };
        init { @evm_stop(); }
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
            @evm_stop();
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
fn test_cross_file_struct_lit_not_a_struct() {
    assert_project_diagnostics(
        TestProject::root(
            "
            import m::other::T;
            init {
                let x = T { value: 1 };
                @evm_stop();
            }
            ",
        )
        .add_file("other", "const T = bool;")
        .add_module("m", ""),
        &[r#"
        error: expected struct type
         --> main.plk:3:13
          |
        3 |     let x = T { value: 1 };
          |             ^ `bool` is not a struct type
          |
         ::: other.plk:1:1
          |
        1 | const T = bool;
          | --------------- defined here
        "#],
    );
}

#[test]
fn test_cross_file_type_not_type() {
    assert_project_diagnostics(
        TestProject::root(
            "
            import m::other::T;
            init {
                let x = T { };
                @evm_stop();
            }
            ",
        )
        .add_file("other", "const T = 42;")
        .add_module("m", ""),
        &[r#"
        error: value used as type
         --> main.plk:3:13
          |
        3 |     let x = T { };
          |             ^ expected type, got value of type `u256`
          |
         ::: other.plk:1:1
          |
        1 | const T = 42;
          | ------------- defined here
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
            @evm_stop();
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
            @evm_stop();
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
            @evm_stop();
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
        init { @evm_stop(); }
        "#,
        &[r#"
        error: no fields on type
         --> main.plk:2:11
          |
        1 | const x: u256 = 5;
          | ------------------ defined here
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
            let x: u256 = @evm_calldataload(0);
            let y = x.foo;
            @evm_stop();
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
fn test_cross_file_member_on_non_struct() {
    assert_project_diagnostics(
        TestProject::root(
            "
            import m::other::x;
            const y = x.foo;
            init { @evm_stop(); }
            ",
        )
        .add_file("other", "const x: u256 = 5;")
        .add_module("m", ""),
        &[r#"
        error: no fields on type
         --> main.plk:2:11
          |
        2 | const y = x.foo;
          |           ^ value of type `u256` is not a struct type
          |
         ::: other.plk:1:1
          |
        1 | const x: u256 = 5;
          | ------------------ defined here
        "#],
    );
}

#[test]
fn test_struct_def_duplicate_field() {
    assert_diagnostics(
        r#"
        const S = struct { x: u256, x: bool };
        init { @evm_stop(); }
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
fn test_type_index_expr_eagerly_evaluates() {
    assert_lowers_to(
        r#"
        const ident = fn (x: u256) u256 { x };

        init {
            let y = 34;
            let T = struct ident(y) {
                wow: u256
            };
            let mut t = T { wow: 67 };

            @evm_stop();
        }
        "#,
        r#"
        ==== Functions ====
        ; init
        @fn0() -> never {
            %0 : struct@main.plk:5:13 = struct@main.plk:5:13 {
                67,
            }
            %1 : never = @evm_stop()
        }
        "#,
    );
}
