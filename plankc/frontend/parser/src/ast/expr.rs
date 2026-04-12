use crate::{
    cst::{BinaryOp, NodeKind, NodeView, NumLitId, UnaryOp},
    lexer::TokenSpan,
};
use plank_session::StrId;

#[derive(Debug, Clone, Copy)]
pub enum Expr<'cst> {
    Binary(BinaryExpr<'cst>),
    Unary(UnaryExpr<'cst>),
    Call(CallExpr<'cst>),
    Member(MemberExpr<'cst>),
    StructDef(StructDef<'cst>),
    StructLit(StructLit<'cst>),
    If(IfExpr<'cst>),
    FnDef(FnDef<'cst>),
    Block(BlockExpr<'cst>),
    ComptimeBlock(BlockExpr<'cst>),
    BoolLiteral { value: bool, span: TokenSpan },
    NumLiteral { id: NumLitId, span: TokenSpan },
    Ident { name: StrId, span: TokenSpan },
    Error { span: TokenSpan },
}

impl<'cst> Expr<'cst> {
    pub fn new_unwrap(view: NodeView<'cst>) -> Self {
        Expr::new(view).unwrap_or(Expr::Error { span: view.span() })
    }

    /// Creates an Expr from a NodeView. Returns `None` for non-expression nodes.
    ///
    /// ParenExpr nodes are transparently peeled — the resulting Expr
    /// points to the innermost non-paren expression.
    pub fn new(mut view: NodeView<'cst>) -> Option<Self> {
        const MAX_PAREN_UNWRAPS: usize = 16_000;
        for _ in 0..MAX_PAREN_UNWRAPS {
            let span = view.span();
            let expr = match view.kind() {
                NodeKind::ParenExpr => match view.child(0) {
                    Some(inner) => {
                        view = inner;
                        continue;
                    }
                    None => Expr::Error { span },
                },
                NodeKind::BinaryExpr(op) => match view.child(1) {
                    Some(op_node) => Expr::Binary(BinaryExpr { op, op_span: op_node.span(), view }),
                    None => Expr::Error { span },
                },
                NodeKind::UnaryExpr(op) => Expr::Unary(UnaryExpr { op, view }),
                NodeKind::CallExpr => Expr::Call(CallExpr { view }),
                NodeKind::MemberExpr => match MemberExpr::new(view) {
                    Some(member) => Expr::Member(member),
                    None => Expr::Error { span },
                },
                NodeKind::StructDef => Expr::StructDef(StructDef { view }),
                NodeKind::StructLit => Expr::StructLit(StructLit { view }),
                NodeKind::If => match view.child(1) {
                    Some(body_node) => Expr::If(IfExpr { body_node, view }),
                    None => Expr::Error { span },
                },
                NodeKind::FnDef => match (view.child(0), view.child(2)) {
                    (Some(param_list), Some(body_node)) => {
                        Expr::FnDef(FnDef { param_list, body_node, view })
                    }
                    _ => Expr::Error { span },
                },
                NodeKind::Block => Expr::Block(BlockExpr { view }),
                NodeKind::ComptimeBlock => Expr::ComptimeBlock(BlockExpr { view }),
                NodeKind::BoolLiteral(value) => Expr::BoolLiteral { value, span },
                NodeKind::NumLiteral { id } => Expr::NumLiteral { id, span },
                NodeKind::Identifier { ident } => Expr::Ident { name: ident, span },
                NodeKind::Error => Expr::Error { span },
                _ => return None,
            };
            return Some(expr);
        }

        unreachable!("Nested paren over {MAX_PAREN_UNWRAPS} deep");
    }

    pub fn span(&self) -> TokenSpan {
        match self {
            Expr::Binary(BinaryExpr { view, .. })
            | Expr::Unary(UnaryExpr { view, .. })
            | Expr::Call(CallExpr { view, .. })
            | Expr::Member(MemberExpr { view, .. })
            | Expr::StructDef(StructDef { view, .. })
            | Expr::StructLit(StructLit { view, .. })
            | Expr::If(IfExpr { view, .. })
            | Expr::FnDef(FnDef { view, .. })
            | Expr::Block(BlockExpr { view, .. })
            | Expr::ComptimeBlock(BlockExpr { view, .. }) => view.span(),
            Expr::BoolLiteral { span, .. }
            | Expr::NumLiteral { span, .. }
            | Expr::Ident { span, .. }
            | Expr::Error { span, .. } => *span,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct BinaryExpr<'cst> {
    pub op: BinaryOp,
    op_span: TokenSpan,
    view: NodeView<'cst>,
}

impl<'cst> BinaryExpr<'cst> {
    pub fn lhs(&self) -> Expr<'cst> {
        self.view.child(0).map(Expr::new_unwrap).unwrap_or(Expr::Error { span: self.view.span() })
    }

    pub fn op_span(&self) -> TokenSpan {
        self.op_span
    }

    pub fn rhs(&self) -> Expr<'cst> {
        self.view.child(2).map(Expr::new_unwrap).unwrap_or(Expr::Error { span: self.view.span() })
    }

    pub fn node(&self) -> NodeView<'cst> {
        self.view
    }
}

/// Unary expression: `op operand`
#[derive(Debug, Clone, Copy)]
pub struct UnaryExpr<'cst> {
    pub op: UnaryOp,
    view: NodeView<'cst>,
}

impl<'cst> UnaryExpr<'cst> {
    pub fn operand(&self) -> Expr<'cst> {
        self.view.child(0).map(Expr::new_unwrap).unwrap_or(Expr::Error { span: self.view.span() })
    }

    pub fn node(&self) -> NodeView<'cst> {
        self.view
    }
}

/// Call expression: `callee(arg1, arg2, ...)`
#[derive(Debug, Clone, Copy)]
pub struct CallExpr<'cst> {
    view: NodeView<'cst>,
}

impl<'cst> CallExpr<'cst> {
    pub fn callee(&self) -> Expr<'cst> {
        self.view.child(0).map(Expr::new_unwrap).unwrap_or(Expr::Error { span: self.view.span() })
    }

    pub fn args(&self) -> impl Iterator<Item = Expr<'cst>> {
        self.view.children().skip(1).map(Expr::new_unwrap)
    }

    pub fn node(&self) -> NodeView<'cst> {
        self.view
    }
}

/// Member access expression: `object.member`
#[derive(Debug, Clone, Copy)]
pub struct MemberExpr<'cst> {
    pub member: StrId,
    view: NodeView<'cst>,
}

impl<'cst> MemberExpr<'cst> {
    pub fn new(view: NodeView<'cst>) -> Option<Self> {
        if view.kind() != NodeKind::MemberExpr {
            return None;
        }
        let member = view.child(1).and_then(NodeView::ident)?;
        Some(Self { member, view })
    }

    pub fn object(&self) -> Expr<'cst> {
        self.view.child(0).map(Expr::new_unwrap).unwrap_or(Expr::Error { span: self.view.span() })
    }

    pub fn node(&self) -> NodeView<'cst> {
        self.view
    }
}

/// Struct definition: `struct { field1: Type1, ... }` or
/// `struct TypeIndex { field1: Type1, ... }`
#[derive(Debug, Clone, Copy)]
pub struct StructDef<'cst> {
    view: NodeView<'cst>,
}

impl<'cst> StructDef<'cst> {
    pub fn index_expr(&self) -> Option<Expr<'cst>> {
        self.view.children().next().and_then(|child| match child.kind() {
            NodeKind::FieldDef => None,
            _ => Expr::new(child),
        })
    }

    pub fn fields(&self) -> impl Iterator<Item = Result<FieldDef<'cst>, TokenSpan>> {
        self.view.children().filter_map(FieldDef::try_new)
    }

    pub fn node(&self) -> NodeView<'cst> {
        self.view
    }
}

/// Field definition within a struct: `name: Type`
#[derive(Debug, Clone, Copy)]
pub struct FieldDef<'cst> {
    pub name: StrId,
    pub name_span: TokenSpan,
    view: NodeView<'cst>,
}

impl<'cst> FieldDef<'cst> {
    /// Returns `None` for non-FieldDef nodes, `Some(Err(span))` for malformed FieldDef nodes.
    fn try_new(view: NodeView<'cst>) -> Option<Result<Self, TokenSpan>> {
        match view.kind() {
            NodeKind::FieldDef => {
                let Some(name_node) = view.child(0) else { return Some(Err(view.span())) };
                let Some(name) = name_node.kind().as_ident() else { return Some(Err(view.span())) };
                Some(Ok(Self { name, name_span: name_node.span(), view }))
            }
            _ => None,
        }
    }

    pub fn type_expr(&self) -> Expr<'cst> {
        self.view.child(1).map(Expr::new_unwrap).unwrap_or(Expr::Error { span: self.view.span() })
    }

    pub fn name_span(&self) -> TokenSpan {
        self.name_span
    }

    pub fn node(&self) -> NodeView<'cst> {
        self.view
    }
}

/// Struct literal: `Type { field1: value1, ... }`
#[derive(Debug, Clone, Copy)]
pub struct StructLit<'cst> {
    view: NodeView<'cst>,
}

impl<'cst> StructLit<'cst> {
    pub fn type_expr(&self) -> Expr<'cst> {
        self.view.child(0).map(Expr::new_unwrap).unwrap_or(Expr::Error { span: self.view.span() })
    }

    pub fn fields(&self) -> impl Iterator<Item = Result<FieldAssign<'cst>, TokenSpan>> {
        self.view.children().skip(1).filter_map(FieldAssign::try_new)
    }

    pub fn node(&self) -> NodeView<'cst> {
        self.view
    }
}

/// Field assignment within a struct literal: `name: value`
#[derive(Debug, Clone, Copy)]
pub struct FieldAssign<'cst> {
    pub name: StrId,
    pub name_span: TokenSpan,
    view: NodeView<'cst>,
}

impl<'cst> FieldAssign<'cst> {
    fn try_new(view: NodeView<'cst>) -> Option<Result<Self, TokenSpan>> {
        match view.kind() {
            NodeKind::FieldAssign => {
                let Some(name_node) = view.child(0) else { return Some(Err(view.span())) };
                let Some(name) = name_node.kind().as_ident() else { return Some(Err(view.span())) };
                Some(Ok(Self { name, name_span: name_node.span(), view }))
            }
            _ => None,
        }
    }

    pub fn value(&self) -> Expr<'cst> {
        self.view.child(1).map(Expr::new_unwrap).unwrap_or(Expr::Error { span: self.view.span() })
    }

    pub fn name_span(&self) -> TokenSpan {
        self.name_span
    }

    pub fn node(&self) -> NodeView<'cst> {
        self.view
    }
}

/// If expression: `if condition { body } else if ... else { ... }`
#[derive(Debug, Clone, Copy)]
pub struct IfExpr<'cst> {
    body_node: NodeView<'cst>,
    view: NodeView<'cst>,
}

impl<'cst> IfExpr<'cst> {
    pub fn condition(&self) -> Expr<'cst> {
        self.view.child(0).map(Expr::new_unwrap).unwrap_or(Expr::Error { span: self.view.span() })
    }

    pub fn body(&self) -> BlockExpr<'cst> {
        BlockExpr::new(self.body_node)
    }

    /// Returns an iterator over the else-if branches.
    pub fn else_if_branches(&self) -> impl Iterator<Item = Result<ElseIfBranch<'cst>, TokenSpan>> {
        let else_if_list = self.view.child(2);
        else_if_list.into_iter().flat_map(|list| list.children()).filter_map(ElseIfBranch::try_new)
    }

    /// Returns the else body if present.
    pub fn else_body(&self) -> Option<BlockExpr<'cst>> {
        self.view.child(3).map(BlockExpr::new)
    }

    pub fn node(&self) -> NodeView<'cst> {
        self.view
    }
}

/// An else-if branch: `else if condition { body }`
#[derive(Debug, Clone, Copy)]
pub struct ElseIfBranch<'cst> {
    body_node: NodeView<'cst>,
    view: NodeView<'cst>,
}

impl<'cst> ElseIfBranch<'cst> {
    fn try_new(view: NodeView<'cst>) -> Option<Result<Self, TokenSpan>> {
        match view.kind() {
            NodeKind::ElseIfBranch => {
                let Some(body_node) = view.child(1) else { return Some(Err(view.span())) };
                Some(Ok(Self { body_node, view }))
            }
            _ => None,
        }
    }

    pub fn condition(&self) -> Expr<'cst> {
        self.view.child(0).map(Expr::new_unwrap).unwrap_or(Expr::Error { span: self.view.span() })
    }

    pub fn body(&self) -> BlockExpr<'cst> {
        BlockExpr::new(self.body_node)
    }

    pub fn node(&self) -> NodeView<'cst> {
        self.view
    }
}

/// Function definition: `fn(params) return_type { body }`
#[derive(Debug, Clone, Copy)]
pub struct FnDef<'cst> {
    param_list: NodeView<'cst>,
    body_node: NodeView<'cst>,
    view: NodeView<'cst>,
}

impl<'cst> FnDef<'cst> {
    pub fn param_list_span(&self) -> TokenSpan {
        self.param_list.span()
    }

    pub fn params(&self) -> impl Iterator<Item = Result<Param<'cst>, TokenSpan>> {
        self.param_list.children().filter_map(|child| Param::try_new(child).transpose())
    }

    pub fn return_type(&self) -> Expr<'cst> {
        self.view.child(1).map(Expr::new_unwrap).unwrap_or(Expr::Error { span: self.view.span() })
    }

    pub fn body(&self) -> BlockExpr<'cst> {
        BlockExpr::new(self.body_node)
    }

    pub fn node(&self) -> NodeView<'cst> {
        self.view
    }
}

/// Function parameter: `name: Type` or `comptime name: Type`
#[derive(Debug, Clone, Copy)]
pub struct Param<'cst> {
    pub name: StrId,
    pub name_span: TokenSpan,
    pub is_comptime: bool,
    view: NodeView<'cst>,
}

impl<'cst> Param<'cst> {
    fn try_new(view: NodeView<'cst>) -> Result<Option<Self>, TokenSpan> {
        let is_comptime = match view.kind() {
            NodeKind::Parameter => false,
            NodeKind::ComptimeParameter => true,
            _ => return Ok(None),
        };
        let name_node = view.child(0).ok_or(view.span())?;
        let name = name_node.kind().as_ident().ok_or(view.span())?;
        Ok(Some(Self { name, name_span: name_node.span(), is_comptime, view }))
    }

    pub fn type_expr(&self) -> Expr<'cst> {
        self.view.child(1).map(Expr::new_unwrap).unwrap_or(Expr::Error { span: self.view.span() })
    }

    pub fn name_span(&self) -> TokenSpan {
        self.name_span
    }

    pub fn node(&self) -> NodeView<'cst> {
        self.view
    }
}

#[derive(Debug, Clone, Copy)]
pub struct BlockExpr<'cst> {
    view: NodeView<'cst>,
}

#[derive(Debug, Clone, Copy)]
pub struct LetStmt<'cst> {
    pub name: StrId,
    pub name_span: TokenSpan,
    pub mutable: bool,
    type_view: Option<NodeView<'cst>>,
    value_view: NodeView<'cst>,
}

impl<'cst> LetStmt<'cst> {
    pub fn new(view: NodeView<'cst>) -> Option<Self> {
        let NodeKind::LetStmt { mutable, typed } = view.kind() else {
            return None;
        };
        let mut children = view.children();
        let name_view = children.next()?;
        let name_span = name_view.span();
        let name = name_view.ident()?;
        let type_view = if typed { Some(children.next()?) } else { None };
        let value_view = children.next()?;
        Some(Self { name, name_span, mutable, type_view, value_view })
    }

    pub fn type_expr(&self) -> Option<Expr<'cst>> {
        self.type_view.map(Expr::new_unwrap)
    }

    pub fn value(&self) -> Expr<'cst> {
        Expr::new_unwrap(self.value_view)
    }
}

/// Return statement: `return expr;`
#[derive(Debug, Clone, Copy)]
pub struct ReturnStmt<'cst> {
    view: NodeView<'cst>,
}

impl<'cst> ReturnStmt<'cst> {
    fn new(view: NodeView<'cst>) -> Option<Self> {
        match view.kind() {
            NodeKind::ReturnStmt => Some(Self { view }),
            _ => None,
        }
    }

    pub fn value(&self) -> Expr<'cst> {
        self.view.child(0).map(Expr::new_unwrap).unwrap_or(Expr::Error { span: self.view.span() })
    }

    pub fn node(&self) -> NodeView<'cst> {
        self.view
    }
}

/// Assignment statement: `target = value;`
#[derive(Debug, Clone, Copy)]
pub struct AssignStmt<'cst> {
    view: NodeView<'cst>,
}

impl<'cst> AssignStmt<'cst> {
    fn new(view: NodeView<'cst>) -> Option<Self> {
        match view.kind() {
            NodeKind::AssignStmt => Some(Self { view }),
            _ => None,
        }
    }

    pub fn target(&self) -> Expr<'cst> {
        self.view.child(0).map(Expr::new_unwrap).unwrap_or(Expr::Error { span: self.view.span() })
    }

    pub fn value(&self) -> Expr<'cst> {
        self.view.child(1).map(Expr::new_unwrap).unwrap_or(Expr::Error { span: self.view.span() })
    }

    pub fn node(&self) -> NodeView<'cst> {
        self.view
    }
}

/// While statement: `while condition { body }` or `inline while condition { body }`
#[derive(Debug, Clone, Copy)]
pub struct WhileStmt<'cst> {
    pub inline: bool,
    body_node: NodeView<'cst>,
    view: NodeView<'cst>,
}

impl<'cst> WhileStmt<'cst> {
    fn new(view: NodeView<'cst>) -> Option<Self> {
        let inline = match view.kind() {
            NodeKind::WhileStmt => false,
            NodeKind::InlineWhileStmt => true,
            _ => return None,
        };
        let body_node = view.child(1)?;
        Some(Self { inline, body_node, view })
    }

    pub fn condition(&self) -> Expr<'cst> {
        self.view.child(0).map(Expr::new_unwrap).unwrap_or(Expr::Error { span: self.view.span() })
    }

    pub fn body(&self) -> BlockExpr<'cst> {
        BlockExpr::new(self.body_node)
    }

    pub fn node(&self) -> NodeView<'cst> {
        self.view
    }
}

#[derive(Debug, Clone, Copy)]
pub enum Statement<'cst> {
    Let(LetStmt<'cst>),
    Return(ReturnStmt<'cst>),
    Assign(AssignStmt<'cst>),
    While(WhileStmt<'cst>),
    Expr(Expr<'cst>),
    Error { span: TokenSpan },
}

impl<'cst> Statement<'cst> {
    pub fn new(view: NodeView<'cst>) -> Option<Self> {
        if let Some(let_stmt) = LetStmt::new(view) {
            return Some(Statement::Let(let_stmt));
        }
        if let Some(return_stmt) = ReturnStmt::new(view) {
            return Some(Statement::Return(return_stmt));
        }
        if let Some(assign_stmt) = AssignStmt::new(view) {
            return Some(Statement::Assign(assign_stmt));
        }
        if let Some(while_stmt) = WhileStmt::new(view) {
            return Some(Statement::While(while_stmt));
        }
        if let Some(expr) = Expr::new(view) {
            return Some(Statement::Expr(expr));
        }
        None
    }
}

impl<'cst> BlockExpr<'cst> {
    pub(super) fn new(view: NodeView<'cst>) -> Self {
        assert!(
            matches!(
                view.kind(),
                NodeKind::Block
                    | NodeKind::ComptimeBlock
                    | NodeKind::InitBlock
                    | NodeKind::RunBlock
            ),
            "BlockExpr::new called with non-block node: {:?}",
            view.kind()
        );
        Self { view }
    }

    /// Returns an iterator over the statements in this block.
    pub fn statements(&self) -> impl Iterator<Item = Statement<'cst>> {
        self.view
            .child(0)
            .into_iter()
            .flat_map(|list| list.children())
            .map(|view| Statement::new(view).unwrap_or(Statement::Error { span: view.span() }))
    }

    /// Returns the trailing/end expression if present.
    pub fn end_expr(&self) -> Option<Expr<'cst>> {
        self.view.child(1).map(Expr::new_unwrap)
    }

    pub fn node(&self) -> NodeView<'cst> {
        self.view
    }
}
