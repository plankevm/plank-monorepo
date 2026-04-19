use plank_session::{EvmBuiltin, StrId};

use crate::TypeId;

#[derive(Debug, Clone, Copy)]
pub struct BuiltinSignature {
    pub inputs: &'static [TypeId],
    pub result: TypeId,
}

pub fn arg_count(builtin: EvmBuiltin) -> usize {
    builtin_signatures(builtin)[0].inputs.len()
}

pub fn resolve_result_type(builtin: EvmBuiltin, arg_types: &[TypeId]) -> Option<TypeId> {
    if arg_count(builtin) != arg_types.len() {
        return None;
    }
    for &sig in builtin_signatures(builtin) {
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

macro_rules! sig {
    ([$($arg:ident),* => $ret:ident]) => {
        BuiltinSignature {
            inputs: &[$($arg),*],
            result: $ret,
        }
    };
}

pub fn builtin_signatures(builtin: EvmBuiltin) -> &'static [BuiltinSignature] {
    use EvmBuiltin::*;
    const U256: TypeId = TypeId::U256;
    const BOOL: TypeId = TypeId::BOOL;
    const MP: TypeId = TypeId::MEMORY_POINTER;
    const VOID: TypeId = TypeId::VOID;
    const NEVER: TypeId = TypeId::NEVER;

    match builtin {
        Add => &[sig!([U256, U256 => U256]), sig!([MP, U256 => MP]), sig!([U256, MP => MP])],
        Mul => &[sig!([U256, U256 => U256])],
        Sub => &[sig!([U256, U256 => U256]), sig!([MP, U256 => MP]), sig!([MP, MP => U256])],
        Div => &[sig!([U256, U256 => U256])],
        SDiv => &[sig!([U256, U256 => U256])],
        Mod => &[sig!([U256, U256 => U256])],
        SMod => &[sig!([U256, U256 => U256])],
        AddMod => &[sig!([U256, U256, U256 => U256])],
        MulMod => &[sig!([U256, U256, U256 => U256])],
        Exp => &[sig!([U256, U256 => U256])],
        SignExtend => &[sig!([U256, U256 => U256])],
        Lt => &[sig!([U256, U256 => BOOL]), sig!([MP, MP => BOOL])],
        Gt => &[sig!([U256, U256 => BOOL]), sig!([MP, MP => BOOL])],
        SLt => &[sig!([U256, U256 => BOOL])],
        SGt => &[sig!([U256, U256 => BOOL])],
        Eq => &[sig!([U256, U256 => BOOL]), sig!([MP, MP => BOOL])],
        IsZero => &[sig!([U256 => BOOL])],
        And => &[sig!([U256, U256 => U256])],
        Or => &[sig!([U256, U256 => U256])],
        Xor => &[sig!([U256, U256 => U256])],
        Not => &[sig!([U256 => U256])],
        Byte => &[sig!([U256, U256 => U256])],
        Shl => &[sig!([U256, U256 => U256])],
        Shr => &[sig!([U256, U256 => U256])],
        Sar => &[sig!([U256, U256 => U256])],
        Keccak256 => &[sig!([MP, U256 => U256])],
        Address => &[sig!([=> U256])],
        Balance => &[sig!([U256 => U256])],
        Origin => &[sig!([=> U256])],
        Caller => &[sig!([=> U256])],
        CallValue => &[sig!([=> U256])],
        CallDataLoad => &[sig!([U256 => U256])],
        CallDataSize => &[sig!([=> U256])],
        CallDataCopy => &[sig!([MP, U256, U256 => VOID])],
        CodeSize => &[sig!([=> U256])],
        CodeCopy => &[sig!([MP, U256, U256 => VOID])],
        GasPrice => &[sig!([=> U256])],
        ExtCodeSize => &[sig!([U256 => U256])],
        ExtCodeCopy => &[sig!([U256, MP, U256, U256 => VOID])],
        ReturnDataSize => &[sig!([=> U256])],
        ReturnDataCopy => &[sig!([MP, U256, U256 => VOID])],
        ExtCodeHash => &[sig!([U256 => U256])],
        Gas => &[sig!([=> U256])],
        BlockHash => &[sig!([U256 => U256])],
        Coinbase => &[sig!([=> U256])],
        Timestamp => &[sig!([=> U256])],
        Number => &[sig!([=> U256])],
        Difficulty => &[sig!([=> U256])],
        GasLimit => &[sig!([=> U256])],
        ChainId => &[sig!([=> U256])],
        SelfBalance => &[sig!([=> U256])],
        BaseFee => &[sig!([=> U256])],
        BlobHash => &[sig!([U256 => U256])],
        BlobBaseFee => &[sig!([=> U256])],
        SLoad => &[sig!([U256 => U256])],
        SStore => &[sig!([U256, U256 => VOID])],
        TLoad => &[sig!([U256 => U256])],
        TStore => &[sig!([U256, U256 => VOID])],
        Log0 => &[sig!([MP, U256 => VOID])],
        Log1 => &[sig!([MP, U256, U256 => VOID])],
        Log2 => &[sig!([MP, U256, U256, U256 => VOID])],
        Log3 => &[sig!([MP, U256, U256, U256, U256 => VOID])],
        Log4 => &[sig!([MP, U256, U256, U256, U256, U256 => VOID])],
        Create => &[sig!([U256, MP, U256 => U256])],
        Create2 => &[sig!([U256, MP, U256, U256 => U256])],
        Call => &[sig!([U256, U256, U256, MP, U256, MP, U256 => BOOL])],
        CallCode => &[sig!([U256, U256, U256, MP, U256, MP, U256 => BOOL])],
        DelegateCall => &[sig!([U256, U256, MP, U256, MP, U256 => BOOL])],
        StaticCall => &[sig!([U256, U256, MP, U256, MP, U256 => BOOL])],
        Return => &[sig!([MP, U256 => NEVER])],
        Stop => &[sig!([=> NEVER])],
        Revert => &[sig!([MP, U256 => NEVER])],
        Invalid => &[sig!([=> NEVER])],
        SelfDestruct => &[sig!([U256 => NEVER])],
        DynamicAllocZeroed => &[sig!([U256 => MP])],
        DynamicAllocAnyBytes => &[sig!([U256 => MP])],
        MemoryCopy => &[sig!([MP, MP, U256 => VOID])],
        MLoad1 => &[sig!([MP => U256])],
        MLoad2 => &[sig!([MP => U256])],
        MLoad3 => &[sig!([MP => U256])],
        MLoad4 => &[sig!([MP => U256])],
        MLoad5 => &[sig!([MP => U256])],
        MLoad6 => &[sig!([MP => U256])],
        MLoad7 => &[sig!([MP => U256])],
        MLoad8 => &[sig!([MP => U256])],
        MLoad9 => &[sig!([MP => U256])],
        MLoad10 => &[sig!([MP => U256])],
        MLoad11 => &[sig!([MP => U256])],
        MLoad12 => &[sig!([MP => U256])],
        MLoad13 => &[sig!([MP => U256])],
        MLoad14 => &[sig!([MP => U256])],
        MLoad15 => &[sig!([MP => U256])],
        MLoad16 => &[sig!([MP => U256])],
        MLoad17 => &[sig!([MP => U256])],
        MLoad18 => &[sig!([MP => U256])],
        MLoad19 => &[sig!([MP => U256])],
        MLoad20 => &[sig!([MP => U256])],
        MLoad21 => &[sig!([MP => U256])],
        MLoad22 => &[sig!([MP => U256])],
        MLoad23 => &[sig!([MP => U256])],
        MLoad24 => &[sig!([MP => U256])],
        MLoad25 => &[sig!([MP => U256])],
        MLoad26 => &[sig!([MP => U256])],
        MLoad27 => &[sig!([MP => U256])],
        MLoad28 => &[sig!([MP => U256])],
        MLoad29 => &[sig!([MP => U256])],
        MLoad30 => &[sig!([MP => U256])],
        MLoad31 => &[sig!([MP => U256])],
        MLoad32 => &[sig!([MP => U256])],
        MStore1 => &[sig!([MP, U256 => VOID])],
        MStore2 => &[sig!([MP, U256 => VOID])],
        MStore3 => &[sig!([MP, U256 => VOID])],
        MStore4 => &[sig!([MP, U256 => VOID])],
        MStore5 => &[sig!([MP, U256 => VOID])],
        MStore6 => &[sig!([MP, U256 => VOID])],
        MStore7 => &[sig!([MP, U256 => VOID])],
        MStore8 => &[sig!([MP, U256 => VOID])],
        MStore9 => &[sig!([MP, U256 => VOID])],
        MStore10 => &[sig!([MP, U256 => VOID])],
        MStore11 => &[sig!([MP, U256 => VOID])],
        MStore12 => &[sig!([MP, U256 => VOID])],
        MStore13 => &[sig!([MP, U256 => VOID])],
        MStore14 => &[sig!([MP, U256 => VOID])],
        MStore15 => &[sig!([MP, U256 => VOID])],
        MStore16 => &[sig!([MP, U256 => VOID])],
        MStore17 => &[sig!([MP, U256 => VOID])],
        MStore18 => &[sig!([MP, U256 => VOID])],
        MStore19 => &[sig!([MP, U256 => VOID])],
        MStore20 => &[sig!([MP, U256 => VOID])],
        MStore21 => &[sig!([MP, U256 => VOID])],
        MStore22 => &[sig!([MP, U256 => VOID])],
        MStore23 => &[sig!([MP, U256 => VOID])],
        MStore24 => &[sig!([MP, U256 => VOID])],
        MStore25 => &[sig!([MP, U256 => VOID])],
        MStore26 => &[sig!([MP, U256 => VOID])],
        MStore27 => &[sig!([MP, U256 => VOID])],
        MStore28 => &[sig!([MP, U256 => VOID])],
        MStore29 => &[sig!([MP, U256 => VOID])],
        MStore30 => &[sig!([MP, U256 => VOID])],
        MStore31 => &[sig!([MP, U256 => VOID])],
        MStore32 => &[sig!([MP, U256 => VOID])],
        RuntimeStartOffset => &[sig!([=> U256])],
        InitEndOffset => &[sig!([=> U256])],
        RuntimeLength => &[sig!([=> U256])],
    }
}

impl TypeId {
    pub fn resolve_primitive(name: StrId) -> Option<TypeId> {
        use plank_session::builtins::*;
        Some(match name {
            VOID => TypeId::VOID,
            U256 => TypeId::U256,
            BOOL => TypeId::BOOL,
            MEMORY_POINTER => TypeId::MEMORY_POINTER,
            TYPE => TypeId::TYPE,
            FUNCTION => TypeId::FUNCTION,
            NEVER => TypeId::NEVER,
            _ => return None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builtin_signatures_not_empty() {
        for builtin in enum_iterator::all::<EvmBuiltin>() {
            let sigs = builtin_signatures(builtin);
            assert!(!sigs.is_empty(), "{builtin:?} has no signatures");
            let arg_count = sigs[0].inputs.len();
            for sig in sigs {
                assert_eq!(sig.inputs.len(), arg_count, "{builtin:?} has inconsistent arg counts");
            }
        }
    }

    #[test]
    fn test_resolve_primitive() {
        use plank_session::builtins::*;
        assert_eq!(TypeId::resolve_primitive(VOID), Some(TypeId::VOID));
        assert_eq!(TypeId::resolve_primitive(U256), Some(TypeId::U256));
        assert_eq!(TypeId::resolve_primitive(ADD), None);
    }

    #[test]
    fn test_resolve_result_type() {
        assert_eq!(
            resolve_result_type(EvmBuiltin::Add, &[TypeId::U256, TypeId::U256]),
            Some(TypeId::U256)
        );
        assert_eq!(resolve_result_type(EvmBuiltin::Add, &[TypeId::U256]), None);
    }
}
