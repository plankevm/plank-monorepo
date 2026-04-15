use alloy_primitives::U256;
use plank_session::RuntimeBuiltin;
use sir_data::{
    self as sir,
    builder::BasicBlockBuilder,
    operation::{OpExtraData, OperationKind},
};

pub(crate) fn add_as_op(
    builtin: RuntimeBuiltin,
    inputs: &[sir::LocalId],
    output: Option<sir::LocalId>,
    builder: &mut BasicBlockBuilder<'_, '_>,
) -> Result<OperationKind, sir::operation::OpBuildError> {
    let kind = match builtin {
        // ========== EVM Arithmetic ==========
        RuntimeBuiltin::Add => OperationKind::Add,
        RuntimeBuiltin::Mul => OperationKind::Mul,
        RuntimeBuiltin::Sub => OperationKind::Sub,
        RuntimeBuiltin::Div => OperationKind::Div,
        RuntimeBuiltin::SDiv => OperationKind::SDiv,
        RuntimeBuiltin::Mod => OperationKind::Mod,
        RuntimeBuiltin::SMod => OperationKind::SMod,
        RuntimeBuiltin::AddMod => OperationKind::AddMod,
        RuntimeBuiltin::MulMod => OperationKind::MulMod,
        RuntimeBuiltin::Exp => OperationKind::Exp,
        RuntimeBuiltin::SignExtend => OperationKind::SignExtend,

        // ========== EVM Comparison & Bitwise Logic ==========
        RuntimeBuiltin::Lt => OperationKind::Lt,
        RuntimeBuiltin::Gt => OperationKind::Gt,
        RuntimeBuiltin::SLt => OperationKind::SLt,
        RuntimeBuiltin::SGt => OperationKind::SGt,
        RuntimeBuiltin::Eq => OperationKind::Eq,
        RuntimeBuiltin::IsZero => OperationKind::IsZero,
        RuntimeBuiltin::And => OperationKind::And,
        RuntimeBuiltin::Or => OperationKind::Or,
        RuntimeBuiltin::Xor => OperationKind::Xor,
        RuntimeBuiltin::Not => OperationKind::Not,
        RuntimeBuiltin::Byte => OperationKind::Byte,
        RuntimeBuiltin::Shl => OperationKind::Shl,
        RuntimeBuiltin::Shr => OperationKind::Shr,
        RuntimeBuiltin::Sar => OperationKind::Sar,

        // ========== EVM Keccak-256 ==========
        RuntimeBuiltin::Keccak256 => OperationKind::Keccak256,

        // ========== EVM Environment Information ==========
        RuntimeBuiltin::Address => OperationKind::Address,
        RuntimeBuiltin::Balance => OperationKind::Balance,
        RuntimeBuiltin::Origin => OperationKind::Origin,
        RuntimeBuiltin::Caller => OperationKind::Caller,
        RuntimeBuiltin::CallValue => OperationKind::CallValue,
        RuntimeBuiltin::CallDataLoad => OperationKind::CallDataLoad,
        RuntimeBuiltin::CallDataSize => OperationKind::CallDataSize,
        RuntimeBuiltin::CallDataCopy => OperationKind::CallDataCopy,
        RuntimeBuiltin::CodeSize => OperationKind::CodeSize,
        RuntimeBuiltin::CodeCopy => OperationKind::CodeCopy,
        RuntimeBuiltin::GasPrice => OperationKind::GasPrice,
        RuntimeBuiltin::ExtCodeSize => OperationKind::ExtCodeSize,
        RuntimeBuiltin::ExtCodeCopy => OperationKind::ExtCodeCopy,
        RuntimeBuiltin::ReturnDataSize => OperationKind::ReturnDataSize,
        RuntimeBuiltin::ReturnDataCopy => OperationKind::ReturnDataCopy,
        RuntimeBuiltin::ExtCodeHash => OperationKind::ExtCodeHash,
        RuntimeBuiltin::Gas => OperationKind::Gas,

        // ========== EVM Block Information ==========
        RuntimeBuiltin::BlockHash => OperationKind::BlockHash,
        RuntimeBuiltin::Coinbase => OperationKind::Coinbase,
        RuntimeBuiltin::Timestamp => OperationKind::Timestamp,
        RuntimeBuiltin::Number => OperationKind::Number,
        RuntimeBuiltin::Difficulty => OperationKind::Difficulty,
        RuntimeBuiltin::GasLimit => OperationKind::GasLimit,
        RuntimeBuiltin::ChainId => OperationKind::ChainId,
        RuntimeBuiltin::SelfBalance => OperationKind::SelfBalance,
        RuntimeBuiltin::BaseFee => OperationKind::BaseFee,
        RuntimeBuiltin::BlobHash => OperationKind::BlobHash,
        RuntimeBuiltin::BlobBaseFee => OperationKind::BlobBaseFee,

        // ========== EVM State Manipulation ==========
        RuntimeBuiltin::SLoad => OperationKind::SLoad,
        RuntimeBuiltin::SStore => OperationKind::SStore,
        RuntimeBuiltin::TLoad => OperationKind::TLoad,
        RuntimeBuiltin::TStore => OperationKind::TStore,

        // ========== EVM Logging Operations ==========
        RuntimeBuiltin::Log0 => OperationKind::Log0,
        RuntimeBuiltin::Log1 => OperationKind::Log1,
        RuntimeBuiltin::Log2 => OperationKind::Log2,
        RuntimeBuiltin::Log3 => OperationKind::Log3,
        RuntimeBuiltin::Log4 => OperationKind::Log4,

        // ========== EVM System Calls ==========
        RuntimeBuiltin::Create => OperationKind::Create,
        RuntimeBuiltin::Create2 => OperationKind::Create2,
        RuntimeBuiltin::Call => OperationKind::Call,
        RuntimeBuiltin::CallCode => OperationKind::CallCode,
        RuntimeBuiltin::DelegateCall => OperationKind::DelegateCall,
        RuntimeBuiltin::StaticCall => OperationKind::StaticCall,
        RuntimeBuiltin::Return => OperationKind::Return,
        RuntimeBuiltin::Stop => OperationKind::Stop,
        RuntimeBuiltin::Revert => OperationKind::Revert,
        RuntimeBuiltin::Invalid => OperationKind::Invalid,
        RuntimeBuiltin::SelfDestruct => OperationKind::SelfDestruct,

        // ========== IR Memory Primitives ==========
        RuntimeBuiltin::DynamicAllocZeroed => OperationKind::DynamicAllocZeroed,
        RuntimeBuiltin::DynamicAllocAnyBytes => OperationKind::DynamicAllocAnyBytes,

        // ========== Memory Manipulation ==========
        RuntimeBuiltin::MemoryCopy => OperationKind::MemoryCopy,
        RuntimeBuiltin::MLoad1
        | RuntimeBuiltin::MLoad2
        | RuntimeBuiltin::MLoad3
        | RuntimeBuiltin::MLoad4
        | RuntimeBuiltin::MLoad5
        | RuntimeBuiltin::MLoad6
        | RuntimeBuiltin::MLoad7
        | RuntimeBuiltin::MLoad8
        | RuntimeBuiltin::MLoad9
        | RuntimeBuiltin::MLoad10
        | RuntimeBuiltin::MLoad11
        | RuntimeBuiltin::MLoad12
        | RuntimeBuiltin::MLoad13
        | RuntimeBuiltin::MLoad14
        | RuntimeBuiltin::MLoad15
        | RuntimeBuiltin::MLoad16
        | RuntimeBuiltin::MLoad17
        | RuntimeBuiltin::MLoad18
        | RuntimeBuiltin::MLoad19
        | RuntimeBuiltin::MLoad20
        | RuntimeBuiltin::MLoad21
        | RuntimeBuiltin::MLoad22
        | RuntimeBuiltin::MLoad23
        | RuntimeBuiltin::MLoad24
        | RuntimeBuiltin::MLoad25
        | RuntimeBuiltin::MLoad26
        | RuntimeBuiltin::MLoad27
        | RuntimeBuiltin::MLoad28
        | RuntimeBuiltin::MLoad29
        | RuntimeBuiltin::MLoad30
        | RuntimeBuiltin::MLoad31
        | RuntimeBuiltin::MLoad32 => OperationKind::MemoryLoad,
        RuntimeBuiltin::MStore1
        | RuntimeBuiltin::MStore2
        | RuntimeBuiltin::MStore3
        | RuntimeBuiltin::MStore4
        | RuntimeBuiltin::MStore5
        | RuntimeBuiltin::MStore6
        | RuntimeBuiltin::MStore7
        | RuntimeBuiltin::MStore8
        | RuntimeBuiltin::MStore9
        | RuntimeBuiltin::MStore10
        | RuntimeBuiltin::MStore11
        | RuntimeBuiltin::MStore12
        | RuntimeBuiltin::MStore13
        | RuntimeBuiltin::MStore14
        | RuntimeBuiltin::MStore15
        | RuntimeBuiltin::MStore16
        | RuntimeBuiltin::MStore17
        | RuntimeBuiltin::MStore18
        | RuntimeBuiltin::MStore19
        | RuntimeBuiltin::MStore20
        | RuntimeBuiltin::MStore21
        | RuntimeBuiltin::MStore22
        | RuntimeBuiltin::MStore23
        | RuntimeBuiltin::MStore24
        | RuntimeBuiltin::MStore25
        | RuntimeBuiltin::MStore26
        | RuntimeBuiltin::MStore27
        | RuntimeBuiltin::MStore28
        | RuntimeBuiltin::MStore29
        | RuntimeBuiltin::MStore30
        | RuntimeBuiltin::MStore31
        | RuntimeBuiltin::MStore32 => OperationKind::MemoryStore,

        // ========== Bytecode Introspection ==========
        RuntimeBuiltin::RuntimeStartOffset => OperationKind::RuntimeStartOffset,
        RuntimeBuiltin::InitEndOffset => OperationKind::InitEndOffset,
        RuntimeBuiltin::RuntimeLength => OperationKind::RuntimeLength,
    };
    let op_extra_data = match builtin {
        RuntimeBuiltin::MLoad1 | RuntimeBuiltin::MStore1 => OpExtraData::Num(U256::from(1)),
        RuntimeBuiltin::MLoad2 | RuntimeBuiltin::MStore2 => OpExtraData::Num(U256::from(2)),
        RuntimeBuiltin::MLoad3 | RuntimeBuiltin::MStore3 => OpExtraData::Num(U256::from(3)),
        RuntimeBuiltin::MLoad4 | RuntimeBuiltin::MStore4 => OpExtraData::Num(U256::from(4)),
        RuntimeBuiltin::MLoad5 | RuntimeBuiltin::MStore5 => OpExtraData::Num(U256::from(5)),
        RuntimeBuiltin::MLoad6 | RuntimeBuiltin::MStore6 => OpExtraData::Num(U256::from(6)),
        RuntimeBuiltin::MLoad7 | RuntimeBuiltin::MStore7 => OpExtraData::Num(U256::from(7)),
        RuntimeBuiltin::MLoad8 | RuntimeBuiltin::MStore8 => OpExtraData::Num(U256::from(8)),
        RuntimeBuiltin::MLoad9 | RuntimeBuiltin::MStore9 => OpExtraData::Num(U256::from(9)),
        RuntimeBuiltin::MLoad10 | RuntimeBuiltin::MStore10 => OpExtraData::Num(U256::from(10)),
        RuntimeBuiltin::MLoad11 | RuntimeBuiltin::MStore11 => OpExtraData::Num(U256::from(11)),
        RuntimeBuiltin::MLoad12 | RuntimeBuiltin::MStore12 => OpExtraData::Num(U256::from(12)),
        RuntimeBuiltin::MLoad13 | RuntimeBuiltin::MStore13 => OpExtraData::Num(U256::from(13)),
        RuntimeBuiltin::MLoad14 | RuntimeBuiltin::MStore14 => OpExtraData::Num(U256::from(14)),
        RuntimeBuiltin::MLoad15 | RuntimeBuiltin::MStore15 => OpExtraData::Num(U256::from(15)),
        RuntimeBuiltin::MLoad16 | RuntimeBuiltin::MStore16 => OpExtraData::Num(U256::from(16)),
        RuntimeBuiltin::MLoad17 | RuntimeBuiltin::MStore17 => OpExtraData::Num(U256::from(17)),
        RuntimeBuiltin::MLoad18 | RuntimeBuiltin::MStore18 => OpExtraData::Num(U256::from(18)),
        RuntimeBuiltin::MLoad19 | RuntimeBuiltin::MStore19 => OpExtraData::Num(U256::from(19)),
        RuntimeBuiltin::MLoad20 | RuntimeBuiltin::MStore20 => OpExtraData::Num(U256::from(20)),
        RuntimeBuiltin::MLoad21 | RuntimeBuiltin::MStore21 => OpExtraData::Num(U256::from(21)),
        RuntimeBuiltin::MLoad22 | RuntimeBuiltin::MStore22 => OpExtraData::Num(U256::from(22)),
        RuntimeBuiltin::MLoad23 | RuntimeBuiltin::MStore23 => OpExtraData::Num(U256::from(23)),
        RuntimeBuiltin::MLoad24 | RuntimeBuiltin::MStore24 => OpExtraData::Num(U256::from(24)),
        RuntimeBuiltin::MLoad25 | RuntimeBuiltin::MStore25 => OpExtraData::Num(U256::from(25)),
        RuntimeBuiltin::MLoad26 | RuntimeBuiltin::MStore26 => OpExtraData::Num(U256::from(26)),
        RuntimeBuiltin::MLoad27 | RuntimeBuiltin::MStore27 => OpExtraData::Num(U256::from(27)),
        RuntimeBuiltin::MLoad28 | RuntimeBuiltin::MStore28 => OpExtraData::Num(U256::from(28)),
        RuntimeBuiltin::MLoad29 | RuntimeBuiltin::MStore29 => OpExtraData::Num(U256::from(29)),
        RuntimeBuiltin::MLoad30 | RuntimeBuiltin::MStore30 => OpExtraData::Num(U256::from(30)),
        RuntimeBuiltin::MLoad31 | RuntimeBuiltin::MStore31 => OpExtraData::Num(U256::from(31)),
        RuntimeBuiltin::MLoad32 | RuntimeBuiltin::MStore32 => OpExtraData::Num(U256::from(32)),
        _ => OpExtraData::Empty,
    };
    let outputs = output.as_ref().map_or(&[] as &[_], std::slice::from_ref);
    builder.try_add_op(kind, inputs, outputs, op_extra_data)?;
    Ok(kind)
}
