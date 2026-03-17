use crate::{Session, StrId, types::TypeId};

pub type BuiltinSignature = (&'static [TypeId], TypeId);

macro_rules! define_builtins {
    (
        primitive_types {
            $($pt_const:ident = $pt_str:literal => $pt_type:ident;)*
        }
        builtins {
            $($b_const:ident = $b_str:literal => $b_variant:ident;)*
        }
    ) => {
        pub mod builtin_names {
            $(pub const $pt_const: &str = $pt_str;)*
            $(pub const $b_const: &str = $b_str;)*
        }

        #[doc(hidden)]
        #[repr(u32)]
        enum BuiltinStrIdx {
            $($pt_type,)*
            $($b_variant,)*
        }

        $(pub const $pt_const: StrId = StrId::new(BuiltinStrIdx::$pt_type as u32);)*
        $(pub const $b_const: StrId = StrId::new(BuiltinStrIdx::$b_variant as u32);)*

        pub fn inject_builtins(interner: &mut Session) {
            $(assert_eq!(interner.intern(builtin_names::$pt_const), $pt_const);)*
            $(assert_eq!(interner.intern(builtin_names::$b_const), $b_const);)*
        }

        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub enum Builtin {
            $($b_variant,)*
        }

        impl Builtin {
            pub fn from_str_id(id: StrId) -> Option<Self> {
                Some(match id {
                    $($b_const => Builtin::$b_variant,)*
                    _ => return None,
                })
            }
        }

        impl ::std::fmt::Display for Builtin {
            fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                let name = match self {
                    $(Builtin::$b_variant => builtin_names::$b_const,)*
                };
                f.write_str(name)
            }
        }

        impl TypeId {
            pub fn resolve_primitive(name: StrId) -> Option<TypeId> {
                Some(match name {
                    $($pt_const => TypeId::$pt_const,)*
                    _ => return None,
                })
            }
        }
    };
}

define_builtins! {
    primitive_types {
        VOID = "void" => Void;
        U256 = "u256" => U256;
        BOOL = "bool" => Bool;
        MEMORY_POINTER = "memptr" => MemoryPointer;
        TYPE = "type" => Type;
        FUNCTION = "function" => Function;
        NEVER = "never" => Never;
    }

    builtins {
        // EVM Arithmetic
        ADD = "add" => Add;
        MUL = "mul" => Mul;
        SUB = "sub" => Sub;
        DIV = "raw_div" => Div;
        SDIV = "raw_sdiv" => SDiv;
        MOD = "raw_mod" => Mod;
        SMOD = "raw_smod" => SMod;
        ADDMOD = "raw_addmod" => AddMod;
        MULMOD = "raw_mulmod" => MulMod;
        EXP = "exp" => Exp;
        SIGNEXTEND = "signextend" => SignExtend;

        // EVM Comparison & Bitwise Logic
        LT = "lt" => Lt;
        GT = "gt" => Gt;
        SLT = "slt" => SLt;
        SGT = "sgt" => SGt;
        EQ = "eq" => Eq;
        ISZERO = "iszero" => IsZero;
        AND = "bitwise_and" => And;
        OR = "bitwise_or" => Or;
        XOR = "bitwise_xor" => Xor;
        NOT = "bitwise_not" => Not;
        BYTE = "byte" => Byte;
        SHL = "shl" => Shl;
        SHR = "shr" => Shr;
        SAR = "sar" => Sar;

        // EVM Keccak-256
        KECCAK256 = "keccak256" => Keccak256;

        // EVM Environment Information
        ADDRESS = "address_this" => Address;
        BALANCE = "balance" => Balance;
        ORIGIN = "origin" => Origin;
        CALLER = "caller" => Caller;
        CALLVALUE = "callvalue" => CallValue;
        CALLDATALOAD = "calldataload" => CallDataLoad;
        CALLDATASIZE = "calldatasize" => CallDataSize;
        CALLDATACOPY = "calldatacopy" => CallDataCopy;
        CODESIZE = "codesize" => CodeSize;
        CODECOPY = "codecopy" => CodeCopy;
        GASPRICE = "gasprice" => GasPrice;
        EXTCODESIZE = "extcodesize" => ExtCodeSize;
        EXTCODECOPY = "extcodecopy" => ExtCodeCopy;
        RETURNDATASIZE = "returndatasize" => ReturnDataSize;
        RETURNDATACOPY = "returndatacopy" => ReturnDataCopy;
        EXTCODEHASH = "extcodehash" => ExtCodeHash;
        GAS = "gas" => Gas;

        // EVM Block Information
        BLOCKHASH = "blockhash" => BlockHash;
        COINBASE = "coinbase" => Coinbase;
        TIMESTAMP = "timestamp" => Timestamp;
        NUMBER = "number" => Number;
        DIFFICULTY = "difficulty" => Difficulty;
        GASLIMIT = "gaslimit" => GasLimit;
        CHAINID = "chainid" => ChainId;
        SELFBALANCE = "selfbalance" => SelfBalance;
        BASEFEE = "basefee" => BaseFee;
        BLOBHASH = "blobhash" => BlobHash;
        BLOBBASEFEE = "blobbasefee" => BlobBaseFee;

        // EVM State Manipulation
        SLOAD = "sload" => SLoad;
        SSTORE = "sstore" => SStore;
        TLOAD = "tload" => TLoad;
        TSTORE = "tstore" => TStore;

        // EVM Logging Operations
        LOG0 = "log0" => Log0;
        LOG1 = "log1" => Log1;
        LOG2 = "log2" => Log2;
        LOG3 = "log3" => Log3;
        LOG4 = "log4" => Log4;

        // EVM System Calls
        CREATE = "create" => Create;
        CREATE2 = "create2" => Create2;
        CALL = "call" => Call;
        CALLCODE = "callcode" => CallCode;
        DELEGATECALL = "delegatecall" => DelegateCall;
        STATICCALL = "staticcall" => StaticCall;
        RETURN = "evm_return" => Return;
        STOP = "evm_stop" => Stop;
        REVERT = "revert" => Revert;
        INVALID = "invalid" => Invalid;
        SELFDESTRUCT = "selfdestruct" => SelfDestruct;

        // IR Memory Primitives
        DYNAMIC_ALLOC_ZEROED = "malloc_zeroed" => DynamicAllocZeroed;
        DYNAMIC_ALLOC_ANY_BYTES = "malloc_uninit" => DynamicAllocAnyBytes;

        // Memory Manipulation
        MEMORY_COPY = "mcopy" => MemoryCopy;
        MLOAD1 = "mload1" => MLoad1;
        MLOAD2 = "mload2" => MLoad2;
        MLOAD3 = "mload3" => MLoad3;
        MLOAD4 = "mload4" => MLoad4;
        MLOAD5 = "mload5" => MLoad5;
        MLOAD6 = "mload6" => MLoad6;
        MLOAD7 = "mload7" => MLoad7;
        MLOAD8 = "mload8" => MLoad8;
        MLOAD9 = "mload9" => MLoad9;
        MLOAD10 = "mload10" => MLoad10;
        MLOAD11 = "mload11" => MLoad11;
        MLOAD12 = "mload12" => MLoad12;
        MLOAD13 = "mload13" => MLoad13;
        MLOAD14 = "mload14" => MLoad14;
        MLOAD15 = "mload15" => MLoad15;
        MLOAD16 = "mload16" => MLoad16;
        MLOAD17 = "mload17" => MLoad17;
        MLOAD18 = "mload18" => MLoad18;
        MLOAD19 = "mload19" => MLoad19;
        MLOAD20 = "mload20" => MLoad20;
        MLOAD21 = "mload21" => MLoad21;
        MLOAD22 = "mload22" => MLoad22;
        MLOAD23 = "mload23" => MLoad23;
        MLOAD24 = "mload24" => MLoad24;
        MLOAD25 = "mload25" => MLoad25;
        MLOAD26 = "mload26" => MLoad26;
        MLOAD27 = "mload27" => MLoad27;
        MLOAD28 = "mload28" => MLoad28;
        MLOAD29 = "mload29" => MLoad29;
        MLOAD30 = "mload30" => MLoad30;
        MLOAD31 = "mload31" => MLoad31;
        MLOAD32 = "mload32" => MLoad32;
        MSTORE1 = "mstore1" => MStore1;
        MSTORE2 = "mstore2" => MStore2;
        MSTORE3 = "mstore3" => MStore3;
        MSTORE4 = "mstore4" => MStore4;
        MSTORE5 = "mstore5" => MStore5;
        MSTORE6 = "mstore6" => MStore6;
        MSTORE7 = "mstore7" => MStore7;
        MSTORE8 = "mstore8" => MStore8;
        MSTORE9 = "mstore9" => MStore9;
        MSTORE10 = "mstore10" => MStore10;
        MSTORE11 = "mstore11" => MStore11;
        MSTORE12 = "mstore12" => MStore12;
        MSTORE13 = "mstore13" => MStore13;
        MSTORE14 = "mstore14" => MStore14;
        MSTORE15 = "mstore15" => MStore15;
        MSTORE16 = "mstore16" => MStore16;
        MSTORE17 = "mstore17" => MStore17;
        MSTORE18 = "mstore18" => MStore18;
        MSTORE19 = "mstore19" => MStore19;
        MSTORE20 = "mstore20" => MStore20;
        MSTORE21 = "mstore21" => MStore21;
        MSTORE22 = "mstore22" => MStore22;
        MSTORE23 = "mstore23" => MStore23;
        MSTORE24 = "mstore24" => MStore24;
        MSTORE25 = "mstore25" => MStore25;
        MSTORE26 = "mstore26" => MStore26;
        MSTORE27 = "mstore27" => MStore27;
        MSTORE28 = "mstore28" => MStore28;
        MSTORE29 = "mstore29" => MStore29;
        MSTORE30 = "mstore30" => MStore30;
        MSTORE31 = "mstore31" => MStore31;
        MSTORE32 = "mstore32" => MStore32;

        // Bytecode Introspection
        RUNTIME_START_OFFSET = "runtime_start_offset" => RuntimeStartOffset;
        INIT_END_OFFSET = "init_end_offset" => InitEndOffset;
        RUNTIME_LENGTH = "runtime_length" => RuntimeLength;
    }
}

impl Builtin {
    pub fn signatures(&self) -> &'static [BuiltinSignature] {
        const U256: TypeId = TypeId::U256;
        const BOOL: TypeId = TypeId::BOOL;
        const MP: TypeId = TypeId::MEMORY_POINTER;
        const VOID: TypeId = TypeId::VOID;
        const NEVER: TypeId = TypeId::NEVER;

        match self {
            Builtin::Add => &[(&[U256, U256], U256), (&[MP, U256], MP), (&[U256, MP], MP)],
            Builtin::Sub => &[(&[U256, U256], U256), (&[MP, U256], MP), (&[MP, MP], U256)],

            Builtin::Lt | Builtin::Gt | Builtin::Eq => &[(&[U256, U256], BOOL), (&[MP, MP], BOOL)],

            Builtin::SLt | Builtin::SGt => &[(&[U256, U256], BOOL)],

            Builtin::IsZero => &[(&[U256], BOOL)],

            Builtin::Mul
            | Builtin::Div
            | Builtin::SDiv
            | Builtin::Mod
            | Builtin::SMod
            | Builtin::Exp
            | Builtin::SignExtend
            | Builtin::And
            | Builtin::Or
            | Builtin::Xor
            | Builtin::Byte
            | Builtin::Shl
            | Builtin::Shr
            | Builtin::Sar => &[(&[U256, U256], U256)],

            Builtin::AddMod | Builtin::MulMod => &[(&[U256, U256, U256], U256)],

            Builtin::Not
            | Builtin::Balance
            | Builtin::ExtCodeSize
            | Builtin::ExtCodeHash
            | Builtin::BlockHash
            | Builtin::BlobHash
            | Builtin::CallDataLoad
            | Builtin::SLoad
            | Builtin::TLoad => &[(&[U256], U256)],

            Builtin::Address
            | Builtin::Origin
            | Builtin::Caller
            | Builtin::CallValue
            | Builtin::CallDataSize
            | Builtin::CodeSize
            | Builtin::GasPrice
            | Builtin::ReturnDataSize
            | Builtin::Gas
            | Builtin::Coinbase
            | Builtin::Timestamp
            | Builtin::Number
            | Builtin::Difficulty
            | Builtin::GasLimit
            | Builtin::ChainId
            | Builtin::SelfBalance
            | Builtin::BaseFee
            | Builtin::BlobBaseFee
            | Builtin::RuntimeStartOffset
            | Builtin::InitEndOffset
            | Builtin::RuntimeLength => &[(&[], U256)],

            Builtin::Keccak256 => &[(&[MP, U256], U256)],

            Builtin::DynamicAllocZeroed | Builtin::DynamicAllocAnyBytes => &[(&[U256], MP)],

            Builtin::MLoad1
            | Builtin::MLoad2
            | Builtin::MLoad3
            | Builtin::MLoad4
            | Builtin::MLoad5
            | Builtin::MLoad6
            | Builtin::MLoad7
            | Builtin::MLoad8
            | Builtin::MLoad9
            | Builtin::MLoad10
            | Builtin::MLoad11
            | Builtin::MLoad12
            | Builtin::MLoad13
            | Builtin::MLoad14
            | Builtin::MLoad15
            | Builtin::MLoad16
            | Builtin::MLoad17
            | Builtin::MLoad18
            | Builtin::MLoad19
            | Builtin::MLoad20
            | Builtin::MLoad21
            | Builtin::MLoad22
            | Builtin::MLoad23
            | Builtin::MLoad24
            | Builtin::MLoad25
            | Builtin::MLoad26
            | Builtin::MLoad27
            | Builtin::MLoad28
            | Builtin::MLoad29
            | Builtin::MLoad30
            | Builtin::MLoad31
            | Builtin::MLoad32 => &[(&[MP], U256)],

            Builtin::MStore1
            | Builtin::MStore2
            | Builtin::MStore3
            | Builtin::MStore4
            | Builtin::MStore5
            | Builtin::MStore6
            | Builtin::MStore7
            | Builtin::MStore8
            | Builtin::MStore9
            | Builtin::MStore10
            | Builtin::MStore11
            | Builtin::MStore12
            | Builtin::MStore13
            | Builtin::MStore14
            | Builtin::MStore15
            | Builtin::MStore16
            | Builtin::MStore17
            | Builtin::MStore18
            | Builtin::MStore19
            | Builtin::MStore20
            | Builtin::MStore21
            | Builtin::MStore22
            | Builtin::MStore23
            | Builtin::MStore24
            | Builtin::MStore25
            | Builtin::MStore26
            | Builtin::MStore27
            | Builtin::MStore28
            | Builtin::MStore29
            | Builtin::MStore30
            | Builtin::MStore31
            | Builtin::MStore32 => &[(&[MP, U256], VOID)],

            Builtin::MemoryCopy => &[(&[MP, MP, U256], VOID)],

            Builtin::CallDataCopy | Builtin::CodeCopy | Builtin::ReturnDataCopy => {
                &[(&[MP, U256, U256], VOID)]
            }

            Builtin::ExtCodeCopy => &[(&[U256, MP, U256, U256], VOID)],

            Builtin::SStore | Builtin::TStore => &[(&[U256, U256], VOID)],

            Builtin::Log0 => &[(&[MP, U256], VOID)],
            Builtin::Log1 => &[(&[MP, U256, U256], VOID)],
            Builtin::Log2 => &[(&[MP, U256, U256, U256], VOID)],
            Builtin::Log3 => &[(&[MP, U256, U256, U256, U256], VOID)],
            Builtin::Log4 => &[(&[MP, U256, U256, U256, U256, U256], VOID)],

            Builtin::Create => &[(&[U256, MP, U256], U256)],
            Builtin::Create2 => &[(&[U256, MP, U256, U256], U256)],

            Builtin::Call | Builtin::CallCode => &[(&[U256, U256, U256, MP, U256, MP, U256], U256)],
            Builtin::DelegateCall | Builtin::StaticCall => {
                &[(&[U256, U256, MP, U256, MP, U256], U256)]
            }

            Builtin::Return | Builtin::Revert => &[(&[MP, U256], NEVER)],

            Builtin::Stop | Builtin::Invalid => &[(&[], NEVER)],

            Builtin::SelfDestruct => &[(&[U256], NEVER)],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_inject_builtins() {
        let mut session = Session::new();
        inject_builtins(&mut session);
    }

    #[test]
    fn test_builtin_roundtrip() {
        assert_eq!(Builtin::from_str_id(ADD), Some(Builtin::Add));
        assert_eq!(Builtin::from_str_id(KECCAK256), Some(Builtin::Keccak256));
        assert_eq!(Builtin::from_str_id(VOID), None);
    }

    #[test]
    fn test_resolve_primitive() {
        assert_eq!(TypeId::resolve_primitive(VOID), Some(TypeId::VOID));
        assert_eq!(TypeId::resolve_primitive(U256), Some(TypeId::U256));
        assert_eq!(TypeId::resolve_primitive(ADD), None);
    }
}
