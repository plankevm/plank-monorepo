use std::fmt;

use alloy_primitives::U256;
use plank_core::IndexVec;

use crate::{
    CodeBlockId, DecodedBytecode, FunctionCandidateId, InstructionId, Opcode, SsaBlockId,
    SsaValueId,
    cfg::{BlockControl, ProvisionalCfg},
    icall::InternalCallInference,
    ownership::{FunctionKind, Ownership},
    symbolic::SymbolicAtom,
    verify::{FunctionArity, Verification},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SsaValueOrigin {
    BlockInput { block: SsaBlockId, position: u16 },
    Operation { block: SsaBlockId, operation: u32, output: u16 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SsaOperationKind {
    Opcode(Opcode),
    Constant(U256),
    InternalCall(FunctionCandidateId),
    SyntheticInvalid,
    SyntheticStop,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SsaOperation {
    pub source: Option<InstructionId>,
    pub kind: SsaOperationKind,
    pub inputs: Vec<SsaValueId>,
    pub outputs: Vec<SsaValueId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SsaControl {
    Goto(SsaBlockId),
    Branch { condition: SsaValueId, non_zero: SsaBlockId, zero: SsaBlockId },
    InternalReturn,
    Terminates,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SsaBlock {
    pub source: Option<CodeBlockId>,
    pub inputs: Vec<SsaValueId>,
    pub outputs: Vec<SsaValueId>,
    pub operations: Vec<SsaOperation>,
    pub control: SsaControl,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SsaFunction {
    pub entry: SsaBlockId,
    pub blocks: Vec<SsaBlockId>,
    pub kind: FunctionKind,
    pub inputs: u16,
    pub outputs: u16,
}

#[derive(Debug, Clone)]
pub struct SsaProgram {
    functions: IndexVec<FunctionCandidateId, SsaFunction>,
    blocks: IndexVec<SsaBlockId, SsaBlock>,
    values: IndexVec<SsaValueId, SsaValueOrigin>,
    source_blocks: IndexVec<FunctionCandidateId, IndexVec<CodeBlockId, Option<SsaBlockId>>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StackValue {
    Data(SsaValueId),
    FunctionReturn,
    CallReturn(InstructionId),
    Control(InstructionId),
}

pub fn build_ssa(
    decoded: &DecodedBytecode,
    inference: &InternalCallInference,
    cfg: &ProvisionalCfg,
    ownership: &Ownership,
    verification: &Verification,
) -> Result<SsaProgram, SsaError> {
    let mut blocks = IndexVec::new();
    let mut source_blocks = IndexVec::with_capacity(ownership.functions().len());
    for _ in ownership.functions().iter_idx() {
        source_blocks.push(IndexVec::from_vec(vec![None; inference.code_blocks().blocks().len()]));
    }
    for block in inference.code_blocks().blocks().iter_idx() {
        for &function in ownership.owners(block) {
            source_blocks[function][block] = Some(blocks.push(SsaBlock {
                source: Some(block),
                inputs: Vec::new(),
                outputs: Vec::new(),
                operations: Vec::new(),
                control: SsaControl::Terminates,
            }));
        }
    }

    let mut functions = IndexVec::with_capacity(ownership.functions().len());
    for (function, candidate) in ownership.functions().enumerate_idx() {
        let entry = source_blocks[function][candidate.entry]
            .expect("function entry should have a contextual SSA block");
        let arity = verification.function(function).arity;
        functions.push(SsaFunction {
            entry,
            blocks: inference
                .code_blocks()
                .blocks()
                .iter_idx()
                .filter(|&block| ownership.is_owned_by(block, function))
                .map(|block| {
                    source_blocks[function][block]
                        .expect("owned block should have a contextual SSA block")
                })
                .collect(),
            kind: candidate.kind,
            inputs: arity.sir_inputs(),
            outputs: arity.outputs(),
        });
    }

    let mut program = SsaProgram { functions, blocks, values: IndexVec::new(), source_blocks };
    for block in inference.code_blocks().blocks().iter_idx() {
        for &function in ownership.owners(block) {
            lower_block_to_ssa(
                function,
                block,
                decoded,
                inference,
                cfg,
                verification,
                &mut program,
            )?;
        }
    }
    Ok(program)
}

#[allow(clippy::too_many_arguments)]
fn lower_block_to_ssa(
    function: FunctionCandidateId,
    block: CodeBlockId,
    decoded: &DecodedBytecode,
    inference: &InternalCallInference,
    cfg: &ProvisionalCfg,
    verification: &Verification,
    program: &mut SsaProgram,
) -> Result<(), SsaError> {
    let ssa_block =
        program.source_blocks[function][block].expect("contextual block should be allocated");
    let arity = verification.function(function).arity;
    let state = verification
        .entry_state(function, block)
        .expect("owned reachable block should have a verified entry state");
    let symbolic_values = state
        .finite_values(arity.physical_inputs())
        .map_err(|_| SsaError::InvalidVerifiedStack { function, block })?;
    let return_input = match arity {
        FunctionArity::Returning(arity) => Some(arity.return_input),
        FunctionArity::Root => None,
    };
    let mut stack = Vec::with_capacity(symbolic_values.len());
    let mut inputs = Vec::new();
    for (position, value) in symbolic_values.into_iter().enumerate() {
        let stack_value = if return_input.is_some_and(|return_input| {
            value.len() == 1 && value.contains(&SymbolicAtom::FunctionInput(return_input))
        }) {
            StackValue::FunctionReturn
        } else if let Some(push) = unique_return_push(function, &value, verification) {
            StackValue::CallReturn(push)
        } else {
            let value = program.values.push(SsaValueOrigin::BlockInput {
                block: ssa_block,
                position: u16::try_from(inputs.len())
                    .map_err(|_| SsaError::StackTooDeep { block })?,
            });
            inputs.push(value);
            StackValue::Data(value)
        };
        let _ = position;
        stack.push(stack_value);
    }

    let mut operations = Vec::new();
    let mut branch_condition = None;
    let code_block = inference.code_blocks().block(block);
    for instruction_id in code_block.instructions.iter() {
        let instruction = decoded.instruction(instruction_id);
        match instruction.op {
            Err(_) => {
                push_operation(
                    ssa_block,
                    &mut program.values,
                    &mut operations,
                    Some(instruction_id),
                    SsaOperationKind::Opcode(Opcode::Invalid),
                    Vec::new(),
                    0,
                )?;
                break;
            }
            Ok(op) if op.is_push() || op == Opcode::Push0 => {
                if verification.is_return_push(function, instruction_id) {
                    stack.insert(0, StackValue::CallReturn(instruction_id));
                } else if verification.is_control_push(function, instruction_id) {
                    stack.insert(0, StackValue::Control(instruction_id));
                } else {
                    let value = if op == Opcode::Push0 {
                        U256::ZERO
                    } else {
                        instruction.immediate_as_u256().expect("PUSH has immediate")
                    };
                    let outputs = push_operation(
                        ssa_block,
                        &mut program.values,
                        &mut operations,
                        Some(instruction_id),
                        SsaOperationKind::Constant(value),
                        Vec::new(),
                        1,
                    )?;
                    stack.insert(0, StackValue::Data(outputs[0]));
                }
            }
            Ok(op) if op.is_dup().is_some() => {
                let depth = op.is_dup().expect("checked DUP") as usize;
                let value = *stack
                    .get(depth - 1)
                    .ok_or(SsaError::StackUnderflow { block, instruction: instruction_id })?;
                stack.insert(0, value);
            }
            Ok(op) if op.is_swap().is_some() => {
                let depth = op.is_swap().expect("checked SWAP") as usize;
                if stack.len() <= depth {
                    return Err(SsaError::StackUnderflow { block, instruction: instruction_id });
                }
                stack.swap(0, depth);
            }
            Ok(Opcode::Pop) => {
                pop(&mut stack, block, instruction_id)?;
            }
            Ok(Opcode::JumpDest) => {}
            Ok(Opcode::Jump) => {
                let destination = pop(&mut stack, block, instruction_id)?;
                if let Some(call) = inference.call(instruction_id) {
                    expect_control(destination, call.destination_push, block, instruction_id)?;
                    let callee = verification.function(call.function).arity;
                    let FunctionArity::Returning(callee) = callee else {
                        return Err(SsaError::InvalidVerifiedCall { call: instruction_id });
                    };
                    if stack.len() < callee.physical.inputs as usize {
                        return Err(SsaError::StackUnderflow {
                            block,
                            instruction: instruction_id,
                        });
                    }
                    let call_inputs =
                        stack.drain(..callee.physical.inputs as usize).collect::<Vec<_>>();
                    let expected_return = verification
                        .return_push(function, instruction_id)
                        .expect("verified returning call has return PUSH");
                    let mut inputs = Vec::with_capacity(callee.physical.inputs as usize - 1);
                    for (position, input) in call_inputs.into_iter().enumerate() {
                        if position == callee.return_input as usize {
                            if input != StackValue::CallReturn(expected_return) {
                                return Err(SsaError::InvalidVerifiedCall { call: instruction_id });
                            }
                        } else {
                            inputs.push(expect_data(input, block, instruction_id)?);
                        }
                    }
                    let outputs = push_operation(
                        ssa_block,
                        &mut program.values,
                        &mut operations,
                        Some(instruction_id),
                        SsaOperationKind::InternalCall(call.function),
                        inputs,
                        callee.physical.outputs,
                    )?;
                    for &output in outputs.iter().rev() {
                        stack.insert(0, StackValue::Data(output));
                    }
                } else if cfg.control(block) == BlockControl::InternalReturn {
                    if destination != StackValue::FunctionReturn {
                        return Err(SsaError::InvalidVerifiedReturn { function, block });
                    }
                } else if let Some(static_destination) = inference.static_jump(instruction_id) {
                    expect_control(destination, static_destination.push, block, instruction_id)?;
                }
            }
            Ok(Opcode::JumpI) => {
                let destination = pop(&mut stack, block, instruction_id)?;
                let condition = pop(&mut stack, block, instruction_id)?;
                let static_destination = inference
                    .static_jump(instruction_id)
                    .expect("verified JUMPI has direct destination");
                expect_control(destination, static_destination.push, block, instruction_id)?;
                branch_condition = Some(expect_data(condition, block, instruction_id)?);
            }
            Ok(op) => {
                let io = op.stack_io();
                let mut operation_inputs = Vec::with_capacity(io.inputs as usize);
                for _ in 0..io.inputs {
                    let value = pop(&mut stack, block, instruction_id)?;
                    operation_inputs.push(expect_data(value, block, instruction_id)?);
                }
                let outputs = push_operation(
                    ssa_block,
                    &mut program.values,
                    &mut operations,
                    Some(instruction_id),
                    SsaOperationKind::Opcode(op),
                    operation_inputs,
                    io.outputs,
                )?;
                for &output in outputs.iter().rev() {
                    stack.insert(0, StackValue::Data(output));
                }
                if op.is_terminating() {
                    break;
                }
            }
        }
    }

    let data_stack = stack
        .iter()
        .filter_map(|value| match value {
            StackValue::Data(value) => Some(*value),
            StackValue::FunctionReturn | StackValue::CallReturn(_) | StackValue::Control(_) => None,
        })
        .collect::<Vec<_>>();
    let control = match cfg.control(block) {
        BlockControl::Fallthrough(target) | BlockControl::Goto(target) => SsaControl::Goto(
            program.source_blocks[function][target]
                .expect("verified contextual target should have an SSA block"),
        ),
        BlockControl::Branch { non_zero, zero } => {
            let condition = branch_condition.ok_or(SsaError::MissingBranchCondition {
                instruction: code_block.instructions.end - 1,
            })?;
            let non_zero = program.source_blocks[function][non_zero]
                .expect("verified contextual target should have an SSA block");
            let zero = program.source_blocks[function][zero]
                .expect("verified contextual target should have an SSA block");
            SsaControl::Branch { condition, non_zero, zero }
        }
        BlockControl::InternalReturn => SsaControl::InternalReturn,
        BlockControl::Terminates => SsaControl::Terminates,
        BlockControl::EndOfCode => {
            push_operation(
                ssa_block,
                &mut program.values,
                &mut operations,
                None,
                SsaOperationKind::SyntheticStop,
                Vec::new(),
                0,
            )?;
            SsaControl::Terminates
        }
        BlockControl::InvalidJump => {
            push_operation(
                ssa_block,
                &mut program.values,
                &mut operations,
                None,
                SsaOperationKind::SyntheticInvalid,
                Vec::new(),
                0,
            )?;
            SsaControl::Terminates
        }
        BlockControl::UnresolvedJump
        | BlockControl::UnresolvedJumpI
        | BlockControl::UnresolvedCall => return Err(SsaError::InvalidVerifiedControl { block }),
    };
    let outputs = if matches!(control, SsaControl::Terminates) { Vec::new() } else { data_stack };
    program.blocks[ssa_block] =
        SsaBlock { source: Some(block), inputs, outputs, operations, control };
    Ok(())
}

fn unique_return_push(
    function: FunctionCandidateId,
    value: &std::collections::BTreeSet<SymbolicAtom>,
    verification: &Verification,
) -> Option<InstructionId> {
    let mut atoms = value.iter();
    let SymbolicAtom::Constant { instruction, .. } = atoms.next()? else { return None };
    (atoms.next().is_none() && verification.is_return_push(function, *instruction))
        .then_some(*instruction)
}

fn push_operation(
    block: SsaBlockId,
    values: &mut IndexVec<SsaValueId, SsaValueOrigin>,
    operations: &mut Vec<SsaOperation>,
    source: Option<InstructionId>,
    kind: SsaOperationKind,
    inputs: Vec<SsaValueId>,
    output_count: u16,
) -> Result<Vec<SsaValueId>, SsaError> {
    let operation =
        u32::try_from(operations.len()).map_err(|_| SsaError::TooManyOperations { block })?;
    let outputs = (0..output_count)
        .map(|output| values.push(SsaValueOrigin::Operation { block, operation, output }))
        .collect::<Vec<_>>();
    operations.push(SsaOperation { source, kind, inputs, outputs: outputs.clone() });
    Ok(outputs)
}

fn pop(
    stack: &mut Vec<StackValue>,
    block: CodeBlockId,
    instruction: InstructionId,
) -> Result<StackValue, SsaError> {
    if stack.is_empty() {
        Err(SsaError::StackUnderflow { block, instruction })
    } else {
        Ok(stack.remove(0))
    }
}

fn expect_data(
    value: StackValue,
    block: CodeBlockId,
    instruction: InstructionId,
) -> Result<SsaValueId, SsaError> {
    match value {
        StackValue::Data(value) => Ok(value),
        _ => Err(SsaError::ControlValueUsedSemantically { block, instruction }),
    }
}

fn expect_control(
    value: StackValue,
    push: InstructionId,
    block: CodeBlockId,
    instruction: InstructionId,
) -> Result<(), SsaError> {
    if value == StackValue::Control(push) {
        Ok(())
    } else {
        Err(SsaError::InvalidControlValue { block, instruction })
    }
}

impl SsaProgram {
    pub fn functions(&self) -> &IndexVec<FunctionCandidateId, SsaFunction> {
        &self.functions
    }

    pub fn blocks(&self) -> &IndexVec<SsaBlockId, SsaBlock> {
        &self.blocks
    }

    pub fn values(&self) -> &IndexVec<SsaValueId, SsaValueOrigin> {
        &self.values
    }

    pub fn source_block(
        &self,
        function: FunctionCandidateId,
        block: CodeBlockId,
    ) -> Option<SsaBlockId> {
        self.source_blocks[function][block]
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SsaError {
    #[error("verified stack for f{function} @{block} cannot be materialized")]
    InvalidVerifiedStack { function: FunctionCandidateId, block: CodeBlockId },
    #[error("stack underflow in @{block} at #{instruction}")]
    StackUnderflow { block: CodeBlockId, instruction: InstructionId },
    #[error("stack too deep in @{block}")]
    StackTooDeep { block: CodeBlockId },
    #[error("too many operations in SSA block {block}")]
    TooManyOperations { block: SsaBlockId },
    #[error("control value used semantically in @{block} at #{instruction}")]
    ControlValueUsedSemantically { block: CodeBlockId, instruction: InstructionId },
    #[error("invalid control value in @{block} at #{instruction}")]
    InvalidControlValue { block: CodeBlockId, instruction: InstructionId },
    #[error("invalid verified returning call #{call}")]
    InvalidVerifiedCall { call: InstructionId },
    #[error("invalid verified return in f{function} @{block}")]
    InvalidVerifiedReturn { function: FunctionCandidateId, block: CodeBlockId },
    #[error("missing branch condition at #{instruction}")]
    MissingBranchCondition { instruction: InstructionId },
    #[error("invalid verified control in @{block}")]
    InvalidVerifiedControl { block: CodeBlockId },
}

impl fmt::Display for SsaProgram {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (function, data) in self.functions.enumerate_idx() {
            writeln!(
                f,
                "fn f{function} {:?} inputs={} outputs={} entry=%{}",
                data.kind, data.inputs, data.outputs, data.entry
            )?;
            for &block in &data.blocks {
                let block_data = &self.blocks[block];
                write!(f, "    %{block}")?;
                for input in &block_data.inputs {
                    write!(f, " v{input}")?;
                }
                if !block_data.outputs.is_empty() {
                    write!(f, " ->")?;
                    for output in &block_data.outputs {
                        write!(f, " v{output}")?;
                    }
                }
                match block_data.source {
                    Some(source) => writeln!(f, " ; @{source}")?,
                    None => writeln!(f, " ; synthetic")?,
                }
                for operation in &block_data.operations {
                    write!(f, "        ")?;
                    if !operation.outputs.is_empty() {
                        for output in &operation.outputs {
                            write!(f, "v{output} ")?;
                        }
                        write!(f, "= ")?;
                    }
                    match operation.kind {
                        SsaOperationKind::Opcode(op) => write!(f, "{op}")?,
                        SsaOperationKind::Constant(value) => write!(f, "const {value:#x}")?,
                        SsaOperationKind::InternalCall(callee) => write!(f, "icall f{callee}")?,
                        SsaOperationKind::SyntheticInvalid => write!(f, "invalid [synthetic]")?,
                        SsaOperationKind::SyntheticStop => write!(f, "stop [synthetic]")?,
                    }
                    for input in &operation.inputs {
                        write!(f, " v{input}")?;
                    }
                    writeln!(f)?;
                }
                write!(f, "        => ")?;
                match block_data.control {
                    SsaControl::Goto(target) => writeln!(f, "%{target}")?,
                    SsaControl::Branch { condition, non_zero, zero } => {
                        writeln!(f, "v{condition} ? %{non_zero} : %{zero}")?
                    }
                    SsaControl::InternalReturn => writeln!(f, "iret")?,
                    SsaControl::Terminates => writeln!(f, "terminates")?,
                }
            }
        }
        Ok(())
    }
}
