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
        assert_eq!(interner.intern("void"), Self::VOID_TYPE_NAME);
        assert_eq!(interner.intern("u256"), Self::U256_TYPE_NAME);
        assert_eq!(interner.intern("bool"), Self::BOOL_TYPE_NAME);
        assert_eq!(interner.intern("memptr"), Self::MEMPTR_TYPE_NAME);
        assert_eq!(interner.intern("type"), Self::TYPE_TYPE_NAME);
        assert_eq!(interner.intern("function"), Self::FUNCTION_TYPE_NAME);

        // ========== EVM Arithmetic ==========
        assert_eq!(interner.intern("add"), Self::ADD);
        assert_eq!(interner.intern("mul"), Self::MUL);
        assert_eq!(interner.intern("sub"), Self::SUB);
        assert_eq!(interner.intern("raw_div"), Self::DIV);
        assert_eq!(interner.intern("raw_sdiv"), Self::SDIV);
        assert_eq!(interner.intern("raw_mod"), Self::MOD);
        assert_eq!(interner.intern("raw_smod"), Self::SMOD);
        assert_eq!(interner.intern("raw_addmod"), Self::ADDMOD);
        assert_eq!(interner.intern("raw_mulmod"), Self::MULMOD);
        assert_eq!(interner.intern("exp"), Self::EXP);
        assert_eq!(interner.intern("signextend"), Self::SIGNEXTEND);

        // ========== EVM Comparison & Bitwise Logic ==========
        assert_eq!(interner.intern("lt"), Self::LT);
        assert_eq!(interner.intern("gt"), Self::GT);
        assert_eq!(interner.intern("slt"), Self::SLT);
        assert_eq!(interner.intern("sgt"), Self::SGT);
        assert_eq!(interner.intern("eq"), Self::EQ);
        assert_eq!(interner.intern("iszero"), Self::ISZERO);
        assert_eq!(interner.intern("bitwise_and"), Self::AND);
        assert_eq!(interner.intern("bitwise_or"), Self::OR);
        assert_eq!(interner.intern("bitwise_xor"), Self::XOR);
        assert_eq!(interner.intern("bitwise_not"), Self::NOT);
        assert_eq!(interner.intern("byte"), Self::BYTE);
        assert_eq!(interner.intern("shl"), Self::SHL);
        assert_eq!(interner.intern("shr"), Self::SHR);
        assert_eq!(interner.intern("sar"), Self::SAR);

        // ========== EVM Keccak-256 ==========
        assert_eq!(interner.intern("keccak256"), Self::KECCAK256);

        // ========== EVM Environment Information ==========
        assert_eq!(interner.intern("address_this"), Self::ADDRESS);
        assert_eq!(interner.intern("balance"), Self::BALANCE);
        assert_eq!(interner.intern("origin"), Self::ORIGIN);
        assert_eq!(interner.intern("caller"), Self::CALLER);
        assert_eq!(interner.intern("callvalue"), Self::CALLVALUE);
        assert_eq!(interner.intern("calldataload"), Self::CALLDATALOAD);
        assert_eq!(interner.intern("calldatasize"), Self::CALLDATASIZE);
        assert_eq!(interner.intern("calldatacopy"), Self::CALLDATACOPY);
        assert_eq!(interner.intern("codesize"), Self::CODESIZE);
        assert_eq!(interner.intern("codecopy"), Self::CODECOPY);
        assert_eq!(interner.intern("gasprice"), Self::GASPRICE);
        assert_eq!(interner.intern("extcodesize"), Self::EXTCODESIZE);
        assert_eq!(interner.intern("extcodecopy"), Self::EXTCODECOPY);
        assert_eq!(interner.intern("returndatasize"), Self::RETURNDATASIZE);
        assert_eq!(interner.intern("returndatacopy"), Self::RETURNDATACOPY);
        assert_eq!(interner.intern("extcodehash"), Self::EXTCODEHASH);
        assert_eq!(interner.intern("gas"), Self::GAS);

        // ========== EVM Block Information ==========
        assert_eq!(interner.intern("blockhash"), Self::BLOCKHASH);
        assert_eq!(interner.intern("coinbase"), Self::COINBASE);
        assert_eq!(interner.intern("timestamp"), Self::TIMESTAMP);
        assert_eq!(interner.intern("number"), Self::NUMBER);
        assert_eq!(interner.intern("difficulty"), Self::DIFFICULTY);
        assert_eq!(interner.intern("gaslimit"), Self::GASLIMIT);
        assert_eq!(interner.intern("chainid"), Self::CHAINID);
        assert_eq!(interner.intern("selfbalance"), Self::SELFBALANCE);
        assert_eq!(interner.intern("basefee"), Self::BASEFEE);
        assert_eq!(interner.intern("blobhash"), Self::BLOBHASH);
        assert_eq!(interner.intern("blobbasefee"), Self::BLOBBASEFEE);

        // ========== EVM State Manipulation ==========
        assert_eq!(interner.intern("sload"), Self::SLOAD);
        assert_eq!(interner.intern("sstore"), Self::SSTORE);
        assert_eq!(interner.intern("tload"), Self::TLOAD);
        assert_eq!(interner.intern("tstore"), Self::TSTORE);

        // ========== EVM Logging Operations ==========
        assert_eq!(interner.intern("log0"), Self::LOG0);
        assert_eq!(interner.intern("log1"), Self::LOG1);
        assert_eq!(interner.intern("log2"), Self::LOG2);
        assert_eq!(interner.intern("log3"), Self::LOG3);
        assert_eq!(interner.intern("log4"), Self::LOG4);

        // ========== EVM System Calls ==========
        assert_eq!(interner.intern("create"), Self::CREATE);
        assert_eq!(interner.intern("create2"), Self::CREATE2);
        assert_eq!(interner.intern("call"), Self::CALL);
        assert_eq!(interner.intern("callcode"), Self::CALLCODE);
        assert_eq!(interner.intern("delegatecall"), Self::DELEGATECALL);
        assert_eq!(interner.intern("staticcall"), Self::STATICCALL);
        assert_eq!(interner.intern("return"), Self::RETURN);
        assert_eq!(interner.intern("stop"), Self::STOP);
        assert_eq!(interner.intern("revert"), Self::REVERT);
        assert_eq!(interner.intern("invalid"), Self::INVALID);
        assert_eq!(interner.intern("selfdestruct"), Self::SELFDESTRUCT);

        // ========== IR Memory Primitives ==========
        assert_eq!(interner.intern("malloc_zeroed"), Self::DYNAMIC_ALLOC_ZEROED);
        assert_eq!(interner.intern("malloc_uninit"), Self::DYNAMIC_ALLOC_ANY_BYTES);

        // ========== Memory Manipulation ==========
        assert_eq!(interner.intern("mcopy"), Self::MEMORY_COPY);
        assert_eq!(interner.intern("mload1"), Self::MLOAD1);
        assert_eq!(interner.intern("mload2"), Self::MLOAD2);
        assert_eq!(interner.intern("mload3"), Self::MLOAD3);
        assert_eq!(interner.intern("mload4"), Self::MLOAD4);
        assert_eq!(interner.intern("mload5"), Self::MLOAD5);
        assert_eq!(interner.intern("mload6"), Self::MLOAD6);
        assert_eq!(interner.intern("mload7"), Self::MLOAD7);
        assert_eq!(interner.intern("mload8"), Self::MLOAD8);
        assert_eq!(interner.intern("mload9"), Self::MLOAD9);
        assert_eq!(interner.intern("mload10"), Self::MLOAD10);
        assert_eq!(interner.intern("mload11"), Self::MLOAD11);
        assert_eq!(interner.intern("mload12"), Self::MLOAD12);
        assert_eq!(interner.intern("mload13"), Self::MLOAD13);
        assert_eq!(interner.intern("mload14"), Self::MLOAD14);
        assert_eq!(interner.intern("mload15"), Self::MLOAD15);
        assert_eq!(interner.intern("mload16"), Self::MLOAD16);
        assert_eq!(interner.intern("mload17"), Self::MLOAD17);
        assert_eq!(interner.intern("mload18"), Self::MLOAD18);
        assert_eq!(interner.intern("mload19"), Self::MLOAD19);
        assert_eq!(interner.intern("mload20"), Self::MLOAD20);
        assert_eq!(interner.intern("mload21"), Self::MLOAD21);
        assert_eq!(interner.intern("mload22"), Self::MLOAD22);
        assert_eq!(interner.intern("mload23"), Self::MLOAD23);
        assert_eq!(interner.intern("mload24"), Self::MLOAD24);
        assert_eq!(interner.intern("mload25"), Self::MLOAD25);
        assert_eq!(interner.intern("mload26"), Self::MLOAD26);
        assert_eq!(interner.intern("mload27"), Self::MLOAD27);
        assert_eq!(interner.intern("mload28"), Self::MLOAD28);
        assert_eq!(interner.intern("mload29"), Self::MLOAD29);
        assert_eq!(interner.intern("mload30"), Self::MLOAD30);
        assert_eq!(interner.intern("mload31"), Self::MLOAD31);
        assert_eq!(interner.intern("mload32"), Self::MLOAD32);
        assert_eq!(interner.intern("mstore1"), Self::MSTORE1);
        assert_eq!(interner.intern("mstore2"), Self::MSTORE2);
        assert_eq!(interner.intern("mstore3"), Self::MSTORE3);
        assert_eq!(interner.intern("mstore4"), Self::MSTORE4);
        assert_eq!(interner.intern("mstore5"), Self::MSTORE5);
        assert_eq!(interner.intern("mstore6"), Self::MSTORE6);
        assert_eq!(interner.intern("mstore7"), Self::MSTORE7);
        assert_eq!(interner.intern("mstore8"), Self::MSTORE8);
        assert_eq!(interner.intern("mstore9"), Self::MSTORE9);
        assert_eq!(interner.intern("mstore10"), Self::MSTORE10);
        assert_eq!(interner.intern("mstore11"), Self::MSTORE11);
        assert_eq!(interner.intern("mstore12"), Self::MSTORE12);
        assert_eq!(interner.intern("mstore13"), Self::MSTORE13);
        assert_eq!(interner.intern("mstore14"), Self::MSTORE14);
        assert_eq!(interner.intern("mstore15"), Self::MSTORE15);
        assert_eq!(interner.intern("mstore16"), Self::MSTORE16);
        assert_eq!(interner.intern("mstore17"), Self::MSTORE17);
        assert_eq!(interner.intern("mstore18"), Self::MSTORE18);
        assert_eq!(interner.intern("mstore19"), Self::MSTORE19);
        assert_eq!(interner.intern("mstore20"), Self::MSTORE20);
        assert_eq!(interner.intern("mstore21"), Self::MSTORE21);
        assert_eq!(interner.intern("mstore22"), Self::MSTORE22);
        assert_eq!(interner.intern("mstore23"), Self::MSTORE23);
        assert_eq!(interner.intern("mstore24"), Self::MSTORE24);
        assert_eq!(interner.intern("mstore25"), Self::MSTORE25);
        assert_eq!(interner.intern("mstore26"), Self::MSTORE26);
        assert_eq!(interner.intern("mstore27"), Self::MSTORE27);
        assert_eq!(interner.intern("mstore28"), Self::MSTORE28);
        assert_eq!(interner.intern("mstore29"), Self::MSTORE29);
        assert_eq!(interner.intern("mstore30"), Self::MSTORE30);
        assert_eq!(interner.intern("mstore31"), Self::MSTORE31);
        assert_eq!(interner.intern("mstore32"), Self::MSTORE32);

        // ========== Bytecode Introspection ==========
        assert_eq!(interner.intern("runtime_start_offset"), Self::RUNTIME_START_OFFSET);
        assert_eq!(interner.intern("init_end_offset"), Self::INIT_END_OFFSET);
        assert_eq!(interner.intern("runtime_length"), Self::RUNTIME_LENGTH);
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
