// Type names
pub const VOID_TYPE_NAME: &str = "void";
pub const U256_TYPE_NAME: &str = "u256";
pub const BOOL_TYPE_NAME: &str = "bool";
pub const MEMPTR_TYPE_NAME: &str = "memptr";
pub const TYPE_TYPE_NAME: &str = "type";
pub const FUNCTION_TYPE_NAME: &str = "function";

// ========== EVM Arithmetic ==========
pub const ADD: &str = "add";
pub const MUL: &str = "mul";
pub const SUB: &str = "sub";
pub const DIV: &str = "raw_div";
pub const SDIV: &str = "raw_sdiv";
pub const MOD: &str = "raw_mod";
pub const SMOD: &str = "raw_smod";
pub const ADDMOD: &str = "raw_addmod";
pub const MULMOD: &str = "raw_mulmod";
pub const EXP: &str = "exp";
pub const SIGNEXTEND: &str = "signextend";

// ========== EVM Comparison & Bitwise Logic ==========
pub const LT: &str = "lt";
pub const GT: &str = "gt";
pub const SLT: &str = "slt";
pub const SGT: &str = "sgt";
pub const EQ: &str = "eq";
pub const ISZERO: &str = "iszero";
pub const AND: &str = "bitwise_and";
pub const OR: &str = "bitwise_or";
pub const XOR: &str = "bitwise_xor";
pub const NOT: &str = "bitwise_not";
pub const BYTE: &str = "byte";
pub const SHL: &str = "shl";
pub const SHR: &str = "shr";
pub const SAR: &str = "sar";

// ========== EVM Keccak-256 ==========
pub const KECCAK256: &str = "keccak256";

// ========== EVM Environment Information ==========
pub const ADDRESS: &str = "address_this";
pub const BALANCE: &str = "balance";
pub const ORIGIN: &str = "origin";
pub const CALLER: &str = "caller";
pub const CALLVALUE: &str = "callvalue";
pub const CALLDATALOAD: &str = "calldataload";
pub const CALLDATASIZE: &str = "calldatasize";
pub const CALLDATACOPY: &str = "calldatacopy";
pub const CODESIZE: &str = "codesize";
pub const CODECOPY: &str = "codecopy";
pub const GASPRICE: &str = "gasprice";
pub const EXTCODESIZE: &str = "extcodesize";
pub const EXTCODECOPY: &str = "extcodecopy";
pub const RETURNDATASIZE: &str = "returndatasize";
pub const RETURNDATACOPY: &str = "returndatacopy";
pub const EXTCODEHASH: &str = "extcodehash";
pub const GAS: &str = "gas";

// ========== EVM Block Information ==========
pub const BLOCKHASH: &str = "blockhash";
pub const COINBASE: &str = "coinbase";
pub const TIMESTAMP: &str = "timestamp";
pub const NUMBER: &str = "number";
pub const DIFFICULTY: &str = "difficulty";
pub const GASLIMIT: &str = "gaslimit";
pub const CHAINID: &str = "chainid";
pub const SELFBALANCE: &str = "selfbalance";
pub const BASEFEE: &str = "basefee";
pub const BLOBHASH: &str = "blobhash";
pub const BLOBBASEFEE: &str = "blobbasefee";

// ========== EVM State Manipulation ==========
pub const SLOAD: &str = "sload";
pub const SSTORE: &str = "sstore";
pub const TLOAD: &str = "tload";
pub const TSTORE: &str = "tstore";

// ========== EVM Logging Operations ==========
pub const LOG0: &str = "log0";
pub const LOG1: &str = "log1";
pub const LOG2: &str = "log2";
pub const LOG3: &str = "log3";
pub const LOG4: &str = "log4";

// ========== EVM System Calls ==========
pub const CREATE: &str = "create";
pub const CREATE2: &str = "create2";
pub const CALL: &str = "call";
pub const CALLCODE: &str = "callcode";
pub const DELEGATECALL: &str = "delegatecall";
pub const STATICCALL: &str = "staticcall";
pub const RETURN: &str = "evm_return";
pub const STOP: &str = "evm_stop";
pub const REVERT: &str = "revert";
pub const INVALID: &str = "invalid";
pub const SELFDESTRUCT: &str = "selfdestruct";

// ========== IR Memory Primitives ==========
pub const DYNAMIC_ALLOC_ZEROED: &str = "malloc_zeroed";
pub const DYNAMIC_ALLOC_ANY_BYTES: &str = "malloc_uninit";

// ========== Memory Manipulation ==========
pub const MEMORY_COPY: &str = "mcopy";
pub const MLOAD1: &str = "mload1";
pub const MLOAD2: &str = "mload2";
pub const MLOAD3: &str = "mload3";
pub const MLOAD4: &str = "mload4";
pub const MLOAD5: &str = "mload5";
pub const MLOAD6: &str = "mload6";
pub const MLOAD7: &str = "mload7";
pub const MLOAD8: &str = "mload8";
pub const MLOAD9: &str = "mload9";
pub const MLOAD10: &str = "mload10";
pub const MLOAD11: &str = "mload11";
pub const MLOAD12: &str = "mload12";
pub const MLOAD13: &str = "mload13";
pub const MLOAD14: &str = "mload14";
pub const MLOAD15: &str = "mload15";
pub const MLOAD16: &str = "mload16";
pub const MLOAD17: &str = "mload17";
pub const MLOAD18: &str = "mload18";
pub const MLOAD19: &str = "mload19";
pub const MLOAD20: &str = "mload20";
pub const MLOAD21: &str = "mload21";
pub const MLOAD22: &str = "mload22";
pub const MLOAD23: &str = "mload23";
pub const MLOAD24: &str = "mload24";
pub const MLOAD25: &str = "mload25";
pub const MLOAD26: &str = "mload26";
pub const MLOAD27: &str = "mload27";
pub const MLOAD28: &str = "mload28";
pub const MLOAD29: &str = "mload29";
pub const MLOAD30: &str = "mload30";
pub const MLOAD31: &str = "mload31";
pub const MLOAD32: &str = "mload32";
pub const MSTORE1: &str = "mstore1";
pub const MSTORE2: &str = "mstore2";
pub const MSTORE3: &str = "mstore3";
pub const MSTORE4: &str = "mstore4";
pub const MSTORE5: &str = "mstore5";
pub const MSTORE6: &str = "mstore6";
pub const MSTORE7: &str = "mstore7";
pub const MSTORE8: &str = "mstore8";
pub const MSTORE9: &str = "mstore9";
pub const MSTORE10: &str = "mstore10";
pub const MSTORE11: &str = "mstore11";
pub const MSTORE12: &str = "mstore12";
pub const MSTORE13: &str = "mstore13";
pub const MSTORE14: &str = "mstore14";
pub const MSTORE15: &str = "mstore15";
pub const MSTORE16: &str = "mstore16";
pub const MSTORE17: &str = "mstore17";
pub const MSTORE18: &str = "mstore18";
pub const MSTORE19: &str = "mstore19";
pub const MSTORE20: &str = "mstore20";
pub const MSTORE21: &str = "mstore21";
pub const MSTORE22: &str = "mstore22";
pub const MSTORE23: &str = "mstore23";
pub const MSTORE24: &str = "mstore24";
pub const MSTORE25: &str = "mstore25";
pub const MSTORE26: &str = "mstore26";
pub const MSTORE27: &str = "mstore27";
pub const MSTORE28: &str = "mstore28";
pub const MSTORE29: &str = "mstore29";
pub const MSTORE30: &str = "mstore30";
pub const MSTORE31: &str = "mstore31";
pub const MSTORE32: &str = "mstore32";

// ========== Bytecode Introspection ==========
pub const RUNTIME_START_OFFSET: &str = "runtime_start_offset";
pub const INIT_END_OFFSET: &str = "init_end_offset";
pub const RUNTIME_LENGTH: &str = "runtime_length";
