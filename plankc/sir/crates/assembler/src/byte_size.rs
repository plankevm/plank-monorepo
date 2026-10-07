use crate::op;

/// A byte count in `1..=32`: the size of a `PUSH<n>` immediate or of a partial EVM word.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ByteSize {
    B1 = 1,
    B2 = 2,
    B3 = 3,
    B4 = 4,
    B5 = 5,
    B6 = 6,
    B7 = 7,
    B8 = 8,
    B9 = 9,
    B10 = 10,
    B11 = 11,
    B12 = 12,
    B13 = 13,
    B14 = 14,
    B15 = 15,
    B16 = 16,
    B17 = 17,
    B18 = 18,
    B19 = 19,
    B20 = 20,
    B21 = 21,
    B22 = 22,
    B23 = 23,
    B24 = 24,
    B25 = 25,
    B26 = 26,
    B27 = 27,
    B28 = 28,
    B29 = 29,
    B30 = 30,
    B31 = 31,
    B32 = 32,
}

impl ByteSize {
    pub const MIN: Self = Self::B1;
    pub const MAX: Self = Self::B32;

    pub fn try_from_u8(x: u8) -> Option<Self> {
        match x {
            1 => Some(Self::B1),
            2 => Some(Self::B2),
            3 => Some(Self::B3),
            4 => Some(Self::B4),
            5 => Some(Self::B5),
            6 => Some(Self::B6),
            7 => Some(Self::B7),
            8 => Some(Self::B8),
            9 => Some(Self::B9),
            10 => Some(Self::B10),
            11 => Some(Self::B11),
            12 => Some(Self::B12),
            13 => Some(Self::B13),
            14 => Some(Self::B14),
            15 => Some(Self::B15),
            16 => Some(Self::B16),
            17 => Some(Self::B17),
            18 => Some(Self::B18),
            19 => Some(Self::B19),
            20 => Some(Self::B20),
            21 => Some(Self::B21),
            22 => Some(Self::B22),
            23 => Some(Self::B23),
            24 => Some(Self::B24),
            25 => Some(Self::B25),
            26 => Some(Self::B26),
            27 => Some(Self::B27),
            28 => Some(Self::B28),
            29 => Some(Self::B29),
            30 => Some(Self::B30),
            31 => Some(Self::B31),
            32 => Some(Self::B32),
            _ => None,
        }
    }

    pub fn bits(&self) -> u16 {
        (*self as u16) * 8
    }

    pub fn push_opcode(self) -> u8 {
        op::PUSH1 + self as u8 - 1
    }
}
