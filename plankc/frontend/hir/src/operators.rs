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

impl BinaryOp {
    pub const fn symbol(self) -> &'static str {
        match self {
            Self::NotEquals => "!=",
            Self::Equals => "==",
            Self::LessThan => "<",
            Self::GreaterThan => ">",
            Self::LessEquals => "<=",
            Self::GreaterEquals => ">=",
            Self::BitwiseOr => "|",
            Self::BitwiseXor => "^",
            Self::BitwiseAnd => "&",
            Self::ShiftLeft => "<<",
            Self::ShiftRight => ">>",
            Self::Add => "+",
            Self::Subtract => "-",
            Self::AddWrap => "+%",
            Self::SubtractWrap => "-%",
            Self::Mul => "*",
            Self::Mod => "%",
            Self::MulWrap => "*%",
            Self::DivRoundPos => "+/",
            Self::DivRoundNeg => "-/",
            Self::DivRoundToZero => "</",
            Self::DivRoundAwayFromZero => ">/",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum UnaryOp {
    Negate,
    BitwiseNot,
}

impl UnaryOp {
    pub const fn symbol(self) -> &'static str {
        match self {
            Self::Negate => "-",
            Self::BitwiseNot => "~",
        }
    }
}
