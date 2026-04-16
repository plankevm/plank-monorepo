use std::fmt::{self, Display, Formatter};

use crate::{Session, StrId, types::TypeId};

#[derive(Debug, Clone, Copy)]
pub struct BuiltinSignature {
    pub inputs: &'static [TypeId],
    pub result: TypeId,
}

#[derive(Debug, Clone, Copy)]
pub enum BuiltinKind {
    /// Runtime builtin with no side effects; can be constant-folded when all
    /// inputs are comptime, otherwise emitted to MIR.
    RuntimeFoldable(&'static [BuiltinSignature]),
    /// Runtime builtin with side effects; always emitted to MIR, rejected in
    /// comptime context.
    RuntimeOnly(&'static [BuiltinSignature]),
    /// Comptime-only builtin with static signatures (e.g. type reflection).
    Comptime(&'static [BuiltinSignature]),
    /// Comptime-only builtin whose result type is determined by evaluation,
    /// not by static signatures.
    ComptimePolymorphic { arg_count: usize },
}

/// Builds a `BuiltinKind::$variant(SIGS)` value, wrapping the signature
/// list in a compile-time check that all overloads share the same arg count.
macro_rules! sig_kind {
    ($variant:ident, $( [ $($arg:ident),* => $ret:ident ] ),+) => {{
        const SIGS: &[BuiltinSignature] = &[$(BuiltinSignature {
            inputs: &[$($arg),*],
            result: $ret
        }),+];
        // Invariant: Each builtin has at least 1 sig and all sigs have the
        // same number of inputs.
        const {
            assert!(!SIGS.is_empty());
            let mut i = 1;
            while i < SIGS.len() {
                assert!(SIGS[0].inputs.len() == SIGS[i].inputs.len());
                i += 1;
            }
        };
        BuiltinKind::$variant(SIGS)
    }};
}

macro_rules! define_builtins {
    (
        primitive_types {
            $($pt_const:ident = $pt_str:literal => $pt_type:ident;)*
        }
        runtime_foldable_builtins {
            $(
                $pure_const:ident $pure_str:literal => $pure_variant:ident
                { $( [$($pure_arg:ident),* => $pure_ret:ident] ),+ };
            )*
        }
        runtime_only_builtins {
            $(
                $imp_const:ident $imp_str:literal => $imp_variant:ident
                { $( [$($imp_arg:ident),* => $imp_ret:ident] ),+ };
            )*
        }
        comptime_builtins {
            $(
                $cb_const:ident $cb_str:literal => $cb_variant:ident
                { $( [$($cb_arg:ident),* => $cb_ret:ident] ),+ };
            )*
        }
        comptime_polymorphic_builtins {
            $(
                $pb_const:ident $pb_str:literal => $pb_variant:ident($pb_arg_count:literal);
            )*
        }
    ) => {
        pub mod builtin_names {
            $(pub const $pt_const: &str = $pt_str;)*
            $(pub const $pure_const: &str = $pure_str;)*
            $(pub const $imp_const: &str = $imp_str;)*
            $(pub const $cb_const: &str = $cb_str;)*
            $(pub const $pb_const: &str = $pb_str;)*
        }

        #[doc(hidden)]
        #[allow(dead_code)]
        #[repr(u32)]
        enum BuiltinStrIdx {
            $($pt_type,)*
            $($pure_variant,)*
            $($imp_variant,)*
            $($cb_variant,)*
            $($pb_variant,)*
            _Count,
        }

        $(pub const $pt_const: StrId = StrId::new(BuiltinStrIdx::$pt_type as u32);)*
        $(pub const $pure_const: StrId = StrId::new(BuiltinStrIdx::$pure_variant as u32);)*
        $(pub const $imp_const: StrId = StrId::new(BuiltinStrIdx::$imp_variant as u32);)*
        $(pub const $cb_const: StrId = StrId::new(BuiltinStrIdx::$cb_variant as u32);)*
        $(pub const $pb_const: StrId = StrId::new(BuiltinStrIdx::$pb_variant as u32);)*

        pub fn inject_builtins(interner: &mut Session) {
            $(assert_eq!(interner.intern(builtin_names::$pt_const), $pt_const);)*
            $(assert_eq!(interner.intern(builtin_names::$pure_const), $pure_const);)*
            $(assert_eq!(interner.intern(builtin_names::$imp_const), $imp_const);)*
            $(assert_eq!(interner.intern(builtin_names::$cb_const), $cb_const);)*
            $(assert_eq!(interner.intern(builtin_names::$pb_const), $pb_const);)*
        }

        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub enum Builtin {
            $($pure_variant,)*
            $($imp_variant,)*
            $($cb_variant,)*
            $($pb_variant,)*
        }

        impl Builtin {
            pub fn from_str_id(id: StrId) -> Option<Self> {
                Some(match id {
                    $($pure_const => Builtin::$pure_variant,)*
                    $($imp_const => Builtin::$imp_variant,)*
                    $($cb_const => Builtin::$cb_variant,)*
                    $($pb_const => Builtin::$pb_variant,)*
                    _ => return None,
                })
            }

            pub fn name(self) -> &'static str {
                match self {
                    $(Self::$pure_variant => $pure_str,)*
                    $(Self::$imp_variant => $imp_str,)*
                    $(Self::$cb_variant => $cb_str,)*
                    $(Self::$pb_variant => $pb_str,)*
                }
            }

            pub fn kind(self) -> BuiltinKind {
                const U256: TypeId = TypeId::U256;
                const BOOL: TypeId = TypeId::BOOL;
                const MP: TypeId = TypeId::MEMORY_POINTER;
                const VOID: TypeId = TypeId::VOID;
                const NEVER: TypeId = TypeId::NEVER;
                const TYPE: TypeId = TypeId::TYPE;

                match self {
                    $(Self::$pure_variant => sig_kind!(RuntimeFoldable, $([$($pure_arg),* => $pure_ret]),+),)*
                    $(Self::$imp_variant => sig_kind!(RuntimeOnly, $([$($imp_arg),* => $imp_ret]),+),)*
                    $(Self::$cb_variant => sig_kind!(Comptime, $([$($cb_arg),* => $cb_ret]),+),)*
                    $(Self::$pb_variant => BuiltinKind::ComptimePolymorphic { arg_count: $pb_arg_count },)*
                }
            }

            pub fn signatures(self) -> &'static [BuiltinSignature] {
                match self.kind() {
                    BuiltinKind::RuntimeFoldable(s)
                    | BuiltinKind::RuntimeOnly(s)
                    | BuiltinKind::Comptime(s) => s,
                    BuiltinKind::ComptimePolymorphic { .. } => &[],
                }
            }

            /// All sigs have the same arg count, guaranteed by compile time check in
            /// `kind`.
            pub fn arg_count(self) -> usize {
                match self.kind() {
                    BuiltinKind::RuntimeFoldable(s)
                    | BuiltinKind::RuntimeOnly(s)
                    | BuiltinKind::Comptime(s) => s[0].inputs.len(),
                    BuiltinKind::ComptimePolymorphic { arg_count } => arg_count,
                }
            }

            pub fn resolve_result_type(self, arg_types: &[TypeId]) -> Option<TypeId> {
                let signatures = self.signatures();
                if signatures.is_empty() || signatures[0].inputs.len() != arg_types.len() {
                    return None;
                }
                for sig in signatures {
                    if sig
                        .inputs
                        .iter()
                        .zip(arg_types)
                        .all(|(&sig_in, &arg_in)| arg_in.is_assignable_to(sig_in))
                    {
                        return Some(sig.result);
                    }
                }
                None
            }
        }

        impl Display for Builtin {
            fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
                f.write_str(self.name())
            }
        }

        /// Newtype around [`Builtin`], statically known to hold a runtime
        /// builtin (Pure or Impure kind). Used by MIR to enforce at the
        /// HIR→MIR boundary that only runtime builtins reach code generation.
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub struct RuntimeBuiltin(Builtin);

        #[allow(non_upper_case_globals)]
        impl RuntimeBuiltin {
            $(pub const $pure_variant: Self = Self(Builtin::$pure_variant);)*
            $(pub const $imp_variant: Self = Self(Builtin::$imp_variant);)*

            pub fn inner(self) -> Builtin { self.0 }
        }

        impl TryFrom<Builtin> for RuntimeBuiltin {
            type Error = ();
            fn try_from(b: Builtin) -> Result<Self, Self::Error> {
                match b.kind() {
                    BuiltinKind::RuntimeFoldable(_) | BuiltinKind::RuntimeOnly(_) => Ok(Self(b)),
                    BuiltinKind::Comptime(_) | BuiltinKind::ComptimePolymorphic { .. } => Err(()),
                }
            }
        }

        impl Display for RuntimeBuiltin {
            fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
                self.0.fmt(f)
            }
        }

        impl TypeId {
            pub fn resolve_primitive(name: StrId) -> Option<TypeId> {
                Some(match name {
                    $($pt_const => TypeId::$pt_const,)*
                    _ => return None,
                })
            }

            pub fn primitive_name(self) -> Option<&'static str> {
                match self {
                    $(Self::$pt_const => Some($pt_str),)*
                    _ => None
                }

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

    runtime_foldable_builtins {
        // EVM Arithmetic
        ADD  "@evm_add" => Add
            { [U256, U256 => U256], [MP, U256 => MP], [U256, MP => MP] };
        MUL "@evm_mul" => Mul { [U256, U256 => U256] };
        SUB "@evm_sub" => Sub
            { [U256, U256 => U256], [MP, U256 => MP], [MP, MP => U256] };
        DIV "@evm_div" => Div { [U256, U256 => U256] };
        SDIV "@evm_sdiv" => SDiv { [U256, U256 => U256] };
        MOD "@evm_mod" => Mod { [U256, U256 => U256] };
        SMOD "@evm_smod" => SMod { [U256, U256 => U256] };
        ADDMOD "@evm_addmod" => AddMod { [U256, U256, U256 => U256] };
        MULMOD "@evm_mulmod" => MulMod { [U256, U256, U256 => U256] };
        EXP "@evm_exp" => Exp { [U256, U256 => U256] };
        SIGNEXTEND "@evm_signextend" => SignExtend { [U256, U256 => U256] };

        // EVM Comparison & Bitwise Logic
        LT "@evm_lt" => Lt { [U256, U256 => BOOL], [MP, MP => BOOL] };
        GT "@evm_gt" => Gt { [U256, U256 => BOOL], [MP, MP => BOOL] };
        SLT "@evm_slt" => SLt { [U256, U256 => BOOL] };
        SGT "@evm_sgt" => SGt { [U256, U256 => BOOL] };
        EQ "@evm_eq" => Eq { [U256, U256 => BOOL], [MP, MP => BOOL] };
        ISZERO "@evm_iszero" => IsZero { [U256 => BOOL] };
        AND "@evm_and" => And { [U256, U256 => U256] };
        OR "@evm_or" => Or { [U256, U256 => U256] };
        XOR "@evm_xor" => Xor { [U256, U256 => U256] };
        NOT "@evm_not" => Not { [U256 => U256] };
        BYTE "@evm_byte" => Byte { [U256, U256 => U256] };
        SHL "@evm_shl" => Shl { [U256, U256 => U256] };
        SHR "@evm_shr" => Shr { [U256, U256 => U256] };
        SAR "@evm_sar" => Sar { [U256, U256 => U256] };
    }

    runtime_only_builtins {
        // EVM Keccak-256
        KECCAK256 "@evm_keccak256" => Keccak256 { [MP, U256 => U256] };

        // EVM Environment Information
        ADDRESS "@evm_address_this" => Address { [=> U256] };
        BALANCE "@evm_balance" => Balance { [U256 => U256] };
        ORIGIN "@evm_origin" => Origin { [=> U256] };
        CALLER "@evm_caller" => Caller { [=> U256] };
        CALLVALUE "@evm_callvalue" => CallValue { [=> U256] };
        CALLDATALOAD "@evm_calldataload" => CallDataLoad { [U256 => U256] };
        CALLDATASIZE "@evm_calldatasize" => CallDataSize { [=> U256] };
        CALLDATACOPY "@evm_calldatacopy" => CallDataCopy { [MP, U256, U256 => VOID] };
        CODESIZE "@evm_codesize" => CodeSize { [=> U256] };
        CODECOPY "@evm_codecopy" => CodeCopy { [MP, U256, U256 => VOID] };
        GASPRICE "@evm_gasprice" => GasPrice { [=> U256] };
        EXTCODESIZE "@evm_extcodesize" => ExtCodeSize { [U256 => U256] };
        EXTCODECOPY "@evm_extcodecopy" => ExtCodeCopy { [U256, MP, U256, U256 => VOID] };
        RETURNDATASIZE "@evm_returndatasize" => ReturnDataSize { [=> U256] };
        RETURNDATACOPY "@evm_returndatacopy" => ReturnDataCopy { [MP, U256, U256 => VOID] };
        EXTCODEHASH "@evm_extcodehash" => ExtCodeHash { [U256 => U256] };
        GAS "@evm_gas" => Gas { [=> U256] };

        // EVM Block Information
        BLOCKHASH "@evm_blockhash" => BlockHash { [U256 => U256] };
        COINBASE "@evm_coinbase" => Coinbase { [=> U256] };
        TIMESTAMP "@evm_timestamp" => Timestamp { [=> U256] };
        NUMBER "@evm_number" => Number { [=> U256] };
        DIFFICULTY "@evm_difficulty" => Difficulty { [=> U256] };
        GASLIMIT "@evm_gaslimit" => GasLimit { [=> U256] };
        CHAINID "@evm_chainid" => ChainId { [=> U256] };
        SELFBALANCE "@evm_selfbalance" => SelfBalance { [=> U256] };
        BASEFEE "@evm_basefee" => BaseFee { [=> U256] };
        BLOBHASH "@evm_blobhash" => BlobHash { [U256 => U256] };
        BLOBBASEFEE "@evm_blobbasefee" => BlobBaseFee { [=> U256] };

        // EVM State Manipulation
        SLOAD "@evm_sload" => SLoad { [U256 => U256] };
        SSTORE "@evm_sstore" => SStore { [U256, U256 => VOID] };
        TLOAD "@evm_tload" => TLoad { [U256 => U256] };
        TSTORE "@evm_tstore" => TStore { [U256, U256 => VOID] };

        // EVM Logging Operations
        LOG0 "@evm_log0" => Log0 { [MP, U256 => VOID] };
        LOG1 "@evm_log1" => Log1 { [MP, U256, U256 => VOID] };
        LOG2 "@evm_log2" => Log2 { [MP, U256, U256, U256 => VOID] };
        LOG3 "@evm_log3" => Log3 { [MP, U256, U256, U256, U256 => VOID] };
        LOG4 "@evm_log4" => Log4 { [MP, U256, U256, U256, U256, U256 => VOID] };

        // EVM System Calls
        CREATE "@evm_create" => Create { [U256, MP, U256 => U256] };
        CREATE2 "@evm_create2" => Create2 { [U256, MP, U256, U256 => U256] };
        CALL "@evm_call" => Call { [U256, U256, U256, MP, U256, MP, U256 => BOOL] };
        CALLCODE "@evm_callcode" => CallCode { [U256, U256, U256, MP, U256, MP, U256 => BOOL] };
        DELEGATECALL "@evm_delegatecall" => DelegateCall { [U256, U256, MP, U256, MP, U256 => BOOL] };
        STATICCALL "@evm_staticcall" => StaticCall { [U256, U256, MP, U256, MP, U256 => BOOL] };
        RETURN "@evm_return" => Return { [MP, U256 => NEVER] };
        STOP "@evm_stop" => Stop { [=> NEVER] };
        REVERT "@evm_revert" => Revert { [MP, U256 => NEVER] };
        INVALID "@evm_invalid" => Invalid { [=> NEVER] };
        SELFDESTRUCT "@evm_selfdestruct" => SelfDestruct { [U256 => NEVER] };

        // IR Memory Primitives
        DYNAMIC_ALLOC_ZEROED "@malloc_zeroed" => DynamicAllocZeroed { [U256 => MP] };
        DYNAMIC_ALLOC_ANY_BYTES "@malloc_uninit" => DynamicAllocAnyBytes { [U256 => MP] };

        // Memory Manipulation
        MEMORY_COPY "@mcopy" => MemoryCopy { [MP, MP, U256 => VOID] };
        MLOAD1 "@mload1" => MLoad1 { [MP => U256] };
        MLOAD2 "@mload2" => MLoad2 { [MP => U256] };
        MLOAD3 "@mload3" => MLoad3 { [MP => U256] };
        MLOAD4 "@mload4" => MLoad4 { [MP => U256] };
        MLOAD5 "@mload5" => MLoad5 { [MP => U256] };
        MLOAD6 "@mload6" => MLoad6 { [MP => U256] };
        MLOAD7 "@mload7" => MLoad7 { [MP => U256] };
        MLOAD8 "@mload8" => MLoad8 { [MP => U256] };
        MLOAD9 "@mload9" => MLoad9 { [MP => U256] };
        MLOAD10 "@mload10" => MLoad10 { [MP => U256] };
        MLOAD11 "@mload11" => MLoad11 { [MP => U256] };
        MLOAD12 "@mload12" => MLoad12 { [MP => U256] };
        MLOAD13 "@mload13" => MLoad13 { [MP => U256] };
        MLOAD14 "@mload14" => MLoad14 { [MP => U256] };
        MLOAD15 "@mload15" => MLoad15 { [MP => U256] };
        MLOAD16 "@mload16" => MLoad16 { [MP => U256] };
        MLOAD17 "@mload17" => MLoad17 { [MP => U256] };
        MLOAD18 "@mload18" => MLoad18 { [MP => U256] };
        MLOAD19 "@mload19" => MLoad19 { [MP => U256] };
        MLOAD20 "@mload20" => MLoad20 { [MP => U256] };
        MLOAD21 "@mload21" => MLoad21 { [MP => U256] };
        MLOAD22 "@mload22" => MLoad22 { [MP => U256] };
        MLOAD23 "@mload23" => MLoad23 { [MP => U256] };
        MLOAD24 "@mload24" => MLoad24 { [MP => U256] };
        MLOAD25 "@mload25" => MLoad25 { [MP => U256] };
        MLOAD26 "@mload26" => MLoad26 { [MP => U256] };
        MLOAD27 "@mload27" => MLoad27 { [MP => U256] };
        MLOAD28 "@mload28" => MLoad28 { [MP => U256] };
        MLOAD29 "@mload29" => MLoad29 { [MP => U256] };
        MLOAD30 "@mload30" => MLoad30 { [MP => U256] };
        MLOAD31 "@mload31" => MLoad31 { [MP => U256] };
        MLOAD32 "@mload32" => MLoad32 { [MP => U256] };
        MSTORE1 "@mstore1" => MStore1 { [MP, U256 => VOID] };
        MSTORE2 "@mstore2" => MStore2 { [MP, U256 => VOID] };
        MSTORE3 "@mstore3" => MStore3 { [MP, U256 => VOID] };
        MSTORE4 "@mstore4" => MStore4 { [MP, U256 => VOID] };
        MSTORE5 "@mstore5" => MStore5 { [MP, U256 => VOID] };
        MSTORE6 "@mstore6" => MStore6 { [MP, U256 => VOID] };
        MSTORE7 "@mstore7" => MStore7 { [MP, U256 => VOID] };
        MSTORE8 "@mstore8" => MStore8 { [MP, U256 => VOID] };
        MSTORE9 "@mstore9" => MStore9 { [MP, U256 => VOID] };
        MSTORE10 "@mstore10" => MStore10 { [MP, U256 => VOID] };
        MSTORE11 "@mstore11" => MStore11 { [MP, U256 => VOID] };
        MSTORE12 "@mstore12" => MStore12 { [MP, U256 => VOID] };
        MSTORE13 "@mstore13" => MStore13 { [MP, U256 => VOID] };
        MSTORE14 "@mstore14" => MStore14 { [MP, U256 => VOID] };
        MSTORE15 "@mstore15" => MStore15 { [MP, U256 => VOID] };
        MSTORE16 "@mstore16" => MStore16 { [MP, U256 => VOID] };
        MSTORE17 "@mstore17" => MStore17 { [MP, U256 => VOID] };
        MSTORE18 "@mstore18" => MStore18 { [MP, U256 => VOID] };
        MSTORE19 "@mstore19" => MStore19 { [MP, U256 => VOID] };
        MSTORE20 "@mstore20" => MStore20 { [MP, U256 => VOID] };
        MSTORE21 "@mstore21" => MStore21 { [MP, U256 => VOID] };
        MSTORE22 "@mstore22" => MStore22 { [MP, U256 => VOID] };
        MSTORE23 "@mstore23" => MStore23 { [MP, U256 => VOID] };
        MSTORE24 "@mstore24" => MStore24 { [MP, U256 => VOID] };
        MSTORE25 "@mstore25" => MStore25 { [MP, U256 => VOID] };
        MSTORE26 "@mstore26" => MStore26 { [MP, U256 => VOID] };
        MSTORE27 "@mstore27" => MStore27 { [MP, U256 => VOID] };
        MSTORE28 "@mstore28" => MStore28 { [MP, U256 => VOID] };
        MSTORE29 "@mstore29" => MStore29 { [MP, U256 => VOID] };
        MSTORE30 "@mstore30" => MStore30 { [MP, U256 => VOID] };
        MSTORE31 "@mstore31" => MStore31 { [MP, U256 => VOID] };
        MSTORE32 "@mstore32" => MStore32 { [MP, U256 => VOID] };

        // Bytecode Introspection
        RUNTIME_START_OFFSET "@runtime_start_offset" => RuntimeStartOffset { [=> U256] };
        INIT_END_OFFSET "@init_end_offset" => InitEndOffset { [=> U256] };
        RUNTIME_LENGTH "@runtime_length" => RuntimeLength { [=> U256] };
    }

    comptime_builtins {
        // Type Reflection
        IS_STRUCT "@is_struct" => IsStruct { [TYPE => BOOL] };
        FIELD_COUNT "@field_count" => FieldCount { [TYPE => U256] };
    }

    comptime_polymorphic_builtins {
        FIELD_TYPE "@field_type" => FieldType(2);
        GET_FIELD "@get_field" => GetField(3);
        SET_FIELD "@set_field" => SetField(4);
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
