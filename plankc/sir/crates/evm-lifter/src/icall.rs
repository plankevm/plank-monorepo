use std::{collections::BTreeMap, fmt};

use plank_core::{Idx, IndexVec, Span};

use crate::{
    CodeBlockId, DecodedBytecode, FunctionCandidateId, InstructionId, Opcode,
    primitive_blocks::{PrimitiveBlocks, StaticJumpDestination},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallInferenceKind {
    ExactPattern,
    PropagatedDestination,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InferredCall {
    pub jump: InstructionId,
    pub destination_push: InstructionId,
    pub destination_pc: u32,
    pub continuation_pc: Option<u32>,
    pub function: FunctionCandidateId,
    pub kind: CallInferenceKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InferredFunction {
    pub entry_pc: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CodeBlock {
    pub instructions: Span<InstructionId>,
    pub start_pc: u32,
    pub end_pc: u32,
}

#[derive(Debug, Clone)]
pub struct CodeBlocks {
    blocks: IndexVec<CodeBlockId, CodeBlock>,
    jumpdestinations: BTreeMap<u32, CodeBlockId>,
    instruction_blocks: IndexVec<InstructionId, CodeBlockId>,
}

#[derive(Debug, Clone)]
pub struct InternalCallInference {
    calls: IndexVec<InstructionId, Option<InferredCall>>,
    functions: IndexVec<FunctionCandidateId, InferredFunction>,
    function_by_entry_pc: BTreeMap<u32, FunctionCandidateId>,
    internal_returns: IndexVec<InstructionId, bool>,
    static_jumps: IndexVec<InstructionId, Option<StaticJumpDestination>>,
    code_blocks: CodeBlocks,
}

pub fn infer_internal_calls(
    decoded: &DecodedBytecode,
    primitive: &PrimitiveBlocks,
) -> InternalCallInference {
    let instructions = decoded.instructions();
    let direct_jumps = instructions
        .enumerate_idx()
        .filter_map(|(id, instruction)| {
            matches!(instruction.op, Ok(Opcode::Jump | Opcode::JumpI))
                .then(|| {
                    primitive
                        .static_jump(id)
                        .map(|destination| (id, destination.push, destination.pc))
                })
                .flatten()
        })
        .collect::<Vec<_>>();
    let direct_destinations =
        direct_jumps.iter().map(|&(_, _, pc)| pc).collect::<std::collections::HashSet<_>>();
    let jumpdestinations = instructions
        .iter()
        .filter(|instruction| instruction.op == Ok(Opcode::JumpDest))
        .map(|instruction| instruction.pc)
        .collect::<std::collections::HashSet<_>>();

    let mut exact_calls = BTreeMap::<InstructionId, (InstructionId, u32, u32)>::new();
    for &(jump, push, destination_pc) in &direct_jumps {
        let instruction = decoded.instruction(jump);
        let Some(next_id) = instructions.get(jump + 1).map(|_| jump + 1) else { continue };
        let continuation = decoded.instruction(next_id);
        if instruction.op == Ok(Opcode::Jump)
            && continuation.op == Ok(Opcode::JumpDest)
            && !direct_destinations.contains(&continuation.pc)
            && jumpdestinations.contains(&destination_pc)
        {
            exact_calls.insert(jump, (push, destination_pc, continuation.pc));
        }
    }

    let (function_entry_pcs, active_calls) =
        discover_reachable_calls(decoded, &direct_jumps, &exact_calls);
    let mut functions = IndexVec::with_capacity(function_entry_pcs.len());
    let mut function_by_entry_pc = BTreeMap::new();
    for entry_pc in function_entry_pcs {
        let id = functions.push(InferredFunction { entry_pc });
        function_by_entry_pc.insert(entry_pc, id);
    }

    let mut calls = IndexVec::from_vec(vec![None; instructions.len()]);
    for &(jump, push, destination_pc) in &direct_jumps {
        if !active_calls.contains(&jump) {
            continue;
        }
        let function = function_by_entry_pc[&destination_pc];
        let (continuation_pc, kind) = match exact_calls.get(&jump) {
            Some(&(_, _, continuation_pc)) => {
                (Some(continuation_pc), CallInferenceKind::ExactPattern)
            }
            None => (None, CallInferenceKind::PropagatedDestination),
        };
        calls[jump] = Some(InferredCall {
            jump,
            destination_push: push,
            destination_pc,
            continuation_pc,
            function,
            kind,
        });
    }

    let mut internal_returns = IndexVec::from_vec(vec![false; instructions.len()]);
    let mut static_jumps = IndexVec::from_vec(vec![None; instructions.len()]);
    for (id, instruction) in instructions.enumerate_idx() {
        static_jumps[id] = primitive.static_jump(id);
        if instruction.op == Ok(Opcode::Jump) && static_jumps[id].is_none() {
            internal_returns[id] = true;
        }
    }

    let code_blocks = build_call_aware_blocks(decoded, &calls);
    InternalCallInference {
        calls,
        functions,
        function_by_entry_pc,
        internal_returns,
        static_jumps,
        code_blocks,
    }
}

fn discover_reachable_calls(
    decoded: &DecodedBytecode,
    direct_jumps: &[(InstructionId, InstructionId, u32)],
    exact_calls: &BTreeMap<InstructionId, (InstructionId, u32, u32)>,
) -> (std::collections::BTreeSet<u32>, std::collections::BTreeSet<InstructionId>) {
    let instructions = decoded.instructions();
    let direct_destinations = direct_jumps
        .iter()
        .map(|&(jump, _, destination)| (jump, destination))
        .collect::<BTreeMap<_, _>>();
    let mut function_entries = std::collections::BTreeSet::new();
    let mut active_calls = std::collections::BTreeSet::new();

    loop {
        let previous_function_count = function_entries.len();
        let previous_call_count = active_calls.len();
        let mut reached = IndexVec::from_vec(vec![false; instructions.len()]);
        let mut worklist = Vec::new();
        if let Some(root) = instructions.iter_idx().next() {
            worklist.push(root);
        }
        worklist
            .extend(function_entries.iter().filter_map(|&entry| decoded.instruction_at_pc(entry)));

        while let Some(instruction_id) = worklist.pop() {
            if std::mem::replace(&mut reached[instruction_id], true) {
                continue;
            }
            let instruction = decoded.instruction(instruction_id);
            let next = instructions.get(instruction_id + 1).map(|_| instruction_id + 1);
            match instruction.op {
                Err(_) => {}
                Ok(op) if op.is_terminating() => {}
                Ok(Opcode::Jump) => {
                    let Some(&destination) = direct_destinations.get(&instruction_id) else {
                        continue;
                    };
                    if exact_calls.contains_key(&instruction_id)
                        || function_entries.contains(&destination)
                    {
                        active_calls.insert(instruction_id);
                        function_entries.insert(destination);
                        if exact_calls.contains_key(&instruction_id)
                            && let Some(continuation) = next
                        {
                            worklist.push(continuation);
                        }
                        if let Some(callee) = decoded.instruction_at_pc(destination) {
                            worklist.push(callee);
                        }
                    } else if let Some(target) = decoded.instruction_at_pc(destination) {
                        worklist.push(target);
                    }
                }
                Ok(Opcode::JumpI) => {
                    if let Some(&destination) = direct_destinations.get(&instruction_id)
                        && let Some(target) = decoded.instruction_at_pc(destination)
                    {
                        worklist.push(target);
                    }
                    if let Some(next) = next {
                        worklist.push(next);
                    }
                }
                Ok(_) => {
                    if let Some(next) = next {
                        worklist.push(next);
                    }
                }
            }
        }

        if function_entries.len() == previous_function_count
            && active_calls.len() == previous_call_count
        {
            return (function_entries, active_calls);
        }
    }
}

fn build_call_aware_blocks(
    decoded: &DecodedBytecode,
    calls: &IndexVec<InstructionId, Option<InferredCall>>,
) -> CodeBlocks {
    let instructions = decoded.instructions();
    let mut ranges = Vec::with_capacity(instructions.len() / 4);
    let mut start = InstructionId::ZERO;
    let mut previous_was_merged_call = false;

    for (id, instruction) in instructions.enumerate_idx() {
        if instruction.op == Ok(Opcode::JumpDest) && start < id && !previous_was_merged_call {
            ranges.push(Span::new(start, id));
            start = id;
        }

        let merged_call = calls[id].is_some_and(|call| call.continuation_pc.is_some());
        if ((matches!(instruction.op, Ok(Opcode::Jump | Opcode::JumpI) | Err(_)) && !merged_call)
            || instruction.op.is_ok_and(Opcode::is_terminating))
            && start <= id
        {
            ranges.push(Span::new(start, id + 1));
            start = id + 1;
        }
        previous_was_merged_call = merged_call;
    }
    if start < instructions.len_idx() {
        ranges.push(Span::new(start, instructions.len_idx()));
    }

    let mut blocks = IndexVec::with_capacity(ranges.len());
    let mut instruction_blocks = IndexVec::from_vec(vec![CodeBlockId::ZERO; instructions.len()]);
    let mut jumpdestinations = BTreeMap::new();
    for range in ranges {
        let first = decoded.instruction(range.start);
        let last = decoded.instruction(range.end - 1);
        let block = blocks.push(CodeBlock {
            instructions: range,
            start_pc: first.pc,
            end_pc: last.actual_byte_range().end,
        });
        for instruction in range.iter() {
            instruction_blocks[instruction] = block;
            let instruction = decoded.instruction(instruction);
            if instruction.op == Ok(Opcode::JumpDest) {
                jumpdestinations.insert(instruction.pc, block);
            }
        }
    }
    CodeBlocks { blocks, jumpdestinations, instruction_blocks }
}

impl InternalCallInference {
    pub fn call(&self, instruction: InstructionId) -> Option<InferredCall> {
        self.calls[instruction]
    }

    pub fn calls(&self) -> impl Iterator<Item = InferredCall> + '_ {
        self.calls.iter().filter_map(|call| *call)
    }

    pub fn functions(&self) -> &IndexVec<FunctionCandidateId, InferredFunction> {
        &self.functions
    }

    pub fn function_by_entry_pc(&self, pc: u32) -> Option<FunctionCandidateId> {
        self.function_by_entry_pc.get(&pc).copied()
    }

    pub fn is_internal_return(&self, instruction: InstructionId) -> bool {
        self.internal_returns[instruction]
    }

    pub fn static_jump(&self, instruction: InstructionId) -> Option<StaticJumpDestination> {
        self.static_jumps[instruction]
    }

    pub fn code_blocks(&self) -> &CodeBlocks {
        &self.code_blocks
    }

    pub fn display<'a>(&'a self, decoded: &'a DecodedBytecode) -> InferenceDisplay<'a> {
        InferenceDisplay { decoded, inference: self }
    }
}

impl CodeBlocks {
    pub fn blocks(&self) -> &IndexVec<CodeBlockId, CodeBlock> {
        &self.blocks
    }

    pub fn block(&self, id: CodeBlockId) -> &CodeBlock {
        &self.blocks[id]
    }

    pub fn jumpdest_block(&self, pc: u32) -> Option<CodeBlockId> {
        self.jumpdestinations.get(&pc).copied()
    }

    pub fn instruction_block(&self, instruction: InstructionId) -> CodeBlockId {
        self.instruction_blocks[instruction]
    }
}

pub struct InferenceDisplay<'a> {
    decoded: &'a DecodedBytecode,
    inference: &'a InternalCallInference,
}

impl fmt::Display for InferenceDisplay<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "functions:")?;
        for (id, function) in self.inference.functions.enumerate_idx() {
            writeln!(f, "    f{id}: entry=0x{:x}", function.entry_pc)?;
        }
        writeln!(f, "blocks:")?;
        for (block_id, block) in self.inference.code_blocks.blocks.enumerate_idx() {
            writeln!(f, "    @{block_id} pc=[0x{:x},0x{:x})", block.start_pc, block.end_pc)?;
            for instruction_id in block.instructions.iter() {
                let instruction = self.decoded.instruction(instruction_id);
                write!(f, "        #{instruction_id} {:08x}: ", instruction.pc)?;
                match instruction.op {
                    Ok(op) => write!(f, "{op}")?,
                    Err(byte) => write!(f, "UNKNOWN(0x{byte:02X})")?,
                }
                if let Some(call) = self.inference.call(instruction_id) {
                    write!(f, " ; call f{}", call.function)?;
                    if let Some(continuation) = call.continuation_pc {
                        write!(f, " return=0x{continuation:x}")?;
                    } else {
                        write!(f, " propagated-without-continuation")?;
                    }
                } else if self.inference.is_internal_return(instruction_id) {
                    write!(f, " ; iret candidate")?;
                }
                writeln!(f)?;
            }
        }
        Ok(())
    }
}
