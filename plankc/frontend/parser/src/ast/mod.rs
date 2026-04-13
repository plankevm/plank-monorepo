mod expr;

pub use expr::*;

use plank_core::Span;

use crate::{
    cst::{NodeKind, NodeView},
    lexer::TokenSpan,
};
use plank_session::StrId;

#[derive(Debug, Clone, Copy)]
pub struct InitBlock<'cst> {
    view: NodeView<'cst>,
}

impl<'cst> InitBlock<'cst> {
    pub fn new(view: NodeView<'cst>) -> Option<Self> {
        match view.kind() {
            NodeKind::InitBlock => Some(Self { view }),
            _ => None,
        }
    }

    pub fn body(&self) -> BlockExpr<'cst> {
        BlockExpr::new(self.view)
    }

    pub fn node(&self) -> NodeView<'cst> {
        self.view
    }
}

#[derive(Debug, Clone, Copy)]
pub struct RunBlock<'cst> {
    view: NodeView<'cst>,
}

impl<'cst> RunBlock<'cst> {
    pub fn new(view: NodeView<'cst>) -> Option<Self> {
        match view.kind() {
            NodeKind::RunBlock => Some(Self { view }),
            _ => None,
        }
    }

    pub fn body(&self) -> BlockExpr<'cst> {
        BlockExpr::new(self.view)
    }

    pub fn node(&self) -> NodeView<'cst> {
        self.view
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ConstDecl<'cst> {
    pub name: StrId,
    pub name_span: TokenSpan,
    view: NodeView<'cst>,
    pub r#type: Option<Expr<'cst>>,
    pub assign: Expr<'cst>,
}

impl<'cst> ConstDecl<'cst> {
    /// Returns `Ok(None)` for non-ConstDecl nodes, `Err(span)` for malformed ConstDecl nodes.
    fn try_new(view: NodeView<'cst>) -> Result<Option<Self>, TokenSpan> {
        let NodeKind::ConstDecl { typed } = view.kind() else {
            return Ok(None);
        };
        let mut children = view.children();
        let name_node = children.next().ok_or(view.span())?;
        let name_span = name_node.span();
        let name = name_node.ident().ok_or(view.span())?;
        let r#type = if typed {
            Some(children.next().and_then(Expr::new).ok_or(view.span())?)
        } else {
            None
        };
        let assign = children.next().and_then(Expr::new).ok_or(view.span())?;
        Ok(Some(Self { name, name_span, view, r#type, assign }))
    }

    pub fn span(&self) -> TokenSpan {
        self.view.span()
    }

    pub fn name_span(&self) -> TokenSpan {
        self.name_span
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportSuffix {
    As(Option<StrId>),
    All,
}

#[derive(Debug, Clone, Copy)]
pub struct Import<'cst> {
    path_node: NodeView<'cst>,
    first_child: NodeView<'cst>,
    pub suffix: ImportSuffix,
    view: NodeView<'cst>,
}

impl<'cst> Import<'cst> {
    /// Returns `Ok(None)` for non-Import nodes, `Err(span)` for malformed Import nodes.
    fn try_new(view: NodeView<'cst>) -> Result<Option<Self>, TokenSpan> {
        let (path_node, suffix) = match view.kind() {
            NodeKind::ImportAsDecl => {
                let mut children = view.children();
                let path = children.next().ok_or(view.span())?;
                let as_name =
                    children.next().and_then(|c| c.kind().as_ident()).ok_or(view.span())?;
                (path, ImportSuffix::As(Some(as_name)))
            }
            NodeKind::ImportDecl { glob: false } => (view, ImportSuffix::As(None)),
            NodeKind::ImportDecl { glob: true } => (view, ImportSuffix::All),
            _ => return Ok(None),
        };
        let first_child = path_node.child(0).ok_or(view.span())?;
        first_child.ident().ok_or(view.span())?;
        Ok(Some(Self { path_node, first_child, suffix, view }))
    }

    pub fn node(&self) -> NodeView<'cst> {
        self.view
    }

    pub fn collect_path_segments(&self, buf: &mut Vec<StrId>) {
        for child in self.path_node.children() {
            if let Some(ident) = child.ident() {
                buf.push(ident);
            }
        }
    }

    pub fn last_path_segment_span(&self) -> TokenSpan {
        let mut last = self.first_child;
        for child in self.path_node.children().filter(|c| c.ident().is_some()) {
            last = child;
        }
        last.span()
    }

    pub fn first_path_segment_span(&self) -> TokenSpan {
        self.first_child.span()
    }

    /// Span covering the segments that determine the imported file path.
    /// For `import m::sub::X;` this is `m::sub`, for `import m::sub::*;` this is `m::sub`.
    pub fn file_path_span(&self) -> TokenSpan {
        let mut second_to_last = self.first_child;
        let mut last = self.first_child;
        for ident in self.path_node.children().filter(|c| c.ident().is_some()) {
            second_to_last = last;
            last = ident;
        }
        let end = match self.suffix {
            ImportSuffix::All => last,
            ImportSuffix::As(_) => second_to_last,
        };
        Span::new(self.first_child.span().start, end.span().end)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ImportGroupItemView<'cst> {
    view: NodeView<'cst>,
}

impl<'cst> ImportGroupItemView<'cst> {
    pub fn name(&self) -> Option<StrId> {
        self.view.child(0)?.ident()
    }

    pub fn name_span(&self) -> Option<TokenSpan> {
        Some(self.view.child(0)?.span())
    }

    pub fn alias(&self) -> Option<StrId> {
        self.view.child(1)?.ident()
    }

    pub fn span(&self) -> TokenSpan {
        self.view.span()
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ImportGroup<'cst> {
    view: NodeView<'cst>,
    first_child: NodeView<'cst>,
}

impl<'cst> ImportGroup<'cst> {
    /// Returns `Ok(None)` for non-ImportGroupDecl nodes, `Err(span)` for malformed ImportGroupDecl
    /// nodes.
    fn try_new(view: NodeView<'cst>) -> Result<Option<Self>, TokenSpan> {
        match view.kind() {
            NodeKind::ImportGroupDecl => {
                let first_child = view.child(0).ok_or(view.span())?;
                first_child.ident().ok_or(view.span())?;
                Ok(Some(Self { view, first_child }))
            }
            _ => Ok(None),
        }
    }

    pub fn node(&self) -> NodeView<'cst> {
        self.view
    }

    pub fn collect_path_segments(&self, buf: &mut Vec<StrId>) {
        for child in self.view.children() {
            if child.kind() == NodeKind::ImportGroupItem {
                break;
            }
            if let Some(ident) = child.ident() {
                buf.push(ident);
            }
        }
    }

    pub fn items(&self) -> impl Iterator<Item = ImportGroupItemView<'cst>> {
        self.view.children().filter_map(|child| match child.kind() {
            NodeKind::ImportGroupItem => Some(ImportGroupItemView { view: child }),
            _ => None,
        })
    }

    pub fn first_path_segment_span(&self) -> TokenSpan {
        self.first_child.span()
    }

    pub fn file_path_span(&self) -> TokenSpan {
        let mut last_path_ident = self.first_child;
        for child in self.view.children() {
            if child.kind() == NodeKind::ImportGroupItem {
                break;
            }
            if child.ident().is_some() {
                last_path_ident = child;
            }
        }
        Span::new(self.first_child.span().start, last_path_ident.span().end)
    }
}

#[derive(Debug, Clone, Copy)]
pub enum TopLevelDef<'cst> {
    Init(InitBlock<'cst>),
    Run(RunBlock<'cst>),
    Const(ConstDecl<'cst>),
    Import(Import<'cst>),
    ImportGroup(ImportGroup<'cst>),
    Error { span: TokenSpan },
}

#[derive(Debug, Clone, Copy)]
pub struct File<'cst>(NodeView<'cst>);

impl<'cst> File<'cst> {
    pub fn new(view: NodeView<'cst>) -> Option<Self> {
        match view.kind() {
            NodeKind::File => Some(Self(view)),
            _ => None,
        }
    }

    pub fn iter_defs(&self) -> impl Iterator<Item = TopLevelDef<'cst>> {
        self.0.children().map(|child| {
            match ConstDecl::try_new(child) {
                Ok(Some(def)) => return TopLevelDef::Const(def),
                Err(span) => return TopLevelDef::Error { span },
                Ok(None) => {}
            }
            if let Some(def) = InitBlock::new(child) {
                return TopLevelDef::Init(def);
            }
            if let Some(def) = RunBlock::new(child) {
                return TopLevelDef::Run(def);
            }
            match Import::try_new(child) {
                Ok(Some(def)) => return TopLevelDef::Import(def),
                Err(span) => return TopLevelDef::Error { span },
                Ok(None) => {}
            }
            match ImportGroup::try_new(child) {
                Ok(Some(def)) => return TopLevelDef::ImportGroup(def),
                Err(span) => return TopLevelDef::Error { span },
                Ok(None) => {}
            }
            TopLevelDef::Error { span: child.span() }
        })
    }
}
