use sensei_parser::{PlankInterner, builtin_names};
use sensei_values::TypeId;
use std::fmt;

pub type BuiltinSignature = (&'static [TypeId], TypeId);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Builtin {
    // ========== EVM Arithmetic ==========
    Add,
    Mul,
    Sub,
    Div,
    SDiv,
    Mod,
    SMod,
    AddMod,
    MulMod,
    Exp,
    SignExtend,

    // ========== EVM Comparison & Bitwise Logic ==========
    Lt,
    Gt,
    SLt,
    SGt,
    Eq,
    IsZero,
    And,
    Or,
    Xor,
    Not,
    Byte,
    Shl,
    Shr,
    Sar,

    // ========== EVM Keccak-256 ==========
    Keccak256,

    // ========== EVM Environment Information ==========
    Address,
    Balance,
    Origin,
    Caller,
    CallValue,
    CallDataLoad,
    CallDataSize,
    CallDataCopy,
    CodeSize,
    CodeCopy,
    GasPrice,
    ExtCodeSize,
    ExtCodeCopy,
    ReturnDataSize,
    ReturnDataCopy,
    ExtCodeHash,
    Gas,

    // ========== EVM Block Information ==========
    BlockHash,
    Coinbase,
    Timestamp,
    Number,
    Difficulty,
    GasLimit,
    ChainId,
    SelfBalance,
    BaseFee,
    BlobHash,
    BlobBaseFee,

    // ========== EVM State Manipulation ==========
    SLoad,
    SStore,
    TLoad,
    TStore,

    // ========== EVM Logging Operations ==========
    Log0,
    Log1,
    Log2,
    Log3,
    Log4,

    // ========== EVM System Calls ==========
    Create,
    Create2,
    Call,
    CallCode,
    DelegateCall,
    StaticCall,
    Return,
    Stop,
    Revert,
    Invalid,
    SelfDestruct,

    // ========== IR Memory Primitives ==========
    DynamicAllocZeroed,
    DynamicAllocAnyBytes,

    // ========== Memory Manipulation ==========
    MemoryCopy,
    MLoad1,
    MLoad2,
    MLoad3,
    MLoad4,
    MLoad5,
    MLoad6,
    MLoad7,
    MLoad8,
    MLoad9,
    MLoad10,
    MLoad11,
    MLoad12,
    MLoad13,
    MLoad14,
    MLoad15,
    MLoad16,
    MLoad17,
    MLoad18,
    MLoad19,
    MLoad20,
    MLoad21,
    MLoad22,
    MLoad23,
    MLoad24,
    MLoad25,
    MLoad26,
    MLoad27,
    MLoad28,
    MLoad29,
    MLoad30,
    MLoad31,
    MLoad32,
    MStore1,
    MStore2,
    MStore3,
    MStore4,
    MStore5,
    MStore6,
    MStore7,
    MStore8,
    MStore9,
    MStore10,
    MStore11,
    MStore12,
    MStore13,
    MStore14,
    MStore15,
    MStore16,
    MStore17,
    MStore18,
    MStore19,
    MStore20,
    MStore21,
    MStore22,
    MStore23,
    MStore24,
    MStore25,
    MStore26,
    MStore27,
    MStore28,
    MStore29,
    MStore30,
    MStore31,
    MStore32,

    // ========== Bytecode Introspection ==========
    RuntimeStartOffset,
    InitEndOffset,
    RuntimeLength,
}

impl Builtin {
    pub fn from_str_id(str_id: sensei_parser::StrId) -> Option<Self> {
        Some(match str_id {
            // ========== EVM Arithmetic ==========
            PlankInterner::ADD => Builtin::Add,
            PlankInterner::MUL => Builtin::Mul,
            PlankInterner::SUB => Builtin::Sub,
            PlankInterner::DIV => Builtin::Div,
            PlankInterner::SDIV => Builtin::SDiv,
            PlankInterner::MOD => Builtin::Mod,
            PlankInterner::SMOD => Builtin::SMod,
            PlankInterner::ADDMOD => Builtin::AddMod,
            PlankInterner::MULMOD => Builtin::MulMod,
            PlankInterner::EXP => Builtin::Exp,
            PlankInterner::SIGNEXTEND => Builtin::SignExtend,

            // ========== EVM Comparison & Bitwise Logic ==========
            PlankInterner::LT => Builtin::Lt,
            PlankInterner::GT => Builtin::Gt,
            PlankInterner::SLT => Builtin::SLt,
            PlankInterner::SGT => Builtin::SGt,
            PlankInterner::EQ => Builtin::Eq,
            PlankInterner::ISZERO => Builtin::IsZero,
            PlankInterner::AND => Builtin::And,
            PlankInterner::OR => Builtin::Or,
            PlankInterner::XOR => Builtin::Xor,
            PlankInterner::NOT => Builtin::Not,
            PlankInterner::BYTE => Builtin::Byte,
            PlankInterner::SHL => Builtin::Shl,
            PlankInterner::SHR => Builtin::Shr,
            PlankInterner::SAR => Builtin::Sar,

            // ========== EVM Keccak-256 ==========
            PlankInterner::KECCAK256 => Builtin::Keccak256,

            // ========== EVM Environment Information ==========
            PlankInterner::ADDRESS => Builtin::Address,
            PlankInterner::BALANCE => Builtin::Balance,
            PlankInterner::ORIGIN => Builtin::Origin,
            PlankInterner::CALLER => Builtin::Caller,
            PlankInterner::CALLVALUE => Builtin::CallValue,
            PlankInterner::CALLDATALOAD => Builtin::CallDataLoad,
            PlankInterner::CALLDATASIZE => Builtin::CallDataSize,
            PlankInterner::CALLDATACOPY => Builtin::CallDataCopy,
            PlankInterner::CODESIZE => Builtin::CodeSize,
            PlankInterner::CODECOPY => Builtin::CodeCopy,
            PlankInterner::GASPRICE => Builtin::GasPrice,
            PlankInterner::EXTCODESIZE => Builtin::ExtCodeSize,
            PlankInterner::EXTCODECOPY => Builtin::ExtCodeCopy,
            PlankInterner::RETURNDATASIZE => Builtin::ReturnDataSize,
            PlankInterner::RETURNDATACOPY => Builtin::ReturnDataCopy,
            PlankInterner::EXTCODEHASH => Builtin::ExtCodeHash,
            PlankInterner::GAS => Builtin::Gas,

            // ========== EVM Block Information ==========
            PlankInterner::BLOCKHASH => Builtin::BlockHash,
            PlankInterner::COINBASE => Builtin::Coinbase,
            PlankInterner::TIMESTAMP => Builtin::Timestamp,
            PlankInterner::NUMBER => Builtin::Number,
            PlankInterner::DIFFICULTY => Builtin::Difficulty,
            PlankInterner::GASLIMIT => Builtin::GasLimit,
            PlankInterner::CHAINID => Builtin::ChainId,
            PlankInterner::SELFBALANCE => Builtin::SelfBalance,
            PlankInterner::BASEFEE => Builtin::BaseFee,
            PlankInterner::BLOBHASH => Builtin::BlobHash,
            PlankInterner::BLOBBASEFEE => Builtin::BlobBaseFee,

            // ========== EVM State Manipulation ==========
            PlankInterner::SLOAD => Builtin::SLoad,
            PlankInterner::SSTORE => Builtin::SStore,
            PlankInterner::TLOAD => Builtin::TLoad,
            PlankInterner::TSTORE => Builtin::TStore,

            // ========== EVM Logging Operations ==========
            PlankInterner::LOG0 => Builtin::Log0,
            PlankInterner::LOG1 => Builtin::Log1,
            PlankInterner::LOG2 => Builtin::Log2,
            PlankInterner::LOG3 => Builtin::Log3,
            PlankInterner::LOG4 => Builtin::Log4,

            // ========== EVM System Calls ==========
            PlankInterner::CREATE => Builtin::Create,
            PlankInterner::CREATE2 => Builtin::Create2,
            PlankInterner::CALL => Builtin::Call,
            PlankInterner::CALLCODE => Builtin::CallCode,
            PlankInterner::DELEGATECALL => Builtin::DelegateCall,
            PlankInterner::STATICCALL => Builtin::StaticCall,
            PlankInterner::RETURN => Builtin::Return,
            PlankInterner::STOP => Builtin::Stop,
            PlankInterner::REVERT => Builtin::Revert,
            PlankInterner::INVALID => Builtin::Invalid,
            PlankInterner::SELFDESTRUCT => Builtin::SelfDestruct,

            // ========== IR Memory Primitives ==========
            PlankInterner::DYNAMIC_ALLOC_ZEROED => Builtin::DynamicAllocZeroed,
            PlankInterner::DYNAMIC_ALLOC_ANY_BYTES => Builtin::DynamicAllocAnyBytes,

            // ========== Memory Manipulation ==========
            PlankInterner::MEMORY_COPY => Builtin::MemoryCopy,
            PlankInterner::MLOAD1 => Builtin::MLoad1,
            PlankInterner::MLOAD2 => Builtin::MLoad2,
            PlankInterner::MLOAD3 => Builtin::MLoad3,
            PlankInterner::MLOAD4 => Builtin::MLoad4,
            PlankInterner::MLOAD5 => Builtin::MLoad5,
            PlankInterner::MLOAD6 => Builtin::MLoad6,
            PlankInterner::MLOAD7 => Builtin::MLoad7,
            PlankInterner::MLOAD8 => Builtin::MLoad8,
            PlankInterner::MLOAD9 => Builtin::MLoad9,
            PlankInterner::MLOAD10 => Builtin::MLoad10,
            PlankInterner::MLOAD11 => Builtin::MLoad11,
            PlankInterner::MLOAD12 => Builtin::MLoad12,
            PlankInterner::MLOAD13 => Builtin::MLoad13,
            PlankInterner::MLOAD14 => Builtin::MLoad14,
            PlankInterner::MLOAD15 => Builtin::MLoad15,
            PlankInterner::MLOAD16 => Builtin::MLoad16,
            PlankInterner::MLOAD17 => Builtin::MLoad17,
            PlankInterner::MLOAD18 => Builtin::MLoad18,
            PlankInterner::MLOAD19 => Builtin::MLoad19,
            PlankInterner::MLOAD20 => Builtin::MLoad20,
            PlankInterner::MLOAD21 => Builtin::MLoad21,
            PlankInterner::MLOAD22 => Builtin::MLoad22,
            PlankInterner::MLOAD23 => Builtin::MLoad23,
            PlankInterner::MLOAD24 => Builtin::MLoad24,
            PlankInterner::MLOAD25 => Builtin::MLoad25,
            PlankInterner::MLOAD26 => Builtin::MLoad26,
            PlankInterner::MLOAD27 => Builtin::MLoad27,
            PlankInterner::MLOAD28 => Builtin::MLoad28,
            PlankInterner::MLOAD29 => Builtin::MLoad29,
            PlankInterner::MLOAD30 => Builtin::MLoad30,
            PlankInterner::MLOAD31 => Builtin::MLoad31,
            PlankInterner::MLOAD32 => Builtin::MLoad32,
            PlankInterner::MSTORE1 => Builtin::MStore1,
            PlankInterner::MSTORE2 => Builtin::MStore2,
            PlankInterner::MSTORE3 => Builtin::MStore3,
            PlankInterner::MSTORE4 => Builtin::MStore4,
            PlankInterner::MSTORE5 => Builtin::MStore5,
            PlankInterner::MSTORE6 => Builtin::MStore6,
            PlankInterner::MSTORE7 => Builtin::MStore7,
            PlankInterner::MSTORE8 => Builtin::MStore8,
            PlankInterner::MSTORE9 => Builtin::MStore9,
            PlankInterner::MSTORE10 => Builtin::MStore10,
            PlankInterner::MSTORE11 => Builtin::MStore11,
            PlankInterner::MSTORE12 => Builtin::MStore12,
            PlankInterner::MSTORE13 => Builtin::MStore13,
            PlankInterner::MSTORE14 => Builtin::MStore14,
            PlankInterner::MSTORE15 => Builtin::MStore15,
            PlankInterner::MSTORE16 => Builtin::MStore16,
            PlankInterner::MSTORE17 => Builtin::MStore17,
            PlankInterner::MSTORE18 => Builtin::MStore18,
            PlankInterner::MSTORE19 => Builtin::MStore19,
            PlankInterner::MSTORE20 => Builtin::MStore20,
            PlankInterner::MSTORE21 => Builtin::MStore21,
            PlankInterner::MSTORE22 => Builtin::MStore22,
            PlankInterner::MSTORE23 => Builtin::MStore23,
            PlankInterner::MSTORE24 => Builtin::MStore24,
            PlankInterner::MSTORE25 => Builtin::MStore25,
            PlankInterner::MSTORE26 => Builtin::MStore26,
            PlankInterner::MSTORE27 => Builtin::MStore27,
            PlankInterner::MSTORE28 => Builtin::MStore28,
            PlankInterner::MSTORE29 => Builtin::MStore29,
            PlankInterner::MSTORE30 => Builtin::MStore30,
            PlankInterner::MSTORE31 => Builtin::MStore31,
            PlankInterner::MSTORE32 => Builtin::MStore32,

            // ========== Bytecode Introspection ==========
            PlankInterner::RUNTIME_START_OFFSET => Builtin::RuntimeStartOffset,
            PlankInterner::INIT_END_OFFSET => Builtin::InitEndOffset,
            PlankInterner::RUNTIME_LENGTH => Builtin::RuntimeLength,

            _ => return None,
        })
    }

    pub fn signatures(&self) -> &'static [BuiltinSignature] {
        const U256: TypeId = TypeId::U256;
        const BOOL: TypeId = TypeId::BOOL;
        const MP: TypeId = TypeId::MEMORY_POINTER;
        const VOID: TypeId = TypeId::VOID;
        const NEVER: TypeId = TypeId::NEVER;

        match self {
            // Pointer offset: ptr + offset or offset + ptr
            Builtin::Add => &[(&[U256, U256], U256), (&[MP, U256], MP), (&[U256, MP], MP)],

            // Pointer arithmetic: ptr - offset -> ptr, ptr - ptr -> distance
            Builtin::Sub => &[(&[U256, U256], U256), (&[MP, U256], MP), (&[MP, MP], U256)],

            // Polymorphic comparison (bool return) - includes pointer comparison
            Builtin::Lt | Builtin::Gt | Builtin::Eq => &[(&[U256, U256], BOOL), (&[MP, MP], BOOL)],

            // Signed comparison - integers only
            Builtin::SLt | Builtin::SGt => &[(&[U256, U256], BOOL)],

            // Unary bool return
            Builtin::IsZero => &[(&[U256], BOOL)],

            // Standard binary u256 -> u256
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

            // Ternary u256 -> u256
            Builtin::AddMod | Builtin::MulMod => &[(&[U256, U256, U256], U256)],

            // Unary u256 -> u256
            Builtin::Not
            | Builtin::Balance
            | Builtin::ExtCodeSize
            | Builtin::ExtCodeHash
            | Builtin::BlockHash
            | Builtin::BlobHash
            | Builtin::CallDataLoad
            | Builtin::SLoad
            | Builtin::TLoad => &[(&[U256], U256)],

            // No args -> u256
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

            // Keccak256: (memptr, u256) -> u256
            Builtin::Keccak256 => &[(&[MP, U256], U256)],

            // Memory allocation: (u256) -> memptr
            Builtin::DynamicAllocZeroed | Builtin::DynamicAllocAnyBytes => &[(&[U256], MP)],

            // MLoad*: (memptr) -> u256
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

            // MStore*: (memptr, u256) -> void
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

            // MemoryCopy: (dst_mp, src_mp, len_u256) -> void
            Builtin::MemoryCopy => &[(&[MP, MP, U256], VOID)],

            // Copy ops: (dst_mp, src_offset_u256, len_u256) -> void
            Builtin::CallDataCopy | Builtin::CodeCopy | Builtin::ReturnDataCopy => {
                &[(&[MP, U256, U256], VOID)]
            }

            // ExtCodeCopy: (addr, dst_mp, src_offset, len) -> void
            Builtin::ExtCodeCopy => &[(&[U256, MP, U256, U256], VOID)],

            // SStore, TStore: (key, value) -> void
            Builtin::SStore | Builtin::TStore => &[(&[U256, U256], VOID)],

            // Log0-4: (memptr, size, topic0..topicN) -> void
            Builtin::Log0 => &[(&[MP, U256], VOID)],
            Builtin::Log1 => &[(&[MP, U256, U256], VOID)],
            Builtin::Log2 => &[(&[MP, U256, U256, U256], VOID)],
            Builtin::Log3 => &[(&[MP, U256, U256, U256, U256], VOID)],
            Builtin::Log4 => &[(&[MP, U256, U256, U256, U256, U256], VOID)],

            // Create: (value, offset, size) -> u256
            Builtin::Create => &[(&[U256, MP, U256], U256)],
            // Create2: (value, offset, size, salt) -> u256
            Builtin::Create2 => &[(&[U256, MP, U256, U256], U256)],

            // Call: (gas, addr, value, argsOffset, argsSize, retOffset, retSize) -> u256
            Builtin::Call | Builtin::CallCode => &[(&[U256, U256, U256, MP, U256, MP, U256], U256)],
            // DelegateCall/StaticCall: (gas, addr, argsOffset, argsSize, retOffset, retSize) ->
            // u256
            Builtin::DelegateCall | Builtin::StaticCall => {
                &[(&[U256, U256, MP, U256, MP, U256], U256)]
            }

            // Control flow: (memptr, size) -> never
            Builtin::Return | Builtin::Revert => &[(&[MP, U256], NEVER)],

            // No args -> never
            Builtin::Stop | Builtin::Invalid => &[(&[], NEVER)],

            // SelfDestruct: (addr) -> never
            Builtin::SelfDestruct => &[(&[U256], NEVER)],
        }
    }
}

impl fmt::Display for Builtin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            // EVM Arithmetic
            Builtin::Add => builtin_names::ADD,
            Builtin::Mul => builtin_names::MUL,
            Builtin::Sub => builtin_names::SUB,
            Builtin::Div => builtin_names::DIV,
            Builtin::SDiv => builtin_names::SDIV,
            Builtin::Mod => builtin_names::MOD,
            Builtin::SMod => builtin_names::SMOD,
            Builtin::AddMod => builtin_names::ADDMOD,
            Builtin::MulMod => builtin_names::MULMOD,
            Builtin::Exp => builtin_names::EXP,
            Builtin::SignExtend => builtin_names::SIGNEXTEND,

            // EVM Comparison & Bitwise Logic
            Builtin::Lt => builtin_names::LT,
            Builtin::Gt => builtin_names::GT,
            Builtin::SLt => builtin_names::SLT,
            Builtin::SGt => builtin_names::SGT,
            Builtin::Eq => builtin_names::EQ,
            Builtin::IsZero => builtin_names::ISZERO,
            Builtin::And => builtin_names::AND,
            Builtin::Or => builtin_names::OR,
            Builtin::Xor => builtin_names::XOR,
            Builtin::Not => builtin_names::NOT,
            Builtin::Byte => builtin_names::BYTE,
            Builtin::Shl => builtin_names::SHL,
            Builtin::Shr => builtin_names::SHR,
            Builtin::Sar => builtin_names::SAR,

            // EVM Keccak-256
            Builtin::Keccak256 => builtin_names::KECCAK256,

            // EVM Environment Information
            Builtin::Address => builtin_names::ADDRESS,
            Builtin::Balance => builtin_names::BALANCE,
            Builtin::Origin => builtin_names::ORIGIN,
            Builtin::Caller => builtin_names::CALLER,
            Builtin::CallValue => builtin_names::CALLVALUE,
            Builtin::CallDataLoad => builtin_names::CALLDATALOAD,
            Builtin::CallDataSize => builtin_names::CALLDATASIZE,
            Builtin::CallDataCopy => builtin_names::CALLDATACOPY,
            Builtin::CodeSize => builtin_names::CODESIZE,
            Builtin::CodeCopy => builtin_names::CODECOPY,
            Builtin::GasPrice => builtin_names::GASPRICE,
            Builtin::ExtCodeSize => builtin_names::EXTCODESIZE,
            Builtin::ExtCodeCopy => builtin_names::EXTCODECOPY,
            Builtin::ReturnDataSize => builtin_names::RETURNDATASIZE,
            Builtin::ReturnDataCopy => builtin_names::RETURNDATACOPY,
            Builtin::ExtCodeHash => builtin_names::EXTCODEHASH,
            Builtin::Gas => builtin_names::GAS,

            // EVM Block Information
            Builtin::BlockHash => builtin_names::BLOCKHASH,
            Builtin::Coinbase => builtin_names::COINBASE,
            Builtin::Timestamp => builtin_names::TIMESTAMP,
            Builtin::Number => builtin_names::NUMBER,
            Builtin::Difficulty => builtin_names::DIFFICULTY,
            Builtin::GasLimit => builtin_names::GASLIMIT,
            Builtin::ChainId => builtin_names::CHAINID,
            Builtin::SelfBalance => builtin_names::SELFBALANCE,
            Builtin::BaseFee => builtin_names::BASEFEE,
            Builtin::BlobHash => builtin_names::BLOBHASH,
            Builtin::BlobBaseFee => builtin_names::BLOBBASEFEE,

            // EVM State Manipulation
            Builtin::SLoad => builtin_names::SLOAD,
            Builtin::SStore => builtin_names::SSTORE,
            Builtin::TLoad => builtin_names::TLOAD,
            Builtin::TStore => builtin_names::TSTORE,

            // EVM Logging Operations
            Builtin::Log0 => builtin_names::LOG0,
            Builtin::Log1 => builtin_names::LOG1,
            Builtin::Log2 => builtin_names::LOG2,
            Builtin::Log3 => builtin_names::LOG3,
            Builtin::Log4 => builtin_names::LOG4,

            // EVM System Calls
            Builtin::Create => builtin_names::CREATE,
            Builtin::Create2 => builtin_names::CREATE2,
            Builtin::Call => builtin_names::CALL,
            Builtin::CallCode => builtin_names::CALLCODE,
            Builtin::DelegateCall => builtin_names::DELEGATECALL,
            Builtin::StaticCall => builtin_names::STATICCALL,
            Builtin::Return => builtin_names::RETURN,
            Builtin::Stop => builtin_names::STOP,
            Builtin::Revert => builtin_names::REVERT,
            Builtin::Invalid => builtin_names::INVALID,
            Builtin::SelfDestruct => builtin_names::SELFDESTRUCT,

            // IR Memory Primitives
            Builtin::DynamicAllocZeroed => builtin_names::DYNAMIC_ALLOC_ZEROED,
            Builtin::DynamicAllocAnyBytes => builtin_names::DYNAMIC_ALLOC_ANY_BYTES,

            // Memory Manipulation
            Builtin::MemoryCopy => builtin_names::MEMORY_COPY,
            Builtin::MLoad1 => builtin_names::MLOAD1,
            Builtin::MLoad2 => builtin_names::MLOAD2,
            Builtin::MLoad3 => builtin_names::MLOAD3,
            Builtin::MLoad4 => builtin_names::MLOAD4,
            Builtin::MLoad5 => builtin_names::MLOAD5,
            Builtin::MLoad6 => builtin_names::MLOAD6,
            Builtin::MLoad7 => builtin_names::MLOAD7,
            Builtin::MLoad8 => builtin_names::MLOAD8,
            Builtin::MLoad9 => builtin_names::MLOAD9,
            Builtin::MLoad10 => builtin_names::MLOAD10,
            Builtin::MLoad11 => builtin_names::MLOAD11,
            Builtin::MLoad12 => builtin_names::MLOAD12,
            Builtin::MLoad13 => builtin_names::MLOAD13,
            Builtin::MLoad14 => builtin_names::MLOAD14,
            Builtin::MLoad15 => builtin_names::MLOAD15,
            Builtin::MLoad16 => builtin_names::MLOAD16,
            Builtin::MLoad17 => builtin_names::MLOAD17,
            Builtin::MLoad18 => builtin_names::MLOAD18,
            Builtin::MLoad19 => builtin_names::MLOAD19,
            Builtin::MLoad20 => builtin_names::MLOAD20,
            Builtin::MLoad21 => builtin_names::MLOAD21,
            Builtin::MLoad22 => builtin_names::MLOAD22,
            Builtin::MLoad23 => builtin_names::MLOAD23,
            Builtin::MLoad24 => builtin_names::MLOAD24,
            Builtin::MLoad25 => builtin_names::MLOAD25,
            Builtin::MLoad26 => builtin_names::MLOAD26,
            Builtin::MLoad27 => builtin_names::MLOAD27,
            Builtin::MLoad28 => builtin_names::MLOAD28,
            Builtin::MLoad29 => builtin_names::MLOAD29,
            Builtin::MLoad30 => builtin_names::MLOAD30,
            Builtin::MLoad31 => builtin_names::MLOAD31,
            Builtin::MLoad32 => builtin_names::MLOAD32,
            Builtin::MStore1 => builtin_names::MSTORE1,
            Builtin::MStore2 => builtin_names::MSTORE2,
            Builtin::MStore3 => builtin_names::MSTORE3,
            Builtin::MStore4 => builtin_names::MSTORE4,
            Builtin::MStore5 => builtin_names::MSTORE5,
            Builtin::MStore6 => builtin_names::MSTORE6,
            Builtin::MStore7 => builtin_names::MSTORE7,
            Builtin::MStore8 => builtin_names::MSTORE8,
            Builtin::MStore9 => builtin_names::MSTORE9,
            Builtin::MStore10 => builtin_names::MSTORE10,
            Builtin::MStore11 => builtin_names::MSTORE11,
            Builtin::MStore12 => builtin_names::MSTORE12,
            Builtin::MStore13 => builtin_names::MSTORE13,
            Builtin::MStore14 => builtin_names::MSTORE14,
            Builtin::MStore15 => builtin_names::MSTORE15,
            Builtin::MStore16 => builtin_names::MSTORE16,
            Builtin::MStore17 => builtin_names::MSTORE17,
            Builtin::MStore18 => builtin_names::MSTORE18,
            Builtin::MStore19 => builtin_names::MSTORE19,
            Builtin::MStore20 => builtin_names::MSTORE20,
            Builtin::MStore21 => builtin_names::MSTORE21,
            Builtin::MStore22 => builtin_names::MSTORE22,
            Builtin::MStore23 => builtin_names::MSTORE23,
            Builtin::MStore24 => builtin_names::MSTORE24,
            Builtin::MStore25 => builtin_names::MSTORE25,
            Builtin::MStore26 => builtin_names::MSTORE26,
            Builtin::MStore27 => builtin_names::MSTORE27,
            Builtin::MStore28 => builtin_names::MSTORE28,
            Builtin::MStore29 => builtin_names::MSTORE29,
            Builtin::MStore30 => builtin_names::MSTORE30,
            Builtin::MStore31 => builtin_names::MSTORE31,
            Builtin::MStore32 => builtin_names::MSTORE32,

            // Bytecode Introspection
            Builtin::RuntimeStartOffset => builtin_names::RUNTIME_START_OFFSET,
            Builtin::InitEndOffset => builtin_names::INIT_END_OFFSET,
            Builtin::RuntimeLength => builtin_names::RUNTIME_LENGTH,
        };
        write!(f, "{name}")
    }
}
