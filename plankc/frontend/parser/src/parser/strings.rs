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

fn hex_value(byte: char) -> Option<u8> {
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

        let start = self.tokens.current();
        let mut end = None;
        let end = loop {
            let ti = self.tokens.current();
            match self.current_token() {
                Token::LooseStringLiteral => self.decode_string_token(ti),
                Token::LooseHexStringLiteral => self.decode_hex_token(ti),
                _ => break end?,
            }
            self.advance();
            end = Some(self.tokens.current());
            self.skip_trivia();
        };

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
            let '\\' = c else {
                let mut buf = [0u8; 4];
                let encoded = c.encode_utf8(&mut buf);
                self.string_buf.extend_from_slice(encoded.as_bytes());
                continue;
            };
            let (_, nc) = chars.next().expect("lexer guarantees backslash not end");
            let byte = match nc {
                'n' => b'\n',
                'r' => b'\r',
                't' => b'\t',
                '0' => b'\0',
                '\\' => b'\\',
                '"' => b'"',
                'x' => {
                    // Assume next two chars were intended as escapes (even if first is invalid).
                    let d1 = chars.next().and_then(|(_, d)| hex_value(d));
                    let d2 = chars.next().and_then(|(_, d)| hex_value(d));
                    let (Some(hi), Some(lo)) = (d1, d2) else {
                        let end = chars.peek().map_or(src.len(), |&(end, _)| end);
                        self.emit_invalid_hex_escape(Span::new(
                            src_start + start as u32,
                            src_start + end as u32,
                        ));
                        continue;
                    };
                    (hi << 4) | lo
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
        let mut supress_further_non_hex_errors = false;
        while let Some((c2_offset, c1)) = chars.next() {
            let hi = hex_value(c1);
            if hi.is_none() && !supress_further_non_hex_errors {
                supress_further_non_hex_errors = true;
                self.emit_non_hex_digit(src_start + c2_offset as u32, c1);
            }
            let Some((c2_offset, c2)) = chars.next() else {
                self.emit_odd_hex_digit_count(ti);
                break;
            };
            let lo = hex_value(c2);
            if lo.is_none() && !supress_further_non_hex_errors {
                supress_further_non_hex_errors = true;
                self.emit_non_hex_digit(src_start + c2_offset as u32, c2);
            }
            self.string_buf.push((hi.unwrap_or(0) << 4) | lo.unwrap_or(0))
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        cst::NodeKind,
        tests::{assert_session_errors, parse_single_source},
    };
    use plank_session::Session;

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
        assert_session_errors(&session, expected_errors);
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
