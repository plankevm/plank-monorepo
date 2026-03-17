use crate::tests::assert_parser_errors;

#[test]
fn test_missing_semicolon() {
    assert_parser_errors(
        r#"
            const x =
            init {
                if (false) {
                    awesome = a == 5;
                }
            }
        "#,
        &[r#"
            error: unexpected `init`
             --> test.plk:2:1
              |
            2 | init {
              | ^^^^ unexpected `init`, expected one of `-`, `!`, `~`, `true`, `false`, identifier, `(`, `comptime`, `fn`, `struct`, `{`, `if`
        "#],
    );
}

#[test]
fn test_unclosed_if() {
    assert_parser_errors(
        r#"
            run {
                if (wow) {
                    my_awesome_statement(3 + a, nice);



            }
        "#,
        &[r#"
            error: unexpected EOF
             --> test.plk:4:2
              |
            4 | }
              |  ^ unexpected EOF, expected `}`
        "#],
    );
}

#[test]
fn test_missing_open_run_block() {
    assert_parser_errors(
        r#"
            run }
        "#,
        &[r#"
            error: unexpected `}`
             --> test.plk:1:5
              |
            1 | run }
              |     ^ unexpected `}`, expected `{`
        "#],
    );
}

#[test]
fn test_missing_close_run_block() {
    assert_parser_errors(
        r#"
            run {
        "#,
        &[r#"
            error: unexpected EOF
             --> test.plk:1:6
              |
            1 | run {
              |      ^ unexpected EOF, expected `}`
        "#],
    );
}

#[test]
fn test_unexpected_token_at_top_level() {
    assert_parser_errors(
        r#"
            5;
        "#,
        &[r#"
            error: unexpected decimal literal
             --> test.plk:1:1
              |
            1 | 5;
              | ^ unexpected decimal literal, expected one of `init`, `run`, `const`, `import`
        "#],
    );
}

#[test]
fn test_unexpected_token_post_const_decl() {
    assert_parser_errors(
        r#"
            const name run {}
        "#,
        &[r#"
            error: missing `=`
             --> test.plk:1:11
              |
            1 | const name run {}
              |           ^ missing `=`one of `:`, `=`
        "#],
    );
}

#[test]
fn test_const_decl_missing_expr() {
    assert_parser_errors(
        r#"
            const x =
            init { }
        "#,
        &[r#"
            error: unexpected `init`
             --> test.plk:2:1
              |
            2 | init { }
              | ^^^^ unexpected `init`, expected one of `-`, `!`, `~`, `true`, `false`, identifier, `(`, `comptime`, `fn`, `struct`, `{`, `if`
        "#],
    );
}

// ==============================================================================
// Tests exposing brittle/weak parsing patterns
// ==============================================================================

#[test]
fn test_name_path_dot_not_followed_by_ident() {
    assert_parser_errors(
        r#"
            run { foo.123; }
        "#,
        &[r#"
            error: unexpected decimal literal
             --> test.plk:1:11
              |
            1 | run { foo.123; }
              |           ^^^ unexpected decimal literal, expected identifier
        "#],
    );
}

#[test]
fn test_field_list_garbage_silent_exit() {
    assert_parser_errors(
        r#"
            const S = struct { x: u32, 123 y: u32 };
        "#,
        &[r#"
            error: unexpected decimal literal
             --> test.plk:1:28
              |
            1 | const S = struct { x: u32, 123 y: u32 };
              |                            ^^^ unexpected decimal literal, expected one of identifier, `}`
        "#],
    );
}

#[test]
fn test_field_list_multiple_garbage_tokens() {
    assert_parser_errors(
        r#"
            const S = struct { x: u32, 123 456 y: u32 };
        "#,
        &[r#"
            error: unexpected decimal literal
             --> test.plk:1:28
              |
            1 | const S = struct { x: u32, 123 456 y: u32 };
              |                            ^^^ unexpected decimal literal, expected one of identifier, `}`
        "#],
    );
}

#[test]
fn test_arg_list_empty_after_comma() {
    assert_parser_errors(
        r#"
            run { foo(a, , b); }
        "#,
        &[r#"
            error: unexpected `,`
             --> test.plk:1:14
              |
            1 | run { foo(a, , b); }
              |              ^ unexpected `,`, expected one of `-`, `!`, `~`, `true`, `false`, identifier, `(`, `comptime`, `fn`, `struct`, `{`, `if`, `)`
        "#],
    );
}

#[test]
fn test_param_list_empty_after_comma() {
    assert_parser_errors(
        r#"
            const f = fn(x: u32, , y: u32) u32 { return x; };
        "#,
        &[r#"
            error: unexpected `,`
             --> test.plk:1:22
              |
            1 | const f = fn(x: u32, , y: u32) u32 { return x; };
              |                      ^ unexpected `,`, expected one of `comptime`, identifier, `)`
        "#],
    );
}

#[test]
fn test_member_access_missing_ident() {
    assert_parser_errors(
        r#"
            run { foo.; }
        "#,
        &[r#"
            error: unexpected `;`
             --> test.plk:1:11
              |
            1 | run { foo.; }
              |           ^ unexpected `;`, expected identifier
        "#],
    );
}

#[test]
fn test_struct_literal_field_list_garbage() {
    assert_parser_errors(
        r#"
            run { let x: S = S { a: 1, 123 b: 2 }; }
        "#,
        &[r#"
            error: unexpected decimal literal
             --> test.plk:1:28
              |
            1 | run { let x: S = S { a: 1, 123 b: 2 }; }
              |                            ^^^ unexpected decimal literal, expected one of identifier, `}`
        "#],
    );
}

#[test]
fn test_missing_semicolon_mid_block() {
    assert_parser_errors(
        r#"
            run { x + 1  y + 2; }
        "#,
        &[r#"
            error: missing `;`
             --> test.plk:1:12
              |
            1 | run { x + 1  y + 2; }
              |            ^^
        "#],
    );
}

#[test]
fn test_binary_expr_missing_rhs() {
    assert_parser_errors(
        r#"
            run { x = 1 + ; }
        "#,
        &[r#"
            error: unexpected `;`
             --> test.plk:1:15
              |
            1 | run { x = 1 + ; }
              |               ^ unexpected `;`, expected one of `-`, `!`, `~`, `true`, `false`, identifier, `(`, `comptime`, `fn`, `struct`, `{`, `if`
        "#],
    );
}

#[test]
fn test_unary_expr_missing_operand() {
    assert_parser_errors(
        r#"
            run { x = -; }
        "#,
        &[r#"
            error: unexpected `;`
             --> test.plk:1:12
              |
            1 | run { x = -; }
              |            ^ unexpected `;`, expected one of `-`, `!`, `~`, `true`, `false`, identifier, `(`, `comptime`, `fn`, `struct`, `{`, `if`
        "#],
    );
}

#[test]
fn test_paren_expr_empty() {
    assert_parser_errors(
        r#"
            run { x = (); }
        "#,
        &[r#"
            error: unexpected `)`
             --> test.plk:1:12
              |
            1 | run { x = (); }
              |            ^ unexpected `)`, expected one of `-`, `!`, `~`, `true`, `false`, identifier, `(`, `comptime`, `fn`, `struct`, `{`, `if`
        "#],
    );
}

#[test]
fn test_missing_semicolon_after_fn_const() {
    assert_parser_errors(
        "
        const to_addr = fn (raw: u256) u256 { raw }
        init { }
        ",
        &["
            error: missing `;`
             --> test.plk:1:44
              |
            1 |   const to_addr = fn (raw: u256) u256 { raw }
              |  ____________________________________________^
            2 | | init { }
              | |_^ missing `;`
        "],
    );
}

#[test]
fn test_missing_semicolon_unexpected_garbage() {
    assert_parser_errors(
        "
        const to_addr = {}
        bob
        init { }
        ",
        &["
            error: unexpected identifier
             --> test.plk:2:1
              |
            2 | bob
              | ^^^ unexpected identifier, expected `;`
        "],
    );
}
