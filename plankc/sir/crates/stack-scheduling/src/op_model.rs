use sir_data::operation::OperationKind;

pub fn is_flippable(op: OperationKind) -> bool {
    match op {
        OperationKind::Add
        | OperationKind::Mul
        | OperationKind::AddMod
        | OperationKind::MulMod
        | OperationKind::Lt
        | OperationKind::Gt
        | OperationKind::SLt
        | OperationKind::SGt
        | OperationKind::Eq
        | OperationKind::And
        | OperationKind::Or
        | OperationKind::Xor => true,

        OperationKind::Sub
        | OperationKind::Div
        | OperationKind::SDiv
        | OperationKind::Mod
        | OperationKind::SMod
        | OperationKind::Exp
        | OperationKind::SignExtend
        | OperationKind::IsZero
        | OperationKind::Not
        | OperationKind::Byte
        | OperationKind::Shl
        | OperationKind::Shr
        | OperationKind::Sar
        | OperationKind::Keccak256
        | OperationKind::Address
        | OperationKind::Balance
        | OperationKind::Origin
        | OperationKind::Caller
        | OperationKind::CallValue
        | OperationKind::CallDataLoad
        | OperationKind::CallDataSize
        | OperationKind::CallDataCopy
        | OperationKind::CodeSize
        | OperationKind::CodeCopy
        | OperationKind::GasPrice
        | OperationKind::ExtCodeSize
        | OperationKind::ExtCodeCopy
        | OperationKind::ReturnDataSize
        | OperationKind::ReturnDataCopy
        | OperationKind::ExtCodeHash
        | OperationKind::Gas
        | OperationKind::BlockHash
        | OperationKind::Coinbase
        | OperationKind::Timestamp
        | OperationKind::Number
        | OperationKind::Difficulty
        | OperationKind::GasLimit
        | OperationKind::ChainId
        | OperationKind::SelfBalance
        | OperationKind::BaseFee
        | OperationKind::BlobHash
        | OperationKind::BlobBaseFee
        | OperationKind::SLoad
        | OperationKind::SStore
        | OperationKind::TLoad
        | OperationKind::TStore
        | OperationKind::Log0
        | OperationKind::Log1
        | OperationKind::Log2
        | OperationKind::Log3
        | OperationKind::Log4
        | OperationKind::Create
        | OperationKind::Create2
        | OperationKind::Call
        | OperationKind::CallCode
        | OperationKind::DelegateCall
        | OperationKind::StaticCall
        | OperationKind::Return
        | OperationKind::Stop
        | OperationKind::Revert
        | OperationKind::Invalid
        | OperationKind::SelfDestruct
        | OperationKind::DynamicAllocZeroed
        | OperationKind::DynamicAllocAnyBytes
        | OperationKind::AcquireFreePointer
        | OperationKind::StaticAllocZeroed
        | OperationKind::StaticAllocAnyBytes
        | OperationKind::MemoryCopy
        | OperationKind::MemoryLoad
        | OperationKind::MemoryStore
        | OperationKind::SetCopy
        | OperationKind::SetSmallConst
        | OperationKind::SetLargeConst
        | OperationKind::SetDataOffset
        | OperationKind::Noop
        | OperationKind::InternalCall
        | OperationKind::RuntimeStartOffset
        | OperationKind::InitEndOffset
        | OperationKind::RuntimeLength => false,
    }
}
