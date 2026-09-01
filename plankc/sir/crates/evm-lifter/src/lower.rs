use plank_core::IndexVec;
use sir_data::{
    BasicBlockId, Branch, Control, EthIRProgram, FunctionId, LocalId, Operation, OperationIdx,
    builder::{BuildError, EthIRBuilder},
    operation::{OpBuildError, OpExtraData, OperationKind},
};
use sir_passes::{AnalysesStore, Legalizer, analyses::LegalizerError};

use crate::{
    CodeBlockId, FunctionCandidateId, InstructionId, Opcode, SsaBlockId, SsaValueId,
    classify::ClassifiedProgram,
    ssa::{SsaControl, SsaOperationKind, SsaProgram},
};

#[derive(Debug)]
pub struct LiftProvenance {
    pub functions: IndexVec<FunctionId, FunctionCandidateId>,
    pub blocks: IndexVec<BasicBlockId, Option<CodeBlockId>>,
    pub block_functions: IndexVec<BasicBlockId, FunctionCandidateId>,
    pub operations: IndexVec<OperationIdx, Option<InstructionId>>,
}

#[derive(Debug)]
pub struct LiftedProgram {
    pub program: EthIRProgram,
    pub provenance: LiftProvenance,
}

pub fn lower_to_sir(
    decoded: &crate::DecodedBytecode,
    classified: &ClassifiedProgram,
    ssa: &SsaProgram,
    postorder: &[FunctionCandidateId],
    root: FunctionCandidateId,
) -> Result<LiftedProgram, LowerError> {
    validate_supported_operations(ssa)?;
    let mut builder = EthIRBuilder::new();
    for section in classified.data_sections().iter() {
        builder.push_data_bytes(
            &decoded.bytes()[section.bytes.start as usize..section.bytes.end as usize],
        );
    }

    let mut function_map = IndexVec::from_vec(vec![None; ssa.functions().len()]);
    let mut block_map = IndexVec::from_vec(vec![None; ssa.blocks().len()]);
    let mut value_map = IndexVec::from_vec(vec![None; ssa.values().len()]);
    let mut function_provenance = IndexVec::new();
    let mut block_provenance = IndexVec::new();
    let mut block_function_provenance = IndexVec::new();
    let mut operation_provenance = IndexVec::new();

    for &function in postorder {
        let ssa_function = &ssa.functions()[function];
        let mut function_builder = builder.begin_function();
        for &ssa_block in &ssa_function.blocks {
            let block = &ssa.blocks()[ssa_block];
            let mut block_builder = function_builder.begin_basic_block();
            let mut input_locals = Vec::with_capacity(block.inputs.len());
            for &value in &block.inputs {
                let local = block_builder.new_local();
                value_map[value] = Some(local);
                input_locals.push(local);
            }
            block_builder.set_inputs(&input_locals);

            for operation in &block.operations {
                let inputs = operation
                    .inputs
                    .iter()
                    .map(|&value| mapped_value(&value_map, value))
                    .collect::<Result<Vec<_>, _>>()?;
                let mut outputs = Vec::with_capacity(operation.outputs.len());
                for &value in &operation.outputs {
                    let local = block_builder.new_local();
                    value_map[value] = Some(local);
                    outputs.push(local);
                }
                let operation_id = match operation.kind {
                    SsaOperationKind::Constant(value) => {
                        let &[output] = outputs.as_slice() else {
                            return Err(LowerError::InvalidSsaOperation);
                        };
                        block_builder.add_set_const_op(output, value)
                    }
                    SsaOperationKind::InternalCall(callee) => {
                        let callee = function_map[callee]
                            .ok_or(LowerError::CalleeNotLowered { caller: function, callee })?;
                        add_operation(
                            &mut block_builder,
                            OperationKind::InternalCall,
                            &inputs,
                            &outputs,
                            OpExtraData::FuncId(callee),
                        )?
                    }
                    SsaOperationKind::SyntheticInvalid => {
                        block_builder.add_operation(Operation::Invalid(()))
                    }
                    SsaOperationKind::SyntheticStop => {
                        block_builder.add_operation(Operation::Stop(()))
                    }
                    SsaOperationKind::Opcode(opcode) => {
                        add_opcode_operation(&mut block_builder, opcode, &inputs, &outputs)?
                    }
                };
                let provenance_id = operation_provenance.push(operation.source);
                assert_eq!(operation_id, provenance_id, "operation provenance out of sync");
            }

            let output_locals = block
                .outputs
                .iter()
                .map(|&value| mapped_value(&value_map, value))
                .collect::<Result<Vec<_>, _>>()?;
            block_builder.set_outputs(&output_locals);
            let basic_block = block_builder.finish_with_placeholder_control();
            let provenance_id = block_provenance.push(block.source);
            assert_eq!(basic_block, provenance_id, "block provenance out of sync");
            let function_provenance_id = block_function_provenance.push(function);
            assert_eq!(
                basic_block, function_provenance_id,
                "block function provenance out of sync"
            );
            block_map[ssa_block] = Some(basic_block);
        }

        for &ssa_block in &ssa_function.blocks {
            let block = &ssa.blocks()[ssa_block];
            let basic_block = block_map[ssa_block].expect("function block should be lowered");
            let control = match block.control {
                SsaControl::Goto(target) => Control::ContinuesTo(mapped_block(&block_map, target)?),
                SsaControl::Branch { condition, non_zero, zero } => Control::Branches(Branch {
                    condition: mapped_value(&value_map, condition)?,
                    non_zero_target: mapped_block(&block_map, non_zero)?,
                    zero_target: mapped_block(&block_map, zero)?,
                }),
                SsaControl::InternalReturn => Control::InternalReturn,
                SsaControl::Terminates => Control::LastOpTerminates,
            };
            function_builder.set_control(basic_block, control)?;
        }

        let entry = mapped_block(&block_map, ssa_function.entry)?;
        let sir_function = function_builder.finish(entry);
        function_map[function] = Some(sir_function);
        let provenance_id = function_provenance.push(function);
        assert_eq!(sir_function, provenance_id, "function provenance out of sync");
    }

    let init_entry = function_map[root].expect("root should be lowered");
    let program = builder.build(init_entry, None);
    Legalizer::default().run(&program, &AnalysesStore::default())?;
    Ok(LiftedProgram {
        program,
        provenance: LiftProvenance {
            functions: function_provenance,
            blocks: block_provenance,
            block_functions: block_function_provenance,
            operations: operation_provenance,
        },
    })
}

fn validate_supported_operations(ssa: &SsaProgram) -> Result<(), LowerError> {
    for block in ssa.blocks().iter() {
        for operation in &block.operations {
            if let SsaOperationKind::Opcode(opcode @ (Opcode::Pc | Opcode::MSize)) = operation.kind
            {
                return Err(LowerError::UnsupportedOpcode(opcode));
            }
        }
    }
    Ok(())
}

fn mapped_value(
    map: &IndexVec<SsaValueId, Option<LocalId>>,
    value: SsaValueId,
) -> Result<LocalId, LowerError> {
    map[value].ok_or(LowerError::UndefinedSsaValue { value })
}

fn mapped_block(
    map: &IndexVec<SsaBlockId, Option<BasicBlockId>>,
    block: SsaBlockId,
) -> Result<BasicBlockId, LowerError> {
    map[block].ok_or(LowerError::UndefinedSsaBlock { block })
}

fn add_operation(
    block: &mut sir_data::builder::BasicBlockBuilder<'_, '_>,
    kind: OperationKind,
    inputs: &[LocalId],
    outputs: &[LocalId],
    extra: OpExtraData,
) -> Result<OperationIdx, LowerError> {
    let operation = Operation::try_build(kind, inputs, outputs, extra, block.as_mut())?;
    Ok(block.add_operation(operation))
}

fn add_opcode_operation(
    block: &mut sir_data::builder::BasicBlockBuilder<'_, '_>,
    opcode: Opcode,
    inputs: &[LocalId],
    outputs: &[LocalId],
) -> Result<OperationIdx, LowerError> {
    let (kind, extra) = match opcode {
        Opcode::Pc | Opcode::MSize => return Err(LowerError::UnsupportedOpcode(opcode)),
        Opcode::MLoad => {
            (OperationKind::MemoryLoad, OpExtraData::Num(alloy_primitives::U256::from(32u32)))
        }
        Opcode::MStore => {
            (OperationKind::MemoryStore, OpExtraData::Num(alloy_primitives::U256::from(32u32)))
        }
        Opcode::MStore8 => {
            (OperationKind::MemoryStore, OpExtraData::Num(alloy_primitives::U256::from(1u32)))
        }
        opcode => (literal_operation_kind(opcode)?, OpExtraData::Empty),
    };
    add_operation(block, kind, inputs, outputs, extra)
}

fn literal_operation_kind(opcode: Opcode) -> Result<OperationKind, LowerError> {
    let kind = match opcode {
        Opcode::Add => OperationKind::Add,
        Opcode::Mul => OperationKind::Mul,
        Opcode::Sub => OperationKind::Sub,
        Opcode::Div => OperationKind::Div,
        Opcode::Sdiv => OperationKind::SDiv,
        Opcode::Mod => OperationKind::Mod,
        Opcode::Smod => OperationKind::SMod,
        Opcode::AddMod => OperationKind::AddMod,
        Opcode::MulMod => OperationKind::MulMod,
        Opcode::Exp => OperationKind::Exp,
        Opcode::SignExtend => OperationKind::SignExtend,
        Opcode::Lt => OperationKind::Lt,
        Opcode::Gt => OperationKind::Gt,
        Opcode::Slt => OperationKind::SLt,
        Opcode::Sgt => OperationKind::SGt,
        Opcode::Eq => OperationKind::Eq,
        Opcode::IsZero => OperationKind::IsZero,
        Opcode::And => OperationKind::And,
        Opcode::Or => OperationKind::Or,
        Opcode::Xor => OperationKind::Xor,
        Opcode::Not => OperationKind::Not,
        Opcode::Byte => OperationKind::Byte,
        Opcode::Shl => OperationKind::Shl,
        Opcode::Shr => OperationKind::Shr,
        Opcode::Sar => OperationKind::Sar,
        Opcode::Clz => OperationKind::Clz,
        Opcode::Keccak256 => OperationKind::Keccak256,
        Opcode::Address => OperationKind::Address,
        Opcode::Balance => OperationKind::Balance,
        Opcode::Origin => OperationKind::Origin,
        Opcode::Caller => OperationKind::Caller,
        Opcode::CallValue => OperationKind::CallValue,
        Opcode::CallDataLoad => OperationKind::CallDataLoad,
        Opcode::CallDataSize => OperationKind::CallDataSize,
        Opcode::CallDataCopy => OperationKind::CallDataCopy,
        Opcode::CodeSize => OperationKind::CodeSize,
        Opcode::CodeCopy => OperationKind::CodeCopy,
        Opcode::GasPrice => OperationKind::GasPrice,
        Opcode::ExtCodeSize => OperationKind::ExtCodeSize,
        Opcode::ExtCodeCopy => OperationKind::ExtCodeCopy,
        Opcode::ReturnDataSize => OperationKind::ReturnDataSize,
        Opcode::ReturnDataCopy => OperationKind::ReturnDataCopy,
        Opcode::ExtCodeHash => OperationKind::ExtCodeHash,
        Opcode::BlockHash => OperationKind::BlockHash,
        Opcode::Coinbase => OperationKind::Coinbase,
        Opcode::Timestamp => OperationKind::Timestamp,
        Opcode::Number => OperationKind::Number,
        Opcode::PrevRandao => OperationKind::Difficulty,
        Opcode::GasLimit => OperationKind::GasLimit,
        Opcode::ChainId => OperationKind::ChainId,
        Opcode::SelfBalance => OperationKind::SelfBalance,
        Opcode::BaseFee => OperationKind::BaseFee,
        Opcode::BlobHash => OperationKind::BlobHash,
        Opcode::BlobBaseFee => OperationKind::BlobBaseFee,
        Opcode::Gas => OperationKind::Gas,
        Opcode::SLoad => OperationKind::SLoad,
        Opcode::SStore => OperationKind::SStore,
        Opcode::TLoad => OperationKind::TLoad,
        Opcode::TStore => OperationKind::TStore,
        Opcode::MCopy => OperationKind::MemoryCopy,
        Opcode::Log0 => OperationKind::Log0,
        Opcode::Log1 => OperationKind::Log1,
        Opcode::Log2 => OperationKind::Log2,
        Opcode::Log3 => OperationKind::Log3,
        Opcode::Log4 => OperationKind::Log4,
        Opcode::Create => OperationKind::Create,
        Opcode::Call => OperationKind::Call,
        Opcode::CallCode => OperationKind::CallCode,
        Opcode::Return => OperationKind::Return,
        Opcode::DelegateCall => OperationKind::DelegateCall,
        Opcode::Create2 => OperationKind::Create2,
        Opcode::StaticCall => OperationKind::StaticCall,
        Opcode::Revert => OperationKind::Revert,
        Opcode::Invalid => OperationKind::Invalid,
        Opcode::SelfDestruct => OperationKind::SelfDestruct,
        Opcode::Stop => OperationKind::Stop,
        Opcode::Push0
        | Opcode::Push1
        | Opcode::Push2
        | Opcode::Push3
        | Opcode::Push4
        | Opcode::Push5
        | Opcode::Push6
        | Opcode::Push7
        | Opcode::Push8
        | Opcode::Push9
        | Opcode::Push10
        | Opcode::Push11
        | Opcode::Push12
        | Opcode::Push13
        | Opcode::Push14
        | Opcode::Push15
        | Opcode::Push16
        | Opcode::Push17
        | Opcode::Push18
        | Opcode::Push19
        | Opcode::Push20
        | Opcode::Push21
        | Opcode::Push22
        | Opcode::Push23
        | Opcode::Push24
        | Opcode::Push25
        | Opcode::Push26
        | Opcode::Push27
        | Opcode::Push28
        | Opcode::Push29
        | Opcode::Push30
        | Opcode::Push31
        | Opcode::Push32
        | Opcode::Pop
        | Opcode::Jump
        | Opcode::JumpI
        | Opcode::JumpDest
        | Opcode::Dup1
        | Opcode::Dup2
        | Opcode::Dup3
        | Opcode::Dup4
        | Opcode::Dup5
        | Opcode::Dup6
        | Opcode::Dup7
        | Opcode::Dup8
        | Opcode::Dup9
        | Opcode::Dup10
        | Opcode::Dup11
        | Opcode::Dup12
        | Opcode::Dup13
        | Opcode::Dup14
        | Opcode::Dup15
        | Opcode::Dup16
        | Opcode::Swap1
        | Opcode::Swap2
        | Opcode::Swap3
        | Opcode::Swap4
        | Opcode::Swap5
        | Opcode::Swap6
        | Opcode::Swap7
        | Opcode::Swap8
        | Opcode::Swap9
        | Opcode::Swap10
        | Opcode::Swap11
        | Opcode::Swap12
        | Opcode::Swap13
        | Opcode::Swap14
        | Opcode::Swap15
        | Opcode::Swap16
        | Opcode::MLoad
        | Opcode::MStore
        | Opcode::MStore8
        | Opcode::Pc
        | Opcode::MSize => return Err(LowerError::UnexpectedOpcode(opcode)),
    };
    Ok(kind)
}

#[derive(Debug, thiserror::Error)]
pub enum LowerError {
    #[error("unsupported reachable opcode {0}")]
    UnsupportedOpcode(Opcode),
    #[error("unexpected opcode {0} in semantic SSA operation")]
    UnexpectedOpcode(Opcode),
    #[error("callee f{callee} has not been lowered before caller f{caller}")]
    CalleeNotLowered { caller: FunctionCandidateId, callee: FunctionCandidateId },
    #[error("SSA value v{value} is undefined")]
    UndefinedSsaValue { value: SsaValueId },
    #[error("SSA block %{block} is undefined")]
    UndefinedSsaBlock { block: SsaBlockId },
    #[error("invalid SSA operation")]
    InvalidSsaOperation,
    #[error(transparent)]
    OperationBuild(#[from] OpBuildError),
    #[error(transparent)]
    ProgramBuild(#[from] BuildError),
    #[error(transparent)]
    Legalizer(#[from] LegalizerError),
}
