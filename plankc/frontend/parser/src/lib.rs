pub mod ast;
pub mod cst;
pub mod lexer;
pub mod parser;

pub use plank_session::{SourceByteOffset, SourceId, SourceSpan, StrId};
pub mod const_print;

#[cfg(test)]
pub mod tests;

/// Core crate assumption.
const _USIZE_AT_LEAST_U32: () = const {
    assert!(u32::BITS <= usize::BITS);
};
