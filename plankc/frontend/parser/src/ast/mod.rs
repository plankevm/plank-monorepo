mod expr;

pub use expr::*;
use plank_core::Span;

use crate::{
    StrId,
    cst::{NodeKind, NodeView},
    lexer::TokenIdx,
};

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
    view: NodeView<'cst>,
    pub r#type: Option<Expr<'cst>>,
    pub assign: Expr<'cst>,
}

impl<'cst> ConstDecl<'cst> {
    pub fn new(view: NodeView<'cst>) -> Option<Self> {
        let NodeKind::ConstDecl { typed } = view.kind() else {
            return None;
        };
        let mut children = view.children();
        let name = children.next().and_then(NodeView::ident).expect("TODO: malformed");
        let r#type = if typed {
            Some(children.next().and_then(Expr::new).expect("TODO: malformed"))
        } else {
            None
        };
        let assign = children.next().and_then(Expr::new).expect("TODO: malformed");
        Some(Self { name, view, r#type, assign })
    }

    pub fn span(&self) -> Span<TokenIdx> {
        self.view.span()
    }

    pub fn name_span(&self) -> Span<TokenIdx> {
        self.view.child(0).expect("ConstDecl must have name child").span()
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
    pub suffix: ImportSuffix,
    view: NodeView<'cst>,
}

impl<'cst> Import<'cst> {
    fn new(view: NodeView<'cst>) -> Option<Self> {
        let (path_node, suffix) = match view.kind() {
            NodeKind::ImportAsDecl => {
                let mut children = view.children();
                let path = children.next()?;
                let as_name = children.next()?.kind().as_ident()?;
                (path, ImportSuffix::As(Some(as_name)))
            }
            NodeKind::ImportDecl { glob: false } => (view, ImportSuffix::As(None)),
            NodeKind::ImportDecl { glob: true } => (view, ImportSuffix::All),
            _ => return None,
        };
        Some(Self { path_node, suffix, view })
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

    pub fn last_path_segment_span(&self) -> Span<TokenIdx> {
        self.path_node
            .children()
            .filter(|c| c.ident().is_some())
            .last()
            .expect("import must have at least one path segment")
            .span()
    }

    pub fn first_path_segment_span(&self) -> Span<TokenIdx> {
        self.path_node
            .children()
            .find(|c| c.ident().is_some())
            .expect("import must have at least one path segment")
            .span()
    }

    /// Span covering the segments that determine the imported file path.
    /// For `import m::sub::X;` this is `m::sub`, for `import m::sub::*;` this is `m::sub`.
    pub fn file_path_span(&self) -> Span<TokenIdx> {
        let mut idents = self.path_node.children().filter(|c| c.ident().is_some());
        let first = idents.next().expect("import must have at least one path segment");
        match self.suffix {
            ImportSuffix::All => {
                let mut last = first;
                for ident in idents {
                    last = ident;
                }
                Span::new(first.span().start, last.span().end)
            }
            ImportSuffix::As(_) => {
                let mut second_to_last = first;
                let mut last = first;
                for ident in idents {
                    second_to_last = last;
                    last = ident;
                }
                Span::new(first.span().start, second_to_last.span().end)
            }
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum TopLevelDef<'cst> {
    Init(InitBlock<'cst>),
    Run(RunBlock<'cst>),
    Const(ConstDecl<'cst>),
    Import(Import<'cst>),
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
            if let Some(def) = ConstDecl::new(child) {
                return TopLevelDef::Const(def);
            }
            if let Some(def) = InitBlock::new(child) {
                return TopLevelDef::Init(def);
            }
            if let Some(def) = RunBlock::new(child) {
                return TopLevelDef::Run(def);
            }
            if let Some(def) = Import::new(child) {
                return TopLevelDef::Import(def);
            }
            panic!("unexpected top-level node kind: {:?}", child.kind())
        })
    }
}
