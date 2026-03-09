use alloy_primitives::U256;
use plank_hir::builtins::Builtin;
use sir_data::{
    self as sir,
    builder::BasicBlockBuilder,
    operation::{OpExtraData, OperationKind},
};

pub(crate) fn add_as_op(
    builtin: Builtin,
    inputs: &[sir::LocalId],
    output: Option<sir::LocalId>,
    builder: &mut BasicBlockBuilder<'_, '_>,
) -> Result<OperationKind, sir::operation::OpBuildError> {
    let kind = match builtin {
        // ========== EVM Arithmetic ==========
        Builtin::Add => OperationKind::Add,
        Builtin::Mul => OperationKind::Mul,
        Builtin::Sub => OperationKind::Sub,
        Builtin::Div => OperationKind::Div,
        Builtin::SDiv => OperationKind::SDiv,
        Builtin::Mod => OperationKind::Mod,
        Builtin::SMod => OperationKind::SMod,
        Builtin::AddMod => OperationKind::AddMod,
        Builtin::MulMod => OperationKind::MulMod,
        Builtin::Exp => OperationKind::Exp,
        Builtin::SignExtend => OperationKind::SignExtend,

        // ========== EVM Comparison & Bitwise Logic ==========
        Builtin::Lt => OperationKind::Lt,
        Builtin::Gt => OperationKind::Gt,
        Builtin::SLt => OperationKind::SLt,
        Builtin::SGt => OperationKind::SGt,
        Builtin::Eq => OperationKind::Eq,
        Builtin::IsZero => OperationKind::IsZero,
        Builtin::And => OperationKind::And,
        Builtin::Or => OperationKind::Or,
        Builtin::Xor => OperationKind::Xor,
        Builtin::Not => OperationKind::Not,
        Builtin::Byte => OperationKind::Byte,
        Builtin::Shl => OperationKind::Shl,
        Builtin::Shr => OperationKind::Shr,
        Builtin::Sar => OperationKind::Sar,

        // ========== EVM Keccak-256 ==========
        Builtin::Keccak256 => OperationKind::Keccak256,

        // ========== EVM Environment Information ==========
        Builtin::Address => OperationKind::Address,
        Builtin::Balance => OperationKind::Balance,
        Builtin::Origin => OperationKind::Origin,
        Builtin::Caller => OperationKind::Caller,
        Builtin::CallValue => OperationKind::CallValue,
        Builtin::CallDataLoad => OperationKind::CallDataLoad,
        Builtin::CallDataSize => OperationKind::CallDataSize,
        Builtin::CallDataCopy => OperationKind::CallDataCopy,
        Builtin::CodeSize => OperationKind::CodeSize,
        Builtin::CodeCopy => OperationKind::CodeCopy,
        Builtin::GasPrice => OperationKind::GasPrice,
        Builtin::ExtCodeSize => OperationKind::ExtCodeSize,
        Builtin::ExtCodeCopy => OperationKind::ExtCodeCopy,
        Builtin::ReturnDataSize => OperationKind::ReturnDataSize,
        Builtin::ReturnDataCopy => OperationKind::ReturnDataCopy,
        Builtin::ExtCodeHash => OperationKind::ExtCodeHash,
        Builtin::Gas => OperationKind::Gas,

        // ========== EVM Block Information ==========
        Builtin::BlockHash => OperationKind::BlockHash,
        Builtin::Coinbase => OperationKind::Coinbase,
        Builtin::Timestamp => OperationKind::Timestamp,
        Builtin::Number => OperationKind::Number,
        Builtin::Difficulty => OperationKind::Difficulty,
        Builtin::GasLimit => OperationKind::GasLimit,
        Builtin::ChainId => OperationKind::ChainId,
        Builtin::SelfBalance => OperationKind::SelfBalance,
        Builtin::BaseFee => OperationKind::BaseFee,
        Builtin::BlobHash => OperationKind::BlobHash,
        Builtin::BlobBaseFee => OperationKind::BlobBaseFee,

        // ========== EVM State Manipulation ==========
        Builtin::SLoad => OperationKind::SLoad,
        Builtin::SStore => OperationKind::SStore,
        Builtin::TLoad => OperationKind::TLoad,
        Builtin::TStore => OperationKind::TStore,

        // ========== EVM Logging Operations ==========
        Builtin::Log0 => OperationKind::Log0,
        Builtin::Log1 => OperationKind::Log1,
        Builtin::Log2 => OperationKind::Log2,
        Builtin::Log3 => OperationKind::Log3,
        Builtin::Log4 => OperationKind::Log4,

        // ========== EVM System Calls ==========
        Builtin::Create => OperationKind::Create,
        Builtin::Create2 => OperationKind::Create2,
        Builtin::Call => OperationKind::Call,
        Builtin::CallCode => OperationKind::CallCode,
        Builtin::DelegateCall => OperationKind::DelegateCall,
        Builtin::StaticCall => OperationKind::StaticCall,
        Builtin::Return => OperationKind::Return,
        Builtin::Stop => OperationKind::Stop,
        Builtin::Revert => OperationKind::Revert,
        Builtin::Invalid => OperationKind::Invalid,
        Builtin::SelfDestruct => OperationKind::SelfDestruct,

        // ========== IR Memory Primitives ==========
        Builtin::DynamicAllocZeroed => OperationKind::DynamicAllocZeroed,
        Builtin::DynamicAllocAnyBytes => OperationKind::DynamicAllocAnyBytes,

        // ========== Memory Manipulation ==========
        Builtin::MemoryCopy => OperationKind::MemoryCopy,
        Builtin::MLoad1
        | Builtin::MLoad2
        | Builtin::MLoad3
        | Builtin::MLoad4
        | Builtin::MLoad5
        | Builtin::MLoad6
        | Builtin::MLoad7
        | Builtin::MLoad8
        | Builtin::MLoad9
        | Builtin::MLoad10
        | Builtin::MLoad11
        | Builtin::MLoad12
        | Builtin::MLoad13
        | Builtin::MLoad14
        | Builtin::MLoad15
        | Builtin::MLoad16
        | Builtin::MLoad17
        | Builtin::MLoad18
        | Builtin::MLoad19
        | Builtin::MLoad20
        | Builtin::MLoad21
        | Builtin::MLoad22
        | Builtin::MLoad23
        | Builtin::MLoad24
        | Builtin::MLoad25
        | Builtin::MLoad26
        | Builtin::MLoad27
        | Builtin::MLoad28
        | Builtin::MLoad29
        | Builtin::MLoad30
        | Builtin::MLoad31
        | Builtin::MLoad32 => OperationKind::MemoryLoad,
        Builtin::MStore1
        | Builtin::MStore2
        | Builtin::MStore3
        | Builtin::MStore4
        | Builtin::MStore5
        | Builtin::MStore6
        | Builtin::MStore7
        | Builtin::MStore8
        | Builtin::MStore9
        | Builtin::MStore10
        | Builtin::MStore11
        | Builtin::MStore12
        | Builtin::MStore13
        | Builtin::MStore14
        | Builtin::MStore15
        | Builtin::MStore16
        | Builtin::MStore17
        | Builtin::MStore18
        | Builtin::MStore19
        | Builtin::MStore20
        | Builtin::MStore21
        | Builtin::MStore22
        | Builtin::MStore23
        | Builtin::MStore24
        | Builtin::MStore25
        | Builtin::MStore26
        | Builtin::MStore27
        | Builtin::MStore28
        | Builtin::MStore29
        | Builtin::MStore30
        | Builtin::MStore31
        | Builtin::MStore32 => OperationKind::MemoryStore,

        // ========== Bytecode Introspection ==========
        Builtin::RuntimeStartOffset => OperationKind::RuntimeStartOffset,
        Builtin::InitEndOffset => OperationKind::InitEndOffset,
        Builtin::RuntimeLength => OperationKind::RuntimeLength,
    };
    let op_extra_data = match builtin {
        Builtin::MLoad1 | Builtin::MStore1 => OpExtraData::Num(U256::from(1)),
        Builtin::MLoad2 | Builtin::MStore2 => OpExtraData::Num(U256::from(2)),
        Builtin::MLoad3 | Builtin::MStore3 => OpExtraData::Num(U256::from(3)),
        Builtin::MLoad4 | Builtin::MStore4 => OpExtraData::Num(U256::from(4)),
        Builtin::MLoad5 | Builtin::MStore5 => OpExtraData::Num(U256::from(5)),
        Builtin::MLoad6 | Builtin::MStore6 => OpExtraData::Num(U256::from(6)),
        Builtin::MLoad7 | Builtin::MStore7 => OpExtraData::Num(U256::from(7)),
        Builtin::MLoad8 | Builtin::MStore8 => OpExtraData::Num(U256::from(8)),
        Builtin::MLoad9 | Builtin::MStore9 => OpExtraData::Num(U256::from(9)),
        Builtin::MLoad10 | Builtin::MStore10 => OpExtraData::Num(U256::from(10)),
        Builtin::MLoad11 | Builtin::MStore11 => OpExtraData::Num(U256::from(11)),
        Builtin::MLoad12 | Builtin::MStore12 => OpExtraData::Num(U256::from(12)),
        Builtin::MLoad13 | Builtin::MStore13 => OpExtraData::Num(U256::from(13)),
        Builtin::MLoad14 | Builtin::MStore14 => OpExtraData::Num(U256::from(14)),
        Builtin::MLoad15 | Builtin::MStore15 => OpExtraData::Num(U256::from(15)),
        Builtin::MLoad16 | Builtin::MStore16 => OpExtraData::Num(U256::from(16)),
        Builtin::MLoad17 | Builtin::MStore17 => OpExtraData::Num(U256::from(17)),
        Builtin::MLoad18 | Builtin::MStore18 => OpExtraData::Num(U256::from(18)),
        Builtin::MLoad19 | Builtin::MStore19 => OpExtraData::Num(U256::from(19)),
        Builtin::MLoad20 | Builtin::MStore20 => OpExtraData::Num(U256::from(20)),
        Builtin::MLoad21 | Builtin::MStore21 => OpExtraData::Num(U256::from(21)),
        Builtin::MLoad22 | Builtin::MStore22 => OpExtraData::Num(U256::from(22)),
        Builtin::MLoad23 | Builtin::MStore23 => OpExtraData::Num(U256::from(23)),
        Builtin::MLoad24 | Builtin::MStore24 => OpExtraData::Num(U256::from(24)),
        Builtin::MLoad25 | Builtin::MStore25 => OpExtraData::Num(U256::from(25)),
        Builtin::MLoad26 | Builtin::MStore26 => OpExtraData::Num(U256::from(26)),
        Builtin::MLoad27 | Builtin::MStore27 => OpExtraData::Num(U256::from(27)),
        Builtin::MLoad28 | Builtin::MStore28 => OpExtraData::Num(U256::from(28)),
        Builtin::MLoad29 | Builtin::MStore29 => OpExtraData::Num(U256::from(29)),
        Builtin::MLoad30 | Builtin::MStore30 => OpExtraData::Num(U256::from(30)),
        Builtin::MLoad31 | Builtin::MStore31 => OpExtraData::Num(U256::from(31)),
        Builtin::MLoad32 | Builtin::MStore32 => OpExtraData::Num(U256::from(32)),
        _ => OpExtraData::Empty,
    };
    let outputs = output.as_ref().map_or(&[] as &[_], std::slice::from_ref);
    builder.try_add_op(kind, inputs, outputs, op_extra_data)?;
    Ok(kind)
}
