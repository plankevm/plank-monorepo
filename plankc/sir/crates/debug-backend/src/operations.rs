use sir_assembler::op;
use sir_data::operation::OperationKind;

pub(crate) fn op_kind_to_direct_op(kind: OperationKind) -> Option<u8> {
    let evm_op = match kind {
        OperationKind::DynamicAllocZeroed
        | OperationKind::DynamicAllocAnyBytes
        | OperationKind::AcquireFreePointer
        | OperationKind::StaticAllocZeroed
        | OperationKind::StaticAllocAnyBytes
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
        | OperationKind::RuntimeLength => return None,

        // ========== EVM Arithmetic ==========
        OperationKind::Add => op::ADD,
        OperationKind::Mul => op::MUL,
        OperationKind::Sub => op::SUB,
        OperationKind::Div => op::DIV,
        OperationKind::SDiv => op::SDIV,
        OperationKind::Mod => op::MOD,
        OperationKind::SMod => op::SMOD,
        OperationKind::AddMod => op::ADDMOD,
        OperationKind::MulMod => op::MULMOD,
        OperationKind::Exp => op::EXP,
        OperationKind::SignExtend => op::SIGNEXTEND,

        // ========== EVM Comparison & Bitwise Logic ==========
        OperationKind::Lt => op::LT,
        OperationKind::Gt => op::GT,
        OperationKind::SLt => op::SLT,
        OperationKind::SGt => op::SGT,
        OperationKind::Eq => op::EQ,
        OperationKind::IsZero => op::ISZERO,
        OperationKind::And => op::AND,
        OperationKind::Or => op::OR,
        OperationKind::Xor => op::XOR,
        OperationKind::Not => op::NOT,
        OperationKind::Byte => op::BYTE,
        OperationKind::Shl => op::SHL,
        OperationKind::Shr => op::SHR,
        OperationKind::Sar => op::SAR,

        // ========== EVM Keccak-256 ==========
        OperationKind::Keccak256 => op::KECCAK256,

        // ========== EVM Environment Information ==========
        OperationKind::Address => op::ADDRESS,
        OperationKind::Balance => op::BALANCE,
        OperationKind::Origin => op::ORIGIN,
        OperationKind::Caller => op::CALLER,
        OperationKind::CallValue => op::CALLVALUE,
        OperationKind::CallDataLoad => op::CALLDATALOAD,
        OperationKind::CallDataSize => op::CALLDATASIZE,
        OperationKind::CallDataCopy => op::CALLDATACOPY,
        OperationKind::CodeSize => op::CODESIZE,
        OperationKind::CodeCopy => op::CODECOPY,
        OperationKind::GasPrice => op::GASPRICE,
        OperationKind::ExtCodeSize => op::EXTCODESIZE,
        OperationKind::ExtCodeCopy => op::EXTCODECOPY,
        OperationKind::ReturnDataSize => op::RETURNDATASIZE,
        OperationKind::ReturnDataCopy => op::RETURNDATACOPY,
        OperationKind::ExtCodeHash => op::EXTCODEHASH,
        OperationKind::Gas => op::GAS,

        // ========== EVM Block Information ==========
        OperationKind::BlockHash => op::BLOCKHASH,
        OperationKind::Coinbase => op::COINBASE,
        OperationKind::Timestamp => op::TIMESTAMP,
        OperationKind::Number => op::NUMBER,
        OperationKind::Difficulty => op::PREVRANDAO,
        OperationKind::GasLimit => op::GASLIMIT,
        OperationKind::ChainId => op::CHAINID,
        OperationKind::SelfBalance => op::SELFBALANCE,
        OperationKind::BaseFee => op::BASEFEE,
        OperationKind::BlobHash => op::BLOBHASH,
        OperationKind::BlobBaseFee => op::BLOBBASEFEE,

        // ========== EVM State Manipulation ==========
        OperationKind::SLoad => op::SLOAD,
        OperationKind::SStore => op::SSTORE,
        OperationKind::TLoad => op::TLOAD,
        OperationKind::TStore => op::TSTORE,

        // ========== Memory Manipulation ==========
        OperationKind::MemoryCopy => op::MCOPY,

        // ========== EVM Logging Operations ==========
        OperationKind::Log0 => op::LOG0,
        OperationKind::Log1 => op::LOG1,
        OperationKind::Log2 => op::LOG2,
        OperationKind::Log3 => op::LOG3,
        OperationKind::Log4 => op::LOG4,

        // ========== EVM System Calls ==========
        OperationKind::Create => op::CREATE,
        OperationKind::Create2 => op::CREATE2,
        OperationKind::Call => op::CALL,
        OperationKind::CallCode => op::CALLCODE,
        OperationKind::DelegateCall => op::DELEGATECALL,
        OperationKind::StaticCall => op::STATICCALL,
        OperationKind::Return => op::RETURN,
        OperationKind::Stop => op::STOP,
        OperationKind::Revert => op::REVERT,
        OperationKind::Invalid => op::INVALID,
        OperationKind::SelfDestruct => op::SELFDESTRUCT,
    };
    Some(evm_op)
}
