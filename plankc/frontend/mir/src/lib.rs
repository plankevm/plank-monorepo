pub mod display;

use plank_core::{
    Idx, IndexVec, Span, const_print::const_assert_mem_size, list_of_lists::ListOfLists,
    newtype_index,
};
use plank_session::RuntimeBuiltin;
use plank_values::{TypeId, TypeInterner, ValueId};

newtype_index! {
    pub struct FnId;
    pub struct BlockId;
    pub struct LocalId;
    pub struct ArgsId;
}

#[derive(Debug, Clone, Copy)]
pub enum Expr {
    LocalRef(LocalId),
    Const(ValueId),
    Call { callee: FnId, args: ArgsId },
    RuntimeBuiltinCall { builtin: RuntimeBuiltin, args: ArgsId },
    FieldAccess { object: LocalId, field_index: u32 },
    StructLit { ty: TypeId, fields: ArgsId },
}

const _EXPR_SIZE: () = const_assert_mem_size::<Expr>(12);
const _INSTR_SIZE: () = const_assert_mem_size::<Instruction>(16);

#[derive(Debug, Clone, Copy)]
pub enum Instruction {
    Set { target: LocalId, expr: Expr },
    Return(LocalId),
    If { condition: LocalId, then_block: BlockId, else_block: BlockId },
    While { condition_block: BlockId, condition: LocalId, body: BlockId },
}

#[derive(Debug, Clone, Copy)]
pub struct FnDef {
    pub body: BlockId,
    pub param_count: u32,
    pub return_type: TypeId,
}

impl FnDef {
    pub fn iter_params(&self) -> impl Iterator<Item = LocalId> {
        Span::new(LocalId::ZERO, LocalId::new(self.param_count)).iter()
    }
}

#[derive(Debug)]
pub struct Mir {
    pub blocks: ListOfLists<BlockId, Instruction>,
    pub args: ListOfLists<ArgsId, LocalId>,
    pub fns: IndexVec<FnId, FnDef>,
    pub fn_locals: ListOfLists<FnId, TypeId>,
    pub types: TypeInterner,
    pub init: FnId,
    pub run: Option<FnId>,
}
