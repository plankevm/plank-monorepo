use crate::builtin_names;
use sensei_core::{intern::StringInterner, newtype_index};

newtype_index! {
    /// String ID
    pub struct StrId;
}

pub struct PlankInterner {
    inner: StringInterner<StrId>,
}

impl PlankInterner {
    pub const VOID_TYPE_NAME: StrId = StrId::new(0);
    pub const U256_TYPE_NAME: StrId = StrId::new(1);
    pub const BOOL_TYPE_NAME: StrId = StrId::new(2);
    pub const MEMPTR_TYPE_NAME: StrId = StrId::new(3);
    pub const TYPE_TYPE_NAME: StrId = StrId::new(4);
    pub const FUNCTION_TYPE_NAME: StrId = StrId::new(5);

    // ========== EVM Arithmetic ==========
    pub const ADD: StrId = StrId::new(6);
    pub const MUL: StrId = StrId::new(7);
    pub const SUB: StrId = StrId::new(8);
    pub const DIV: StrId = StrId::new(9);
    pub const SDIV: StrId = StrId::new(10);
    pub const MOD: StrId = StrId::new(11);
    pub const SMOD: StrId = StrId::new(12);
    pub const ADDMOD: StrId = StrId::new(13);
    pub const MULMOD: StrId = StrId::new(14);
    pub const EXP: StrId = StrId::new(15);
    pub const SIGNEXTEND: StrId = StrId::new(16);

    // ========== EVM Comparison & Bitwise Logic ==========
    pub const LT: StrId = StrId::new(17);
    pub const GT: StrId = StrId::new(18);
    pub const SLT: StrId = StrId::new(19);
    pub const SGT: StrId = StrId::new(20);
    pub const EQ: StrId = StrId::new(21);
    pub const ISZERO: StrId = StrId::new(22);
    pub const AND: StrId = StrId::new(23);
    pub const OR: StrId = StrId::new(24);
    pub const XOR: StrId = StrId::new(25);
    pub const NOT: StrId = StrId::new(26);
    pub const BYTE: StrId = StrId::new(27);
    pub const SHL: StrId = StrId::new(28);
    pub const SHR: StrId = StrId::new(29);
    pub const SAR: StrId = StrId::new(30);

    // ========== EVM Keccak-256 ==========
    pub const KECCAK256: StrId = StrId::new(31);

    // ========== EVM Environment Information ==========
    pub const ADDRESS: StrId = StrId::new(32);
    pub const BALANCE: StrId = StrId::new(33);
    pub const ORIGIN: StrId = StrId::new(34);
    pub const CALLER: StrId = StrId::new(35);
    pub const CALLVALUE: StrId = StrId::new(36);
    pub const CALLDATALOAD: StrId = StrId::new(37);
    pub const CALLDATASIZE: StrId = StrId::new(38);
    pub const CALLDATACOPY: StrId = StrId::new(39);
    pub const CODESIZE: StrId = StrId::new(40);
    pub const CODECOPY: StrId = StrId::new(41);
    pub const GASPRICE: StrId = StrId::new(42);
    pub const EXTCODESIZE: StrId = StrId::new(43);
    pub const EXTCODECOPY: StrId = StrId::new(44);
    pub const RETURNDATASIZE: StrId = StrId::new(45);
    pub const RETURNDATACOPY: StrId = StrId::new(46);
    pub const EXTCODEHASH: StrId = StrId::new(47);
    pub const GAS: StrId = StrId::new(48);

    // ========== EVM Block Information ==========
    pub const BLOCKHASH: StrId = StrId::new(49);
    pub const COINBASE: StrId = StrId::new(50);
    pub const TIMESTAMP: StrId = StrId::new(51);
    pub const NUMBER: StrId = StrId::new(52);
    pub const DIFFICULTY: StrId = StrId::new(53);
    pub const GASLIMIT: StrId = StrId::new(54);
    pub const CHAINID: StrId = StrId::new(55);
    pub const SELFBALANCE: StrId = StrId::new(56);
    pub const BASEFEE: StrId = StrId::new(57);
    pub const BLOBHASH: StrId = StrId::new(58);
    pub const BLOBBASEFEE: StrId = StrId::new(59);

    // ========== EVM State Manipulation ==========
    pub const SLOAD: StrId = StrId::new(60);
    pub const SSTORE: StrId = StrId::new(61);
    pub const TLOAD: StrId = StrId::new(62);
    pub const TSTORE: StrId = StrId::new(63);

    // ========== EVM Logging Operations ==========
    pub const LOG0: StrId = StrId::new(64);
    pub const LOG1: StrId = StrId::new(65);
    pub const LOG2: StrId = StrId::new(66);
    pub const LOG3: StrId = StrId::new(67);
    pub const LOG4: StrId = StrId::new(68);

    // ========== EVM System Calls ==========
    pub const CREATE: StrId = StrId::new(69);
    pub const CREATE2: StrId = StrId::new(70);
    pub const CALL: StrId = StrId::new(71);
    pub const CALLCODE: StrId = StrId::new(72);
    pub const DELEGATECALL: StrId = StrId::new(73);
    pub const STATICCALL: StrId = StrId::new(74);
    pub const RETURN: StrId = StrId::new(75);
    pub const STOP: StrId = StrId::new(76);
    pub const REVERT: StrId = StrId::new(77);
    pub const INVALID: StrId = StrId::new(78);
    pub const SELFDESTRUCT: StrId = StrId::new(79);

    // ========== IR Memory Primitives ==========
    pub const DYNAMIC_ALLOC_ZEROED: StrId = StrId::new(80);
    pub const DYNAMIC_ALLOC_ANY_BYTES: StrId = StrId::new(81);

    // ========== Memory Manipulation ==========
    pub const MEMORY_COPY: StrId = StrId::new(82);
    pub const MLOAD1: StrId = StrId::new(83);
    pub const MLOAD2: StrId = StrId::new(84);
    pub const MLOAD3: StrId = StrId::new(85);
    pub const MLOAD4: StrId = StrId::new(86);
    pub const MLOAD5: StrId = StrId::new(87);
    pub const MLOAD6: StrId = StrId::new(88);
    pub const MLOAD7: StrId = StrId::new(89);
    pub const MLOAD8: StrId = StrId::new(90);
    pub const MLOAD9: StrId = StrId::new(91);
    pub const MLOAD10: StrId = StrId::new(92);
    pub const MLOAD11: StrId = StrId::new(93);
    pub const MLOAD12: StrId = StrId::new(94);
    pub const MLOAD13: StrId = StrId::new(95);
    pub const MLOAD14: StrId = StrId::new(96);
    pub const MLOAD15: StrId = StrId::new(97);
    pub const MLOAD16: StrId = StrId::new(98);
    pub const MLOAD17: StrId = StrId::new(99);
    pub const MLOAD18: StrId = StrId::new(100);
    pub const MLOAD19: StrId = StrId::new(101);
    pub const MLOAD20: StrId = StrId::new(102);
    pub const MLOAD21: StrId = StrId::new(103);
    pub const MLOAD22: StrId = StrId::new(104);
    pub const MLOAD23: StrId = StrId::new(105);
    pub const MLOAD24: StrId = StrId::new(106);
    pub const MLOAD25: StrId = StrId::new(107);
    pub const MLOAD26: StrId = StrId::new(108);
    pub const MLOAD27: StrId = StrId::new(109);
    pub const MLOAD28: StrId = StrId::new(110);
    pub const MLOAD29: StrId = StrId::new(111);
    pub const MLOAD30: StrId = StrId::new(112);
    pub const MLOAD31: StrId = StrId::new(113);
    pub const MLOAD32: StrId = StrId::new(114);
    pub const MSTORE1: StrId = StrId::new(115);
    pub const MSTORE2: StrId = StrId::new(116);
    pub const MSTORE3: StrId = StrId::new(117);
    pub const MSTORE4: StrId = StrId::new(118);
    pub const MSTORE5: StrId = StrId::new(119);
    pub const MSTORE6: StrId = StrId::new(120);
    pub const MSTORE7: StrId = StrId::new(121);
    pub const MSTORE8: StrId = StrId::new(122);
    pub const MSTORE9: StrId = StrId::new(123);
    pub const MSTORE10: StrId = StrId::new(124);
    pub const MSTORE11: StrId = StrId::new(125);
    pub const MSTORE12: StrId = StrId::new(126);
    pub const MSTORE13: StrId = StrId::new(127);
    pub const MSTORE14: StrId = StrId::new(128);
    pub const MSTORE15: StrId = StrId::new(129);
    pub const MSTORE16: StrId = StrId::new(130);
    pub const MSTORE17: StrId = StrId::new(131);
    pub const MSTORE18: StrId = StrId::new(132);
    pub const MSTORE19: StrId = StrId::new(133);
    pub const MSTORE20: StrId = StrId::new(134);
    pub const MSTORE21: StrId = StrId::new(135);
    pub const MSTORE22: StrId = StrId::new(136);
    pub const MSTORE23: StrId = StrId::new(137);
    pub const MSTORE24: StrId = StrId::new(138);
    pub const MSTORE25: StrId = StrId::new(139);
    pub const MSTORE26: StrId = StrId::new(140);
    pub const MSTORE27: StrId = StrId::new(141);
    pub const MSTORE28: StrId = StrId::new(142);
    pub const MSTORE29: StrId = StrId::new(143);
    pub const MSTORE30: StrId = StrId::new(144);
    pub const MSTORE31: StrId = StrId::new(145);
    pub const MSTORE32: StrId = StrId::new(146);

    // ========== Bytecode Introspection ==========
    pub const RUNTIME_START_OFFSET: StrId = StrId::new(147);
    pub const INIT_END_OFFSET: StrId = StrId::new(148);
    pub const RUNTIME_LENGTH: StrId = StrId::new(149);

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

    fn inject_primitives(interner: &mut StringInterner<StrId>) {
        assert_eq!(interner.intern(builtin_names::VOID_TYPE_NAME), Self::VOID_TYPE_NAME);
        assert_eq!(interner.intern(builtin_names::U256_TYPE_NAME), Self::U256_TYPE_NAME);
        assert_eq!(interner.intern(builtin_names::BOOL_TYPE_NAME), Self::BOOL_TYPE_NAME);
        assert_eq!(interner.intern(builtin_names::MEMPTR_TYPE_NAME), Self::MEMPTR_TYPE_NAME);
        assert_eq!(interner.intern(builtin_names::TYPE_TYPE_NAME), Self::TYPE_TYPE_NAME);
        assert_eq!(interner.intern(builtin_names::FUNCTION_TYPE_NAME), Self::FUNCTION_TYPE_NAME);

        // ========== EVM Arithmetic ==========
        assert_eq!(interner.intern(builtin_names::ADD), Self::ADD);
        assert_eq!(interner.intern(builtin_names::MUL), Self::MUL);
        assert_eq!(interner.intern(builtin_names::SUB), Self::SUB);
        assert_eq!(interner.intern(builtin_names::DIV), Self::DIV);
        assert_eq!(interner.intern(builtin_names::SDIV), Self::SDIV);
        assert_eq!(interner.intern(builtin_names::MOD), Self::MOD);
        assert_eq!(interner.intern(builtin_names::SMOD), Self::SMOD);
        assert_eq!(interner.intern(builtin_names::ADDMOD), Self::ADDMOD);
        assert_eq!(interner.intern(builtin_names::MULMOD), Self::MULMOD);
        assert_eq!(interner.intern(builtin_names::EXP), Self::EXP);
        assert_eq!(interner.intern(builtin_names::SIGNEXTEND), Self::SIGNEXTEND);

        // ========== EVM Comparison & Bitwise Logic ==========
        assert_eq!(interner.intern(builtin_names::LT), Self::LT);
        assert_eq!(interner.intern(builtin_names::GT), Self::GT);
        assert_eq!(interner.intern(builtin_names::SLT), Self::SLT);
        assert_eq!(interner.intern(builtin_names::SGT), Self::SGT);
        assert_eq!(interner.intern(builtin_names::EQ), Self::EQ);
        assert_eq!(interner.intern(builtin_names::ISZERO), Self::ISZERO);
        assert_eq!(interner.intern(builtin_names::AND), Self::AND);
        assert_eq!(interner.intern(builtin_names::OR), Self::OR);
        assert_eq!(interner.intern(builtin_names::XOR), Self::XOR);
        assert_eq!(interner.intern(builtin_names::NOT), Self::NOT);
        assert_eq!(interner.intern(builtin_names::BYTE), Self::BYTE);
        assert_eq!(interner.intern(builtin_names::SHL), Self::SHL);
        assert_eq!(interner.intern(builtin_names::SHR), Self::SHR);
        assert_eq!(interner.intern(builtin_names::SAR), Self::SAR);

        // ========== EVM Keccak-256 ==========
        assert_eq!(interner.intern(builtin_names::KECCAK256), Self::KECCAK256);

        // ========== EVM Environment Information ==========
        assert_eq!(interner.intern(builtin_names::ADDRESS), Self::ADDRESS);
        assert_eq!(interner.intern(builtin_names::BALANCE), Self::BALANCE);
        assert_eq!(interner.intern(builtin_names::ORIGIN), Self::ORIGIN);
        assert_eq!(interner.intern(builtin_names::CALLER), Self::CALLER);
        assert_eq!(interner.intern(builtin_names::CALLVALUE), Self::CALLVALUE);
        assert_eq!(interner.intern(builtin_names::CALLDATALOAD), Self::CALLDATALOAD);
        assert_eq!(interner.intern(builtin_names::CALLDATASIZE), Self::CALLDATASIZE);
        assert_eq!(interner.intern(builtin_names::CALLDATACOPY), Self::CALLDATACOPY);
        assert_eq!(interner.intern(builtin_names::CODESIZE), Self::CODESIZE);
        assert_eq!(interner.intern(builtin_names::CODECOPY), Self::CODECOPY);
        assert_eq!(interner.intern(builtin_names::GASPRICE), Self::GASPRICE);
        assert_eq!(interner.intern(builtin_names::EXTCODESIZE), Self::EXTCODESIZE);
        assert_eq!(interner.intern(builtin_names::EXTCODECOPY), Self::EXTCODECOPY);
        assert_eq!(interner.intern(builtin_names::RETURNDATASIZE), Self::RETURNDATASIZE);
        assert_eq!(interner.intern(builtin_names::RETURNDATACOPY), Self::RETURNDATACOPY);
        assert_eq!(interner.intern(builtin_names::EXTCODEHASH), Self::EXTCODEHASH);
        assert_eq!(interner.intern(builtin_names::GAS), Self::GAS);

        // ========== EVM Block Information ==========
        assert_eq!(interner.intern(builtin_names::BLOCKHASH), Self::BLOCKHASH);
        assert_eq!(interner.intern(builtin_names::COINBASE), Self::COINBASE);
        assert_eq!(interner.intern(builtin_names::TIMESTAMP), Self::TIMESTAMP);
        assert_eq!(interner.intern(builtin_names::NUMBER), Self::NUMBER);
        assert_eq!(interner.intern(builtin_names::DIFFICULTY), Self::DIFFICULTY);
        assert_eq!(interner.intern(builtin_names::GASLIMIT), Self::GASLIMIT);
        assert_eq!(interner.intern(builtin_names::CHAINID), Self::CHAINID);
        assert_eq!(interner.intern(builtin_names::SELFBALANCE), Self::SELFBALANCE);
        assert_eq!(interner.intern(builtin_names::BASEFEE), Self::BASEFEE);
        assert_eq!(interner.intern(builtin_names::BLOBHASH), Self::BLOBHASH);
        assert_eq!(interner.intern(builtin_names::BLOBBASEFEE), Self::BLOBBASEFEE);

        // ========== EVM State Manipulation ==========
        assert_eq!(interner.intern(builtin_names::SLOAD), Self::SLOAD);
        assert_eq!(interner.intern(builtin_names::SSTORE), Self::SSTORE);
        assert_eq!(interner.intern(builtin_names::TLOAD), Self::TLOAD);
        assert_eq!(interner.intern(builtin_names::TSTORE), Self::TSTORE);

        // ========== EVM Logging Operations ==========
        assert_eq!(interner.intern(builtin_names::LOG0), Self::LOG0);
        assert_eq!(interner.intern(builtin_names::LOG1), Self::LOG1);
        assert_eq!(interner.intern(builtin_names::LOG2), Self::LOG2);
        assert_eq!(interner.intern(builtin_names::LOG3), Self::LOG3);
        assert_eq!(interner.intern(builtin_names::LOG4), Self::LOG4);

        // ========== EVM System Calls ==========
        assert_eq!(interner.intern(builtin_names::CREATE), Self::CREATE);
        assert_eq!(interner.intern(builtin_names::CREATE2), Self::CREATE2);
        assert_eq!(interner.intern(builtin_names::CALL), Self::CALL);
        assert_eq!(interner.intern(builtin_names::CALLCODE), Self::CALLCODE);
        assert_eq!(interner.intern(builtin_names::DELEGATECALL), Self::DELEGATECALL);
        assert_eq!(interner.intern(builtin_names::STATICCALL), Self::STATICCALL);
        assert_eq!(interner.intern(builtin_names::RETURN), Self::RETURN);
        assert_eq!(interner.intern(builtin_names::STOP), Self::STOP);
        assert_eq!(interner.intern(builtin_names::REVERT), Self::REVERT);
        assert_eq!(interner.intern(builtin_names::INVALID), Self::INVALID);
        assert_eq!(interner.intern(builtin_names::SELFDESTRUCT), Self::SELFDESTRUCT);

        // ========== IR Memory Primitives ==========
        assert_eq!(
            interner.intern(builtin_names::DYNAMIC_ALLOC_ZEROED),
            Self::DYNAMIC_ALLOC_ZEROED
        );
        assert_eq!(
            interner.intern(builtin_names::DYNAMIC_ALLOC_ANY_BYTES),
            Self::DYNAMIC_ALLOC_ANY_BYTES
        );

        // ========== Memory Manipulation ==========
        assert_eq!(interner.intern(builtin_names::MEMORY_COPY), Self::MEMORY_COPY);
        assert_eq!(interner.intern(builtin_names::MLOAD1), Self::MLOAD1);
        assert_eq!(interner.intern(builtin_names::MLOAD2), Self::MLOAD2);
        assert_eq!(interner.intern(builtin_names::MLOAD3), Self::MLOAD3);
        assert_eq!(interner.intern(builtin_names::MLOAD4), Self::MLOAD4);
        assert_eq!(interner.intern(builtin_names::MLOAD5), Self::MLOAD5);
        assert_eq!(interner.intern(builtin_names::MLOAD6), Self::MLOAD6);
        assert_eq!(interner.intern(builtin_names::MLOAD7), Self::MLOAD7);
        assert_eq!(interner.intern(builtin_names::MLOAD8), Self::MLOAD8);
        assert_eq!(interner.intern(builtin_names::MLOAD9), Self::MLOAD9);
        assert_eq!(interner.intern(builtin_names::MLOAD10), Self::MLOAD10);
        assert_eq!(interner.intern(builtin_names::MLOAD11), Self::MLOAD11);
        assert_eq!(interner.intern(builtin_names::MLOAD12), Self::MLOAD12);
        assert_eq!(interner.intern(builtin_names::MLOAD13), Self::MLOAD13);
        assert_eq!(interner.intern(builtin_names::MLOAD14), Self::MLOAD14);
        assert_eq!(interner.intern(builtin_names::MLOAD15), Self::MLOAD15);
        assert_eq!(interner.intern(builtin_names::MLOAD16), Self::MLOAD16);
        assert_eq!(interner.intern(builtin_names::MLOAD17), Self::MLOAD17);
        assert_eq!(interner.intern(builtin_names::MLOAD18), Self::MLOAD18);
        assert_eq!(interner.intern(builtin_names::MLOAD19), Self::MLOAD19);
        assert_eq!(interner.intern(builtin_names::MLOAD20), Self::MLOAD20);
        assert_eq!(interner.intern(builtin_names::MLOAD21), Self::MLOAD21);
        assert_eq!(interner.intern(builtin_names::MLOAD22), Self::MLOAD22);
        assert_eq!(interner.intern(builtin_names::MLOAD23), Self::MLOAD23);
        assert_eq!(interner.intern(builtin_names::MLOAD24), Self::MLOAD24);
        assert_eq!(interner.intern(builtin_names::MLOAD25), Self::MLOAD25);
        assert_eq!(interner.intern(builtin_names::MLOAD26), Self::MLOAD26);
        assert_eq!(interner.intern(builtin_names::MLOAD27), Self::MLOAD27);
        assert_eq!(interner.intern(builtin_names::MLOAD28), Self::MLOAD28);
        assert_eq!(interner.intern(builtin_names::MLOAD29), Self::MLOAD29);
        assert_eq!(interner.intern(builtin_names::MLOAD30), Self::MLOAD30);
        assert_eq!(interner.intern(builtin_names::MLOAD31), Self::MLOAD31);
        assert_eq!(interner.intern(builtin_names::MLOAD32), Self::MLOAD32);
        assert_eq!(interner.intern(builtin_names::MSTORE1), Self::MSTORE1);
        assert_eq!(interner.intern(builtin_names::MSTORE2), Self::MSTORE2);
        assert_eq!(interner.intern(builtin_names::MSTORE3), Self::MSTORE3);
        assert_eq!(interner.intern(builtin_names::MSTORE4), Self::MSTORE4);
        assert_eq!(interner.intern(builtin_names::MSTORE5), Self::MSTORE5);
        assert_eq!(interner.intern(builtin_names::MSTORE6), Self::MSTORE6);
        assert_eq!(interner.intern(builtin_names::MSTORE7), Self::MSTORE7);
        assert_eq!(interner.intern(builtin_names::MSTORE8), Self::MSTORE8);
        assert_eq!(interner.intern(builtin_names::MSTORE9), Self::MSTORE9);
        assert_eq!(interner.intern(builtin_names::MSTORE10), Self::MSTORE10);
        assert_eq!(interner.intern(builtin_names::MSTORE11), Self::MSTORE11);
        assert_eq!(interner.intern(builtin_names::MSTORE12), Self::MSTORE12);
        assert_eq!(interner.intern(builtin_names::MSTORE13), Self::MSTORE13);
        assert_eq!(interner.intern(builtin_names::MSTORE14), Self::MSTORE14);
        assert_eq!(interner.intern(builtin_names::MSTORE15), Self::MSTORE15);
        assert_eq!(interner.intern(builtin_names::MSTORE16), Self::MSTORE16);
        assert_eq!(interner.intern(builtin_names::MSTORE17), Self::MSTORE17);
        assert_eq!(interner.intern(builtin_names::MSTORE18), Self::MSTORE18);
        assert_eq!(interner.intern(builtin_names::MSTORE19), Self::MSTORE19);
        assert_eq!(interner.intern(builtin_names::MSTORE20), Self::MSTORE20);
        assert_eq!(interner.intern(builtin_names::MSTORE21), Self::MSTORE21);
        assert_eq!(interner.intern(builtin_names::MSTORE22), Self::MSTORE22);
        assert_eq!(interner.intern(builtin_names::MSTORE23), Self::MSTORE23);
        assert_eq!(interner.intern(builtin_names::MSTORE24), Self::MSTORE24);
        assert_eq!(interner.intern(builtin_names::MSTORE25), Self::MSTORE25);
        assert_eq!(interner.intern(builtin_names::MSTORE26), Self::MSTORE26);
        assert_eq!(interner.intern(builtin_names::MSTORE27), Self::MSTORE27);
        assert_eq!(interner.intern(builtin_names::MSTORE28), Self::MSTORE28);
        assert_eq!(interner.intern(builtin_names::MSTORE29), Self::MSTORE29);
        assert_eq!(interner.intern(builtin_names::MSTORE30), Self::MSTORE30);
        assert_eq!(interner.intern(builtin_names::MSTORE31), Self::MSTORE31);
        assert_eq!(interner.intern(builtin_names::MSTORE32), Self::MSTORE32);

        // ========== Bytecode Introspection ==========
        assert_eq!(
            interner.intern(builtin_names::RUNTIME_START_OFFSET),
            Self::RUNTIME_START_OFFSET
        );
        assert_eq!(interner.intern(builtin_names::INIT_END_OFFSET), Self::INIT_END_OFFSET);
        assert_eq!(interner.intern(builtin_names::RUNTIME_LENGTH), Self::RUNTIME_LENGTH);
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
