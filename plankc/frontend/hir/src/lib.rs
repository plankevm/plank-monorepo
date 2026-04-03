use plank_core::{IndexVec, list_of_lists::ListOfLists, newtype_index};
use plank_parser::const_print::const_assert_eq;
use plank_session::{EvmBuiltin, SourceByteOffset, SourceId, SourceSpan, SrcLoc, StrId, TypeId};

pub use plank_values;

pub mod display;
mod lowerer;
pub mod operators;

pub use lowerer::lower;
pub use plank_values::{BigNumId, BigNumInterner};

newtype_index! {
    pub struct ConstId;
    pub struct LocalId;
    pub struct BlockId;
    pub struct FnDefId;
    pub struct StructDefId;
    pub struct CallArgsId;
    pub struct FieldsId;
}

#[derive(Debug, Clone, Copy)]
pub struct Expr {
    pub source_id: SourceId,
    pub span: SourceSpan,
    pub kind: ExprKind,
}

impl Expr {
    pub fn src_loc(&self) -> SrcLoc {
        SrcLoc::new(self.source_id, self.span)
    }
}

#[derive(Debug, Clone, Copy)]
pub enum ExprKind {
    ConstRef(ConstId),
    LocalRef(LocalId),
    FnDef(FnDefId),

    Bool(bool),
    Void,
    BigNum(BigNumId),
    Type(TypeId),

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

    /// Indicates the expr that evaluated to the value had some error that was already handled,
    /// to avoid cascades any expression downstream from it also needs to become an error.
    Error,
}

/// [`ExprKind`] memory size check. May be changed intentionally.
const _EXPR_KIND_SIZE: () = const_assert_eq(std::mem::size_of::<ExprKind>(), 12);

#[derive(Debug, Clone, Copy)]
pub enum InstructionKind {
    Set {
        local: LocalId,
        r#type: Option<LocalId>,
        expr: Expr,
    },
    BranchSet {
        local: LocalId,
        expr: Expr,
    },
    Assign {
        target: LocalId,
        value: Expr,
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
    pub loc: SrcLoc,
    pub kind: InstructionKind,
}

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

#[derive(Debug, Clone)]
pub struct Hir {
    pub init: BlockId,
    pub run: Option<BlockId>,

    pub blocks: ListOfLists<BlockId, Instruction>,
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
