use alloy_primitives::U256;
use plank_session::EvmBuiltin;
use sir_data::{
    self as sir,
    builder::BasicBlockBuilder,
    operation::{OpExtraData, OperationKind},
};

pub(crate) fn add_as_op(
    builtin: EvmBuiltin,
    inputs: &[sir::LocalId],
    output: Option<sir::LocalId>,
    builder: &mut BasicBlockBuilder<'_, '_>,
) -> Result<OperationKind, sir::operation::OpBuildError> {
    let kind = match builtin {
        // ========== EVM Arithmetic ==========
        EvmBuiltin::Add => OperationKind::Add,
        EvmBuiltin::Mul => OperationKind::Mul,
        EvmBuiltin::Sub => OperationKind::Sub,
        EvmBuiltin::Div => OperationKind::Div,
        EvmBuiltin::SDiv => OperationKind::SDiv,
        EvmBuiltin::Mod => OperationKind::Mod,
        EvmBuiltin::SMod => OperationKind::SMod,
        EvmBuiltin::AddMod => OperationKind::AddMod,
        EvmBuiltin::MulMod => OperationKind::MulMod,
        EvmBuiltin::Exp => OperationKind::Exp,
        EvmBuiltin::SignExtend => OperationKind::SignExtend,

        // ========== EVM Comparison & Bitwise Logic ==========
        EvmBuiltin::Lt => OperationKind::Lt,
        EvmBuiltin::Gt => OperationKind::Gt,
        EvmBuiltin::SLt => OperationKind::SLt,
        EvmBuiltin::SGt => OperationKind::SGt,
        EvmBuiltin::Eq => OperationKind::Eq,
        EvmBuiltin::IsZero => OperationKind::IsZero,
        EvmBuiltin::And => OperationKind::And,
        EvmBuiltin::Or => OperationKind::Or,
        EvmBuiltin::Xor => OperationKind::Xor,
        EvmBuiltin::Not => OperationKind::Not,
        EvmBuiltin::Byte => OperationKind::Byte,
        EvmBuiltin::Shl => OperationKind::Shl,
        EvmBuiltin::Shr => OperationKind::Shr,
        EvmBuiltin::Sar => OperationKind::Sar,

        // ========== EVM Keccak-256 ==========
        EvmBuiltin::Keccak256 => OperationKind::Keccak256,

        // ========== EVM Environment Information ==========
        EvmBuiltin::Address => OperationKind::Address,
        EvmBuiltin::Balance => OperationKind::Balance,
        EvmBuiltin::Origin => OperationKind::Origin,
        EvmBuiltin::Caller => OperationKind::Caller,
        EvmBuiltin::CallValue => OperationKind::CallValue,
        EvmBuiltin::CallDataLoad => OperationKind::CallDataLoad,
        EvmBuiltin::CallDataSize => OperationKind::CallDataSize,
        EvmBuiltin::CallDataCopy => OperationKind::CallDataCopy,
        EvmBuiltin::CodeSize => OperationKind::CodeSize,
        EvmBuiltin::CodeCopy => OperationKind::CodeCopy,
        EvmBuiltin::GasPrice => OperationKind::GasPrice,
        EvmBuiltin::ExtCodeSize => OperationKind::ExtCodeSize,
        EvmBuiltin::ExtCodeCopy => OperationKind::ExtCodeCopy,
        EvmBuiltin::ReturnDataSize => OperationKind::ReturnDataSize,
        EvmBuiltin::ReturnDataCopy => OperationKind::ReturnDataCopy,
        EvmBuiltin::ExtCodeHash => OperationKind::ExtCodeHash,
        EvmBuiltin::Gas => OperationKind::Gas,

        // ========== EVM Block Information ==========
        EvmBuiltin::BlockHash => OperationKind::BlockHash,
        EvmBuiltin::Coinbase => OperationKind::Coinbase,
        EvmBuiltin::Timestamp => OperationKind::Timestamp,
        EvmBuiltin::Number => OperationKind::Number,
        EvmBuiltin::Difficulty => OperationKind::Difficulty,
        EvmBuiltin::GasLimit => OperationKind::GasLimit,
        EvmBuiltin::ChainId => OperationKind::ChainId,
        EvmBuiltin::SelfBalance => OperationKind::SelfBalance,
        EvmBuiltin::BaseFee => OperationKind::BaseFee,
        EvmBuiltin::BlobHash => OperationKind::BlobHash,
        EvmBuiltin::BlobBaseFee => OperationKind::BlobBaseFee,

        // ========== EVM State Manipulation ==========
        EvmBuiltin::SLoad => OperationKind::SLoad,
        EvmBuiltin::SStore => OperationKind::SStore,
        EvmBuiltin::TLoad => OperationKind::TLoad,
        EvmBuiltin::TStore => OperationKind::TStore,

        // ========== EVM Logging Operations ==========
        EvmBuiltin::Log0 => OperationKind::Log0,
        EvmBuiltin::Log1 => OperationKind::Log1,
        EvmBuiltin::Log2 => OperationKind::Log2,
        EvmBuiltin::Log3 => OperationKind::Log3,
        EvmBuiltin::Log4 => OperationKind::Log4,

        // ========== EVM System Calls ==========
        EvmBuiltin::Create => OperationKind::Create,
        EvmBuiltin::Create2 => OperationKind::Create2,
        EvmBuiltin::Call => OperationKind::Call,
        EvmBuiltin::CallCode => OperationKind::CallCode,
        EvmBuiltin::DelegateCall => OperationKind::DelegateCall,
        EvmBuiltin::StaticCall => OperationKind::StaticCall,
        EvmBuiltin::Return => OperationKind::Return,
        EvmBuiltin::Stop => OperationKind::Stop,
        EvmBuiltin::Revert => OperationKind::Revert,
        EvmBuiltin::Invalid => OperationKind::Invalid,
        EvmBuiltin::SelfDestruct => OperationKind::SelfDestruct,

        // ========== IR Memory Primitives ==========
        EvmBuiltin::DynamicAllocZeroed => OperationKind::DynamicAllocZeroed,
        EvmBuiltin::DynamicAllocAnyBytes => OperationKind::DynamicAllocAnyBytes,

        // ========== Memory Manipulation ==========
        EvmBuiltin::MemoryCopy => OperationKind::MemoryCopy,
        EvmBuiltin::MLoad1
        | EvmBuiltin::MLoad2
        | EvmBuiltin::MLoad3
        | EvmBuiltin::MLoad4
        | EvmBuiltin::MLoad5
        | EvmBuiltin::MLoad6
        | EvmBuiltin::MLoad7
        | EvmBuiltin::MLoad8
        | EvmBuiltin::MLoad9
        | EvmBuiltin::MLoad10
        | EvmBuiltin::MLoad11
        | EvmBuiltin::MLoad12
        | EvmBuiltin::MLoad13
        | EvmBuiltin::MLoad14
        | EvmBuiltin::MLoad15
        | EvmBuiltin::MLoad16
        | EvmBuiltin::MLoad17
        | EvmBuiltin::MLoad18
        | EvmBuiltin::MLoad19
        | EvmBuiltin::MLoad20
        | EvmBuiltin::MLoad21
        | EvmBuiltin::MLoad22
        | EvmBuiltin::MLoad23
        | EvmBuiltin::MLoad24
        | EvmBuiltin::MLoad25
        | EvmBuiltin::MLoad26
        | EvmBuiltin::MLoad27
        | EvmBuiltin::MLoad28
        | EvmBuiltin::MLoad29
        | EvmBuiltin::MLoad30
        | EvmBuiltin::MLoad31
        | EvmBuiltin::MLoad32 => OperationKind::MemoryLoad,
        EvmBuiltin::MStore1
        | EvmBuiltin::MStore2
        | EvmBuiltin::MStore3
        | EvmBuiltin::MStore4
        | EvmBuiltin::MStore5
        | EvmBuiltin::MStore6
        | EvmBuiltin::MStore7
        | EvmBuiltin::MStore8
        | EvmBuiltin::MStore9
        | EvmBuiltin::MStore10
        | EvmBuiltin::MStore11
        | EvmBuiltin::MStore12
        | EvmBuiltin::MStore13
        | EvmBuiltin::MStore14
        | EvmBuiltin::MStore15
        | EvmBuiltin::MStore16
        | EvmBuiltin::MStore17
        | EvmBuiltin::MStore18
        | EvmBuiltin::MStore19
        | EvmBuiltin::MStore20
        | EvmBuiltin::MStore21
        | EvmBuiltin::MStore22
        | EvmBuiltin::MStore23
        | EvmBuiltin::MStore24
        | EvmBuiltin::MStore25
        | EvmBuiltin::MStore26
        | EvmBuiltin::MStore27
        | EvmBuiltin::MStore28
        | EvmBuiltin::MStore29
        | EvmBuiltin::MStore30
        | EvmBuiltin::MStore31
        | EvmBuiltin::MStore32 => OperationKind::MemoryStore,

        // ========== Bytecode Introspection ==========
        EvmBuiltin::RuntimeStartOffset => OperationKind::RuntimeStartOffset,
        EvmBuiltin::InitEndOffset => OperationKind::InitEndOffset,
        EvmBuiltin::RuntimeLength => OperationKind::RuntimeLength,
    };
    let op_extra_data = match builtin {
        EvmBuiltin::MLoad1 | EvmBuiltin::MStore1 => OpExtraData::Num(U256::from(1)),
        EvmBuiltin::MLoad2 | EvmBuiltin::MStore2 => OpExtraData::Num(U256::from(2)),
        EvmBuiltin::MLoad3 | EvmBuiltin::MStore3 => OpExtraData::Num(U256::from(3)),
        EvmBuiltin::MLoad4 | EvmBuiltin::MStore4 => OpExtraData::Num(U256::from(4)),
        EvmBuiltin::MLoad5 | EvmBuiltin::MStore5 => OpExtraData::Num(U256::from(5)),
        EvmBuiltin::MLoad6 | EvmBuiltin::MStore6 => OpExtraData::Num(U256::from(6)),
        EvmBuiltin::MLoad7 | EvmBuiltin::MStore7 => OpExtraData::Num(U256::from(7)),
        EvmBuiltin::MLoad8 | EvmBuiltin::MStore8 => OpExtraData::Num(U256::from(8)),
        EvmBuiltin::MLoad9 | EvmBuiltin::MStore9 => OpExtraData::Num(U256::from(9)),
        EvmBuiltin::MLoad10 | EvmBuiltin::MStore10 => OpExtraData::Num(U256::from(10)),
        EvmBuiltin::MLoad11 | EvmBuiltin::MStore11 => OpExtraData::Num(U256::from(11)),
        EvmBuiltin::MLoad12 | EvmBuiltin::MStore12 => OpExtraData::Num(U256::from(12)),
        EvmBuiltin::MLoad13 | EvmBuiltin::MStore13 => OpExtraData::Num(U256::from(13)),
        EvmBuiltin::MLoad14 | EvmBuiltin::MStore14 => OpExtraData::Num(U256::from(14)),
        EvmBuiltin::MLoad15 | EvmBuiltin::MStore15 => OpExtraData::Num(U256::from(15)),
        EvmBuiltin::MLoad16 | EvmBuiltin::MStore16 => OpExtraData::Num(U256::from(16)),
        EvmBuiltin::MLoad17 | EvmBuiltin::MStore17 => OpExtraData::Num(U256::from(17)),
        EvmBuiltin::MLoad18 | EvmBuiltin::MStore18 => OpExtraData::Num(U256::from(18)),
        EvmBuiltin::MLoad19 | EvmBuiltin::MStore19 => OpExtraData::Num(U256::from(19)),
        EvmBuiltin::MLoad20 | EvmBuiltin::MStore20 => OpExtraData::Num(U256::from(20)),
        EvmBuiltin::MLoad21 | EvmBuiltin::MStore21 => OpExtraData::Num(U256::from(21)),
        EvmBuiltin::MLoad22 | EvmBuiltin::MStore22 => OpExtraData::Num(U256::from(22)),
        EvmBuiltin::MLoad23 | EvmBuiltin::MStore23 => OpExtraData::Num(U256::from(23)),
        EvmBuiltin::MLoad24 | EvmBuiltin::MStore24 => OpExtraData::Num(U256::from(24)),
        EvmBuiltin::MLoad25 | EvmBuiltin::MStore25 => OpExtraData::Num(U256::from(25)),
        EvmBuiltin::MLoad26 | EvmBuiltin::MStore26 => OpExtraData::Num(U256::from(26)),
        EvmBuiltin::MLoad27 | EvmBuiltin::MStore27 => OpExtraData::Num(U256::from(27)),
        EvmBuiltin::MLoad28 | EvmBuiltin::MStore28 => OpExtraData::Num(U256::from(28)),
        EvmBuiltin::MLoad29 | EvmBuiltin::MStore29 => OpExtraData::Num(U256::from(29)),
        EvmBuiltin::MLoad30 | EvmBuiltin::MStore30 => OpExtraData::Num(U256::from(30)),
        EvmBuiltin::MLoad31 | EvmBuiltin::MStore31 => OpExtraData::Num(U256::from(31)),
        EvmBuiltin::MLoad32 | EvmBuiltin::MStore32 => OpExtraData::Num(U256::from(32)),
        _ => OpExtraData::Empty,
    };
    let outputs = output.as_ref().map_or(&[] as &[_], std::slice::from_ref);
    builder.try_add_op(kind, inputs, outputs, op_extra_data)?;
    Ok(kind)
}
