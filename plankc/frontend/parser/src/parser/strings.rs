//! Validating decoders for string literal segments.
//!
//! The lexer scans string literals loosely (only caring about termination);
//! these decoders perform full content validation when the parser consumes a
//! string token, reporting precise sub-token errors via callback.

use allocator_api2::vec::Vec;

/// Content error inside a single string/hex-string token. Offsets and lengths
/// are in bytes, relative to the token start.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StringSegmentError {
    UnrecognizedEscape { offset: u32, len: u32 },
    InvalidHexEscape { offset: u32, len: u32 },
    NonHexDigit { offset: u32, len: u32 },
    OddHexDigitCount,
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        _ => None,
    }
}

/// Decodes the contents of a `"..."` token (including the quotes) into `out`,
/// resolving the escapes `\n`, `\r`, `\t`, `\0`, `\\`, `\"` and `\xHH`.
pub(crate) fn decode_string_segment(
    src: &str,
    out: &mut Vec<u8>,
    mut on_err: impl FnMut(StringSegmentError),
) {
    const QUOTE_LEN: usize = 1;
    let inner = &src[QUOTE_LEN..src.len() - 1];
    let bytes = inner.as_bytes();

    let mut pos = 0;
    while pos < bytes.len() {
        let rest = &inner[pos..];
        let Some(escape) = rest.find('\\') else {
            out.extend_from_slice(rest.as_bytes());
            break;
        };
        out.extend_from_slice(&rest.as_bytes()[..escape]);
        pos += escape;
        let escape_offset = (QUOTE_LEN + pos) as u32;
        pos += 1;

        let escaped = inner[pos..]
            .chars()
            .next()
            .expect("lexer guarantees a character follows every backslash");
        pos += escaped.len_utf8();
        match escaped {
            'n' => out.push(b'\n'),
            'r' => out.push(b'\r'),
            't' => out.push(b'\t'),
            '0' => out.push(b'\0'),
            '\\' => out.push(b'\\'),
            '"' => out.push(b'"'),
            'x' => {
                let digits = bytes
                    .get(pos)
                    .copied()
                    .and_then(hex_value)
                    .zip(bytes.get(pos + 1).copied().and_then(hex_value));
                match digits {
                    Some((hi, lo)) => {
                        out.push(hi << 4 | lo);
                        pos += 2;
                    }
                    None => on_err(StringSegmentError::InvalidHexEscape {
                        offset: escape_offset,
                        len: 2 + inner[pos..].chars().take(2).map(char::len_utf8).sum::<usize>()
                            as u32,
                    }),
                }
            }
            other => on_err(StringSegmentError::UnrecognizedEscape {
                offset: escape_offset,
                len: 1 + other.len_utf8() as u32,
            }),
        }
    }
}

/// Decodes the contents of a `hex"..."` token (including prefix and quotes)
/// into `out`, validating that it contains an even number of hex digits and
/// nothing else.
pub(crate) fn decode_hex_segment(
    src: &str,
    out: &mut Vec<u8>,
    mut on_err: impl FnMut(StringSegmentError),
) {
    const PREFIX_LEN: usize = 4;
    let inner = &src[PREFIX_LEN..src.len() - 1];

    let mut pending_hi: Option<u8> = None;
    for (i, c) in inner.char_indices() {
        let digit = u8::try_from(c).ok().and_then(hex_value);
        let Some(digit) = digit else {
            on_err(StringSegmentError::NonHexDigit {
                offset: (PREFIX_LEN + i) as u32,
                len: c.len_utf8() as u32,
            });
            continue;
        };
        match pending_hi.take() {
            None => pending_hi = Some(digit),
            Some(hi) => out.push(hi << 4 | digit),
        }
    }
    if pending_hi.is_some() {
        on_err(StringSegmentError::OddHexDigitCount);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decode_str(src: &str) -> (std::vec::Vec<u8>, std::vec::Vec<StringSegmentError>) {
        let mut out = Vec::new();
        let mut errors = std::vec::Vec::new();
        decode_string_segment(src, &mut out, |e| errors.push(e));
        (out.to_vec(), errors)
    }

    fn decode_hex(src: &str) -> (std::vec::Vec<u8>, std::vec::Vec<StringSegmentError>) {
        let mut out = Vec::new();
        let mut errors = std::vec::Vec::new();
        decode_hex_segment(src, &mut out, |e| errors.push(e));
        (out.to_vec(), errors)
    }

    #[test]
    fn plain_and_escaped_strings() {
        assert_eq!(decode_str(r#""hello""#), (b"hello".to_vec(), vec![]));
        assert_eq!(decode_str(r#""""#), (b"".to_vec(), vec![]));
        let (value, errors) = decode_str(r#""a\n\r\t\0\\\"b\x7fc""#);
        assert_eq!(value, b"a\n\r\t\0\\\"b\x7fc");
        assert!(errors.is_empty());
    }

    #[test]
    fn unrecognized_escape() {
        let (value, errors) = decode_str(r#""a\qb""#);
        assert_eq!(value, b"ab");
        assert!(matches!(
            errors.as_slice(),
            [StringSegmentError::UnrecognizedEscape { offset: 2, len: 2 }]
        ));
    }

    #[test]
    fn invalid_hex_escape() {
        let (_, errors) = decode_str(r#""\xZG""#);
        assert!(matches!(errors.as_slice(), [StringSegmentError::InvalidHexEscape { .. }]));

        let (_, errors) = decode_str(r#""\x1""#);
        assert!(matches!(errors.as_slice(), [StringSegmentError::InvalidHexEscape { .. }]));
    }

    #[test]
    fn hex_segments() {
        assert_eq!(decode_hex(r#"hex"01aF""#), (vec![0x01, 0xaf], vec![]));
        assert_eq!(decode_hex(r#"hex"""#), (vec![], vec![]));

        let (value, errors) = decode_hex(r#"hex"01z2""#);
        assert_eq!(value, vec![0x01]);
        assert!(matches!(
            errors.as_slice(),
            [
                StringSegmentError::NonHexDigit { offset: 6, len: 1 },
                StringSegmentError::OddHexDigitCount
            ]
        ));

        let (_, errors) = decode_hex(r#"hex"012""#);
        assert!(matches!(errors.as_slice(), [StringSegmentError::OddHexDigitCount]));
    }
}
