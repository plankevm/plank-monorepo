use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
};

use plank_core::IndexVec;

use crate::{
    CodeBlockId, DecodedBytecode, FunctionCandidateId, InstructionId, Opcode, StackIO,
    cfg::{BlockControl, ProvisionalCfg},
    icall::InternalCallInference,
    ownership::{FunctionKind, Ownership},
    symbolic::{StackShapeMismatch, SymbolicAtom, SymbolicStack, SymbolicValue},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReturningArity {
    pub physical: StackIO,
    pub return_input: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FunctionArity {
    Root,
    Returning(ReturningArity),
}

impl FunctionArity {
    pub fn sir_inputs(self) -> u16 {
        match self {
            Self::Root => 0,
            Self::Returning(arity) => arity.physical.inputs - 1,
        }
    }

    pub fn outputs(self) -> u16 {
        match self {
            Self::Returning(arity) => arity.physical.outputs,
            Self::Root => 0,
        }
    }

    pub fn physical_inputs(self) -> u16 {
        match self {
            Self::Root => 0,
            Self::Returning(arity) => arity.physical.inputs,
        }
    }
}

#[derive(Debug, Clone)]
pub struct FunctionVerification {
    pub arity: FunctionArity,
    entry_states: IndexVec<CodeBlockId, Option<SymbolicStack>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct FunctionInstruction {
    function: FunctionCandidateId,
    instruction: InstructionId,
}

#[derive(Debug, Clone)]
pub struct Verification {
    functions: IndexVec<FunctionCandidateId, FunctionVerification>,
    postorder: Vec<FunctionCandidateId>,
    control_pushes: BTreeSet<FunctionInstruction>,
    return_pushes: BTreeMap<FunctionInstruction, InstructionId>,
}

pub fn verify(
    decoded: &DecodedBytecode,
    inference: &InternalCallInference,
    cfg: &ProvisionalCfg,
    ownership: &Ownership,
) -> Result<Verification, VerificationError> {
    let postorder = call_graph_postorder(inference, cfg, ownership)?;
    let mut functions = IndexVec::from_vec(vec![None; ownership.functions().len()]);
    let mut control_pushes = BTreeSet::new();
    let mut return_pushes = BTreeMap::new();
    let mut semantic_constant_uses = BTreeSet::new();

    for &function in &postorder {
        let verification = verify_function(
            function,
            decoded,
            inference,
            cfg,
            ownership,
            &functions,
            &mut control_pushes,
            &mut return_pushes,
            &mut semantic_constant_uses,
        )?;
        functions[function] = Some(verification);
    }

    for (&call, &return_push) in &return_pushes {
        if semantic_constant_uses
            .contains(&FunctionInstruction { function: call.function, instruction: return_push })
        {
            return Err(VerificationError::ReturnDestinationUsedSemantically {
                call: call.instruction,
                push: return_push,
            });
        }
    }

    let mut verified_functions = IndexVec::with_capacity(functions.len());
    for function in functions.raw {
        verified_functions.push(function.expect("postorder should include every function"));
    }
    Ok(Verification { functions: verified_functions, postorder, control_pushes, return_pushes })
}

fn call_graph_postorder(
    inference: &InternalCallInference,
    cfg: &ProvisionalCfg,
    ownership: &Ownership,
) -> Result<Vec<FunctionCandidateId>, VerificationError> {
    let mut callees = IndexVec::from_vec(vec![BTreeSet::new(); ownership.functions().len()]);
    for block in inference.code_blocks().blocks().iter_idx() {
        for &owner in ownership.owners(block) {
            for &call_instruction in cfg.calls(block) {
                let call = inference.call(call_instruction).expect("CFG call should be inferred");
                callees[owner].insert(call.function);
            }
        }
    }

    #[derive(Clone, Copy, PartialEq, Eq)]
    enum Visit {
        New,
        Active,
        Done,
    }
    fn visit(
        function: FunctionCandidateId,
        callees: &IndexVec<FunctionCandidateId, BTreeSet<FunctionCandidateId>>,
        states: &mut IndexVec<FunctionCandidateId, Visit>,
        postorder: &mut Vec<FunctionCandidateId>,
    ) -> Result<(), VerificationError> {
        match states[function] {
            Visit::Done => return Ok(()),
            Visit::Active => return Err(VerificationError::RecursiveCall { function }),
            Visit::New => {}
        }
        states[function] = Visit::Active;
        for &callee in &callees[function] {
            visit(callee, callees, states, postorder)?;
        }
        states[function] = Visit::Done;
        postorder.push(function);
        Ok(())
    }

    let mut states = IndexVec::from_vec(vec![Visit::New; ownership.functions().len()]);
    let mut postorder = Vec::with_capacity(ownership.functions().len());
    visit(ownership.root(), &callees, &mut states, &mut postorder)?;
    if postorder.len() != ownership.functions().len() {
        return Err(VerificationError::UnreachableFunctionCandidate);
    }
    Ok(postorder)
}

#[allow(clippy::too_many_arguments)]
fn verify_function(
    function: FunctionCandidateId,
    decoded: &DecodedBytecode,
    inference: &InternalCallInference,
    cfg: &ProvisionalCfg,
    ownership: &Ownership,
    verified: &IndexVec<FunctionCandidateId, Option<FunctionVerification>>,
    control_pushes: &mut BTreeSet<FunctionInstruction>,
    return_pushes: &mut BTreeMap<FunctionInstruction, InstructionId>,
    semantic_constant_uses: &mut BTreeSet<FunctionInstruction>,
) -> Result<FunctionVerification, VerificationError> {
    let candidate = ownership.functions()[function];
    let open = candidate.kind != FunctionKind::Root;
    let initial = if open { SymbolicStack::open() } else { SymbolicStack::empty() };
    let mut entry_states = IndexVec::from_vec(vec![None; inference.code_blocks().blocks().len()]);
    entry_states[candidate.entry] = Some(initial);
    let mut worklist = vec![candidate.entry];
    let mut return_arities = IndexVec::from_vec(vec![None; inference.code_blocks().blocks().len()]);

    while let Some(block) = worklist.pop() {
        if !ownership.is_owned_by(block, function) {
            return Err(VerificationError::WrongFunctionOwner { function, block });
        }
        let mut stack = entry_states[block].clone().expect("queued block should have a state");
        let code_block = inference.code_blocks().block(block);
        for instruction_id in code_block.instructions.iter() {
            let instruction = decoded.instruction(instruction_id);
            match instruction.op {
                Err(_) => break,
                Ok(op) if op.is_push() => {
                    stack.push_atom(SymbolicAtom::Constant {
                        instruction: instruction_id,
                        value: instruction.immediate_as_u256().expect("PUSH has immediate"),
                    });
                }
                Ok(Opcode::Push0) => stack.push_atom(SymbolicAtom::Constant {
                    instruction: instruction_id,
                    value: alloy_primitives::U256::ZERO,
                }),
                Ok(op) if op.is_dup().is_some() => stack
                    .duplicate(op.is_dup().expect("checked DUP"))
                    .map_err(|_| underflow(function, block, instruction_id))?,
                Ok(op) if op.is_swap().is_some() => stack
                    .swap(op.is_swap().expect("checked SWAP"))
                    .map_err(|_| underflow(function, block, instruction_id))?,
                Ok(Opcode::Pop) => {
                    stack.pop().map_err(|_| underflow(function, block, instruction_id))?;
                }
                Ok(Opcode::JumpDest) => {}
                Ok(Opcode::Jump) => {
                    let destination =
                        stack.pop().map_err(|_| underflow(function, block, instruction_id))?;
                    if let Some(call) = inference.call(instruction_id) {
                        verify_direct_control_value(
                            &destination,
                            call.destination_push,
                            call.destination_pc,
                            function,
                            block,
                            instruction_id,
                        )?;
                        control_pushes.insert(FunctionInstruction {
                            function,
                            instruction: call.destination_push,
                        });
                        let callee = verified[call.function]
                            .as_ref()
                            .expect("callee should precede caller in postorder");
                        let FunctionArity::Returning(callee_arity) = callee.arity else {
                            return Err(VerificationError::ReturningCallToNonReturningFunction {
                                call: instruction_id,
                                callee: call.function,
                            });
                        };
                        verify_and_apply_call(
                            function,
                            block,
                            instruction_id,
                            call.continuation_pc,
                            callee_arity,
                            &mut stack,
                            return_pushes,
                        )?;
                    } else if cfg.control(block) == BlockControl::InternalReturn {
                        let arity = verify_return(
                            function,
                            block,
                            instruction_id,
                            destination,
                            &mut stack,
                        )?;
                        return_arities[block] = Some(arity);
                    } else if let Some(static_destination) = inference.static_jump(instruction_id) {
                        verify_direct_control_value(
                            &destination,
                            static_destination.push,
                            static_destination.pc,
                            function,
                            block,
                            instruction_id,
                        )?;
                        control_pushes.insert(FunctionInstruction {
                            function,
                            instruction: static_destination.push,
                        });
                    }
                }
                Ok(Opcode::JumpI) => {
                    let destination =
                        stack.pop().map_err(|_| underflow(function, block, instruction_id))?;
                    let condition =
                        stack.pop().map_err(|_| underflow(function, block, instruction_id))?;
                    record_semantic_constants(function, &condition, semantic_constant_uses);
                    if let Some(static_destination) = inference.static_jump(instruction_id) {
                        verify_direct_control_value(
                            &destination,
                            static_destination.push,
                            static_destination.pc,
                            function,
                            block,
                            instruction_id,
                        )?;
                        control_pushes.insert(FunctionInstruction {
                            function,
                            instruction: static_destination.push,
                        });
                    }
                }
                Ok(op) => {
                    let io = op.stack_io();
                    for _ in 0..io.inputs {
                        let value =
                            stack.pop().map_err(|_| underflow(function, block, instruction_id))?;
                        record_semantic_constants(function, &value, semantic_constant_uses);
                    }
                    if op.is_terminating() {
                        break;
                    }
                    for output in (0..io.outputs).rev() {
                        stack.push_atom(SymbolicAtom::InstructionResult {
                            instruction: instruction_id,
                            output: output.try_into().expect("opcode output count fits u8"),
                        });
                    }
                }
            }
        }

        match cfg.control(block) {
            BlockControl::Fallthrough(target) | BlockControl::Goto(target) => {
                propagate(
                    function,
                    block,
                    target,
                    stack,
                    ownership,
                    &mut entry_states,
                    &mut worklist,
                )?;
            }
            BlockControl::Branch { non_zero, zero } => {
                for target in [non_zero, zero] {
                    propagate(
                        function,
                        block,
                        target,
                        stack.clone(),
                        ownership,
                        &mut entry_states,
                        &mut worklist,
                    )?;
                }
            }
            BlockControl::InternalReturn => {
                if candidate.kind != FunctionKind::Returning {
                    return Err(VerificationError::UnexpectedReturn { function, block });
                }
            }
            BlockControl::Terminates | BlockControl::EndOfCode | BlockControl::InvalidJump => {}
            BlockControl::UnresolvedJump
            | BlockControl::UnresolvedJumpI
            | BlockControl::UnresolvedCall => {
                return Err(VerificationError::UnresolvedControl { function, block });
            }
        }
    }

    let arity = match candidate.kind {
        FunctionKind::Root => FunctionArity::Root,
        FunctionKind::Returning => {
            let mut returns = return_arities.iter().flatten().copied();
            let arity = returns.next().ok_or(VerificationError::NoInternalReturn { function })?;
            for actual in returns {
                if actual != arity {
                    return Err(VerificationError::InconsistentReturnArity {
                        function,
                        expected: arity,
                        actual,
                    });
                }
            }
            FunctionArity::Returning(arity)
        }
    };
    Ok(FunctionVerification { arity, entry_states })
}

fn verify_and_apply_call(
    function: FunctionCandidateId,
    block: CodeBlockId,
    call: InstructionId,
    continuation_pc: Option<u32>,
    callee: ReturningArity,
    stack: &mut SymbolicStack,
    return_pushes: &mut BTreeMap<FunctionInstruction, InstructionId>,
) -> Result<(), VerificationError> {
    let continuation_pc =
        continuation_pc.ok_or(VerificationError::MissingCallContinuation { call })?;
    stack
        .materialize(callee.physical.inputs as usize)
        .map_err(|_| underflow(function, block, call))?;
    let return_value = &stack.values_top_first()[callee.return_input as usize];
    let mut return_atoms = return_value.iter().copied();
    let Some(SymbolicAtom::Constant { instruction: return_push, value: return_pc }) =
        return_atoms.next()
    else {
        return Err(VerificationError::AmbiguousReturnDestination { call });
    };
    if return_atoms.next().is_some() {
        return Err(VerificationError::AmbiguousReturnDestination { call });
    }
    if return_pc != alloy_primitives::U256::from(continuation_pc) {
        return Err(VerificationError::WrongReturnDestination {
            call,
            expected_pc: continuation_pc,
            actual: return_pc,
        });
    }
    let expected_atom = SymbolicAtom::Constant { instruction: return_push, value: return_pc };
    let occurrences =
        stack.values_top_first().iter().filter(|value| value.contains(&expected_atom)).count();
    if occurrences != 1 {
        return Err(VerificationError::DuplicatedReturnDestination { call, push: return_push });
    }
    if return_pushes
        .insert(FunctionInstruction { function, instruction: call }, return_push)
        .is_some_and(|old| old != return_push)
    {
        return Err(VerificationError::AmbiguousReturnDestination { call });
    }
    for _ in 0..callee.physical.inputs {
        stack.pop().expect("materialized call inputs");
    }
    for output in (0..callee.physical.outputs).rev() {
        stack.push_atom(SymbolicAtom::CallResult { call, output });
    }
    Ok(())
}

fn verify_return(
    function: FunctionCandidateId,
    block: CodeBlockId,
    instruction: InstructionId,
    destination: SymbolicValue,
    stack: &mut SymbolicStack,
) -> Result<ReturningArity, VerificationError> {
    let mut destination_atoms = destination.iter().copied();
    let Some(SymbolicAtom::FunctionInput(return_input)) = destination_atoms.next() else {
        return Err(VerificationError::InvalidReturnDestination { function, block, instruction });
    };
    if destination_atoms.next().is_some() {
        return Err(VerificationError::InvalidReturnDestination { function, block, instruction });
    }
    stack.normalize();
    let physical_inputs = stack.required_inputs();
    if return_input >= physical_inputs || stack.next_input() > physical_inputs {
        return Err(VerificationError::ReturnInputOutOfBounds {
            function,
            return_input,
            physical_inputs,
        });
    }
    for value in stack.values_top_first() {
        if value.iter().any(
            |atom| matches!(atom, SymbolicAtom::FunctionInput(input) if *input >= physical_inputs),
        ) {
            return Err(VerificationError::InvalidReturnValue { function, block });
        }
    }
    let implicit_outputs = physical_inputs - stack.next_input();
    let explicit_outputs = u16::try_from(stack.values_top_first().len())
        .map_err(|_| VerificationError::StackTooDeep { function, block })?;
    Ok(ReturningArity {
        physical: StackIO {
            inputs: physical_inputs,
            outputs: explicit_outputs
                .checked_add(implicit_outputs)
                .ok_or(VerificationError::StackTooDeep { function, block })?,
        },
        return_input,
    })
}

fn propagate(
    function: FunctionCandidateId,
    source: CodeBlockId,
    target: CodeBlockId,
    mut state: SymbolicStack,
    ownership: &Ownership,
    entry_states: &mut IndexVec<CodeBlockId, Option<SymbolicStack>>,
    worklist: &mut Vec<CodeBlockId>,
) -> Result<(), VerificationError> {
    if !ownership.is_owned_by(target, function) {
        return Err(VerificationError::CrossFunctionCfgEdge {
            function,
            source_block: source,
            target,
        });
    }
    state.normalize();
    match &mut entry_states[target] {
        Some(existing) => {
            let changed = existing.merge(&state).map_err(|error| {
                VerificationError::IncomingStackMismatch {
                    function,
                    source_block: source,
                    target,
                    error,
                }
            })?;
            if changed {
                worklist.push(target);
            }
        }
        slot @ None => {
            *slot = Some(state);
            worklist.push(target);
        }
    }
    Ok(())
}

fn verify_direct_control_value(
    value: &SymbolicValue,
    push: InstructionId,
    pc: u32,
    function: FunctionCandidateId,
    block: CodeBlockId,
    instruction: InstructionId,
) -> Result<(), VerificationError> {
    let expected =
        SymbolicAtom::Constant { instruction: push, value: alloy_primitives::U256::from(pc) };
    if value.len() == 1 && value.contains(&expected) {
        Ok(())
    } else {
        Err(VerificationError::InvalidDirectControlValue { function, block, instruction })
    }
}

fn record_semantic_constants(
    function: FunctionCandidateId,
    value: &SymbolicValue,
    uses: &mut BTreeSet<FunctionInstruction>,
) {
    uses.extend(value.iter().filter_map(|atom| match atom {
        SymbolicAtom::Constant { instruction, .. } => {
            Some(FunctionInstruction { function, instruction: *instruction })
        }
        _ => None,
    }));
}

fn underflow(
    function: FunctionCandidateId,
    block: CodeBlockId,
    instruction: impl Into<InstructionId>,
) -> VerificationError {
    VerificationError::StackUnderflow { function, block, instruction: instruction.into() }
}

impl Verification {
    pub fn function(&self, function: FunctionCandidateId) -> &FunctionVerification {
        &self.functions[function]
    }

    pub fn postorder(&self) -> &[FunctionCandidateId] {
        &self.postorder
    }

    pub fn is_control_push(
        &self,
        function: FunctionCandidateId,
        instruction: InstructionId,
    ) -> bool {
        self.control_pushes.contains(&FunctionInstruction { function, instruction })
    }

    pub fn return_push(
        &self,
        function: FunctionCandidateId,
        call: InstructionId,
    ) -> Option<InstructionId> {
        self.return_pushes.get(&FunctionInstruction { function, instruction: call }).copied()
    }

    pub fn is_return_push(
        &self,
        function: FunctionCandidateId,
        instruction: InstructionId,
    ) -> bool {
        self.return_pushes
            .iter()
            .any(|(site, &push)| site.function == function && push == instruction)
    }

    pub fn entry_state(
        &self,
        function: FunctionCandidateId,
        block: CodeBlockId,
    ) -> Option<&SymbolicStack> {
        self.functions[function].entry_states[block].as_ref()
    }

    pub fn display<'a>(&'a self, ownership: &'a Ownership) -> VerificationDisplay<'a> {
        VerificationDisplay { verification: self, ownership }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum VerificationError {
    #[error("inferred function candidate is not reachable from the PC 0 call graph")]
    UnreachableFunctionCandidate,
    #[error("recursive internal call involving f{function}")]
    RecursiveCall { function: FunctionCandidateId },
    #[error("block @{block} is not owned by f{function}")]
    WrongFunctionOwner { function: FunctionCandidateId, block: CodeBlockId },
    #[error("stack underflow in f{function} @{block} at instruction #{instruction}")]
    StackUnderflow { function: FunctionCandidateId, block: CodeBlockId, instruction: InstructionId },
    #[error("unresolved control in f{function} @{block}")]
    UnresolvedControl { function: FunctionCandidateId, block: CodeBlockId },
    #[error("f{function} has no internal return")]
    NoInternalReturn { function: FunctionCandidateId },
    #[error("unexpected internal return in f{function} @{block}")]
    UnexpectedReturn { function: FunctionCandidateId, block: CodeBlockId },
    #[error("invalid return destination in f{function} @{block} at #{instruction}")]
    InvalidReturnDestination {
        function: FunctionCandidateId,
        block: CodeBlockId,
        instruction: InstructionId,
    },
    #[error("invalid returned value in f{function} @{block}")]
    InvalidReturnValue { function: FunctionCandidateId, block: CodeBlockId },
    #[error(
        "return input {return_input} is outside f{function}'s {physical_inputs} physical inputs"
    )]
    ReturnInputOutOfBounds {
        function: FunctionCandidateId,
        return_input: u16,
        physical_inputs: u16,
    },
    #[error("inconsistent return arity in f{function}: expected {expected:?}, got {actual:?}")]
    InconsistentReturnArity {
        function: FunctionCandidateId,
        expected: ReturningArity,
        actual: ReturningArity,
    },
    #[error("stack too deep in f{function} @{block}")]
    StackTooDeep { function: FunctionCandidateId, block: CodeBlockId },
    #[error("incoming stack mismatch from @{source_block} to @{target} in f{function}: {error}")]
    IncomingStackMismatch {
        function: FunctionCandidateId,
        source_block: CodeBlockId,
        target: CodeBlockId,
        error: StackShapeMismatch,
    },
    #[error("CFG edge from @{source_block} to @{target} crosses out of f{function}")]
    CrossFunctionCfgEdge {
        function: FunctionCandidateId,
        source_block: CodeBlockId,
        target: CodeBlockId,
    },
    #[error("invalid direct control value in f{function} @{block} at #{instruction}")]
    InvalidDirectControlValue {
        function: FunctionCandidateId,
        block: CodeBlockId,
        instruction: InstructionId,
    },
    #[error("returning call #{call} targets non-returning f{callee}")]
    ReturningCallToNonReturningFunction { call: InstructionId, callee: FunctionCandidateId },
    #[error("call #{call} has no continuation")]
    MissingCallContinuation { call: InstructionId },
    #[error("call #{call} has an ambiguous return destination")]
    AmbiguousReturnDestination { call: InstructionId },
    #[error("call #{call} returns to {actual:#x}, expected {expected_pc:#x}")]
    WrongReturnDestination { call: InstructionId, expected_pc: u32, actual: alloy_primitives::U256 },
    #[error("call #{call} duplicates return-destination PUSH #{push}")]
    DuplicatedReturnDestination { call: InstructionId, push: InstructionId },
    #[error("call #{call} uses return-destination PUSH #{push} semantically")]
    ReturnDestinationUsedSemantically { call: InstructionId, push: InstructionId },
}

pub struct VerificationDisplay<'a> {
    verification: &'a Verification,
    ownership: &'a Ownership,
}

impl fmt::Display for VerificationDisplay<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for &function in &self.verification.postorder {
            let candidate = self.ownership.functions()[function];
            let verified = self.verification.function(function);
            writeln!(f, "f{function} {:?} {:?}", candidate.kind, verified.arity)?;
            for block in self.verification.functions[function].entry_states.iter_idx() {
                if !self.ownership.is_owned_by(block, function) {
                    continue;
                }
                let Some(state) = self.verification.entry_state(function, block) else { continue };
                write!(f, "    @{block}: [")?;
                for (index, value) in state.values_top_first().iter().enumerate() {
                    if index != 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{value:?}")?;
                }
                if state.is_open() {
                    if !state.values_top_first().is_empty() {
                        write!(f, ", ")?;
                    }
                    write!(f, "input{}..", state.next_input())?;
                }
                writeln!(f, "]")?;
            }
        }
        if !self.verification.return_pushes.is_empty() {
            writeln!(f, "return pushes:")?;
            for (call, push) in &self.verification.return_pushes {
                writeln!(f, "    f{} call #{} <- push #{}", call.function, call.instruction, push)?;
            }
        }
        Ok(())
    }
}
