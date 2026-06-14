//! Validating decoders for string literal segments.
//!
//! The lexer scans string literals loosely (only caring about termination);
//! the parser performs full content validation when it consumes a string
//! token, reporting precise sub-token errors as it decodes.

use plank_core::Span;

use crate::{
    cst::{NodeIdx, NodeKind},
    lexer::{Token, TokenIdx},
    parser::Parser,
};

fn parse_nibble(byte: char) -> Option<u8> {
    match byte {
        '0'..='9' => Some(byte as u8 - b'0'),
        'A'..='F' => Some(byte as u8 - b'A' + 10),
        'a'..='f' => Some(byte as u8 - b'a' + 10),
        _ => None,
    }
}

impl Parser<'_> {
    /// Parses a string literal, merging any directly following string/hex
    /// string tokens into a single value: `"ab" "c" hex"01"` == `"abc\x01"`.
    pub(crate) fn try_parse_string_literal(&mut self) -> Option<NodeIdx> {
        self.skip_trivia();

        self.string_buf.clear();

        let start = self.current_token_index();
        let mut end = None;
        loop {
            let ti = self.current_token_index();
            match self.current_token() {
                Token::LooseStringLiteral => self.decode_string_token(ti),
                Token::LooseHexStringLiteral => self.decode_hex_token(ti),
                Token::MultilineStringError | Token::UnclosedStringError => {}
                _ => break,
            }
            self.advance();
            end = Some(self.current_token_index());
            self.skip_trivia();
        }
        let end = end?;

        let value = self.session.intern_bytes(&self.string_buf);
        let node = self.alloc_node_from(start, NodeKind::StringLiteral { value });
        Some(self.close_node_at(node, end))
    }

    /// Decodes the contents of a `"..."` token (including the quotes) into the
    /// string buffer, resolving the escapes `\n`, `\r`, `\t`, `\0`, `\\`, `\"`
    /// and `\xHH`.
    fn decode_string_token(&mut self, ti: TokenIdx) {
        let token_span = self.tokens.token_src_span(ti);
        let src = &self.source[token_span.usize_range()];
        let src = src.strip_prefix('"').expect("missing opening `\"`");
        let src = src.strip_suffix('"').expect("missing closing `\"`");
        let src_start = token_span.start + 1;

        let mut chars = src.char_indices().peekable();

        while let Some((start, c)) = chars.next() {
            if !c.is_ascii() {
                while chars.next_if(|&(_, c)| !c.is_ascii()).is_some() {}
                let end = chars.peek().map_or(src.len(), |&(end, _)| end);
                self.emit_unicode_disallowed_in_string(Span::new(
                    src_start + start as u32,
                    src_start + end as u32,
                ));
                continue;
            }
            if c != '\\' {
                self.string_buf.push(c as u8);
                continue;
            }
            let (_, c) = chars.next().expect("lexer guarantees backslash not end");
            let byte = match c {
                'n' => b'\n',
                'r' => b'\r',
                't' => b'\t',
                '0' => b'\0',
                '\\' => b'\\',
                '"' => b'"',
                'x' => {
                    let d1 = chars.next().and_then(|(_, d)| parse_nibble(d));
                    let d2 = chars.next().and_then(|(_, d)| parse_nibble(d));
                    let (Some(msb), Some(lsb)) = (d1, d2) else {
                        let end = chars.peek().map_or(src.len(), |&(end, _)| end);
                        self.emit_invalid_hex_escape(Span::new(
                            src_start + start as u32,
                            src_start + end as u32,
                        ));
                        continue;
                    };
                    (msb << 4) | lsb
                }
                other => {
                    let span = Span::new(
                        src_start + start as u32,
                        src_start + chars.peek().map_or(src.len(), |&(end, _)| end) as u32,
                    );
                    self.emit_unrecognized_escape(span, other);
                    continue;
                }
            };
            self.string_buf.push(byte);
        }
    }

    /// Decodes the contents of a `hex"..."` token (including prefix and
    /// quotes) into the string buffer, validating that it contains an even
    /// number of hex digits and nothing else.
    fn decode_hex_token(&mut self, ti: TokenIdx) {
        let token_span = self.tokens.token_src_span(ti);
        let src = &self.source[token_span.usize_range()];
        let src = src.strip_prefix("hex\"").expect("missing opening `hex\"`");
        let src = src.strip_suffix('"').expect("missing closing `\"`");
        let src_start = token_span.start + 4;

        let mut chars = src.char_indices().peekable();
        let mut already_emitted_hex_error = false;
        while let Some((c1_offset, c1)) = chars.next() {
            let msb = parse_nibble(c1);
            if msb.is_none() && !already_emitted_hex_error {
                already_emitted_hex_error = true;
                self.emit_non_hex_digit(src_start + c1_offset as u32, c1);
            }
            let Some((c2_offset, c2)) = chars.next() else {
                self.emit_odd_hex_digit_count(ti);
                break;
            };
            let lsb = parse_nibble(c2);
            if lsb.is_none() && !already_emitted_hex_error {
                already_emitted_hex_error = true;
                self.emit_non_hex_digit(src_start + c2_offset as u32, c2);
            }
            self.string_buf.push((msb.unwrap_or(0) << 4) | lsb.unwrap_or(0));
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{cst::NodeKind, tests::parse_single_source};
    use plank_session::Session;
    use plank_test_utils::assert_diagnostics;

    fn assert_decodes_to(literal: &str, expected_value: &[u8], expected_errors: &[&str]) {
        let source = format!("const x = {literal};");
        let mut session = Session::new();
        let cst = parse_single_source(&source, &mut session);
        let value = cst
            .nodes
            .iter()
            .find_map(|node| match node.kind {
                NodeKind::StringLiteral { value } => Some(value),
                _ => None,
            })
            .expect("source contains a string literal");
        assert_eq!(
            session.lookup_bytes(value),
            expected_value,
            "decoded value mismatch for `{literal}`"
        );
        assert_diagnostics(session.diagnostics(), &session, expected_errors);
    }

    #[test]
    fn plain_and_escaped_strings() {
        assert_decodes_to(r#""hello""#, b"hello", &[]);
        assert_decodes_to(r#""""#, b"", &[]);
        assert_decodes_to(r#""a\n\r\t\0\\\"b\x7fc""#, b"a\n\r\t\0\\\"b\x7fc", &[]);
    }

    #[test]
    fn unrecognized_escape_recovery() {
        assert_decodes_to(
            r#""a\qb""#,
            b"ab",
            &[r#"
            error: unrecognized escape sequence
             --> test.plk:1:13
              |
            1 | const x = "a\qb";
              |             ^^ `\q` is not a recognized escape sequence
              |
              = help: valid escapes are `\n`, `\r`, `\t`, `\0`, `\\`, `\"` and `\xHH`
            "#],
        );
    }

    #[test]
    fn invalid_hex_escape_recovery() {
        assert_decodes_to(
            r#""\xZG""#,
            b"",
            &[r#"
            error: invalid hex escape
             --> test.plk:1:12
              |
            1 | const x = "\xZG";
              |            ^^^^ `\x` must be followed by exactly two hex digits, e.g. `\x7f`
            "#],
        );
        assert_decodes_to(
            r#""\x1""#,
            b"",
            &[r#"
            error: invalid hex escape
             --> test.plk:1:12
              |
            1 | const x = "\x1";
              |            ^^^ `\x` must be followed by exactly two hex digits, e.g. `\x7f`
            "#],
        );
    }

    #[test]
    fn hex_segments() {
        assert_decodes_to(r#"hex"01aF""#, &[0x01, 0xaf], &[]);
        assert_decodes_to(r#"hex"""#, &[], &[]);
        assert_decodes_to(
            r#"hex"01z2""#,
            &[0x01, 0x02],
            &[r#"
            error: invalid digit in hex string literal
             --> test.plk:1:17
              |
            1 | const x = hex"01z2";
              |                 ^ `z` is not a hex digit (0-9, a-f, A-F)
            "#],
        );
        assert_decodes_to(
            r#"hex"012""#,
            &[0x01],
            &[r#"
            error: odd number of digits in hex string literal
             --> test.plk:1:11
              |
            1 | const x = hex"012";
              |           ^^^^^^^^ expected an even number of hex digits
              |
              = help: hex string literals encode whole bytes, so two hex digits are needed per byte
            "#],
        );
    }

    #[test]
    fn merged_segments() {
        assert_decodes_to(r#""abc" "123" hex"01ab""#, b"abc123\x01\xab", &[]);
    }
}
