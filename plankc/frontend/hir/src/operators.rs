//! Overridable semantically relevant operators. Not using [`plank_parser::cst::BinaryOp`] and
//! [`plank_parser::cst::UnaryOp`] as these contain `and`, `or` and `!` which are bool specific and
//! should not be overridable.

use plank_session::RuntimeBuiltin;

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
    pub const fn runtime_equivalent(self) -> Option<RuntimeBuiltin> {
        let builtin = match self {
            Self::LessThan => RuntimeBuiltin::Lt,
            Self::GreaterThan => RuntimeBuiltin::Gt,
            Self::AddWrap => RuntimeBuiltin::Add,
            Self::SubtractWrap => RuntimeBuiltin::Sub,
            Self::MulWrap => RuntimeBuiltin::Mul,
            Self::BitwiseOr => RuntimeBuiltin::Or,
            Self::BitwiseXor => RuntimeBuiltin::Xor,
            Self::BitwiseAnd => RuntimeBuiltin::And,
            Self::ShiftLeft => RuntimeBuiltin::Shl,
            Self::ShiftRight => RuntimeBuiltin::Shr,

            Self::LessEquals
            | Self::GreaterEquals
            | Self::Add
            | Self::Subtract
            | Self::Mul
            | Self::Mod
            | Self::NotEquals
            | Self::Equals
            | Self::DivRoundPos
            | Self::DivRoundNeg
            | Self::DivRoundToZero
            | Self::DivRoundAwayFromZero => return None,
        };
        Some(builtin)
    }

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

impl std::fmt::Display for BinaryOp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.symbol())
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

impl std::fmt::Display for UnaryOp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.symbol())
    }
}
