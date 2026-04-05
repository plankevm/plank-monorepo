pub mod ast;
pub mod cst;
pub mod lexer;
pub mod parser;

#[cfg(test)]
pub mod tests;

/// Core crate assumption.
const _USIZE_AT_LEAST_U32: () = const {
    assert!(u32::BITS <= usize::BITS);
};
