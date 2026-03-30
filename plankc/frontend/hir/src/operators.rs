//! Overridable semantically relevant operators. Not using [`plank_parser::cst::BinaryOp`] and
//! [`plank_parser::cst::UnaryOp`] as these contain `and`, `or` and `!` which are bool specific and
//! should not be overridable.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BinaryOp {
    // Comparison
    NotEquals,
    Equals,
    LessThan,
    GreaterThan,
    LessEquals,
    GreaterEquals,
    // Bitwise
    BitwiseOr,
    BitwiseXor,
    BitwiseAnd,
    ShiftLeft,
    ShiftRight,
    // Arithmetic (additive)
    Add,
    Subtract,
    AddWrap,
    SubtractWrap,
    // Arithmetic (multiplicative)
    Mul,
    Mod,
    MulWrap,
    DivRoundPos,
    DivRoundNeg,
    DivRoundToZero,
    DivRoundAwayFromZero,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum UnaryOp {
    Negate,
    BitwiseNot,
}
