use plank_session::{Builtin, RuntimeBuiltin, StrId, builtins::BuiltinKind};

use crate::TypeId;

#[derive(Debug, Clone, Copy)]
pub struct BuiltinSignature {
    pub inputs: &'static [TypeId],
    pub result: TypeId,
}

pub fn arg_count(builtin: Builtin) -> usize {
    match builtin.kind() {
        BuiltinKind::ComptimeDynamic { arg_count } => arg_count,
        _ => builtin_signatures(builtin)[0].inputs.len(),
    }
}

pub fn resolve_result_type(builtin: Builtin, arg_types: &[TypeId]) -> Option<TypeId> {
    let sigs = builtin_signatures(builtin);
    if sigs.is_empty() || sigs[0].inputs.len() != arg_types.len() {
        return None;
    }
    for sig in sigs {
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

pub fn builtin_signatures(builtin: Builtin) -> &'static [BuiltinSignature] {
    use Builtin as B;
    use RuntimeBuiltin as RB;

    const U256: TypeId = TypeId::U256;
    const BOOL: TypeId = TypeId::BOOL;
    const MP: TypeId = TypeId::MEMORY_POINTER;
    const VOID: TypeId = TypeId::VOID;
    const NEVER: TypeId = TypeId::NEVER;
    const TYPE: TypeId = TypeId::TYPE;

    match builtin {
        // Runtime foldable
        B::Runtime(RB::Add) => {
            &[sig!([U256, U256 => U256]), sig!([MP, U256 => MP]), sig!([U256, MP => MP])]
        }
        B::Runtime(RB::Mul) => &[sig!([U256, U256 => U256])],
        B::Runtime(RB::Sub) => {
            &[sig!([U256, U256 => U256]), sig!([MP, U256 => MP]), sig!([MP, MP => U256])]
        }
        B::Runtime(RB::Div) => &[sig!([U256, U256 => U256])],
        B::Runtime(RB::SDiv) => &[sig!([U256, U256 => U256])],
        B::Runtime(RB::Mod) => &[sig!([U256, U256 => U256])],
        B::Runtime(RB::SMod) => &[sig!([U256, U256 => U256])],
        B::Runtime(RB::AddMod) => &[sig!([U256, U256, U256 => U256])],
        B::Runtime(RB::MulMod) => &[sig!([U256, U256, U256 => U256])],
        B::Runtime(RB::Exp) => &[sig!([U256, U256 => U256])],
        B::Runtime(RB::SignExtend) => &[sig!([U256, U256 => U256])],
        B::Runtime(RB::Lt) => &[sig!([U256, U256 => BOOL]), sig!([MP, MP => BOOL])],
        B::Runtime(RB::Gt) => &[sig!([U256, U256 => BOOL]), sig!([MP, MP => BOOL])],
        B::Runtime(RB::SLt) => &[sig!([U256, U256 => BOOL])],
        B::Runtime(RB::SGt) => &[sig!([U256, U256 => BOOL])],
        B::Runtime(RB::Eq) => {
            &[sig!([U256, U256 => BOOL]), sig!([MP, MP => BOOL]), sig!([BOOL, BOOL => BOOL])]
        }
        B::Runtime(RB::IsZero) => &[sig!([U256 => BOOL])],
        B::Runtime(RB::And) => &[sig!([U256, U256 => U256]), sig!([BOOL, BOOL => BOOL])],
        B::Runtime(RB::Or) => &[sig!([U256, U256 => U256]), sig!([BOOL, BOOL => BOOL])],
        B::Runtime(RB::Xor) => &[sig!([U256, U256 => U256]), sig!([BOOL, BOOL => BOOL])],
        B::Runtime(RB::Not) => &[sig!([U256 => U256])],
        B::Runtime(RB::Byte) => &[sig!([U256, U256 => U256])],
        B::Runtime(RB::Shl) => &[sig!([U256, U256 => U256])],
        B::Runtime(RB::Shr) => &[sig!([U256, U256 => U256])],
        B::Runtime(RB::Sar) => &[sig!([U256, U256 => U256])],

        // Runtime only
        B::Runtime(RB::Keccak256) => &[sig!([MP, U256 => U256])],
        B::Runtime(RB::Address) => &[sig!([=> U256])],
        B::Runtime(RB::Balance) => &[sig!([U256 => U256])],
        B::Runtime(RB::Origin) => &[sig!([=> U256])],
        B::Runtime(RB::Caller) => &[sig!([=> U256])],
        B::Runtime(RB::CallValue) => &[sig!([=> U256])],
        B::Runtime(RB::CallDataLoad) => &[sig!([U256 => U256])],
        B::Runtime(RB::CallDataSize) => &[sig!([=> U256])],
        B::Runtime(RB::CallDataCopy) => &[sig!([MP, U256, U256 => VOID])],
        B::Runtime(RB::CodeSize) => &[sig!([=> U256])],
        B::Runtime(RB::CodeCopy) => &[sig!([MP, U256, U256 => VOID])],
        B::Runtime(RB::GasPrice) => &[sig!([=> U256])],
        B::Runtime(RB::ExtCodeSize) => &[sig!([U256 => U256])],
        B::Runtime(RB::ExtCodeCopy) => &[sig!([U256, MP, U256, U256 => VOID])],
        B::Runtime(RB::ReturnDataSize) => &[sig!([=> U256])],
        B::Runtime(RB::ReturnDataCopy) => &[sig!([MP, U256, U256 => VOID])],
        B::Runtime(RB::ExtCodeHash) => &[sig!([U256 => U256])],
        B::Runtime(RB::Gas) => &[sig!([=> U256])],
        B::Runtime(RB::BlockHash) => &[sig!([U256 => U256])],
        B::Runtime(RB::Coinbase) => &[sig!([=> U256])],
        B::Runtime(RB::Timestamp) => &[sig!([=> U256])],
        B::Runtime(RB::Number) => &[sig!([=> U256])],
        B::Runtime(RB::Difficulty) => &[sig!([=> U256])],
        B::Runtime(RB::GasLimit) => &[sig!([=> U256])],
        B::Runtime(RB::ChainId) => &[sig!([=> U256])],
        B::Runtime(RB::SelfBalance) => &[sig!([=> U256])],
        B::Runtime(RB::BaseFee) => &[sig!([=> U256])],
        B::Runtime(RB::BlobHash) => &[sig!([U256 => U256])],
        B::Runtime(RB::BlobBaseFee) => &[sig!([=> U256])],
        B::Runtime(RB::SLoad) => &[sig!([U256 => U256])],
        B::Runtime(RB::SStore) => &[sig!([U256, U256 => VOID])],
        B::Runtime(RB::TLoad) => &[sig!([U256 => U256])],
        B::Runtime(RB::TStore) => &[sig!([U256, U256 => VOID])],
        B::Runtime(RB::Log0) => &[sig!([MP, U256 => VOID])],
        B::Runtime(RB::Log1) => &[sig!([MP, U256, U256 => VOID])],
        B::Runtime(RB::Log2) => &[sig!([MP, U256, U256, U256 => VOID])],
        B::Runtime(RB::Log3) => &[sig!([MP, U256, U256, U256, U256 => VOID])],
        B::Runtime(RB::Log4) => &[sig!([MP, U256, U256, U256, U256, U256 => VOID])],
        B::Runtime(RB::Create) => &[sig!([U256, MP, U256 => U256])],
        B::Runtime(RB::Create2) => &[sig!([U256, MP, U256, U256 => U256])],
        B::Runtime(RB::Call) => &[sig!([U256, U256, U256, MP, U256, MP, U256 => BOOL])],
        B::Runtime(RB::CallCode) => &[sig!([U256, U256, U256, MP, U256, MP, U256 => BOOL])],
        B::Runtime(RB::DelegateCall) => &[sig!([U256, U256, MP, U256, MP, U256 => BOOL])],
        B::Runtime(RB::StaticCall) => &[sig!([U256, U256, MP, U256, MP, U256 => BOOL])],
        B::Runtime(RB::Return) => &[sig!([MP, U256 => NEVER])],
        B::Runtime(RB::Stop) => &[sig!([=> NEVER])],
        B::Runtime(RB::Revert) => &[sig!([MP, U256 => NEVER])],
        B::Runtime(RB::Invalid) => &[sig!([=> NEVER])],
        B::Runtime(RB::SelfDestruct) => &[sig!([U256 => NEVER])],
        B::Runtime(RB::DynamicAllocZeroed) => &[sig!([U256 => MP])],
        B::Runtime(RB::DynamicAllocAnyBytes) => &[sig!([U256 => MP])],
        B::Runtime(RB::MemoryCopy) => &[sig!([MP, MP, U256 => VOID])],
        B::Runtime(RB::MLoad1) => &[sig!([MP => U256])],
        B::Runtime(RB::MLoad2) => &[sig!([MP => U256])],
        B::Runtime(RB::MLoad3) => &[sig!([MP => U256])],
        B::Runtime(RB::MLoad4) => &[sig!([MP => U256])],
        B::Runtime(RB::MLoad5) => &[sig!([MP => U256])],
        B::Runtime(RB::MLoad6) => &[sig!([MP => U256])],
        B::Runtime(RB::MLoad7) => &[sig!([MP => U256])],
        B::Runtime(RB::MLoad8) => &[sig!([MP => U256])],
        B::Runtime(RB::MLoad9) => &[sig!([MP => U256])],
        B::Runtime(RB::MLoad10) => &[sig!([MP => U256])],
        B::Runtime(RB::MLoad11) => &[sig!([MP => U256])],
        B::Runtime(RB::MLoad12) => &[sig!([MP => U256])],
        B::Runtime(RB::MLoad13) => &[sig!([MP => U256])],
        B::Runtime(RB::MLoad14) => &[sig!([MP => U256])],
        B::Runtime(RB::MLoad15) => &[sig!([MP => U256])],
        B::Runtime(RB::MLoad16) => &[sig!([MP => U256])],
        B::Runtime(RB::MLoad17) => &[sig!([MP => U256])],
        B::Runtime(RB::MLoad18) => &[sig!([MP => U256])],
        B::Runtime(RB::MLoad19) => &[sig!([MP => U256])],
        B::Runtime(RB::MLoad20) => &[sig!([MP => U256])],
        B::Runtime(RB::MLoad21) => &[sig!([MP => U256])],
        B::Runtime(RB::MLoad22) => &[sig!([MP => U256])],
        B::Runtime(RB::MLoad23) => &[sig!([MP => U256])],
        B::Runtime(RB::MLoad24) => &[sig!([MP => U256])],
        B::Runtime(RB::MLoad25) => &[sig!([MP => U256])],
        B::Runtime(RB::MLoad26) => &[sig!([MP => U256])],
        B::Runtime(RB::MLoad27) => &[sig!([MP => U256])],
        B::Runtime(RB::MLoad28) => &[sig!([MP => U256])],
        B::Runtime(RB::MLoad29) => &[sig!([MP => U256])],
        B::Runtime(RB::MLoad30) => &[sig!([MP => U256])],
        B::Runtime(RB::MLoad31) => &[sig!([MP => U256])],
        B::Runtime(RB::MLoad32) => &[sig!([MP => U256])],
        B::Runtime(RB::MStore1) => &[sig!([MP, U256 => VOID])],
        B::Runtime(RB::MStore2) => &[sig!([MP, U256 => VOID])],
        B::Runtime(RB::MStore3) => &[sig!([MP, U256 => VOID])],
        B::Runtime(RB::MStore4) => &[sig!([MP, U256 => VOID])],
        B::Runtime(RB::MStore5) => &[sig!([MP, U256 => VOID])],
        B::Runtime(RB::MStore6) => &[sig!([MP, U256 => VOID])],
        B::Runtime(RB::MStore7) => &[sig!([MP, U256 => VOID])],
        B::Runtime(RB::MStore8) => &[sig!([MP, U256 => VOID])],
        B::Runtime(RB::MStore9) => &[sig!([MP, U256 => VOID])],
        B::Runtime(RB::MStore10) => &[sig!([MP, U256 => VOID])],
        B::Runtime(RB::MStore11) => &[sig!([MP, U256 => VOID])],
        B::Runtime(RB::MStore12) => &[sig!([MP, U256 => VOID])],
        B::Runtime(RB::MStore13) => &[sig!([MP, U256 => VOID])],
        B::Runtime(RB::MStore14) => &[sig!([MP, U256 => VOID])],
        B::Runtime(RB::MStore15) => &[sig!([MP, U256 => VOID])],
        B::Runtime(RB::MStore16) => &[sig!([MP, U256 => VOID])],
        B::Runtime(RB::MStore17) => &[sig!([MP, U256 => VOID])],
        B::Runtime(RB::MStore18) => &[sig!([MP, U256 => VOID])],
        B::Runtime(RB::MStore19) => &[sig!([MP, U256 => VOID])],
        B::Runtime(RB::MStore20) => &[sig!([MP, U256 => VOID])],
        B::Runtime(RB::MStore21) => &[sig!([MP, U256 => VOID])],
        B::Runtime(RB::MStore22) => &[sig!([MP, U256 => VOID])],
        B::Runtime(RB::MStore23) => &[sig!([MP, U256 => VOID])],
        B::Runtime(RB::MStore24) => &[sig!([MP, U256 => VOID])],
        B::Runtime(RB::MStore25) => &[sig!([MP, U256 => VOID])],
        B::Runtime(RB::MStore26) => &[sig!([MP, U256 => VOID])],
        B::Runtime(RB::MStore27) => &[sig!([MP, U256 => VOID])],
        B::Runtime(RB::MStore28) => &[sig!([MP, U256 => VOID])],
        B::Runtime(RB::MStore29) => &[sig!([MP, U256 => VOID])],
        B::Runtime(RB::MStore30) => &[sig!([MP, U256 => VOID])],
        B::Runtime(RB::MStore31) => &[sig!([MP, U256 => VOID])],
        B::Runtime(RB::MStore32) => &[sig!([MP, U256 => VOID])],
        B::Runtime(RB::RuntimeStartOffset) => &[sig!([=> U256])],
        B::Runtime(RB::InitEndOffset) => &[sig!([=> U256])],
        B::Runtime(RB::RuntimeLength) => &[sig!([=> U256])],

        // Comptime builtins
        B::IsStruct => &[sig!([TYPE => BOOL])],
        B::FieldCount => &[sig!([TYPE => U256])],
        B::InComptime => &[sig!([=> BOOL])],

        // Comptime dynamic — no fixed signatures
        B::FieldType | B::GetField | B::SetField | B::Uninit => &[],
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
        for &builtin in Builtin::ALL {
            let sigs = builtin_signatures(builtin);
            if matches!(builtin.kind(), BuiltinKind::ComptimeDynamic { .. }) {
                continue;
            }
            assert!(!sigs.is_empty(), "{builtin:?} has no signatures");
            let ac = sigs[0].inputs.len();
            for sig in sigs {
                assert_eq!(sig.inputs.len(), ac, "{builtin:?} has inconsistent arg counts");
            }
        }
    }

    #[test]
    fn test_comptime_dynamic_builtin_has_no_signatures() {
        for &builtin in Builtin::ALL {
            let sigs = builtin_signatures(builtin);
            if !matches!(builtin.kind(), BuiltinKind::ComptimeDynamic { .. }) {
                continue;
            }
            assert!(sigs.is_empty(), "dynamic builtin {builtin:?} has signatures");
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
            resolve_result_type(Builtin::ADD, &[TypeId::U256, TypeId::U256]),
            Some(TypeId::U256)
        );
        assert_eq!(resolve_result_type(Builtin::ADD, &[TypeId::U256]), None);
    }
}
