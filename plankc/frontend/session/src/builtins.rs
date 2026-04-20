use std::fmt::{self, Display, Formatter};

use crate::{Session, StrId};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuiltinKind {
    RuntimeFoldable,
    RuntimeOnly,
    Comptime,
    ComptimeDynamic { arg_count: usize },
}

macro_rules! define_builtins {
    (
        primitive_types {
            $($pt_const:ident = $pt_str:literal => $pt_type:ident;)*
        }
        runtime_foldable_builtins {
            $($rf_const:ident $rf_str:literal => $rf_variant:ident;)*
        }
        runtime_only_builtins {
            $($ro_const:ident $ro_str:literal => $ro_variant:ident;)*
        }
        comptime_builtins {
            $($ct_const:ident $ct_str:literal => $ct_variant:ident;)*
        }
        comptime_polymorphic_builtins {
            $($cp_const:ident $cp_str:literal => $cp_variant:ident($cp_arg_count:literal);)*
        }
    ) => {
        pub mod builtin_names {
            $(pub const $pt_const: &str = $pt_str;)*
            $(pub const $rf_const: &str = $rf_str;)*
            $(pub const $ro_const: &str = $ro_str;)*
            $(pub const $ct_const: &str = $ct_str;)*
            $(pub const $cp_const: &str = $cp_str;)*
        }

        #[doc(hidden)]
        #[repr(u32)]
        enum BuiltinStrIdx {
            $($pt_type,)*
            $($rf_variant,)*
            $($ro_variant,)*
            $($ct_variant,)*
            $($cp_variant,)*
        }

        $(pub const $pt_const: StrId = StrId::new(BuiltinStrIdx::$pt_type as u32);)*
        $(pub const $rf_const: StrId = StrId::new(BuiltinStrIdx::$rf_variant as u32);)*
        $(pub const $ro_const: StrId = StrId::new(BuiltinStrIdx::$ro_variant as u32);)*
        $(pub const $ct_const: StrId = StrId::new(BuiltinStrIdx::$ct_variant as u32);)*
        $(pub const $cp_const: StrId = StrId::new(BuiltinStrIdx::$cp_variant as u32);)*

        pub fn inject_builtins(interner: &mut Session) {
            $(assert_eq!(interner.intern(builtin_names::$pt_const), $pt_const);)*
            $(assert_eq!(interner.intern(builtin_names::$rf_const), $rf_const);)*
            $(assert_eq!(interner.intern(builtin_names::$ro_const), $ro_const);)*
            $(assert_eq!(interner.intern(builtin_names::$ct_const), $ct_const);)*
            $(assert_eq!(interner.intern(builtin_names::$cp_const), $cp_const);)*
        }

        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub enum RuntimeBuiltin {
            $($rf_variant,)*
            $($ro_variant,)*
        }

        impl RuntimeBuiltin {
            pub fn from_str_id(id: StrId) -> Option<Self> {
                Some(match id {
                    $($rf_const => RuntimeBuiltin::$rf_variant,)*
                    $($ro_const => RuntimeBuiltin::$ro_variant,)*
                    _ => return None,
                })
            }

            pub fn name(self) -> &'static str {
                match self {
                    $(Self::$rf_variant => $rf_str,)*
                    $(Self::$ro_variant => $ro_str,)*
                }
            }

            pub fn foldable(self) -> bool {
                match self {
                    $(Self::$rf_variant => true,)*
                    $(Self::$ro_variant => false,)*
                }
            }
        }

        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub enum Builtin {
            Runtime(RuntimeBuiltin),
            $($ct_variant,)*
            $($cp_variant,)*
        }

        impl Builtin {
            $(pub const $rf_const: Builtin = Builtin::Runtime(RuntimeBuiltin::$rf_variant);)*
            $(pub const $ro_const: Builtin = Builtin::Runtime(RuntimeBuiltin::$ro_variant);)*

            pub const ALL: &[Builtin] = &[
                $(Builtin::Runtime(RuntimeBuiltin::$rf_variant),)*
                $(Builtin::Runtime(RuntimeBuiltin::$ro_variant),)*
                $(Builtin::$ct_variant,)*
                $(Builtin::$cp_variant,)*
            ];

            pub fn from_str_id(id: StrId) -> Option<Self> {
                if let Some(runtime) = RuntimeBuiltin::from_str_id(id) {
                    return Some(Builtin::Runtime(runtime));
                }
                Some(match id {
                    $($ct_const => Builtin::$ct_variant,)*
                    $($cp_const => Builtin::$cp_variant,)*
                    _ => return None,
                })
            }

            pub fn name(self) -> &'static str {
                match self {
                    Self::Runtime(runtime) => runtime.name(),
                    $(Self::$ct_variant => $ct_str,)*
                    $(Self::$cp_variant => $cp_str,)*
                }
            }

            pub fn kind(self) -> BuiltinKind {
                match self {
                    Self::Runtime(runtime) if runtime.foldable() => BuiltinKind::RuntimeFoldable,
                    Self::Runtime(_) => BuiltinKind::RuntimeOnly,
                    $(Self::$ct_variant => BuiltinKind::Comptime,)*
                    $(Self::$cp_variant => BuiltinKind::ComptimeDynamic { arg_count: $cp_arg_count },)*
                }
            }
        }

    };
}

impl Display for RuntimeBuiltin {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

impl Display for Builtin {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

impl From<RuntimeBuiltin> for Builtin {
    fn from(value: RuntimeBuiltin) -> Self {
        Self::Runtime(value)
    }
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

    runtime_foldable_builtins {
        // EVM Arithmetic
        ADD  "@evm_add" => Add;
        MUL "@evm_mul" => Mul;
        SUB "@evm_sub" => Sub;
        DIV "@evm_div" => Div;
        SDIV "@evm_sdiv" => SDiv;
        MOD "@evm_mod" => Mod;
        SMOD "@evm_smod" => SMod;
        ADDMOD "@evm_addmod" => AddMod;
        MULMOD "@evm_mulmod" => MulMod;
        EXP "@evm_exp" => Exp;
        SIGNEXTEND "@evm_signextend" => SignExtend;

        // EVM Comparison & Bitwise Logic
        LT "@evm_lt" => Lt;
        GT "@evm_gt" => Gt;
        SLT "@evm_slt" => SLt;
        SGT "@evm_sgt" => SGt;
        EQ "@evm_eq" => Eq;
        ISZERO "@evm_iszero" => IsZero;
        AND "@evm_and" => And;
        OR "@evm_or" => Or;
        XOR "@evm_xor" => Xor;
        NOT "@evm_not" => Not;
        BYTE "@evm_byte" => Byte;
        SHL "@evm_shl" => Shl;
        SHR "@evm_shr" => Shr;
        SAR "@evm_sar" => Sar;
    }

    runtime_only_builtins {
        // EVM Keccak-256
        KECCAK256 "@evm_keccak256" => Keccak256;

        // EVM Environment Information
        ADDRESS "@evm_address_this" => Address;
        BALANCE "@evm_balance" => Balance;
        ORIGIN "@evm_origin" => Origin;
        CALLER "@evm_caller" => Caller;
        CALLVALUE "@evm_callvalue" => CallValue;
        CALLDATALOAD "@evm_calldataload" => CallDataLoad;
        CALLDATASIZE "@evm_calldatasize" => CallDataSize;
        CALLDATACOPY "@evm_calldatacopy" => CallDataCopy;
        CODESIZE "@evm_codesize" => CodeSize;
        CODECOPY "@evm_codecopy" => CodeCopy;
        GASPRICE "@evm_gasprice" => GasPrice;
        EXTCODESIZE "@evm_extcodesize" => ExtCodeSize;
        EXTCODECOPY "@evm_extcodecopy" => ExtCodeCopy;
        RETURNDATASIZE "@evm_returndatasize" => ReturnDataSize;
        RETURNDATACOPY "@evm_returndatacopy" => ReturnDataCopy;
        EXTCODEHASH "@evm_extcodehash" => ExtCodeHash;
        GAS "@evm_gas" => Gas;

        // EVM Block Information
        BLOCKHASH "@evm_blockhash" => BlockHash;
        COINBASE "@evm_coinbase" => Coinbase;
        TIMESTAMP "@evm_timestamp" => Timestamp;
        NUMBER "@evm_number" => Number;
        DIFFICULTY "@evm_difficulty" => Difficulty;
        GASLIMIT "@evm_gaslimit" => GasLimit;
        CHAINID "@evm_chainid" => ChainId;
        SELFBALANCE "@evm_selfbalance" => SelfBalance;
        BASEFEE "@evm_basefee" => BaseFee;
        BLOBHASH "@evm_blobhash" => BlobHash;
        BLOBBASEFEE "@evm_blobbasefee" => BlobBaseFee;

        // EVM State Manipulation
        SLOAD "@evm_sload" => SLoad;
        SSTORE "@evm_sstore" => SStore;
        TLOAD "@evm_tload" => TLoad;
        TSTORE "@evm_tstore" => TStore;

        // EVM Logging Operations
        LOG0 "@evm_log0" => Log0;
        LOG1 "@evm_log1" => Log1;
        LOG2 "@evm_log2" => Log2;
        LOG3 "@evm_log3" => Log3;
        LOG4 "@evm_log4" => Log4;

        // EVM System Calls
        CREATE "@evm_create" => Create;
        CREATE2 "@evm_create2" => Create2;
        CALL "@evm_call" => Call;
        CALLCODE "@evm_callcode" => CallCode;
        DELEGATECALL "@evm_delegatecall" => DelegateCall;
        STATICCALL "@evm_staticcall" => StaticCall;
        RETURN "@evm_return" => Return;
        STOP "@evm_stop" => Stop;
        REVERT "@evm_revert" => Revert;
        INVALID "@evm_invalid" => Invalid;
        SELFDESTRUCT "@evm_selfdestruct" => SelfDestruct;

        // IR Memory Primitives
        DYNAMIC_ALLOC_ZEROED "@malloc_zeroed" => DynamicAllocZeroed;
        DYNAMIC_ALLOC_ANY_BYTES "@malloc_uninit" => DynamicAllocAnyBytes;

        // Memory Manipulation
        MEMORY_COPY "@mcopy" => MemoryCopy;
        MLOAD1 "@mload1" => MLoad1;
        MLOAD2 "@mload2" => MLoad2;
        MLOAD3 "@mload3" => MLoad3;
        MLOAD4 "@mload4" => MLoad4;
        MLOAD5 "@mload5" => MLoad5;
        MLOAD6 "@mload6" => MLoad6;
        MLOAD7 "@mload7" => MLoad7;
        MLOAD8 "@mload8" => MLoad8;
        MLOAD9 "@mload9" => MLoad9;
        MLOAD10 "@mload10" => MLoad10;
        MLOAD11 "@mload11" => MLoad11;
        MLOAD12 "@mload12" => MLoad12;
        MLOAD13 "@mload13" => MLoad13;
        MLOAD14 "@mload14" => MLoad14;
        MLOAD15 "@mload15" => MLoad15;
        MLOAD16 "@mload16" => MLoad16;
        MLOAD17 "@mload17" => MLoad17;
        MLOAD18 "@mload18" => MLoad18;
        MLOAD19 "@mload19" => MLoad19;
        MLOAD20 "@mload20" => MLoad20;
        MLOAD21 "@mload21" => MLoad21;
        MLOAD22 "@mload22" => MLoad22;
        MLOAD23 "@mload23" => MLoad23;
        MLOAD24 "@mload24" => MLoad24;
        MLOAD25 "@mload25" => MLoad25;
        MLOAD26 "@mload26" => MLoad26;
        MLOAD27 "@mload27" => MLoad27;
        MLOAD28 "@mload28" => MLoad28;
        MLOAD29 "@mload29" => MLoad29;
        MLOAD30 "@mload30" => MLoad30;
        MLOAD31 "@mload31" => MLoad31;
        MLOAD32 "@mload32" => MLoad32;
        MSTORE1 "@mstore1" => MStore1;
        MSTORE2 "@mstore2" => MStore2;
        MSTORE3 "@mstore3" => MStore3;
        MSTORE4 "@mstore4" => MStore4;
        MSTORE5 "@mstore5" => MStore5;
        MSTORE6 "@mstore6" => MStore6;
        MSTORE7 "@mstore7" => MStore7;
        MSTORE8 "@mstore8" => MStore8;
        MSTORE9 "@mstore9" => MStore9;
        MSTORE10 "@mstore10" => MStore10;
        MSTORE11 "@mstore11" => MStore11;
        MSTORE12 "@mstore12" => MStore12;
        MSTORE13 "@mstore13" => MStore13;
        MSTORE14 "@mstore14" => MStore14;
        MSTORE15 "@mstore15" => MStore15;
        MSTORE16 "@mstore16" => MStore16;
        MSTORE17 "@mstore17" => MStore17;
        MSTORE18 "@mstore18" => MStore18;
        MSTORE19 "@mstore19" => MStore19;
        MSTORE20 "@mstore20" => MStore20;
        MSTORE21 "@mstore21" => MStore21;
        MSTORE22 "@mstore22" => MStore22;
        MSTORE23 "@mstore23" => MStore23;
        MSTORE24 "@mstore24" => MStore24;
        MSTORE25 "@mstore25" => MStore25;
        MSTORE26 "@mstore26" => MStore26;
        MSTORE27 "@mstore27" => MStore27;
        MSTORE28 "@mstore28" => MStore28;
        MSTORE29 "@mstore29" => MStore29;
        MSTORE30 "@mstore30" => MStore30;
        MSTORE31 "@mstore31" => MStore31;
        MSTORE32 "@mstore32" => MStore32;

        // Bytecode Introspection
        RUNTIME_START_OFFSET "@runtime_start_offset" => RuntimeStartOffset;
        INIT_END_OFFSET "@init_end_offset" => InitEndOffset;
        RUNTIME_LENGTH "@runtime_length" => RuntimeLength;
    }

    comptime_builtins {
        // Type Reflection
        IS_STRUCT "@is_struct" => IsStruct;
        FIELD_COUNT "@field_count" => FieldCount;
    }

    comptime_polymorphic_builtins {
        // Type Reflection
        FIELD_TYPE "@field_type" => FieldType(2);
        GET_FIELD "@get_field" => GetField(2);
        SET_FIELD "@set_field" => SetField(3);
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
        assert_eq!(Builtin::from_str_id(ADD), Some(Builtin::ADD));
        assert_eq!(Builtin::from_str_id(KECCAK256), Some(Builtin::KECCAK256));
        assert_eq!(Builtin::from_str_id(VOID), None);
    }
}
