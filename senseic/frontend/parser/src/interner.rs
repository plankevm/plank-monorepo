use crate::builtin_names;
use sensei_core::{intern::StringInterner, newtype_index};

newtype_index! {
    /// String ID
    pub struct StrId;
}

pub struct PlankInterner {
    inner: StringInterner<StrId>,
}

macro_rules! builtin_str_ids {
    ($($name:ident => $str_expr:expr),* $(,)?) => {
        #[doc(hidden)]
        #[repr(u32)]
        #[allow(non_camel_case_types, clippy::upper_case_acronyms)]
        enum BuiltinStrIdx { $($name),* }

        impl PlankInterner {
            $(pub const $name: StrId = StrId::new(BuiltinStrIdx::$name as u32);)*

            fn inject_primitives(interner: &mut StringInterner<StrId>) {
                $(assert_eq!(interner.intern($str_expr), Self::$name);)*
            }
        }
    };
}

builtin_str_ids! {
    // ========== Type Names ==========
    VOID_TYPE_NAME     => builtin_names::VOID_TYPE_NAME,
    U256_TYPE_NAME     => builtin_names::U256_TYPE_NAME,
    BOOL_TYPE_NAME     => builtin_names::BOOL_TYPE_NAME,
    MEMPTR_TYPE_NAME   => builtin_names::MEMPTR_TYPE_NAME,
    TYPE_TYPE_NAME     => builtin_names::TYPE_TYPE_NAME,
    FUNCTION_TYPE_NAME => builtin_names::FUNCTION_TYPE_NAME,
    NEVER_TYPE_NAME    => builtin_names::NEVER_TYPE_NAME,

    // ========== EVM Arithmetic ==========
    ADD        => builtin_names::ADD,
    MUL        => builtin_names::MUL,
    SUB        => builtin_names::SUB,
    DIV        => builtin_names::DIV,
    SDIV       => builtin_names::SDIV,
    MOD        => builtin_names::MOD,
    SMOD       => builtin_names::SMOD,
    ADDMOD     => builtin_names::ADDMOD,
    MULMOD     => builtin_names::MULMOD,
    EXP        => builtin_names::EXP,
    SIGNEXTEND => builtin_names::SIGNEXTEND,

    // ========== EVM Comparison & Bitwise Logic ==========
    LT    => builtin_names::LT,
    GT    => builtin_names::GT,
    SLT   => builtin_names::SLT,
    SGT   => builtin_names::SGT,
    EQ    => builtin_names::EQ,
    ISZERO => builtin_names::ISZERO,
    AND   => builtin_names::AND,
    OR    => builtin_names::OR,
    XOR   => builtin_names::XOR,
    NOT   => builtin_names::NOT,
    BYTE  => builtin_names::BYTE,
    SHL   => builtin_names::SHL,
    SHR   => builtin_names::SHR,
    SAR   => builtin_names::SAR,

    // ========== EVM Keccak-256 ==========
    KECCAK256 => builtin_names::KECCAK256,

    // ========== EVM Environment Information ==========
    ADDRESS        => builtin_names::ADDRESS,
    BALANCE        => builtin_names::BALANCE,
    ORIGIN         => builtin_names::ORIGIN,
    CALLER         => builtin_names::CALLER,
    CALLVALUE      => builtin_names::CALLVALUE,
    CALLDATALOAD   => builtin_names::CALLDATALOAD,
    CALLDATASIZE   => builtin_names::CALLDATASIZE,
    CALLDATACOPY   => builtin_names::CALLDATACOPY,
    CODESIZE       => builtin_names::CODESIZE,
    CODECOPY       => builtin_names::CODECOPY,
    GASPRICE       => builtin_names::GASPRICE,
    EXTCODESIZE    => builtin_names::EXTCODESIZE,
    EXTCODECOPY    => builtin_names::EXTCODECOPY,
    RETURNDATASIZE => builtin_names::RETURNDATASIZE,
    RETURNDATACOPY => builtin_names::RETURNDATACOPY,
    EXTCODEHASH    => builtin_names::EXTCODEHASH,
    GAS            => builtin_names::GAS,

    // ========== EVM Block Information ==========
    BLOCKHASH   => builtin_names::BLOCKHASH,
    COINBASE    => builtin_names::COINBASE,
    TIMESTAMP   => builtin_names::TIMESTAMP,
    NUMBER      => builtin_names::NUMBER,
    DIFFICULTY  => builtin_names::DIFFICULTY,
    GASLIMIT    => builtin_names::GASLIMIT,
    CHAINID     => builtin_names::CHAINID,
    SELFBALANCE => builtin_names::SELFBALANCE,
    BASEFEE     => builtin_names::BASEFEE,
    BLOBHASH    => builtin_names::BLOBHASH,
    BLOBBASEFEE => builtin_names::BLOBBASEFEE,

    // ========== EVM State Manipulation ==========
    SLOAD  => builtin_names::SLOAD,
    SSTORE => builtin_names::SSTORE,
    TLOAD  => builtin_names::TLOAD,
    TSTORE => builtin_names::TSTORE,

    // ========== EVM Logging Operations ==========
    LOG0 => builtin_names::LOG0,
    LOG1 => builtin_names::LOG1,
    LOG2 => builtin_names::LOG2,
    LOG3 => builtin_names::LOG3,
    LOG4 => builtin_names::LOG4,

    // ========== EVM System Calls ==========
    CREATE       => builtin_names::CREATE,
    CREATE2      => builtin_names::CREATE2,
    CALL         => builtin_names::CALL,
    CALLCODE     => builtin_names::CALLCODE,
    DELEGATECALL => builtin_names::DELEGATECALL,
    STATICCALL   => builtin_names::STATICCALL,
    RETURN       => builtin_names::RETURN,
    STOP         => builtin_names::STOP,
    REVERT       => builtin_names::REVERT,
    INVALID      => builtin_names::INVALID,
    SELFDESTRUCT => builtin_names::SELFDESTRUCT,

    // ========== IR Memory Primitives ==========
    DYNAMIC_ALLOC_ZEROED    => builtin_names::DYNAMIC_ALLOC_ZEROED,
    DYNAMIC_ALLOC_ANY_BYTES => builtin_names::DYNAMIC_ALLOC_ANY_BYTES,

    // ========== Memory Manipulation ==========
    MEMORY_COPY => builtin_names::MEMORY_COPY,
    MLOAD1  => builtin_names::MLOAD1,
    MLOAD2  => builtin_names::MLOAD2,
    MLOAD3  => builtin_names::MLOAD3,
    MLOAD4  => builtin_names::MLOAD4,
    MLOAD5  => builtin_names::MLOAD5,
    MLOAD6  => builtin_names::MLOAD6,
    MLOAD7  => builtin_names::MLOAD7,
    MLOAD8  => builtin_names::MLOAD8,
    MLOAD9  => builtin_names::MLOAD9,
    MLOAD10 => builtin_names::MLOAD10,
    MLOAD11 => builtin_names::MLOAD11,
    MLOAD12 => builtin_names::MLOAD12,
    MLOAD13 => builtin_names::MLOAD13,
    MLOAD14 => builtin_names::MLOAD14,
    MLOAD15 => builtin_names::MLOAD15,
    MLOAD16 => builtin_names::MLOAD16,
    MLOAD17 => builtin_names::MLOAD17,
    MLOAD18 => builtin_names::MLOAD18,
    MLOAD19 => builtin_names::MLOAD19,
    MLOAD20 => builtin_names::MLOAD20,
    MLOAD21 => builtin_names::MLOAD21,
    MLOAD22 => builtin_names::MLOAD22,
    MLOAD23 => builtin_names::MLOAD23,
    MLOAD24 => builtin_names::MLOAD24,
    MLOAD25 => builtin_names::MLOAD25,
    MLOAD26 => builtin_names::MLOAD26,
    MLOAD27 => builtin_names::MLOAD27,
    MLOAD28 => builtin_names::MLOAD28,
    MLOAD29 => builtin_names::MLOAD29,
    MLOAD30 => builtin_names::MLOAD30,
    MLOAD31 => builtin_names::MLOAD31,
    MLOAD32 => builtin_names::MLOAD32,
    MSTORE1  => builtin_names::MSTORE1,
    MSTORE2  => builtin_names::MSTORE2,
    MSTORE3  => builtin_names::MSTORE3,
    MSTORE4  => builtin_names::MSTORE4,
    MSTORE5  => builtin_names::MSTORE5,
    MSTORE6  => builtin_names::MSTORE6,
    MSTORE7  => builtin_names::MSTORE7,
    MSTORE8  => builtin_names::MSTORE8,
    MSTORE9  => builtin_names::MSTORE9,
    MSTORE10 => builtin_names::MSTORE10,
    MSTORE11 => builtin_names::MSTORE11,
    MSTORE12 => builtin_names::MSTORE12,
    MSTORE13 => builtin_names::MSTORE13,
    MSTORE14 => builtin_names::MSTORE14,
    MSTORE15 => builtin_names::MSTORE15,
    MSTORE16 => builtin_names::MSTORE16,
    MSTORE17 => builtin_names::MSTORE17,
    MSTORE18 => builtin_names::MSTORE18,
    MSTORE19 => builtin_names::MSTORE19,
    MSTORE20 => builtin_names::MSTORE20,
    MSTORE21 => builtin_names::MSTORE21,
    MSTORE22 => builtin_names::MSTORE22,
    MSTORE23 => builtin_names::MSTORE23,
    MSTORE24 => builtin_names::MSTORE24,
    MSTORE25 => builtin_names::MSTORE25,
    MSTORE26 => builtin_names::MSTORE26,
    MSTORE27 => builtin_names::MSTORE27,
    MSTORE28 => builtin_names::MSTORE28,
    MSTORE29 => builtin_names::MSTORE29,
    MSTORE30 => builtin_names::MSTORE30,
    MSTORE31 => builtin_names::MSTORE31,
    MSTORE32 => builtin_names::MSTORE32,

    // ========== Bytecode Introspection ==========
    RUNTIME_START_OFFSET => builtin_names::RUNTIME_START_OFFSET,
    INIT_END_OFFSET      => builtin_names::INIT_END_OFFSET,
    RUNTIME_LENGTH       => builtin_names::RUNTIME_LENGTH,
}

impl PlankInterner {
    pub fn new() -> Self {
        let mut inner = StringInterner::new();
        Self::inject_primitives(&mut inner);
        Self { inner }
    }

    pub fn with_capacities(names: usize, bytes: usize) -> Self {
        let mut inner = StringInterner::with_capacity(names, bytes);
        Self::inject_primitives(&mut inner);
        Self { inner }
    }

    pub fn intern(&mut self, string: &str) -> StrId {
        self.inner.intern(string)
    }
}

impl std::ops::Index<StrId> for PlankInterner {
    type Output = str;

    fn index(&self, index: StrId) -> &Self::Output {
        &self.inner[index]
    }
}

impl Default for PlankInterner {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_interner_initializes_with_all_primitives() {
        let _interner = PlankInterner::new();
    }
}
