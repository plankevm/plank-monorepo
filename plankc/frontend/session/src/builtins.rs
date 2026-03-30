use crate::{Session, StrId, types::TypeId};

pub type BuiltinSignature = (&'static [TypeId], TypeId);

macro_rules! define_builtins {
    (
        primitive_types {
            $($pt_const:ident = $pt_str:literal => $pt_type:ident;)*
        }
        builtins {
            $(
                $b_const:ident $b_str:literal => $b_variant:ident
                { $( [$($arg:ident),* => $ret:ident] ),+ };
            )*
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
        pub enum EvmBuiltin {
            $($b_variant,)*
        }

        impl EvmBuiltin {
            pub fn from_str_id(id: StrId) -> Option<Self> {
                Some(match id {
                    $($b_const => EvmBuiltin::$b_variant,)*
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
                    $(EvmBuiltin::$b_variant => &[ $( (&[$($arg),*], $ret) ),+ ]),*
                }
            }
        }

        impl ::std::fmt::Display for EvmBuiltin {
            fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                let name = match self {
                    $(EvmBuiltin::$b_variant => builtin_names::$b_const,)*
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
        ADD  "add" => Add
            { [U256, U256 => U256], [MP, U256 => MP], [U256, MP => MP] };
        MUL "mul" => Mul { [U256, U256 => U256] };
        SUB "sub" => Sub
            { [U256, U256 => U256], [MP, U256 => MP], [MP, MP => U256] };
        DIV "raw_div" => Div { [U256, U256 => U256] };
        SDIV "raw_sdiv" => SDiv { [U256, U256 => U256] };
        MOD "raw_mod" => Mod { [U256, U256 => U256] };
        SMOD "raw_smod" => SMod { [U256, U256 => U256] };
        ADDMOD "raw_addmod" => AddMod { [U256, U256, U256 => U256] };
        MULMOD "raw_mulmod" => MulMod { [U256, U256, U256 => U256] };
        EXP "exp" => Exp { [U256, U256 => U256] };
        SIGNEXTEND "signextend" => SignExtend { [U256, U256 => U256] };

        // EVM Comparison & Bitwise Logic
        LT "lt" => Lt { [U256, U256 => BOOL], [MP, MP => BOOL] };
        GT "gt" => Gt { [U256, U256 => BOOL], [MP, MP => BOOL] };
        SLT "slt" => SLt { [U256, U256 => BOOL] };
        SGT "sgt" => SGt { [U256, U256 => BOOL] };
        EQ "eq" => Eq { [U256, U256 => BOOL], [MP, MP => BOOL] };
        ISZERO "iszero" => IsZero { [U256 => BOOL] };
        AND "bitwise_and" => And { [U256, U256 => U256] };
        OR "bitwise_or" => Or { [U256, U256 => U256] };
        XOR "bitwise_xor" => Xor { [U256, U256 => U256] };
        NOT "bitwise_not" => Not { [U256 => U256] };
        BYTE "byte" => Byte { [U256, U256 => U256] };
        SHL "shl" => Shl { [U256, U256 => U256] };
        SHR "shr" => Shr { [U256, U256 => U256] };
        SAR "sar" => Sar { [U256, U256 => U256] };

        // EVM Keccak-256
        KECCAK256 "keccak256" => Keccak256 { [MP, U256 => U256] };

        // EVM Environment Information
        ADDRESS "address_this" => Address { [=> U256] };
        BALANCE "balance" => Balance { [U256 => U256] };
        ORIGIN "origin" => Origin { [=> U256] };
        CALLER "caller" => Caller { [=> U256] };
        CALLVALUE "callvalue" => CallValue { [=> U256] };
        CALLDATALOAD "calldataload" => CallDataLoad { [U256 => U256] };
        CALLDATASIZE "calldatasize" => CallDataSize { [=> U256] };
        CALLDATACOPY "calldatacopy" => CallDataCopy { [MP, U256, U256 => VOID] };
        CODESIZE "codesize" => CodeSize { [=> U256] };
        CODECOPY "codecopy" => CodeCopy { [MP, U256, U256 => VOID] };
        GASPRICE "gasprice" => GasPrice { [=> U256] };
        EXTCODESIZE "extcodesize" => ExtCodeSize { [U256 => U256] };
        EXTCODECOPY "extcodecopy" => ExtCodeCopy { [U256, MP, U256, U256 => VOID] };
        RETURNDATASIZE "returndatasize" => ReturnDataSize { [=> U256] };
        RETURNDATACOPY "returndatacopy" => ReturnDataCopy { [MP, U256, U256 => VOID] };
        EXTCODEHASH "extcodehash" => ExtCodeHash { [U256 => U256] };
        GAS "gas" => Gas { [=> U256] };

        // EVM Block Information
        BLOCKHASH "blockhash" => BlockHash { [U256 => U256] };
        COINBASE "coinbase" => Coinbase { [=> U256] };
        TIMESTAMP "timestamp" => Timestamp { [=> U256] };
        NUMBER "number" => Number { [=> U256] };
        DIFFICULTY "difficulty" => Difficulty { [=> U256] };
        GASLIMIT "gaslimit" => GasLimit { [=> U256] };
        CHAINID "chainid" => ChainId { [=> U256] };
        SELFBALANCE "selfbalance" => SelfBalance { [=> U256] };
        BASEFEE "basefee" => BaseFee { [=> U256] };
        BLOBHASH "blobhash" => BlobHash { [U256 => U256] };
        BLOBBASEFEE "blobbasefee" => BlobBaseFee { [=> U256] };

        // EVM State Manipulation
        SLOAD "sload" => SLoad { [U256 => U256] };
        SSTORE "sstore" => SStore { [U256, U256 => VOID] };
        TLOAD "tload" => TLoad { [U256 => U256] };
        TSTORE "tstore" => TStore { [U256, U256 => VOID] };

        // EVM Logging Operations
        LOG0 "log0" => Log0 { [MP, U256 => VOID] };
        LOG1 "log1" => Log1 { [MP, U256, U256 => VOID] };
        LOG2 "log2" => Log2 { [MP, U256, U256, U256 => VOID] };
        LOG3 "log3" => Log3 { [MP, U256, U256, U256, U256 => VOID] };
        LOG4 "log4" => Log4 { [MP, U256, U256, U256, U256, U256 => VOID] };

        // EVM System Calls
        CREATE "create" => Create { [U256, MP, U256 => U256] };
        CREATE2 "create2" => Create2 { [U256, MP, U256, U256 => U256] };
        CALL "call" => Call { [U256, U256, U256, MP, U256, MP, U256 => BOOL] };
        CALLCODE "callcode" => CallCode { [U256, U256, U256, MP, U256, MP, U256 => BOOL] };
        DELEGATECALL "delegatecall" => DelegateCall { [U256, U256, MP, U256, MP, U256 => BOOL] };
        STATICCALL "staticcall" => StaticCall { [U256, U256, MP, U256, MP, U256 => BOOL] };
        RETURN "evm_return" => Return { [MP, U256 => NEVER] };
        STOP "evm_stop" => Stop { [=> NEVER] };
        REVERT "revert" => Revert { [MP, U256 => NEVER] };
        INVALID "invalid" => Invalid { [=> NEVER] };
        SELFDESTRUCT "selfdestruct" => SelfDestruct { [U256 => NEVER] };

        // IR Memory Primitives
        DYNAMIC_ALLOC_ZEROED "malloc_zeroed" => DynamicAllocZeroed { [U256 => MP] };
        DYNAMIC_ALLOC_ANY_BYTES "malloc_uninit" => DynamicAllocAnyBytes { [U256 => MP] };

        // Memory Manipulation
        MEMORY_COPY "mcopy" => MemoryCopy { [MP, MP, U256 => VOID] };
        MLOAD1 "mload1" => MLoad1 { [MP => U256] };
        MLOAD2 "mload2" => MLoad2 { [MP => U256] };
        MLOAD3 "mload3" => MLoad3 { [MP => U256] };
        MLOAD4 "mload4" => MLoad4 { [MP => U256] };
        MLOAD5 "mload5" => MLoad5 { [MP => U256] };
        MLOAD6 "mload6" => MLoad6 { [MP => U256] };
        MLOAD7 "mload7" => MLoad7 { [MP => U256] };
        MLOAD8 "mload8" => MLoad8 { [MP => U256] };
        MLOAD9 "mload9" => MLoad9 { [MP => U256] };
        MLOAD10 "mload10" => MLoad10 { [MP => U256] };
        MLOAD11 "mload11" => MLoad11 { [MP => U256] };
        MLOAD12 "mload12" => MLoad12 { [MP => U256] };
        MLOAD13 "mload13" => MLoad13 { [MP => U256] };
        MLOAD14 "mload14" => MLoad14 { [MP => U256] };
        MLOAD15 "mload15" => MLoad15 { [MP => U256] };
        MLOAD16 "mload16" => MLoad16 { [MP => U256] };
        MLOAD17 "mload17" => MLoad17 { [MP => U256] };
        MLOAD18 "mload18" => MLoad18 { [MP => U256] };
        MLOAD19 "mload19" => MLoad19 { [MP => U256] };
        MLOAD20 "mload20" => MLoad20 { [MP => U256] };
        MLOAD21 "mload21" => MLoad21 { [MP => U256] };
        MLOAD22 "mload22" => MLoad22 { [MP => U256] };
        MLOAD23 "mload23" => MLoad23 { [MP => U256] };
        MLOAD24 "mload24" => MLoad24 { [MP => U256] };
        MLOAD25 "mload25" => MLoad25 { [MP => U256] };
        MLOAD26 "mload26" => MLoad26 { [MP => U256] };
        MLOAD27 "mload27" => MLoad27 { [MP => U256] };
        MLOAD28 "mload28" => MLoad28 { [MP => U256] };
        MLOAD29 "mload29" => MLoad29 { [MP => U256] };
        MLOAD30 "mload30" => MLoad30 { [MP => U256] };
        MLOAD31 "mload31" => MLoad31 { [MP => U256] };
        MLOAD32 "mload32" => MLoad32 { [MP => U256] };
        MSTORE1 "mstore1" => MStore1 { [MP, U256 => VOID] };
        MSTORE2 "mstore2" => MStore2 { [MP, U256 => VOID] };
        MSTORE3 "mstore3" => MStore3 { [MP, U256 => VOID] };
        MSTORE4 "mstore4" => MStore4 { [MP, U256 => VOID] };
        MSTORE5 "mstore5" => MStore5 { [MP, U256 => VOID] };
        MSTORE6 "mstore6" => MStore6 { [MP, U256 => VOID] };
        MSTORE7 "mstore7" => MStore7 { [MP, U256 => VOID] };
        MSTORE8 "mstore8" => MStore8 { [MP, U256 => VOID] };
        MSTORE9 "mstore9" => MStore9 { [MP, U256 => VOID] };
        MSTORE10 "mstore10" => MStore10 { [MP, U256 => VOID] };
        MSTORE11 "mstore11" => MStore11 { [MP, U256 => VOID] };
        MSTORE12 "mstore12" => MStore12 { [MP, U256 => VOID] };
        MSTORE13 "mstore13" => MStore13 { [MP, U256 => VOID] };
        MSTORE14 "mstore14" => MStore14 { [MP, U256 => VOID] };
        MSTORE15 "mstore15" => MStore15 { [MP, U256 => VOID] };
        MSTORE16 "mstore16" => MStore16 { [MP, U256 => VOID] };
        MSTORE17 "mstore17" => MStore17 { [MP, U256 => VOID] };
        MSTORE18 "mstore18" => MStore18 { [MP, U256 => VOID] };
        MSTORE19 "mstore19" => MStore19 { [MP, U256 => VOID] };
        MSTORE20 "mstore20" => MStore20 { [MP, U256 => VOID] };
        MSTORE21 "mstore21" => MStore21 { [MP, U256 => VOID] };
        MSTORE22 "mstore22" => MStore22 { [MP, U256 => VOID] };
        MSTORE23 "mstore23" => MStore23 { [MP, U256 => VOID] };
        MSTORE24 "mstore24" => MStore24 { [MP, U256 => VOID] };
        MSTORE25 "mstore25" => MStore25 { [MP, U256 => VOID] };
        MSTORE26 "mstore26" => MStore26 { [MP, U256 => VOID] };
        MSTORE27 "mstore27" => MStore27 { [MP, U256 => VOID] };
        MSTORE28 "mstore28" => MStore28 { [MP, U256 => VOID] };
        MSTORE29 "mstore29" => MStore29 { [MP, U256 => VOID] };
        MSTORE30 "mstore30" => MStore30 { [MP, U256 => VOID] };
        MSTORE31 "mstore31" => MStore31 { [MP, U256 => VOID] };
        MSTORE32 "mstore32" => MStore32 { [MP, U256 => VOID] };

        // Bytecode Introspection
        RUNTIME_START_OFFSET "runtime_start_offset" => RuntimeStartOffset { [=> U256] };
        INIT_END_OFFSET "init_end_offset" => InitEndOffset { [=> U256] };
        RUNTIME_LENGTH "runtime_length" => RuntimeLength { [=> U256] };
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
        assert_eq!(EvmBuiltin::from_str_id(ADD), Some(EvmBuiltin::Add));
        assert_eq!(EvmBuiltin::from_str_id(KECCAK256), Some(EvmBuiltin::Keccak256));
        assert_eq!(EvmBuiltin::from_str_id(VOID), None);
    }

    #[test]
    fn test_resolve_primitive() {
        assert_eq!(TypeId::resolve_primitive(VOID), Some(TypeId::VOID));
        assert_eq!(TypeId::resolve_primitive(U256), Some(TypeId::U256));
        assert_eq!(TypeId::resolve_primitive(ADD), None);
    }
}
