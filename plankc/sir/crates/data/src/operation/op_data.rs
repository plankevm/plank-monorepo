use crate::{EthIRProgram, Function, builder::EthIRBuilder, index::*};
use alloy_primitives::{U256, ruint::FromUintError};
use plank_core::{Idx, IndexVec, Span};
use std::fmt;

/// Operand access, arena relocation and formatting of an operation's data. `define_operations!`
/// dispatches every `Operation` variant to its data's implementation, so a new data struct only
/// needs this one impl. There are intentionally no defaults: a silently "empty" answer for e.g.
/// `allocated_spans` would be a hard to find bug.
pub(crate) trait OpData {
    fn inputs<'a>(&'a self, ir: &'a EthIRProgram) -> &'a [LocalId];

    fn outputs<'a>(&'a self, ir: &'a EthIRProgram) -> &'a [LocalId];

    fn inputs_mut<'a>(
        &'a mut self,
        locals: &'a mut IndexVec<LocalIdx, LocalId>,
    ) -> &'a mut [LocalId];

    fn outputs_mut<'a>(
        &'a mut self,
        locals: &'a mut IndexVec<LocalIdx, LocalId>,
        functions: &'a IndexVec<FunctionId, Function>,
    ) -> &'a mut [LocalId];

    fn allocated_spans(&self, ir: &EthIRProgram) -> AllocatedSpans;

    /// Re-points arena-backed operands at the copies returned by `clone_span`.
    fn clone_allocated(
        &mut self,
        functions: &IndexVec<FunctionId, Function>,
        clone_span: &mut impl FnMut(Span<LocalIdx>) -> LocalIdx,
    );

    fn fmt_op(&self, f: &mut impl fmt::Write, ir: &EthIRProgram, mnemonic: &str) -> fmt::Result;
}

#[derive(Debug)]
pub struct AllocatedSpans {
    pub input: Option<Span<LocalIdx>>,
    pub output: Option<Span<LocalIdx>>,
}

impl AllocatedSpans {
    pub const NONE: Self = Self { input: None, output: None };
}

fn fmt_locals(f: &mut impl fmt::Write, locals: &[LocalId]) -> fmt::Result {
    let Some((first, rest)) = locals.split_first() else {
        return Ok(());
    };
    write!(f, "${}", first)?;
    for local in rest {
        write!(f, " ${}", local)?;
    }
    Ok(())
}

/// Writes `[outs =] mnemonic [extra] [ins]`, the textual layout shared by all operations.
fn write_op(
    f: &mut impl fmt::Write,
    outs: &[LocalId],
    mnemonic: impl fmt::Display,
    extra: Option<fmt::Arguments<'_>>,
    ins: &[LocalId],
) -> fmt::Result {
    fmt_locals(f, outs)?;
    if outs.is_empty() {
        write!(f, "{mnemonic}")?;
    } else {
        write!(f, " = {mnemonic}")?;
    }
    if let Some(extra) = extra {
        write!(f, " {extra}")?;
    }
    if !ins.is_empty() {
        write!(f, " ")?;
    }
    fmt_locals(f, ins)
}

/// Data whose operands are all stored inline, implementing this instead of `OpData` explicitly
/// opts out of arena handling.
pub(crate) trait InlineOpData {
    fn ins(&self) -> &[LocalId];

    fn outs(&self) -> &[LocalId];

    fn ins_mut(&mut self) -> &mut [LocalId];

    fn outs_mut(&mut self) -> &mut [LocalId];

    fn fmt_inline(&self, f: &mut impl fmt::Write, ir: &EthIRProgram, mnemonic: &str)
    -> fmt::Result;
}

impl<D: InlineOpData> OpData for D {
    fn inputs<'a>(&'a self, _ir: &'a EthIRProgram) -> &'a [LocalId] {
        self.ins()
    }

    fn outputs<'a>(&'a self, _ir: &'a EthIRProgram) -> &'a [LocalId] {
        self.outs()
    }

    fn inputs_mut<'a>(
        &'a mut self,
        _locals: &'a mut IndexVec<LocalIdx, LocalId>,
    ) -> &'a mut [LocalId] {
        self.ins_mut()
    }

    fn outputs_mut<'a>(
        &'a mut self,
        _locals: &'a mut IndexVec<LocalIdx, LocalId>,
        _functions: &'a IndexVec<FunctionId, Function>,
    ) -> &'a mut [LocalId] {
        self.outs_mut()
    }

    fn allocated_spans(&self, _ir: &EthIRProgram) -> AllocatedSpans {
        AllocatedSpans::NONE
    }

    fn clone_allocated(
        &mut self,
        _functions: &IndexVec<FunctionId, Function>,
        _clone_span: &mut impl FnMut(Span<LocalIdx>) -> LocalIdx,
    ) {
    }

    fn fmt_op(&self, f: &mut impl fmt::Write, ir: &EthIRProgram, mnemonic: &str) -> fmt::Result {
        self.fmt_inline(f, ir, mnemonic)
    }
}

impl InlineOpData for () {
    fn ins(&self) -> &[LocalId] {
        &[]
    }

    fn outs(&self) -> &[LocalId] {
        &[]
    }

    fn ins_mut(&mut self) -> &mut [LocalId] {
        &mut []
    }

    fn outs_mut(&mut self) -> &mut [LocalId] {
        &mut []
    }

    fn fmt_inline(
        &self,
        f: &mut impl fmt::Write,
        _ir: &EthIRProgram,
        mnemonic: &str,
    ) -> fmt::Result {
        write_op(f, &[], mnemonic, None, &[])
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpExtraData {
    DataId(DataId),
    FuncId(FunctionId),
    Num(U256),
    Empty,
}

#[derive(Debug, Clone, Copy)]
pub struct InlineOperands<const INS: usize, const OUTS: usize> {
    pub ins: [LocalId; INS],
    pub outs: [LocalId; OUTS],
}

impl Default for InlineOperands<0, 0> {
    fn default() -> Self {
        Self { ins: [], outs: [] }
    }
}

impl<const INS: usize, const OUTS: usize> InlineOpData for InlineOperands<INS, OUTS> {
    fn ins(&self) -> &[LocalId] {
        &self.ins
    }

    fn outs(&self) -> &[LocalId] {
        &self.outs
    }

    fn ins_mut(&mut self) -> &mut [LocalId] {
        &mut self.ins
    }

    fn outs_mut(&mut self) -> &mut [LocalId] {
        &mut self.outs
    }

    fn fmt_inline(
        &self,
        f: &mut impl fmt::Write,
        _ir: &EthIRProgram,
        mnemonic: &str,
    ) -> fmt::Result {
        write_op(f, &self.outs, mnemonic, None, &self.ins)
    }
}

/// Operation data where inputs are allocated in the IR but outputs are stored inline.
#[derive(Debug, Clone, Copy)]
pub struct AllocatedIns<const INS: usize, const OUTS: usize> {
    pub ins_start: LocalIdx,
    pub outs: [LocalId; OUTS],
}

impl<const INS: usize, const OUTS: usize> AllocatedIns<INS, OUTS> {
    pub fn inputs_span(&self) -> Span<LocalIdx> {
        Span::new(self.ins_start, self.ins_start + INS as u32)
    }

    pub fn get_inputs<'ir>(&self, ir: &'ir EthIRProgram) -> &'ir [LocalId; INS] {
        let ins_start = self.ins_start.idx();
        ir.locals.as_raw_slice()[ins_start..ins_start + INS].as_array().unwrap()
    }

    pub fn get_inputs_mut<'ir>(&self, ir: &'ir mut EthIRProgram) -> &'ir mut [LocalId] {
        let ins_start = self.ins_start.idx();
        &mut ir.locals.as_raw_slice_mut()[ins_start..ins_start + INS]
    }
}

impl<const INS: usize, const OUTS: usize> OpData for AllocatedIns<INS, OUTS> {
    fn inputs<'a>(&'a self, ir: &'a EthIRProgram) -> &'a [LocalId] {
        self.get_inputs(ir)
    }

    fn outputs<'a>(&'a self, _ir: &'a EthIRProgram) -> &'a [LocalId] {
        &self.outs
    }

    fn inputs_mut<'a>(
        &'a mut self,
        locals: &'a mut IndexVec<LocalIdx, LocalId>,
    ) -> &'a mut [LocalId] {
        &mut locals[self.inputs_span()]
    }

    fn outputs_mut<'a>(
        &'a mut self,
        _locals: &'a mut IndexVec<LocalIdx, LocalId>,
        _functions: &'a IndexVec<FunctionId, Function>,
    ) -> &'a mut [LocalId] {
        &mut self.outs
    }

    fn allocated_spans(&self, _ir: &EthIRProgram) -> AllocatedSpans {
        AllocatedSpans { input: Some(self.inputs_span()), output: None }
    }

    fn clone_allocated(
        &mut self,
        _functions: &IndexVec<FunctionId, Function>,
        clone_span: &mut impl FnMut(Span<LocalIdx>) -> LocalIdx,
    ) {
        self.ins_start = clone_span(self.inputs_span());
    }

    fn fmt_op(&self, f: &mut impl fmt::Write, ir: &EthIRProgram, mnemonic: &str) -> fmt::Result {
        write_op(f, &self.outs, mnemonic, None, self.get_inputs(ir))
    }
}

#[derive(Debug, Clone, Copy)]
pub struct StaticAllocData {
    pub size: u32,
    pub ptr_out: LocalId,
    pub alloc_id: StaticAllocId,
}

impl InlineOpData for StaticAllocData {
    fn ins(&self) -> &[LocalId] {
        &[]
    }

    fn outs(&self) -> &[LocalId] {
        std::slice::from_ref(&self.ptr_out)
    }

    fn ins_mut(&mut self) -> &mut [LocalId] {
        &mut []
    }

    fn outs_mut(&mut self) -> &mut [LocalId] {
        std::slice::from_mut(&mut self.ptr_out)
    }

    fn fmt_inline(
        &self,
        f: &mut impl fmt::Write,
        _ir: &EthIRProgram,
        mnemonic: &str,
    ) -> fmt::Result {
        let extra = format_args!("{} #{}", self.size, self.alloc_id);
        write_op(f, &[self.ptr_out], mnemonic, Some(extra), &[])
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum IRMemoryIOByteSize {
    B1 = 1,
    B2 = 2,
    B3 = 3,
    B4 = 4,
    B5 = 5,
    B6 = 6,
    B7 = 7,
    B8 = 8,
    B9 = 9,
    B10 = 10,
    B11 = 11,
    B12 = 12,
    B13 = 13,
    B14 = 14,
    B15 = 15,
    B16 = 16,
    B17 = 17,
    B18 = 18,
    B19 = 19,
    B20 = 20,
    B21 = 21,
    B22 = 22,
    B23 = 23,
    B24 = 24,
    B25 = 25,
    B26 = 26,
    B27 = 27,
    B28 = 28,
    B29 = 29,
    B30 = 30,
    B31 = 31,
    B32 = 32,
}

impl IRMemoryIOByteSize {
    const MIN: Self = Self::B1;
    const MAX: Self = Self::B32;

    pub fn try_from_u8(x: u8) -> Option<Self> {
        match x {
            1 => Some(Self::B1),
            2 => Some(Self::B2),
            3 => Some(Self::B3),
            4 => Some(Self::B4),
            5 => Some(Self::B5),
            6 => Some(Self::B6),
            7 => Some(Self::B7),
            8 => Some(Self::B8),
            9 => Some(Self::B9),
            10 => Some(Self::B10),
            11 => Some(Self::B11),
            12 => Some(Self::B12),
            13 => Some(Self::B13),
            14 => Some(Self::B14),
            15 => Some(Self::B15),
            16 => Some(Self::B16),
            17 => Some(Self::B17),
            18 => Some(Self::B18),
            19 => Some(Self::B19),
            20 => Some(Self::B20),
            21 => Some(Self::B21),
            22 => Some(Self::B22),
            23 => Some(Self::B23),
            24 => Some(Self::B24),
            25 => Some(Self::B25),
            26 => Some(Self::B26),
            27 => Some(Self::B27),
            28 => Some(Self::B28),
            29 => Some(Self::B29),
            30 => Some(Self::B30),
            31 => Some(Self::B31),
            32 => Some(Self::B32),
            _ => None,
        }
    }

    pub fn bits(&self) -> u16 {
        (*self as u16) * 8
    }
}

#[derive(Debug, Clone, Copy)]
pub struct MemoryLoadData {
    pub out: LocalId,
    pub ptr: LocalId,
    pub size: IRMemoryIOByteSize,
}

impl InlineOpData for MemoryLoadData {
    fn ins(&self) -> &[LocalId] {
        std::slice::from_ref(&self.ptr)
    }

    fn outs(&self) -> &[LocalId] {
        std::slice::from_ref(&self.out)
    }

    fn ins_mut(&mut self) -> &mut [LocalId] {
        std::slice::from_mut(&mut self.ptr)
    }

    fn outs_mut(&mut self) -> &mut [LocalId] {
        std::slice::from_mut(&mut self.out)
    }

    fn fmt_inline(
        &self,
        f: &mut impl fmt::Write,
        _ir: &EthIRProgram,
        mnemonic: &str,
    ) -> fmt::Result {
        let mnemonic = format_args!("{mnemonic}{}", self.size.bits());
        write_op(f, &[self.out], mnemonic, None, &[self.ptr])
    }
}

#[derive(Debug, Clone, Copy)]
pub struct MemoryStoreData {
    pub ins: [LocalId; 2],
    pub size: IRMemoryIOByteSize,
}

impl MemoryStoreData {
    pub fn ptr(&self) -> LocalId {
        self.ins[0]
    }

    pub fn value(&self) -> LocalId {
        self.ins[1]
    }
}

impl InlineOpData for MemoryStoreData {
    fn ins(&self) -> &[LocalId] {
        &self.ins
    }

    fn outs(&self) -> &[LocalId] {
        &[]
    }

    fn ins_mut(&mut self) -> &mut [LocalId] {
        &mut self.ins
    }

    fn outs_mut(&mut self) -> &mut [LocalId] {
        &mut []
    }

    fn fmt_inline(
        &self,
        f: &mut impl fmt::Write,
        _ir: &EthIRProgram,
        mnemonic: &str,
    ) -> fmt::Result {
        let mnemonic = format_args!("{mnemonic}{}", self.size.bits());
        write_op(f, &[], mnemonic, None, &self.ins)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct SetSmallConstData {
    pub sets: LocalId,
    pub value: u32,
}

impl InlineOpData for SetSmallConstData {
    fn ins(&self) -> &[LocalId] {
        &[]
    }

    fn outs(&self) -> &[LocalId] {
        std::slice::from_ref(&self.sets)
    }

    fn ins_mut(&mut self) -> &mut [LocalId] {
        &mut []
    }

    fn outs_mut(&mut self) -> &mut [LocalId] {
        std::slice::from_mut(&mut self.sets)
    }

    fn fmt_inline(
        &self,
        f: &mut impl fmt::Write,
        _ir: &EthIRProgram,
        mnemonic: &str,
    ) -> fmt::Result {
        write_op(f, &[self.sets], mnemonic, Some(format_args!("{:#x}", self.value)), &[])
    }
}

#[derive(Debug, Clone, Copy)]
pub struct SetLargeConstData {
    pub sets: LocalId,
    pub value: LargeConstId,
}

impl InlineOpData for SetLargeConstData {
    fn ins(&self) -> &[LocalId] {
        &[]
    }

    fn outs(&self) -> &[LocalId] {
        std::slice::from_ref(&self.sets)
    }

    fn ins_mut(&mut self) -> &mut [LocalId] {
        &mut []
    }

    fn outs_mut(&mut self) -> &mut [LocalId] {
        std::slice::from_mut(&mut self.sets)
    }

    fn fmt_inline(
        &self,
        f: &mut impl fmt::Write,
        ir: &EthIRProgram,
        mnemonic: &str,
    ) -> fmt::Result {
        let value = format_args!("{:#x}", ir.large_consts[self.value]);
        write_op(f, &[self.sets], mnemonic, Some(value), &[])
    }
}

#[derive(Debug, Clone, Copy)]
pub struct SetDataOffsetData {
    pub sets: LocalId,
    pub segment_id: DataId,
}

impl InlineOpData for SetDataOffsetData {
    fn ins(&self) -> &[LocalId] {
        &[]
    }

    fn outs(&self) -> &[LocalId] {
        std::slice::from_ref(&self.sets)
    }

    fn ins_mut(&mut self) -> &mut [LocalId] {
        &mut []
    }

    fn outs_mut(&mut self) -> &mut [LocalId] {
        std::slice::from_mut(&mut self.sets)
    }

    fn fmt_inline(
        &self,
        f: &mut impl fmt::Write,
        _ir: &EthIRProgram,
        mnemonic: &str,
    ) -> fmt::Result {
        write_op(f, &[self.sets], mnemonic, Some(format_args!(".{}", self.segment_id)), &[])
    }
}

/// Expects args and outputs to be stored contiguously in the IR arena:
/// - Arguments: `ins_start..outs_start`
/// - Outputs: `outs_start..outs_start + target function output count`
#[derive(Debug, Clone, Copy)]
pub struct InternalCallData {
    pub function: FunctionId,
    pub ins_start: LocalIdx,
    pub outs_start: LocalIdx,
}

impl InternalCallData {
    pub fn get_inputs<'ir>(&self, ir: &'ir EthIRProgram) -> &'ir [LocalId] {
        &ir.locals[self.inputs_span()]
    }

    pub fn get_outputs<'ir>(&self, ir: &'ir EthIRProgram) -> &'ir [LocalId] {
        &ir.locals[self.outputs_span(&ir.functions)]
    }

    pub fn inputs_span(&self) -> Span<LocalIdx> {
        Span::new(self.ins_start, self.outs_start)
    }

    pub fn outputs_span(&self, functions: &IndexVec<FunctionId, Function>) -> Span<LocalIdx> {
        let Some(output_count) = functions[self.function].return_kind().count() else {
            unreachable!("invariant: internal call target @{} never returns", self.function);
        };
        Span::new(self.outs_start, self.outs_start + output_count)
    }
}

impl OpData for InternalCallData {
    fn inputs<'a>(&'a self, ir: &'a EthIRProgram) -> &'a [LocalId] {
        self.get_inputs(ir)
    }

    fn outputs<'a>(&'a self, ir: &'a EthIRProgram) -> &'a [LocalId] {
        self.get_outputs(ir)
    }

    fn inputs_mut<'a>(
        &'a mut self,
        locals: &'a mut IndexVec<LocalIdx, LocalId>,
    ) -> &'a mut [LocalId] {
        &mut locals[self.inputs_span()]
    }

    fn outputs_mut<'a>(
        &'a mut self,
        locals: &'a mut IndexVec<LocalIdx, LocalId>,
        functions: &'a IndexVec<FunctionId, Function>,
    ) -> &'a mut [LocalId] {
        &mut locals[self.outputs_span(functions)]
    }

    fn allocated_spans(&self, ir: &EthIRProgram) -> AllocatedSpans {
        AllocatedSpans {
            input: Some(self.inputs_span()),
            output: Some(self.outputs_span(&ir.functions)),
        }
    }

    fn clone_allocated(
        &mut self,
        functions: &IndexVec<FunctionId, Function>,
        clone_span: &mut impl FnMut(Span<LocalIdx>) -> LocalIdx,
    ) {
        let input_count = self.outs_start - self.ins_start;
        let old_operands = Span::new(self.ins_start, self.outputs_span(functions).end);
        self.ins_start = clone_span(old_operands);
        self.outs_start = self.ins_start + input_count;
    }

    fn fmt_op(&self, f: &mut impl fmt::Write, ir: &EthIRProgram, mnemonic: &str) -> fmt::Result {
        let function = format_args!("@{}", self.function);
        write_op(f, self.get_outputs(ir), mnemonic, Some(function), self.get_inputs(ir))
    }
}

#[derive(Debug, Clone, Copy)]
pub struct InternalCallNeverData {
    pub function: FunctionId,
    pub inputs: Span<LocalIdx>,
}

impl InternalCallNeverData {
    pub fn get_inputs<'ir>(&self, ir: &'ir EthIRProgram) -> &'ir [LocalId] {
        &ir.locals[self.inputs]
    }
}

impl OpData for InternalCallNeverData {
    fn inputs<'a>(&'a self, ir: &'a EthIRProgram) -> &'a [LocalId] {
        self.get_inputs(ir)
    }

    fn outputs<'a>(&'a self, _ir: &'a EthIRProgram) -> &'a [LocalId] {
        &[]
    }

    fn inputs_mut<'a>(
        &'a mut self,
        locals: &'a mut IndexVec<LocalIdx, LocalId>,
    ) -> &'a mut [LocalId] {
        &mut locals[self.inputs]
    }

    fn outputs_mut<'a>(
        &'a mut self,
        _locals: &'a mut IndexVec<LocalIdx, LocalId>,
        _functions: &'a IndexVec<FunctionId, Function>,
    ) -> &'a mut [LocalId] {
        &mut []
    }

    fn allocated_spans(&self, _ir: &EthIRProgram) -> AllocatedSpans {
        AllocatedSpans { input: Some(self.inputs), output: None }
    }

    fn clone_allocated(
        &mut self,
        _functions: &IndexVec<FunctionId, Function>,
        clone_span: &mut impl FnMut(Span<LocalIdx>) -> LocalIdx,
    ) {
        let input_count = self.inputs.end - self.inputs.start;
        let start = clone_span(self.inputs);
        self.inputs = Span::new(start, start + input_count);
    }

    fn fmt_op(&self, f: &mut impl fmt::Write, ir: &EthIRProgram, mnemonic: &str) -> fmt::Result {
        let function = format_args!("@{}", self.function);
        write_op(f, &[], mnemonic, Some(function), self.get_inputs(ir))
    }
}

#[derive(Debug, thiserror::Error)]
pub enum OpBuildError {
    #[error("Wrong input count: got {received}, expected {expected}")]
    WrongInputCount { expected: usize, received: usize },
    #[error("Wrong output count: got {received}, expected {expected}")]
    WrongOutputCount { expected: usize, received: usize },
    #[error("Unexpected extra data: got {received:?}, expected {expected}")]
    UnexpectedExtraData { received: OpExtraData, expected: &'static str },
    #[error("Undefined function @{0}")]
    UndefinedFunction(FunctionId),
    #[error("`icall` cannot target never-returning function @{0}")]
    InternalCallToNever(FunctionId),
    #[error("`icall_never` cannot target returning function @{0}")]
    NeverCallReturns(FunctionId),
    #[error(
        "Provided number {too_large} too large, expected value in range [{valid_lower}; {valid_upper}]"
    )]
    NumTooLarge { too_large: U256, valid_lower: u32, valid_upper: u32 },
}

pub trait FromOpData: Sized {
    fn try_build_op(
        ins: &[LocalId],
        outs: &[LocalId],
        extra: OpExtraData,
        builder: &mut EthIRBuilder,
    ) -> Result<Self, OpBuildError>;
}

fn check_ins_count(ins: &[LocalId], expected: usize) -> Result<(), OpBuildError> {
    if ins.len() != expected {
        return Err(OpBuildError::WrongInputCount { expected, received: ins.len() });
    }
    Ok(())
}

fn check_outs_count(outs: &[LocalId], expected: usize) -> Result<(), OpBuildError> {
    if outs.len() != expected {
        return Err(OpBuildError::WrongOutputCount { expected, received: outs.len() });
    }
    Ok(())
}

impl FromOpData for () {
    fn try_build_op(
        ins: &[LocalId],
        outs: &[LocalId],
        extra: OpExtraData,
        _builder: &mut EthIRBuilder,
    ) -> Result<Self, OpBuildError> {
        if extra != OpExtraData::Empty {
            return Err(OpBuildError::UnexpectedExtraData { received: extra, expected: "Empty" });
        }
        check_ins_count(ins, 0)?;
        check_outs_count(outs, 0)?;

        Ok(())
    }
}

impl<const INS: usize, const OUTS: usize> FromOpData for InlineOperands<INS, OUTS> {
    fn try_build_op(
        ins: &[LocalId],
        outs: &[LocalId],
        extra: OpExtraData,
        _builder: &mut EthIRBuilder,
    ) -> Result<Self, OpBuildError> {
        let outs = outs
            .try_into()
            .map_err(|_| OpBuildError::WrongOutputCount { expected: OUTS, received: outs.len() })?;
        let ins = ins
            .try_into()
            .map_err(|_| OpBuildError::WrongInputCount { expected: INS, received: ins.len() })?;
        if extra != OpExtraData::Empty {
            return Err(OpBuildError::UnexpectedExtraData { received: extra, expected: "Empty" });
        }
        Ok(Self { ins, outs })
    }
}

impl<const INS: usize, const OUTS: usize> FromOpData for AllocatedIns<INS, OUTS> {
    fn try_build_op(
        ins: &[LocalId],
        outs: &[LocalId],
        extra: OpExtraData,
        builder: &mut EthIRBuilder,
    ) -> Result<Self, OpBuildError> {
        let outs = outs
            .try_into()
            .map_err(|_| OpBuildError::WrongOutputCount { expected: OUTS, received: outs.len() })?;
        check_ins_count(ins, INS)?;
        let ins_span = builder.alloc_locals(ins);
        assert_eq!(ins_span.end - ins_span.start, INS as u32);
        if extra != OpExtraData::Empty {
            return Err(OpBuildError::UnexpectedExtraData { received: extra, expected: "Empty" });
        }
        Ok(Self { ins_start: ins_span.start, outs })
    }
}

impl FromOpData for InternalCallData {
    fn try_build_op(
        ins: &[LocalId],
        outs: &[LocalId],
        extra: OpExtraData,
        builder: &mut EthIRBuilder,
    ) -> Result<Self, OpBuildError> {
        let OpExtraData::FuncId(func_id) = extra else {
            return Err(OpBuildError::UnexpectedExtraData {
                received: extra,
                expected: "FunctionId",
            });
        };
        let func = *builder.get_func(func_id).ok_or(OpBuildError::UndefinedFunction(func_id))?;
        let inputs = func.get_inputs(&builder.basic_blocks) as usize;
        let Some(outputs) = func.return_kind().count() else {
            return Err(OpBuildError::InternalCallToNever(func_id));
        };
        let outputs = outputs as usize;

        check_ins_count(ins, inputs)?;
        check_outs_count(outs, outputs)?;

        let ins_span = builder.alloc_locals(ins);
        let outs_span = builder.alloc_locals(outs);
        assert_eq!(
            ins_span.end, outs_span.start,
            "Expecting icall locals to be stored contiguously"
        );

        Ok(InternalCallData {
            function: func_id,
            ins_start: ins_span.start,
            outs_start: outs_span.start,
        })
    }
}

impl FromOpData for InternalCallNeverData {
    fn try_build_op(
        ins: &[LocalId],
        outs: &[LocalId],
        extra: OpExtraData,
        builder: &mut EthIRBuilder,
    ) -> Result<Self, OpBuildError> {
        let OpExtraData::FuncId(function) = extra else {
            return Err(OpBuildError::UnexpectedExtraData {
                received: extra,
                expected: "FunctionId",
            });
        };
        let target =
            *builder.get_func(function).ok_or(OpBuildError::UndefinedFunction(function))?;
        if !target.return_kind().is_never() {
            return Err(OpBuildError::NeverCallReturns(function));
        }

        check_ins_count(ins, target.get_inputs(&builder.basic_blocks) as usize)?;
        check_outs_count(outs, 0)?;

        Ok(Self { function, inputs: builder.alloc_locals(ins) })
    }
}

impl FromOpData for SetLargeConstData {
    fn try_build_op(
        ins: &[LocalId],
        outs: &[LocalId],
        extra: OpExtraData,
        builder: &mut EthIRBuilder,
    ) -> Result<Self, OpBuildError> {
        let OpExtraData::Num(num) = extra else {
            return Err(OpBuildError::UnexpectedExtraData {
                received: extra,
                expected: "Num(u256)",
            });
        };

        check_ins_count(ins, 0)?;
        check_outs_count(outs, 1)?;

        let cid = builder.alloc_u256(num);

        Ok(SetLargeConstData { sets: outs[0], value: cid })
    }
}

fn uint256_to_u32(x: U256) -> Result<u32, OpBuildError> {
    x.try_into().map_err(|err| match err {
        FromUintError::Overflow(_, _, _) => {
            OpBuildError::NumTooLarge { too_large: x, valid_lower: 0, valid_upper: u32::MAX }
        }
    })
}

impl FromOpData for SetSmallConstData {
    fn try_build_op(
        ins: &[LocalId],
        outs: &[LocalId],
        extra: OpExtraData,
        _builder: &mut EthIRBuilder,
    ) -> Result<Self, OpBuildError> {
        let OpExtraData::Num(value) = extra else {
            return Err(OpBuildError::UnexpectedExtraData {
                received: extra,
                expected: "Num(u32)",
            });
        };

        let value = uint256_to_u32(value)?;

        check_ins_count(ins, 0)?;
        check_outs_count(outs, 1)?;

        Ok(SetSmallConstData { sets: outs[0], value })
    }
}

impl FromOpData for StaticAllocData {
    fn try_build_op(
        ins: &[LocalId],
        outs: &[LocalId],
        extra: OpExtraData,
        builder: &mut EthIRBuilder,
    ) -> Result<Self, OpBuildError> {
        let OpExtraData::Num(size) = extra else {
            return Err(OpBuildError::UnexpectedExtraData {
                received: extra,
                expected: "Num(u32)",
            });
        };

        let size = uint256_to_u32(size)?;

        check_ins_count(ins, 0)?;
        check_outs_count(outs, 1)?;
        let alloc_id = builder.new_static_alloc();

        Ok(StaticAllocData { size, ptr_out: outs[0], alloc_id })
    }
}

impl FromOpData for MemoryLoadData {
    fn try_build_op(
        ins: &[LocalId],
        outs: &[LocalId],
        extra: OpExtraData,
        _builder: &mut EthIRBuilder,
    ) -> Result<Self, OpBuildError> {
        let OpExtraData::Num(size) = extra else {
            return Err(OpBuildError::UnexpectedExtraData {
                received: extra,
                expected: "Num(u32)",
            });
        };
        let Some(size) = size.try_into().ok().and_then(IRMemoryIOByteSize::try_from_u8) else {
            return Err(OpBuildError::NumTooLarge {
                too_large: size,
                valid_lower: IRMemoryIOByteSize::B1 as u32,
                valid_upper: IRMemoryIOByteSize::B32 as u32,
            });
        };
        check_ins_count(ins, 1)?;
        check_outs_count(outs, 1)?;

        Ok(MemoryLoadData { out: outs[0], ptr: ins[0], size })
    }
}

impl FromOpData for MemoryStoreData {
    fn try_build_op(
        ins: &[LocalId],
        outs: &[LocalId],
        extra: OpExtraData,
        _builder: &mut EthIRBuilder,
    ) -> Result<Self, OpBuildError> {
        let OpExtraData::Num(size) = extra else {
            return Err(OpBuildError::UnexpectedExtraData {
                received: extra,
                expected: "Num(1..=32)",
            });
        };
        let Some(size) = size.try_into().ok().and_then(IRMemoryIOByteSize::try_from_u8) else {
            return Err(OpBuildError::NumTooLarge {
                too_large: size,
                valid_lower: IRMemoryIOByteSize::MIN as u32,
                valid_upper: IRMemoryIOByteSize::MAX as u32,
            });
        };
        check_ins_count(ins, 2)?;
        check_outs_count(outs, 0)?;

        Ok(MemoryStoreData { ins: [ins[0], ins[1]], size })
    }
}

impl FromOpData for SetDataOffsetData {
    fn try_build_op(
        ins: &[LocalId],
        outs: &[LocalId],
        extra: OpExtraData,
        _builder: &mut EthIRBuilder,
    ) -> Result<Self, OpBuildError> {
        let OpExtraData::DataId(segment_id) = extra else {
            return Err(OpBuildError::UnexpectedExtraData { received: extra, expected: "DataId" });
        };

        check_ins_count(ins, 0)?;
        check_outs_count(outs, 1)?;

        Ok(SetDataOffsetData { sets: outs[0], segment_id })
    }
}
