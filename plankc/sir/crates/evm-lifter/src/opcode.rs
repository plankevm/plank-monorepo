#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct StackIO {
    pub inputs: u16,
    pub outputs: u16,
}

impl std::fmt::Display for StackIO {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{},{}", self.inputs, self.outputs)
    }
}

impl StackIO {
    pub const fn new(inputs: u16, outputs: u16) -> Self {
        Self { inputs, outputs }
    }

    pub const fn as_tup(self) -> (u16, u16) {
        (self.inputs, self.outputs)
    }

    pub const fn chain(self, snd: Self) -> Self {
        Self::new(
            self.inputs + snd.inputs.saturating_sub(self.outputs),
            snd.outputs + self.outputs.saturating_sub(snd.inputs),
        )
    }

    pub const fn matches(self, other: Self) -> bool {
        (self.inputs >= self.outputs) == (other.inputs >= other.outputs)
            && self.inputs.abs_diff(self.outputs) == other.inputs.abs_diff(other.outputs)
    }
}

macro_rules! immediate {
    ($imm:literal) => {
        Some($imm)
    };
    () => {
        None
    };
}

macro_rules! define_opcodes {
    (
        $(
            $byte:literal : $name:ident $display:literal ($inputs:expr, $outputs:expr)
            $($imm:literal)?
        ),* $(,)?
    ) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        #[repr(u8)]
        pub enum Opcode {
            $(
                $name = $byte,
            )*
        }

        impl ::core::fmt::Display for Opcode {
            fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                match self {
                    $(
                        Opcode::$name => f.write_str($display),
                    )*
                }
            }
        }

        impl Opcode {
            /// Returns the opcodes effect on the stack in `(inputs, outputs)`.
            pub const fn stack_io(self) -> StackIO {
                match self {
                    $(
                        Opcode::$name => StackIO::new($inputs, $outputs),
                    )*
                }
            }

            pub const fn immediate_bytes(self) -> Option<u8> {
                match self {
                    $(
                        Self::$name => immediate!($($imm)?),
                    )*
                }
            }

            pub const fn from_byte(byte: u8) -> Option<Opcode> {
                match byte {
                    $(
                        $byte => Some(Opcode::$name),
                    )*
                    _ => None,
                }
            }
        }
    };
}

define_opcodes! {
    0x00: Stop        "STOP"           (0, 0),
    0x01: Add         "ADD"            (2, 1),
    0x02: Mul         "MUL"            (2, 1),
    0x03: Sub         "SUB"            (2, 1),
    0x04: Div         "DIV"            (2, 1),
    0x05: Sdiv        "SDIV"           (2, 1),
    0x06: Mod         "MOD"            (2, 1),
    0x07: Smod        "SMOD"           (2, 1),
    0x08: AddMod      "ADDMOD"         (3, 1),
    0x09: MulMod      "MULMOD"         (3, 1),
    0x0a: Exp         "EXP"            (2, 1),
    0x0b: SignExtend  "SIGNEXTEND"     (2, 1),

    0x10: Lt          "LT"             (2, 1),
    0x11: Gt          "GT"             (2, 1),
    0x12: Slt         "SLT"            (2, 1),
    0x13: Sgt         "SGT"            (2, 1),
    0x14: Eq          "EQ"             (2, 1),
    0x15: IsZero      "ISZERO"         (1, 1),
    0x16: And         "AND"            (2, 1),
    0x17: Or          "OR"             (2, 1),
    0x18: Xor         "XOR"            (2, 1),
    0x19: Not         "NOT"            (1, 1),
    0x1a: Byte        "BYTE"           (2, 1),
    0x1b: Shl         "SHL"            (2, 1),
    0x1c: Shr         "SHR"            (2, 1),
    0x1d: Sar         "SAR"            (2, 1),
    0x1e: Clz         "CLZ"            (1, 1),

    0x20: Keccak256   "KECCAK256"      (2, 1),

    0x30: Address        "ADDRESS"        (0, 1),
    0x31: Balance        "BALANCE"        (1, 1),
    0x32: Origin         "ORIGIN"         (0, 1),
    0x33: Caller         "CALLER"         (0, 1),
    0x34: CallValue      "CALLVALUE"      (0, 1),
    0x35: CallDataLoad   "CALLDATALOAD"   (1, 1),
    0x36: CallDataSize   "CALLDATASIZE"   (0, 1),
    0x37: CallDataCopy   "CALLDATACOPY"   (3, 0),
    0x38: CodeSize       "CODESIZE"       (0, 1),
    0x39: CodeCopy       "CODECOPY"       (3, 0),
    0x3a: GasPrice       "GASPRICE"       (0, 1),
    0x3b: ExtCodeSize    "EXTCODESIZE"    (1, 1),
    0x3c: ExtCodeCopy    "EXTCODECOPY"    (4, 0),
    0x3d: ReturnDataSize "RETURNDATASIZE" (0, 1),
    0x3e: ReturnDataCopy "RETURNDATACOPY" (3, 0),
    0x3f: ExtCodeHash    "EXTCODEHASH"    (1, 1),

    0x40: BlockHash    "BLOCKHASH"    (1, 1),
    0x41: Coinbase     "COINBASE"     (0, 1),
    0x42: Timestamp    "TIMESTAMP"    (0, 1),
    0x43: Number       "NUMBER"       (0, 1),
    0x44: PrevRandao   "PREVRANDAO"   (0, 1),
    0x45: GasLimit     "GASLIMIT"     (0, 1),
    0x46: ChainId      "CHAINID"      (0, 1),
    0x47: SelfBalance  "SELFBALANCE"  (0, 1),
    0x48: BaseFee      "BASEFEE"      (0, 1),
    0x49: BlobHash     "BLOBHASH"     (0, 1),
    0x4a: BlobBaseFee  "BLOBBASEFEE"  (0, 1),

    0x50: Pop       "POP"       (1, 0),
    0x51: MLoad     "MLOAD"     (1, 1),
    0x52: MStore    "MSTORE"    (2, 0),
    0x53: MStore8   "MSTORE8"   (2, 0),
    0x54: SLoad     "SLOAD"     (1, 1),
    0x55: SStore    "SSTORE"    (2, 0),
    0x56: Jump      "JUMP"      (1, 0),
    0x57: JumpI     "JUMPI"     (2, 0),
    0x58: Pc        "PC"        (0, 1),
    0x59: MSize     "MSIZE"     (0, 1),
    0x5a: Gas       "GAS"       (0, 1),
    0x5b: JumpDest  "JUMPDEST"  (0, 0),
    0x5c: TLoad     "TLOAD"     (1, 1),
    0x5d: TStore    "TSTORE"    (2, 0),
    0x5e: MCopy     "MCOPY"     (3, 0),
    0x5f: Push0     "PUSH0"     (0, 1),

    0x60: Push1     "PUSH1"     (0, 1) 1,
    0x61: Push2     "PUSH2"     (0, 1) 2,
    0x62: Push3     "PUSH3"     (0, 1) 3,
    0x63: Push4     "PUSH4"     (0, 1) 4,
    0x64: Push5     "PUSH5"     (0, 1) 5,
    0x65: Push6     "PUSH6"     (0, 1) 6,
    0x66: Push7     "PUSH7"     (0, 1) 7,
    0x67: Push8     "PUSH8"     (0, 1) 8,
    0x68: Push9     "PUSH9"     (0, 1) 9,
    0x69: Push10    "PUSH10"    (0, 1) 10,
    0x6a: Push11    "PUSH11"    (0, 1) 11,
    0x6b: Push12    "PUSH12"    (0, 1) 12,
    0x6c: Push13    "PUSH13"    (0, 1) 13,
    0x6d: Push14    "PUSH14"    (0, 1) 14,
    0x6e: Push15    "PUSH15"    (0, 1) 15,
    0x6f: Push16    "PUSH16"    (0, 1) 16,
    0x70: Push17    "PUSH17"    (0, 1) 17,
    0x71: Push18    "PUSH18"    (0, 1) 18,
    0x72: Push19    "PUSH19"    (0, 1) 19,
    0x73: Push20    "PUSH20"    (0, 1) 20,
    0x74: Push21    "PUSH21"    (0, 1) 21,
    0x75: Push22    "PUSH22"    (0, 1) 22,
    0x76: Push23    "PUSH23"    (0, 1) 23,
    0x77: Push24    "PUSH24"    (0, 1) 24,
    0x78: Push25    "PUSH25"    (0, 1) 25,
    0x79: Push26    "PUSH26"    (0, 1) 26,
    0x7a: Push27    "PUSH27"    (0, 1) 27,
    0x7b: Push28    "PUSH28"    (0, 1) 28,
    0x7c: Push29    "PUSH29"    (0, 1) 29,
    0x7d: Push30    "PUSH30"    (0, 1) 30,
    0x7e: Push31    "PUSH31"    (0, 1) 31,
    0x7f: Push32    "PUSH32"    (0, 1) 32,

    0x80: Dup1      "DUP1"      (1,  2),
    0x81: Dup2      "DUP2"      (2,  3),
    0x82: Dup3      "DUP3"      (3,  4),
    0x83: Dup4      "DUP4"      (4,  5),
    0x84: Dup5      "DUP5"      (5,  6),
    0x85: Dup6      "DUP6"      (6,  7),
    0x86: Dup7      "DUP7"      (7,  8),
    0x87: Dup8      "DUP8"      (8,  9),
    0x88: Dup9      "DUP9"      (9,  10),
    0x89: Dup10     "DUP10"     (10, 11),
    0x8a: Dup11     "DUP11"     (11, 12),
    0x8b: Dup12     "DUP12"     (12, 13),
    0x8c: Dup13     "DUP13"     (13, 14),
    0x8d: Dup14     "DUP14"     (14, 15),
    0x8e: Dup15     "DUP15"     (15, 16),
    0x8f: Dup16     "DUP16"     (16, 17),

    0x90: Swap1     "SWAP1"     (2,  2),
    0x91: Swap2     "SWAP2"     (3,  3),
    0x92: Swap3     "SWAP3"     (4,  4),
    0x93: Swap4     "SWAP4"     (5,  5),
    0x94: Swap5     "SWAP5"     (6,  6),
    0x95: Swap6     "SWAP6"     (7,  7),
    0x96: Swap7     "SWAP7"     (8,  8),
    0x97: Swap8     "SWAP8"     (9,  9),
    0x98: Swap9     "SWAP9"     (10, 10),
    0x99: Swap10    "SWAP10"    (11, 11),
    0x9a: Swap11    "SWAP11"    (12, 12),
    0x9b: Swap12    "SWAP12"    (13, 13),
    0x9c: Swap13    "SWAP13"    (14, 14),
    0x9d: Swap14    "SWAP14"    (15, 15),
    0x9e: Swap15    "SWAP15"    (16, 16),
    0x9f: Swap16    "SWAP16"    (17, 17),

    0xa0: Log0      "LOG0"      (2, 0),
    0xa1: Log1      "LOG1"      (3, 0),
    0xa2: Log2      "LOG2"      (4, 0),
    0xa3: Log3      "LOG3"      (5, 0),
    0xa4: Log4      "LOG4"      (6, 0),

    0xf0: Create       "CREATE"       (3, 1),
    0xf1: Call         "CALL"         (7, 1),
    0xf2: CallCode     "CALLCODE"     (7, 1),
    0xf3: Return       "RETURN"       (2, 0),
    0xf4: DelegateCall "DELEGATECALL" (6, 1),
    0xf5: Create2      "CREATE2"      (4, 1),
    0xfa: StaticCall   "STATICCALL"   (6, 1),
    0xfd: Revert       "REVERT"       (2, 0),
    0xfe: Invalid      "INVALID"      (0, 0),
    0xff: SelfDestruct "SELFDESTRUCT" (1, 0),
}

impl Opcode {
    pub const fn is_terminating(self) -> bool {
        matches!(
            self,
            Opcode::Stop | Opcode::Return | Opcode::Revert | Opcode::Invalid | Opcode::SelfDestruct
        )
    }

    /// Returns `Some(n)` for `PUSH1`..=`PUSH32`.
    pub const fn is_push(self) -> Option<u8> {
        if (Opcode::Push1 as u8) <= (self as u8) && (self as u8) <= (Opcode::Push32 as u8) {
            return Some(self as u8 - Opcode::Push1 as u8 + 1);
        }
        None
    }

    /// Returns `Some(n)` for `DUPn` (1..=16).
    pub const fn is_dup(self) -> Option<u8> {
        if (Opcode::Dup1 as u8) <= (self as u8) && (self as u8) <= (Opcode::Dup16 as u8) {
            return Some(self as u8 - Opcode::Dup1 as u8 + 1);
        }
        None
    }

    /// Returns `Some(n)` for `SWAPn` (1..=16).
    pub const fn is_swap(self) -> Option<u8> {
        if (Opcode::Swap1 as u8) <= (self as u8) && (self as u8) <= (Opcode::Swap16 as u8) {
            return Some(self as u8 - Opcode::Swap1 as u8 + 1);
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stack_io_formats_and_chains() {
        let first = StackIO::new(2, 1);
        let second = StackIO::new(1, 2);

        assert_eq!(first.to_string(), "2,1");
        assert_eq!(first.chain(second), StackIO::new(2, 2));
        assert!(first.matches(StackIO::new(3, 2)));
        assert!(!first.matches(StackIO::new(1, 2)));
    }

    #[test]
    fn known_bytes_round_trip_and_display() {
        let cases = [
            (0x00, Opcode::Stop, "STOP"),
            (0x5f, Opcode::Push0, "PUSH0"),
            (0x60, Opcode::Push1, "PUSH1"),
            (0xff, Opcode::SelfDestruct, "SELFDESTRUCT"),
        ];

        for &(byte, opcode, display) in &cases {
            assert_eq!(Opcode::from_byte(byte), Some(opcode));
            assert_eq!(opcode as u8, byte);
            assert_eq!(opcode.to_string(), display);
        }

        assert_eq!(Opcode::from_byte(0xab), None);
    }

    #[test]
    fn opcode_stack_effects_are_defined() {
        assert_eq!(Opcode::Stop.stack_io(), StackIO::new(0, 0));
        assert_eq!(Opcode::Add.stack_io(), StackIO::new(2, 1));
        assert_eq!(Opcode::Push1.stack_io(), StackIO::new(0, 1));
        assert_eq!(Opcode::Call.stack_io(), StackIO::new(7, 1));
    }

    #[test]
    fn push_opcodes_report_their_immediate_sizes() {
        assert_eq!(Opcode::Push0.immediate_bytes(), None);
        assert_eq!(Opcode::Stop.immediate_bytes(), None);

        for byte in 0x60..=0x7f {
            let opcode = Opcode::from_byte(byte).unwrap();
            assert_eq!(opcode.immediate_bytes(), Some(byte - 0x5f));
        }
    }

    #[test]
    fn opcode_families_and_terminators_are_classified() {
        assert_eq!(Opcode::Push0.is_push(), None);
        assert_eq!(Opcode::Push1.is_push(), Some(1));
        assert_eq!(Opcode::Push32.is_push(), Some(32));
        assert_eq!(Opcode::Add.is_push(), None);

        assert_eq!(Opcode::Dup1.is_dup(), Some(1));
        assert_eq!(Opcode::Dup16.is_dup(), Some(16));
        assert_eq!(Opcode::Add.is_dup(), None);

        assert_eq!(Opcode::Swap1.is_swap(), Some(1));
        assert_eq!(Opcode::Swap16.is_swap(), Some(16));
        assert_eq!(Opcode::Add.is_swap(), None);

        assert!(Opcode::Stop.is_terminating());
        assert!(Opcode::SelfDestruct.is_terminating());
        assert!(!Opcode::Jump.is_terminating());
    }
}
