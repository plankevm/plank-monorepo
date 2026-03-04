use crate::tests::assert_parser_errors;

#[test]
fn test_missing_semicolon() {
    assert_parser_errors(
        r#"const x =
            init {
                if (false) {
                    awesome = a == 5;
                }
            }
            "#,
        &["
                error: unexpected `init`, expected one of `-`, `!`, `~`, `true`, `false`, identifier, `(`, `comptime`, `fn`, `struct`, `{`, `if`
                  --> line 2:1
                   |
                  2| init {
                   | ^^^^
            "],
    );
}

#[test]
fn test_unclosed_if() {
    assert_parser_errors(
        r#"run {
            if (wow) {
                my_awesome_statement(3 + a, nice);



        }"#,
        &["
                error: unexpected EOF, expected `}`
                  --> line 4:2
                   |
                  4| }
                   |  ^
            "],
    );
}

#[test]
fn test_missing_open_run_block() {
    assert_parser_errors(
        r#"
            run }
            "#,
        &["
                error: unexpected `}`, expected `{`
                  --> line 1:5
                   |
                  1| run }
                   |     ^
            "],
    );
}
#[test]
fn test_missing_close_run_block() {
    assert_parser_errors(
        "run {",
        &["
                error: unexpected EOF, expected `}`
                  --> line 1:6
                   |
                  1| run {
                   |      ^
            "],
    );
}

#[test]
fn test_unexpected_token_at_top_level() {
    assert_parser_errors(
        "5;",
        &[
            "
                    error: unexpected decimal literal, expected one of `init`, `run`, `const`, `import`
                      --> line 1:1
                       |
                      1| 5;
                       | ^
                ",
            "
                    error: unexpected `;`, expected one of `init`, `run`, `const`, `import`
                      --> line 1:2
                       |
                      1| 5;
                       |  ^
                ",
        ],
    );
}

#[test]
fn test_unexpcted_token_post_const_decl() {
    assert_parser_errors(
        r#"const name run {}

            "#,
        &[r#"
                error: unexpected `run`, expected one of `:`, `=`
                  --> line 1:12
                   |
                  1| const name run {}
                   |            ^^^
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
        &[
            "
            error: unexpected `init`, expected one of `-`, `!`, `~`, `true`, `false`, identifier, `(`, `comptime`, `fn`, `struct`, `{`, `if`
              --> line 2:1
               |
              2| init { }
               | ^^^^
            ",
        ],
    );
}

// ==============================================================================
// Tests exposing brittle/weak parsing patterns
// ==============================================================================

// Issue: `advance_with_error()` consumes recovery tokens like `init`, causing cascading errors.
// Input: `const x = \n init { }` (missing value before init)
// Expected: One error about missing expression, then `init` block parses normally.
// Actual: `init` gets consumed as error, causing "unexpected `{`" cascade.

/// Issue (fixed): `123` after `.` is now consumed by MemberExpr error recovery.
/// Single error about expecting identifier after `.`.
#[test]
fn test_name_path_dot_not_followed_by_ident() {
    assert_parser_errors(
        r#"run { foo.123; }"#,
        &[r#"
                error: unexpected decimal literal, expected identifier
                  --> line 1:11
                   |
                  1| run { foo.123; }
                   |           ^^^
            "#],
    );
}

/// Issue (fixed): `parse_field_list` used to break silently on unexpected tokens.
/// Now skips garbage and resumes parsing remaining fields.
#[test]
fn test_field_list_garbage_silent_exit() {
    assert_parser_errors(
        r#"const S = struct { x: u32, 123 y: u32 };"#,
        &[r#"
                error: unexpected decimal literal, expected one of identifier, `}`
                  --> line 1:28
                   |
                  1| const S = struct { x: u32, 123 y: u32 };
                   |                            ^^^
            "#],
    );
}

/// Issue: multiple consecutive garbage tokens in a delimited list emit one error per token.
/// Should emit a single error for the entire garbage run.
#[test]
fn test_field_list_multiple_garbage_tokens() {
    assert_parser_errors(
        r#"const S = struct { x: u32, 123 456 y: u32 };"#,
        &[r#"
                error: unexpected decimal literal, expected one of identifier, `}`
                  --> line 1:28
                   |
                  1| const S = struct { x: u32, 123 456 y: u32 };
                   |                            ^^^
            "#],
    );
}

/// Issue (fixed): `parse_arg_list` used to break silently after comma when expression parsing
/// failed. Now skips garbage and resumes parsing remaining arguments.
#[test]
fn test_arg_list_empty_after_comma() {
    assert_parser_errors(
        r#"run { foo(a, , b); }"#,
        &[r#"
                error: unexpected `,`, expected one of `-`, `!`, `~`, `true`, `false`, identifier, `(`, `comptime`, `fn`, `struct`, `{`, `if`, `)`
                  --> line 1:14
                   |
                  1| run { foo(a, , b); }
                   |              ^
            "#],
    );
}

/// Issue (fixed): `parse_param_list` used to break silently when parameter parsing failed.
/// Now skips garbage and resumes parsing remaining parameters.
#[test]
fn test_param_list_empty_after_comma() {
    assert_parser_errors(
        r#"const f = fn(x: u32, , y: u32) u32 { return x; };"#,
        &[r#"
                error: unexpected `,`, expected one of `comptime`, identifier, `)`
                  --> line 1:22
                   |
                  1| const f = fn(x: u32, , y: u32) u32 { return x; };
                   |                      ^
            "#],
    );
}

/// Issue (fixed): `expect_ident` was not calling `emit_unexpected` on failure.
/// Now emits "expected identifier" error.
#[test]
fn test_member_access_missing_ident() {
    assert_parser_errors(
        r#"run { foo.; }"#,
        &[r#"
                error: unexpected `;`, expected identifier
                  --> line 1:11
                   |
                  1| run { foo.; }
                   |           ^
            "#],
    );
}

/// Issue (fixed): `StructLit` field list used to break silently on unexpected tokens.
/// Now skips garbage and resumes parsing remaining fields.
#[test]
fn test_struct_literal_field_list_garbage() {
    assert_parser_errors(
        r#"run { let x: S = S { a: 1, 123 b: 2 }; }"#,
        &[r#"
                error: unexpected decimal literal, expected one of identifier, `}`
                  --> line 1:28
                   |
                  1| run { let x: S = S { a: 1, 123 b: 2 }; }
                   |                            ^^^
            "#],
    );
}

/// Issue (fixed): Missing semicolon mid-block used to cause the parser to break out of the
/// block early. Now emits a "missing `;`" diagnostic and continues parsing.
#[test]
fn test_missing_semicolon_mid_block() {
    assert_parser_errors(
        r#"run { x + 1  y + 2; }"#,
        &[r#"
                error: missing `;`
                  --> line 1:12
                   |
                  1| run { x + 1  y + 2; }
                   |            ^
            "#],
    );
}

/// Issue (fixed): Binary expression RHS used to consume `;` via `advance_with_error()`.
/// Now creates a zero-width Error node, so `;` is properly consumed by `expect(Semicolon)`.
#[test]
fn test_binary_expr_missing_rhs() {
    assert_parser_errors(
        r#"run { x = 1 + ; }"#,
        &[r#"
                error: unexpected `;`, expected one of `-`, `!`, `~`, `true`, `false`, identifier, `(`, `comptime`, `fn`, `struct`, `{`, `if`
                  --> line 1:15
                   |
                  1| run { x = 1 + ; }
                   |               ^
            "#],
    );
}

/// Issue (fixed): Unary expression operand used to consume `;` via `advance_with_error()`.
/// Now creates a zero-width Error node, so `;` is properly consumed by `expect(Semicolon)`.
#[test]
fn test_unary_expr_missing_operand() {
    assert_parser_errors(
        r#"run { x = -; }"#,
        &[r#"
                error: unexpected `;`, expected one of `-`, `!`, `~`, `true`, `false`, identifier, `(`, `comptime`, `fn`, `struct`, `{`, `if`
                  --> line 1:12
                   |
                  1| run { x = -; }
                   |            ^
            "#],
    );
}

/// Issue (fixed): Paren expression used to consume `)` via `advance_with_error()`.
/// Now creates a zero-width Error node, so `)` is properly consumed by `expect(RightRound)`.
#[test]
fn test_paren_expr_empty() {
    assert_parser_errors(
        r#"run { x = (); }"#,
        &[r#"
                error: unexpected `)`, expected one of `-`, `!`, `~`, `true`, `false`, identifier, `(`, `comptime`, `fn`, `struct`, `{`, `if`
                  --> line 1:12
                   |
                  1| run { x = (); }
                   |            ^
            "#],
    );
}
