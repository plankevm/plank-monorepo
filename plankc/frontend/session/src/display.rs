use std::fmt;

pub fn write_bytes_literal(f: &mut impl fmt::Write, bytes: &[u8]) -> fmt::Result {
    f.write_str("\"")?;
    for &byte in bytes {
        match byte {
            b'\n' => f.write_str("\\n")?,
            b'\r' => f.write_str("\\r")?,
            b'\t' => f.write_str("\\t")?,
            b'\\' => f.write_str("\\\\")?,
            b'\"' => f.write_str("\\\"")?,
            0x20..=0x7e => fmt::Write::write_char(f, byte as char)?,
            _ => write!(f, "\\x{byte:02x}")?,
        }
    }
    f.write_str("\"")
}
