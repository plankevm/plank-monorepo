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
    ($($name:ident),* $(,)?) => {
        #[doc(hidden)]
        #[repr(u32)]
        #[allow(non_camel_case_types, clippy::upper_case_acronyms)]
        enum BuiltinStrIdx { $($name),* }

        impl PlankInterner {
            $(pub const $name: StrId = StrId::new(BuiltinStrIdx::$name as u32);)*

            fn inject_primitives(interner: &mut StringInterner<StrId>) {
                $(assert_eq!(interner.intern(builtin_names::$name), Self::$name);)*
            }
        }
    };
}

builtin_str_ids! {
    // ========== Type Names ==========
    VOID_TYPE_NAME,
    U256_TYPE_NAME,
    BOOL_TYPE_NAME,
    MEMPTR_TYPE_NAME,
    TYPE_TYPE_NAME,
    FUNCTION_TYPE_NAME,
    NEVER_TYPE_NAME,

    // ========== EVM Arithmetic ==========
    ADD,
    MUL,
    SUB,
    DIV,
    SDIV,
    MOD,
    SMOD,
    ADDMOD,
    MULMOD,
    EXP,
    SIGNEXTEND,

    // ========== EVM Comparison & Bitwise Logic ==========
    LT,
    GT,
    SLT,
    SGT,
    EQ,
    ISZERO,
    AND,
    OR,
    XOR,
    NOT,
    BYTE,
    SHL,
    SHR,
    SAR,

    // ========== EVM Keccak-256 ==========
    KECCAK256,

    // ========== EVM Environment Information ==========
    ADDRESS,
    BALANCE,
    ORIGIN,
    CALLER,
    CALLVALUE,
    CALLDATALOAD,
    CALLDATASIZE,
    CALLDATACOPY,
    CODESIZE,
    CODECOPY,
    GASPRICE,
    EXTCODESIZE,
    EXTCODECOPY,
    RETURNDATASIZE,
    RETURNDATACOPY,
    EXTCODEHASH,
    GAS,

    // ========== EVM Block Information ==========
    BLOCKHASH,
    COINBASE,
    TIMESTAMP,
    NUMBER,
    DIFFICULTY,
    GASLIMIT,
    CHAINID,
    SELFBALANCE,
    BASEFEE,
    BLOBHASH,
    BLOBBASEFEE,

    // ========== EVM State Manipulation ==========
    SLOAD,
    SSTORE,
    TLOAD,
    TSTORE,

    // ========== EVM Logging Operations ==========
    LOG0,
    LOG1,
    LOG2,
    LOG3,
    LOG4,

    // ========== EVM System Calls ==========
    CREATE,
    CREATE2,
    CALL,
    CALLCODE,
    DELEGATECALL,
    STATICCALL,
    RETURN,
    STOP,
    REVERT,
    INVALID,
    SELFDESTRUCT,

    // ========== IR Memory Primitives ==========
    DYNAMIC_ALLOC_ZEROED,
    DYNAMIC_ALLOC_ANY_BYTES,

    // ========== Memory Manipulation ==========
    MEMORY_COPY,
    MLOAD1,
    MLOAD2,
    MLOAD3,
    MLOAD4,
    MLOAD5,
    MLOAD6,
    MLOAD7,
    MLOAD8,
    MLOAD9,
    MLOAD10,
    MLOAD11,
    MLOAD12,
    MLOAD13,
    MLOAD14,
    MLOAD15,
    MLOAD16,
    MLOAD17,
    MLOAD18,
    MLOAD19,
    MLOAD20,
    MLOAD21,
    MLOAD22,
    MLOAD23,
    MLOAD24,
    MLOAD25,
    MLOAD26,
    MLOAD27,
    MLOAD28,
    MLOAD29,
    MLOAD30,
    MLOAD31,
    MLOAD32,
    MSTORE1,
    MSTORE2,
    MSTORE3,
    MSTORE4,
    MSTORE5,
    MSTORE6,
    MSTORE7,
    MSTORE8,
    MSTORE9,
    MSTORE10,
    MSTORE11,
    MSTORE12,
    MSTORE13,
    MSTORE14,
    MSTORE15,
    MSTORE16,
    MSTORE17,
    MSTORE18,
    MSTORE19,
    MSTORE20,
    MSTORE21,
    MSTORE22,
    MSTORE23,
    MSTORE24,
    MSTORE25,
    MSTORE26,
    MSTORE27,
    MSTORE28,
    MSTORE29,
    MSTORE30,
    MSTORE31,
    MSTORE32,

    // ========== Bytecode Introspection ==========
    RUNTIME_START_OFFSET,
    INIT_END_OFFSET,
    RUNTIME_LENGTH,
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
