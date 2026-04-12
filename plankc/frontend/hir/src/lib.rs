use plank_core::{
    IndexVec, const_print::const_assert_mem_size, list_of_lists::ListOfLists, newtype_index,
};
use plank_session::{
    EvmBuiltin, MaybePoisoned, Poisoned, SourceByteOffset, SourceId, SourceSpan, SrcLoc, StrId,
};

pub mod display;
mod lowerer;
pub mod operators;

pub use lowerer::lower;
pub use plank_values::{ConstId, FnDefId, ValueId};

newtype_index! {
    pub struct LocalId;
    pub struct BlockId;
    pub struct StructDefId;
    pub struct CallArgsId;
    pub struct FieldsId;
}

#[derive(Debug, Clone, Copy)]
pub struct Expr {
    pub span: SourceSpan,
    pub kind: ExprKind,
}

#[derive(Debug, Clone, Copy)]
pub enum ExprKind {
    ConstRef(ConstId),
    LocalRef(LocalId),
    FnDef(FnDefId),
    Value(MaybePoisoned<ValueId>),

    Call {
        callee: LocalId,
        args: CallArgsId,
    },
    EvmBuiltinCall {
        builtin: EvmBuiltin,
        args: CallArgsId,
    },
    UnaryOpCall {
        op: operators::UnaryOp,
        input: LocalId,
    },
    BinaryOpCall {
        op: operators::BinaryOp,
        lhs: LocalId,
        rhs: LocalId,
    },
    Member {
        object: LocalId,
        member: StrId,
    },
    StructLit {
        ty: LocalId,
        fields: FieldsId,
    },
    StructDef(StructDefId),

    /// Bool-specific logical NOT (`!x`). Not in `operators::UnaryOp` because it is not
    /// overridable — it is hardcoded to only work on `bool`.
    LogicalNot {
        input: LocalId,
    },
}

impl ExprKind {
    pub const VOID: Self = ExprKind::Value(Ok(ValueId::VOID));
    pub const POISON: Self = ExprKind::Value(Err(Poisoned));
}

#[derive(Debug, Clone, Copy)]
pub enum InstructionKind {
    Param {
        comptime: bool,
        arg: LocalId,
        r#type: LocalId,
        idx: u32,
    },
    Set {
        local: LocalId,
        r#type: Option<LocalId>,
        expr: Expr,
    },
    BranchSet {
        local: LocalId,
        expr: Expr,
    },
    SetMut {
        local: LocalId,
        r#type: Option<LocalId>,
        expr: Expr,
    },
    Assign {
        target: LocalId,
        expr: Expr,
    },
    Eval(Expr),
    Return(Expr),
    If {
        condition: LocalId,
        then_block: BlockId,
        else_block: BlockId,
    },
    While {
        condition_block: BlockId,
        condition: LocalId,
        body: BlockId,
    },
    /// Forces compile-time evaluation of the block body.
    ComptimeBlock {
        body: BlockId,
    },
}

#[derive(Debug, Clone, Copy)]
pub struct Instruction {
    pub kind: InstructionKind,
}

// Memory size checks. Remember size impacts performance. May be changed intentionally.
const _EXPR_KIND_SIZE: () = const_assert_mem_size::<ExprKind>(12);
const _EXPR_SIZE: () = const_assert_mem_size::<Expr>(20);
const _INSTR_KIND_SIZE: () = const_assert_mem_size::<InstructionKind>(32);
const _INSTR_SIZE: () = const_assert_mem_size::<Instruction>(32);

#[derive(Debug, Clone, Copy)]
pub struct ParamInfo {
    pub is_comptime: bool,
    pub value: LocalId,
    pub r#type: LocalId,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, Copy)]
pub struct CaptureInfo {
    pub outer_local: LocalId,
    pub inner_local: LocalId,
    pub use_span: SourceSpan,
}

#[derive(Debug, Clone, Copy)]
pub struct FieldInfo {
    pub name: StrId,
    pub name_offset: SourceByteOffset,
    pub value: LocalId,
}

#[derive(Debug, Clone, Copy)]
pub struct FnDef {
    /// Parameters & return type comptime type expressions.
    pub type_preamble: BlockId,
    /// Function body.
    pub body: BlockId,
    /// Preamble set local that holds the return type expression.
    pub return_type: LocalId,
    pub source: SourceId,
    pub param_list_span: SourceSpan,
}

impl FnDef {
    pub fn loc(&self, span: SourceSpan) -> SrcLoc {
        SrcLoc::new(self.source, span)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct StructDef {
    pub source_id: SourceId,
    pub source_span: SourceSpan,
    pub type_index: LocalId,
    pub fields: FieldsId,
}

#[derive(Debug, Clone, Copy)]
pub struct ConstDef {
    pub name: StrId,
    pub source_id: SourceId,
    pub source_span: SourceSpan,
    pub body: BlockId,
    pub result: LocalId,
}

impl ConstDef {
    pub fn loc(&self) -> SrcLoc {
        SrcLoc::new(self.source_id, self.source_span)
    }
}

#[derive(Debug, Clone)]
pub struct Hir {
    pub entry_source: SourceId,
    pub init: BlockId,
    pub run: Option<BlockId>,

    pub block_instrs: ListOfLists<BlockId, Instruction>,
    pub block_spans: IndexVec<BlockId, MaybePoisoned<SourceSpan>>,
    pub consts: IndexVec<ConstId, ConstDef>,

    pub call_args: ListOfLists<CallArgsId, LocalId>,
    pub fields: ListOfLists<FieldsId, FieldInfo>,
    pub struct_defs: IndexVec<StructDefId, StructDef>,

    pub fns: IndexVec<FnDefId, FnDef>,
    pub fn_params: ListOfLists<FnDefId, ParamInfo>,
    pub fn_captures: ListOfLists<FnDefId, CaptureInfo>,
}

#[cfg(test)]
mod tests;
