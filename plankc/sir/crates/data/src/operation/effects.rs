use crate::{FunctionId, Operation};

use super::OperationKind;

bitflags::bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
    pub struct Effect: u16 {
        const PURE             = 0;
        const MEMORY_READ      = 1 <<  0;
        const MEMORY_WRITE     = 1 <<  1;
        const RETURNDATA_READ  = 1 <<  2;
        const RETURNDATA_WRITE = 1 <<  3;
        const ACCOUNTS_READ    = 1 <<  4;
        const ACCOUNTS_WRITE   = 1 <<  5;
        const PERSISTENT_READ  = 1 <<  6;
        const PERSISTENT_WRITE = 1 <<  7;
        const TRANSIENT_READ   = 1 <<  8;
        const TRANSIENT_WRITE  = 1 <<  9;
        const REVERT           = 1 << 10;
        const TERMINATE        = 1 << 11;
        const ALLOC_ADVANCE    = 1 << 12;
        const ALLOC_USE_FREE   = 1 << 13;
        const LOGS             = 1 << 14;

        const EXTCALL = Effect::ACCOUNTS_WRITE.bits()
            | Effect::PERSISTENT_WRITE.bits()
            | Effect::TRANSIENT_WRITE.bits()
            | Effect::LOGS.bits()
            | Effect::RETURNDATA_WRITE.bits();

        const MINOR =
            Effect::MEMORY_READ.bits() |
            Effect::RETURNDATA_READ.bits() |
            Effect::ACCOUNTS_READ.bits() |
            Effect::PERSISTENT_READ.bits() |
            Effect::TRANSIENT_READ.bits() |
            Effect::REVERT.bits() |
            Effect::ALLOC_ADVANCE.bits();

        const MAJOR =
            Effect::MEMORY_WRITE.bits() |
            Effect::RETURNDATA_WRITE.bits() |
            Effect::ACCOUNTS_WRITE.bits() |
            Effect::PERSISTENT_WRITE.bits() |
            Effect::TRANSIENT_WRITE.bits() |
            Effect::TERMINATE.bits() |
            Effect::ALLOC_USE_FREE.bits();
    }
}

impl Effect {
    pub fn is_simple(self) -> bool {
        self.simplify() == self
    }

    pub fn simplify(mut self) -> Effect {
        let reads_and_writes = (self & Effect::MINOR) & Effect::from_bits_retain(self.bits() >> 1);
        self.remove(reads_and_writes);
        self
    }

    pub fn of(op: Operation) -> Result<Effect, FunctionId> {
        let op = match op {
            Operation::InternalCall(icall) => return Err(icall.function),
            op => op.kind(),
        };

        let e = match op {
            OperationKind::InternalCall => unreachable!("icall checked above"),
            OperationKind::Add
            | OperationKind::Mul
            | OperationKind::Sub
            | OperationKind::Div
            | OperationKind::SDiv
            | OperationKind::Mod
            | OperationKind::SMod
            | OperationKind::AddMod
            | OperationKind::MulMod
            | OperationKind::Exp
            | OperationKind::SignExtend
            | OperationKind::Lt
            | OperationKind::Gt
            | OperationKind::SLt
            | OperationKind::SGt
            | OperationKind::Eq
            | OperationKind::IsZero
            | OperationKind::And
            | OperationKind::Or
            | OperationKind::Xor
            | OperationKind::Not
            | OperationKind::Byte
            | OperationKind::Shl
            | OperationKind::Shr
            | OperationKind::Sar
            | OperationKind::Clz => Effect::PURE,

            OperationKind::Keccak256 => Effect::MEMORY_READ,

            OperationKind::Address => Effect::PURE,
            OperationKind::Balance => Effect::ACCOUNTS_READ,
            OperationKind::Origin => Effect::PURE,
            OperationKind::Caller => Effect::PURE,
            OperationKind::CallValue => Effect::PURE,
            OperationKind::CallDataLoad => Effect::PURE,
            OperationKind::CallDataSize => Effect::PURE,
            OperationKind::CallDataCopy => Effect::MEMORY_WRITE,
            OperationKind::CodeSize => Effect::PURE,
            OperationKind::CodeCopy => Effect::MEMORY_WRITE,
            OperationKind::GasPrice => Effect::PURE,
            OperationKind::ExtCodeSize => Effect::ACCOUNTS_READ,
            OperationKind::ExtCodeCopy => Effect::ACCOUNTS_READ | Effect::MEMORY_WRITE,
            OperationKind::ReturnDataSize => Effect::RETURNDATA_READ,
            // `returndatacopy` reverts for out of bound returndata copies so it may also
            // `Effect::REVERT`
            OperationKind::ReturnDataCopy => {
                Effect::RETURNDATA_READ | Effect::MEMORY_WRITE | Effect::REVERT
            }
            OperationKind::ExtCodeHash => Effect::ACCOUNTS_READ,

            // Want `gas` to remain ordered relative to storage reads and operations such as
            // `excodesize` that might want to be observed due to "cold"/"warm" account costs.
            OperationKind::Gas => Effect::ACCOUNTS_WRITE | Effect::PERSISTENT_WRITE | Effect::LOGS,

            OperationKind::BlockHash => Effect::PURE,
            OperationKind::Coinbase => Effect::PURE,
            OperationKind::Timestamp => Effect::PURE,
            OperationKind::Number => Effect::PURE,
            OperationKind::Difficulty => Effect::PURE,
            OperationKind::GasLimit => Effect::PURE,
            OperationKind::ChainId => Effect::PURE,
            OperationKind::SelfBalance => Effect::ACCOUNTS_READ,
            OperationKind::BaseFee => Effect::PURE,
            OperationKind::BlobHash => Effect::PURE,
            OperationKind::BlobBaseFee => Effect::PURE,

            OperationKind::SLoad => Effect::PERSISTENT_READ,
            OperationKind::SStore => Effect::PERSISTENT_WRITE,
            OperationKind::TLoad => Effect::TRANSIENT_READ,
            OperationKind::TStore => Effect::TRANSIENT_WRITE,

            OperationKind::Log0
            | OperationKind::Log1
            | OperationKind::Log2
            | OperationKind::Log3
            | OperationKind::Log4 => Effect::MEMORY_READ | Effect::LOGS,

            OperationKind::Create => Effect::EXTCALL | Effect::MEMORY_READ,
            OperationKind::Create2 => Effect::EXTCALL | Effect::MEMORY_READ,
            OperationKind::Call => Effect::EXTCALL | Effect::MEMORY_WRITE,
            OperationKind::CallCode => Effect::EXTCALL | Effect::MEMORY_WRITE,
            OperationKind::DelegateCall => Effect::EXTCALL | Effect::MEMORY_WRITE,
            OperationKind::StaticCall => {
                Effect::ACCOUNTS_READ
                    | Effect::PERSISTENT_READ
                    | Effect::TRANSIENT_READ
                    | Effect::MEMORY_WRITE
                    | Effect::RETURNDATA_WRITE
            }

            OperationKind::Return => Effect::TERMINATE | Effect::MEMORY_READ,
            OperationKind::Stop | OperationKind::SelfDestruct => Effect::TERMINATE,

            // Unlike `RETURN`, `STOP` and `SELFDESTRUCT` these opcodes rollback any effects and
            // can therefore technically be reordered
            OperationKind::Revert => Effect::MEMORY_READ | Effect::REVERT,
            OperationKind::Invalid => Effect::REVERT,

            OperationKind::AcquireFreePointer => Effect::ALLOC_USE_FREE,
            OperationKind::DynamicAllocZeroed | OperationKind::DynamicAllocAnyBytes => {
                Effect::ALLOC_ADVANCE
            }
            OperationKind::StaticAllocZeroed | OperationKind::StaticAllocAnyBytes => Effect::PURE,

            OperationKind::MemoryCopy => Effect::MEMORY_WRITE,
            OperationKind::MemoryLoad => Effect::MEMORY_READ,
            OperationKind::MemoryStore => Effect::MEMORY_WRITE,

            OperationKind::SetCopy
            | OperationKind::SetSmallConst
            | OperationKind::SetLargeConst
            | OperationKind::SetDataOffset
            | OperationKind::Noop
            | OperationKind::RuntimeStartOffset
            | OperationKind::InitEndOffset
            | OperationKind::RuntimeLength => Effect::PURE,
        };

        Ok(e)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn any_effect() -> impl Strategy<Value = Effect> {
        any::<u16>().prop_map(Effect::from_bits_truncate)
    }

    fn naive_simplify(mut effect: Effect) -> Effect {
        for (minor, major) in [
            (Effect::MEMORY_READ, Effect::MEMORY_WRITE),
            (Effect::RETURNDATA_READ, Effect::RETURNDATA_WRITE),
            (Effect::ACCOUNTS_READ, Effect::ACCOUNTS_WRITE),
            (Effect::PERSISTENT_READ, Effect::PERSISTENT_WRITE),
            (Effect::TRANSIENT_READ, Effect::TRANSIENT_WRITE),
            (Effect::REVERT, Effect::TERMINATE),
            (Effect::ALLOC_ADVANCE, Effect::ALLOC_USE_FREE),
        ] {
            if effect.contains(minor | major) {
                effect.remove(minor);
            }
        }

        effect
    }

    proptest! {
        #[test]
        fn simplify_matches_naive(effect in any_effect()) {
            prop_assert_eq!(effect.simplify(), naive_simplify(effect));
        }
    }

    #[test]
    fn is_simple() {
        assert!(!(Effect::MEMORY_WRITE | Effect::MEMORY_READ | Effect::LOGS).is_simple());
        assert!((Effect::MEMORY_WRITE | Effect::TERMINATE | Effect::PERSISTENT_READ).is_simple());
    }
}
