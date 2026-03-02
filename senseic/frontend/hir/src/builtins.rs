use sensei_parser::PlankInterner;
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

            // Control flow: (memptr, size) -> void (divergent)
            Builtin::Return | Builtin::Revert => &[(&[MP, U256], VOID)],

            // No args -> void
            Builtin::Stop | Builtin::Invalid => &[(&[], VOID)],

            // SelfDestruct: (addr) -> void
            Builtin::SelfDestruct => &[(&[U256], VOID)],
        }
    }
}

impl fmt::Display for Builtin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            // EVM Arithmetic
            Builtin::Add => "add",
            Builtin::Mul => "mul",
            Builtin::Sub => "sub",
            Builtin::Div => "raw_div",
            Builtin::SDiv => "raw_sdiv",
            Builtin::Mod => "raw_mod",
            Builtin::SMod => "raw_smod",
            Builtin::AddMod => "raw_addmod",
            Builtin::MulMod => "raw_mulmod",
            Builtin::Exp => "exp",
            Builtin::SignExtend => "signextend",

            // EVM Comparison & Bitwise Logic
            Builtin::Lt => "lt",
            Builtin::Gt => "gt",
            Builtin::SLt => "slt",
            Builtin::SGt => "sgt",
            Builtin::Eq => "eq",
            Builtin::IsZero => "iszero",
            Builtin::And => "bitwise_and",
            Builtin::Or => "bitwise_or",
            Builtin::Xor => "bitwise_xor",
            Builtin::Not => "bitwise_not",
            Builtin::Byte => "byte",
            Builtin::Shl => "shl",
            Builtin::Shr => "shr",
            Builtin::Sar => "sar",

            // EVM Keccak-256
            Builtin::Keccak256 => "keccak256",

            // EVM Environment Information
            Builtin::Address => "address_this",
            Builtin::Balance => "balance",
            Builtin::Origin => "origin",
            Builtin::Caller => "caller",
            Builtin::CallValue => "callvalue",
            Builtin::CallDataLoad => "calldataload",
            Builtin::CallDataSize => "calldatasize",
            Builtin::CallDataCopy => "calldatacopy",
            Builtin::CodeSize => "codesize",
            Builtin::CodeCopy => "codecopy",
            Builtin::GasPrice => "gasprice",
            Builtin::ExtCodeSize => "extcodesize",
            Builtin::ExtCodeCopy => "extcodecopy",
            Builtin::ReturnDataSize => "returndatasize",
            Builtin::ReturnDataCopy => "returndatacopy",
            Builtin::ExtCodeHash => "extcodehash",
            Builtin::Gas => "gas",

            // EVM Block Information
            Builtin::BlockHash => "blockhash",
            Builtin::Coinbase => "coinbase",
            Builtin::Timestamp => "timestamp",
            Builtin::Number => "number",
            Builtin::Difficulty => "difficulty",
            Builtin::GasLimit => "gaslimit",
            Builtin::ChainId => "chainid",
            Builtin::SelfBalance => "selfbalance",
            Builtin::BaseFee => "basefee",
            Builtin::BlobHash => "blobhash",
            Builtin::BlobBaseFee => "blobbasefee",

            // EVM State Manipulation
            Builtin::SLoad => "sload",
            Builtin::SStore => "sstore",
            Builtin::TLoad => "tload",
            Builtin::TStore => "tstore",

            // EVM Logging Operations
            Builtin::Log0 => "log0",
            Builtin::Log1 => "log1",
            Builtin::Log2 => "log2",
            Builtin::Log3 => "log3",
            Builtin::Log4 => "log4",

            // EVM System Calls
            Builtin::Create => "create",
            Builtin::Create2 => "create2",
            Builtin::Call => "call",
            Builtin::CallCode => "callcode",
            Builtin::DelegateCall => "delegatecall",
            Builtin::StaticCall => "staticcall",
            Builtin::Return => "return",
            Builtin::Stop => "stop",
            Builtin::Revert => "revert",
            Builtin::Invalid => "invalid",
            Builtin::SelfDestruct => "selfdestruct",

            // IR Memory Primitives
            Builtin::DynamicAllocZeroed => "malloc_zeroed",
            Builtin::DynamicAllocAnyBytes => "malloc_uninit",

            // Memory Manipulation
            Builtin::MemoryCopy => "mcopy",
            Builtin::MLoad1 => "mload1",
            Builtin::MLoad2 => "mload2",
            Builtin::MLoad3 => "mload3",
            Builtin::MLoad4 => "mload4",
            Builtin::MLoad5 => "mload5",
            Builtin::MLoad6 => "mload6",
            Builtin::MLoad7 => "mload7",
            Builtin::MLoad8 => "mload8",
            Builtin::MLoad9 => "mload9",
            Builtin::MLoad10 => "mload10",
            Builtin::MLoad11 => "mload11",
            Builtin::MLoad12 => "mload12",
            Builtin::MLoad13 => "mload13",
            Builtin::MLoad14 => "mload14",
            Builtin::MLoad15 => "mload15",
            Builtin::MLoad16 => "mload16",
            Builtin::MLoad17 => "mload17",
            Builtin::MLoad18 => "mload18",
            Builtin::MLoad19 => "mload19",
            Builtin::MLoad20 => "mload20",
            Builtin::MLoad21 => "mload21",
            Builtin::MLoad22 => "mload22",
            Builtin::MLoad23 => "mload23",
            Builtin::MLoad24 => "mload24",
            Builtin::MLoad25 => "mload25",
            Builtin::MLoad26 => "mload26",
            Builtin::MLoad27 => "mload27",
            Builtin::MLoad28 => "mload28",
            Builtin::MLoad29 => "mload29",
            Builtin::MLoad30 => "mload30",
            Builtin::MLoad31 => "mload31",
            Builtin::MLoad32 => "mload32",
            Builtin::MStore1 => "mstore1",
            Builtin::MStore2 => "mstore2",
            Builtin::MStore3 => "mstore3",
            Builtin::MStore4 => "mstore4",
            Builtin::MStore5 => "mstore5",
            Builtin::MStore6 => "mstore6",
            Builtin::MStore7 => "mstore7",
            Builtin::MStore8 => "mstore8",
            Builtin::MStore9 => "mstore9",
            Builtin::MStore10 => "mstore10",
            Builtin::MStore11 => "mstore11",
            Builtin::MStore12 => "mstore12",
            Builtin::MStore13 => "mstore13",
            Builtin::MStore14 => "mstore14",
            Builtin::MStore15 => "mstore15",
            Builtin::MStore16 => "mstore16",
            Builtin::MStore17 => "mstore17",
            Builtin::MStore18 => "mstore18",
            Builtin::MStore19 => "mstore19",
            Builtin::MStore20 => "mstore20",
            Builtin::MStore21 => "mstore21",
            Builtin::MStore22 => "mstore22",
            Builtin::MStore23 => "mstore23",
            Builtin::MStore24 => "mstore24",
            Builtin::MStore25 => "mstore25",
            Builtin::MStore26 => "mstore26",
            Builtin::MStore27 => "mstore27",
            Builtin::MStore28 => "mstore28",
            Builtin::MStore29 => "mstore29",
            Builtin::MStore30 => "mstore30",
            Builtin::MStore31 => "mstore31",
            Builtin::MStore32 => "mstore32",

            // Bytecode Introspection
            Builtin::RuntimeStartOffset => "runtime_start_offset",
            Builtin::InitEndOffset => "init_end_offset",
            Builtin::RuntimeLength => "runtime_length",
        };
        write!(f, "{name}")
    }
}
