mod errors;
mod token_items;

use crate::{
    cst::{self, *},
    lexer::*,
};
use allocator_api2::vec::Vec;
use plank_core::{Idx, IndexVec, Span, bigint, list_of_lists::ListOfLists};
use plank_session::{Session, SourceByteOffset, SourceId, SourceSpan, StrId};

const CONST_DEF_EXPR_RECOVERY: &[Token] = &[Token::Init, Token::Run, Token::Const, Token::Import];

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct OpPriority(u8);

impl OpPriority {
    const ZERO: Self = OpPriority(0);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ParseExprMode {
    AllowAll,
    NoPostFixCurlyBrace,
}

impl ParseExprMode {
    fn allows_struct_literal(self) -> bool {
        match self {
            ParseExprMode::AllowAll => true,
            ParseExprMode::NoPostFixCurlyBrace => false,
        }
    }
}

enum StmtResult {
    Statement(NodeIdx),
    EndExprOrStmt(NodeIdx),
    EndExpr(NodeIdx),
}

#[derive(Debug, Clone, Copy)]
struct UnfinishedNode {
    idx: NodeIdx,
    last_child: Option<NodeIdx>,
}

pub(crate) struct Parser<'a> {
    pub(crate) session: &'a mut Session,
    pub(crate) source: &'a str,
    pub(crate) nodes: IndexVec<cst::NodeIdx, cst::Node>,
    pub(crate) num_lit_limbs: ListOfLists<NumLitId, u32>,
    pub(crate) expected: Vec<Token>,
    pub(crate) tokens: token_items::TokenItems<'a>,
    pub(crate) source_id: SourceId,
    pub(crate) last_src_span: SourceSpan,
    pub(crate) last_unexpected: Option<TokenIdx>,
}

const LEN_TO_NODE_CAPACITY: usize = 4;

impl<'a> Parser<'a> {
    const UNARY_PRIORITY: OpPriority = OpPriority(19);
    const MEMBER_PRIORITY: OpPriority = OpPriority(21);
    const FN_CALL_PRIORITY: OpPriority = OpPriority(21);
    const STRUCT_LITERAL_PRIORITY: OpPriority = OpPriority(21);

    fn new(
        session: &'a mut Session,
        lexed: &'a Lexed,
        source: &'a str,
        source_id: SourceId,
    ) -> Self {
        Parser {
            session,
            source,
            tokens: token_items::TokenItems::new(lexed),
            nodes: IndexVec::with_capacity(lexed.len().get() as usize / LEN_TO_NODE_CAPACITY),
            num_lit_limbs: ListOfLists::new(),
            expected: Vec::with_capacity(8),
            source_id,
            last_src_span: Span::new(SourceByteOffset::ZERO, SourceByteOffset::ZERO),
            last_unexpected: None,
        }
    }

    fn assert_complete(&mut self) {
        assert!(self.eof());
        for (i, node) in self.nodes.enumerate_idx() {
            assert!(!node.tokens.is_dummy(), "node #{} has dummy token span", i.get());
        }
    }

    fn current_token(&mut self) -> Token {
        self.tokens.peek().0
    }

    fn advance(&mut self) {
        self.expected.clear();

        let ti = self.tokens.current();
        let (token, src_span) = self.tokens.next();
        self.last_src_span = src_span;
        if let Some(error) = token.lex_error()
            && self.last_unexpected != Some(ti)
        {
            self.emit_lexer_error(error, ti);
        }
    }

    fn at(&mut self, token: Token) -> bool {
        self.current_token() == token
    }

    fn skip_trivia(&mut self) {
        while let token = self.current_token()
            && token.is_trivia()
        {
            self.advance();
        }
    }

    fn check(&mut self, token: Token) -> bool {
        self.skip_trivia();
        if self.at(token) {
            return true;
        }
        if !self.expected.contains(&token) {
            self.expected.push(token);
        }
        false
    }

    fn eat(&mut self, token: Token) -> bool {
        if self.check(token) {
            self.advance();
            return true;
        }
        false
    }

    fn emit_unexpected(&mut self) {
        if self.at_last_unexpected() {
            return;
        }
        let (found, span) = self.tokens.peek();
        self.last_unexpected = Some(self.tokens.current());
        self.emit_unexpected_token(found, span);
        self.expected.clear();
    }

    fn eof(&mut self) -> bool {
        self.skip_trivia();
        self.at(Token::Eof)
    }

    fn skip_until_decl_start(&mut self) {
        self.advance();
        while !self.eof() {
            self.skip_trivia();
            match self.current_token() {
                Token::Init | Token::Run | Token::Const | Token::Import => return,
                _ => self.advance(),
            }
        }
    }

    /// `parse_fn` returns `true` if it successfully parsed an element, `false` if the
    /// current token cannot start an element (must populate `expected` via check/eat).
    fn parse_delimited(
        &mut self,
        opener: Token,
        terminator: Token,
        delimiter: Token,
        mut parse_fn: impl FnMut(&mut Self) -> bool,
    ) {
        self.expect(opener);
        let mut error_emitted = false;
        loop {
            let expected_checkpoint = self.expected.len();
            if parse_fn(self) {
                error_emitted = false;
                if !self.eat(delimiter) {
                    break;
                }
            } else {
                debug_assert!(
                    self.expected.len() > expected_checkpoint,
                    "parse_fn must use check/eat before returning false to populate expected tokens"
                );
                if self.check(terminator) || self.eof() {
                    break;
                }

                if !error_emitted {
                    self.emit_unexpected();
                    error_emitted = true;
                }
                self.advance();
            }
        }
        self.expect(terminator);
    }

    fn at_last_unexpected(&self) -> bool {
        self.last_unexpected.is_some_and(|ti| ti == self.tokens.current())
    }

    fn expect_check_recovery(&mut self, expected: Token, recovery_tokens: &[Token]) -> bool {
        if self.eat(expected) {
            return true;
        }

        if self.at_last_unexpected() {
            return false;
        }
        let (next, _) = self.tokens.peek();
        if recovery_tokens.contains(&next)
            && let Some((_, prev_span)) = self.tokens.get_prev()
        {
            self.emit_missing_token(expected, prev_span);
            self.last_unexpected = Some(self.tokens.current());
            self.expected.clear();
        } else {
            self.emit_unexpected();
        }
        false
    }

    fn expect(&mut self, token: Token) -> bool {
        let eaten = self.eat(token);
        if !eaten {
            self.emit_unexpected();
        }
        eaten
    }

    fn alloc_node_from(&mut self, start: TokenIdx, kind: NodeKind) -> UnfinishedNode {
        let idx = self.nodes.push(Node {
            kind,
            tokens: Span::new(start, start),
            next_sibling: None,
            first_child: None,
        });
        UnfinishedNode { idx, last_child: None }
    }

    fn skip_trivia_start(&mut self) -> TokenIdx {
        self.skip_trivia();
        self.tokens.current()
    }

    fn alloc_node(&mut self, kind: NodeKind) -> UnfinishedNode {
        let idx = self.nodes.push(Node {
            kind,
            tokens: Span::new(self.tokens.current(), self.tokens.current()),
            next_sibling: None,
            first_child: None,
        });
        UnfinishedNode { idx, last_child: None }
    }

    fn close_node(&mut self, node: UnfinishedNode) -> NodeIdx {
        self.nodes[node.idx].tokens.end = self.tokens.current();
        node.idx
    }

    fn update_kind(&mut self, node: UnfinishedNode, kind: NodeKind) {
        self.nodes[node.idx].kind = kind;
    }

    fn push_child(&mut self, parent: &mut UnfinishedNode, child: NodeIdx) {
        match parent.last_child {
            Some(last_child) => {
                debug_assert!(self.nodes[parent.idx].first_child.is_some());
                debug_assert!(self.nodes[last_child].next_sibling.is_none());
                debug_assert!(
                    self.nodes[last_child].tokens.end <= self.nodes[child].tokens.start,
                    "children tokens overlap"
                );
                self.nodes[last_child].next_sibling = Some(child);
                parent.last_child = Some(child);
            }
            None => {
                debug_assert!(self.nodes[parent.idx].first_child.is_none());
                self.nodes[parent.idx].first_child = Some(child);
                parent.last_child = Some(child);
            }
        }
    }

    fn alloc_last_token_as_node(&mut self, kind: NodeKind) -> NodeIdx {
        let node = self.alloc_node_from(self.tokens.current() - 1, kind);
        self.close_node(node)
    }

    fn try_parse_num_literal(&mut self) -> Option<NodeKind> {
        type ParseFn = fn(&str, &mut ListOfLists<NumLitId, u32>) -> NumLitId;

        let token_idx = self.tokens.current();
        let (token, _) = self.tokens.peek();

        let (prefix_len, parse_fn): (usize, ParseFn) = match token {
            Token::DecimalLiteral => (0, bigint::from_radix10_in),
            Token::HexLiteral => (2, bigint::from_radix16_in), // Skip "0x"
            Token::BinLiteral => (2, bigint::from_radix2_in),  // Skip "0b"
            _ => return None,
        };

        self.advance();

        let span = self.tokens.token_src_span(token_idx);
        let src = &self.source[span.usize_range()];
        let (negative, digits) = if let Some(rest) = src.strip_prefix('-') {
            (true, &rest[prefix_len..])
        } else {
            (false, &src[prefix_len..])
        };

        let id = parse_fn(digits, &mut self.num_lit_limbs);
        Some(NodeKind::NumLiteral { negative, id })
    }

    // ======================== EXPRESSION PARSING (PRATT) ========================

    fn check_binary_op(&mut self) -> Option<(OpPriority, OpPriority, BinaryOp)> {
        macro_rules! check_binary_op {
            ($($kind:ident => ($left:literal, $right:literal)),* $(,)?) => {
                $(
                    if self.check(Token::$kind) {
                        return Some((OpPriority($left), OpPriority($right), BinaryOp::$kind));
                    }
                )*
            };
        }

        check_binary_op! {
            Or => (1, 2),
            And => (3, 4),
            DoubleEquals => (5, 6),
            BangEquals => (5, 6),
            LessThan => (5, 6),
            GreaterThan => (5, 6),
            LessEquals => (5, 6),
            GreaterEquals => (5, 6),
            Pipe => (7, 8),
            Caret => (9, 10),
            Ampersand => (11, 12),
            ShiftLeft => (13, 14),
            ShiftRight => (13, 14),
            Plus => (15, 16),
            Minus => (15, 16),
            PlusPercent => (15, 16),
            MinusPercent => (15, 16),
            Star => (17, 18),
            Slash => (17, 18),
            Percent => (17, 18),
            StarPercent => (17, 18),
            SlashPlus => (17, 18),
            SlashNeg => (17, 18),
            SlashLess => (17, 18),
            SlashGreater => (17, 18),
        }

        None
    }

    fn eat_unary(&mut self) -> Option<((), OpPriority, UnaryOp)> {
        if self.eat(Token::Minus) {
            return Some(((), Self::UNARY_PRIORITY, UnaryOp::Minus));
        }
        if self.eat(Token::Bang) {
            return Some(((), Self::UNARY_PRIORITY, UnaryOp::Bang));
        }
        if self.eat(Token::Tilde) {
            return Some(((), Self::UNARY_PRIORITY, UnaryOp::Tilde));
        }
        None
    }

    fn intern(&mut self, ti: TokenIdx) -> StrId {
        let span = self.tokens.token_src_span(ti);
        debug_assert!(
            matches!(self.tokens.get_prev(), Some((Token::Identifier, p_span)) if p_span == span)
        );
        let source = &self.source[span.usize_range()];
        self.session.intern(source)
    }

    fn try_parse_ident(&mut self) -> Option<NodeIdx> {
        if self.eat(Token::Identifier) {
            let ident = self.intern(self.tokens.current() - 1);
            return Some(self.alloc_last_token_as_node(NodeKind::Identifier { ident }));
        }
        None
    }

    fn expect_ident(&mut self) -> NodeIdx {
        self.try_parse_ident().unwrap_or_else(|| {
            self.emit_unexpected();
            let error = self.alloc_node(NodeKind::Error);
            self.close_node(error)
        })
    }

    // ========================== EXPRESSION PARSING ==========================

    fn try_parse_conditional(&mut self) -> Option<NodeIdx> {
        let condition_chain_start = self.tokens.current();
        if !self.eat(Token::If) {
            return None;
        }

        let mut conditional = self.alloc_node_from(condition_chain_start, NodeKind::If);

        let if_condition = self.parse_expr(ParseExprMode::NoPostFixCurlyBrace);
        self.push_child(&mut conditional, if_condition);
        let if_body = self.parse_block(self.tokens.current(), NodeKind::Block);
        self.push_child(&mut conditional, if_body);

        let mut else_ifs = self.alloc_node(NodeKind::ElseIfBranchList);

        let mut r#else = None;
        while self.check(Token::Else) {
            // More robust way to get token offset at the `Else` token than `eat; current-1`.
            let branch_start = self.tokens.current();
            assert!(self.expect(Token::Else));

            if !self.eat(Token::If) {
                let else_body = self.parse_block(self.tokens.current(), NodeKind::Block);
                r#else = Some(else_body);

                break;
            }

            let mut else_if = self.alloc_node_from(branch_start, NodeKind::ElseIfBranch);

            let else_condition = self.parse_expr(ParseExprMode::NoPostFixCurlyBrace);
            self.push_child(&mut else_if, else_condition);
            let branch_body = self.parse_block(self.tokens.current(), NodeKind::Block);
            self.push_child(&mut else_if, branch_body);

            let else_if = self.close_node(else_if);
            self.push_child(&mut else_ifs, else_if);
        }

        let else_ifs = self.close_node(else_ifs);
        self.push_child(&mut conditional, else_ifs);

        if let Some(r#else) = r#else {
            self.nodes[else_ifs].tokens.end = self.nodes[r#else].tokens.start;
            self.push_child(&mut conditional, r#else);
        }

        Some(self.close_node(conditional))
    }

    fn try_parse_standalone_expr(&mut self) -> Option<NodeIdx> {
        let start = self.tokens.current();

        if self.eat(Token::True) {
            return Some(self.alloc_last_token_as_node(NodeKind::BoolLiteral(true)));
        }
        if self.eat(Token::False) {
            return Some(self.alloc_last_token_as_node(NodeKind::BoolLiteral(false)));
        }

        if let Some(kind) = self.try_parse_num_literal() {
            return Some(self.alloc_last_token_as_node(kind));
        }

        if let Some(identifier) = self.try_parse_ident() {
            return Some(identifier);
        }

        if self.eat(Token::LeftRound) {
            // TODO: Track recursion to emit nice error instead of stack overflow.
            let mut paren_expr = self.alloc_node_from(start, NodeKind::ParenExpr);
            let inner_expr = self.parse_expr(ParseExprMode::AllowAll);
            self.push_child(&mut paren_expr, inner_expr);
            self.expect(Token::RightRound);
            return Some(self.close_node(paren_expr));
        }

        if self.eat(Token::Comptime) {
            return Some(self.parse_block(start, NodeKind::ComptimeBlock));
        }

        if self.eat(Token::Fn) {
            return Some(self.parse_function_def(start));
        }

        if self.eat(Token::Struct) {
            return Some(self.parse_struct_def(start));
        }

        if self.check(Token::LeftCurly) {
            return Some(self.parse_block(self.tokens.current(), NodeKind::Block));
        }

        if let Some(conditional) = self.try_parse_conditional() {
            return Some(conditional);
        }

        None
    }

    fn parse_function_def(&mut self, start: TokenIdx) -> NodeIdx {
        let mut function = self.alloc_node_from(start, NodeKind::FnDef);
        let mut parameter_list = self.alloc_node(NodeKind::ParamList);
        self.parse_delimited(Token::LeftRound, Token::RightRound, Token::Comma, |parser| {
            let parameter_start = parser.tokens.current();
            let mut parameter = if parser.eat(Token::Comptime) {
                let mut parameter =
                    parser.alloc_node_from(parameter_start, NodeKind::ComptimeParameter);
                let name = parser.expect_ident();
                parser.push_child(&mut parameter, name);
                parameter
            } else if parser.eat(Token::Identifier) {
                let mut parameter = parser.alloc_node_from(parameter_start, NodeKind::Parameter);
                let ident = parser.intern(parser.tokens.current() - 1);
                let name = parser.alloc_last_token_as_node(NodeKind::Identifier { ident });
                parser.push_child(&mut parameter, name);
                parameter
            } else {
                return false;
            };

            parser.expect(Token::Colon);
            let r#type = parser.parse_expr(ParseExprMode::AllowAll);
            parser.push_child(&mut parameter, r#type);

            let parameter = parser.close_node(parameter);
            parser.push_child(&mut parameter_list, parameter);
            true
        });
        let parameter_list = self.close_node(parameter_list);
        self.push_child(&mut function, parameter_list);

        let return_type = self.parse_expr(ParseExprMode::NoPostFixCurlyBrace);
        self.push_child(&mut function, return_type);

        let body = self.parse_block(self.tokens.current(), NodeKind::Block);
        self.push_child(&mut function, body);

        self.close_node(function)
    }

    fn parse_struct_def(&mut self, start: TokenIdx) -> NodeIdx {
        let mut struct_def = self.alloc_node_from(start, NodeKind::StructDef);
        if !self.check(Token::LeftCurly)
            && let Some(type_index) = self.try_parse_expr(ParseExprMode::NoPostFixCurlyBrace)
        {
            self.push_child(&mut struct_def, type_index);
        }

        self.parse_delimited(Token::LeftCurly, Token::RightCurly, Token::Comma, |parser| {
            if !parser.check(Token::Identifier) {
                return false;
            }

            let mut field = parser.alloc_node(NodeKind::FieldDef);

            let name = parser.try_parse_ident().expect("read ident token, but no ident?");
            parser.push_child(&mut field, name);

            parser.expect(Token::Colon);

            let r#type = parser.parse_expr(ParseExprMode::AllowAll);
            parser.push_child(&mut field, r#type);

            let field = parser.close_node(field);
            parser.push_child(&mut struct_def, field);
            true
        });

        self.close_node(struct_def)
    }

    fn parse_expr(&mut self, mode: ParseExprMode) -> NodeIdx {
        self.try_parse_expr(mode).unwrap_or_else(|| {
            self.emit_unexpected();
            let err = self.alloc_node(NodeKind::Error);
            self.close_node(err)
        })
    }

    fn try_parse_expr(&mut self, mode: ParseExprMode) -> Option<NodeIdx> {
        let checkpoint = self.expected.len();
        let node = self.try_parse_expr_min_bp(mode, OpPriority::ZERO);
        if node.is_some() {
            self.expected.truncate(checkpoint);
        }
        node
    }

    fn try_parse_expr_min_bp(
        &mut self,
        mode: ParseExprMode,
        min_bp: OpPriority,
    ) -> Option<NodeIdx> {
        let start = self.skip_trivia_start();

        let mut expr = if let Some(((), rhs, kind)) = self.eat_unary() {
            let mut unary = self.alloc_node_from(start, NodeKind::UnaryExpr(kind));
            // TODO: Track recursion
            let expr = self.try_parse_expr_min_bp(mode, rhs).unwrap_or_else(|| {
                self.emit_unexpected();
                let err = self.alloc_node(NodeKind::Error);
                self.close_node(err)
            });
            self.push_child(&mut unary, expr);
            self.close_node(unary)
        } else {
            self.try_parse_standalone_expr()?
        };

        loop {
            if Self::MEMBER_PRIORITY > min_bp && self.eat(Token::Dot) {
                let mut member = self.alloc_node_from(start, NodeKind::MemberExpr);
                self.push_child(&mut member, expr);
                let access_name = if let Some(ident) = self.try_parse_ident() {
                    ident
                } else {
                    self.emit_unexpected();
                    let error = self.alloc_node(NodeKind::Error);
                    if !self.at(Token::Semicolon)
                        && !self.at(Token::RightCurly)
                        && !self.at(Token::Eof)
                    {
                        self.advance();
                    }
                    self.close_node(error)
                };
                self.push_child(&mut member, access_name);
                expr = self.close_node(member);
                continue;
            }

            if Self::FN_CALL_PRIORITY > min_bp && self.check(Token::LeftRound) {
                let mut call = self.alloc_node_from(start, NodeKind::CallExpr);
                self.push_child(&mut call, expr);
                self.parse_delimited(Token::LeftRound, Token::RightRound, Token::Comma, |parser| {
                    let Some(argument) = parser.try_parse_expr(ParseExprMode::AllowAll) else {
                        return false;
                    };
                    parser.push_child(&mut call, argument);
                    true
                });
                expr = self.close_node(call);
                continue;
            }

            if mode.allows_struct_literal()
                && Self::STRUCT_LITERAL_PRIORITY > min_bp
                && self.check(Token::LeftCurly)
            {
                let mut struct_literal = self.alloc_node_from(start, NodeKind::StructLit);
                self.push_child(&mut struct_literal, expr);

                self.parse_delimited(Token::LeftCurly, Token::RightCurly, Token::Comma, |parser| {
                    if !parser.check(Token::Identifier) {
                        return false;
                    }

                    let mut field = parser.alloc_node(NodeKind::FieldAssign);
                    let name = parser.try_parse_ident().expect("read ident token, but no ident?");
                    parser.push_child(&mut field, name);
                    parser.expect(Token::Colon);
                    let value = parser.parse_expr(ParseExprMode::AllowAll);
                    parser.push_child(&mut field, value);

                    let field = parser.close_node(field);
                    parser.push_child(&mut struct_literal, field);
                    true
                });
                expr = self.close_node(struct_literal);
                continue;
            }

            if let Some((lhs, rhs, kind)) = self.check_binary_op() {
                if lhs < min_bp {
                    break;
                }
                self.advance(); // consume operator token
                let mut binary_expr = self.alloc_node_from(start, NodeKind::BinaryExpr(kind));
                self.push_child(&mut binary_expr, expr);
                let rhs_expr = self.try_parse_expr_min_bp(mode, rhs).unwrap_or_else(|| {
                    self.emit_unexpected();
                    let err = self.alloc_node(NodeKind::Error);
                    self.close_node(err)
                });
                self.push_child(&mut binary_expr, rhs_expr);
                expr = self.close_node(binary_expr);
                continue;
            }

            break;
        }

        Some(expr)
    }

    // ========================== STATEMENT PARSING ==========================

    fn try_parse_while(&mut self, while_start: TokenIdx) -> Option<NodeIdx> {
        let is_inline = self.eat(Token::Inline);

        if is_inline {
            self.expect(Token::While);
        } else if !self.eat(Token::While) {
            return None;
        }

        let kind = if is_inline { NodeKind::InlineWhileStmt } else { NodeKind::WhileStmt };

        let mut while_stmt = self.alloc_node_from(while_start, kind);

        let condition = self.parse_expr(ParseExprMode::NoPostFixCurlyBrace);
        self.push_child(&mut while_stmt, condition);

        let body = self.parse_block(self.tokens.current(), NodeKind::Block);
        self.push_child(&mut while_stmt, body);

        Some(self.close_node(while_stmt))
    }

    fn try_parse_stmt(&mut self) -> Option<StmtResult> {
        let stmt_start = self.tokens.current();
        let expected_checkpoint = self.expected.len();

        self.skip_trivia();

        if let Some(r#while) = self.try_parse_while(stmt_start) {
            return Some(StmtResult::Statement(r#while));
        }

        if self.eat(Token::Return) {
            let mut r#return = self.alloc_node_from(stmt_start, NodeKind::ReturnStmt);
            let return_expr = self.parse_expr(ParseExprMode::AllowAll);
            self.push_child(&mut r#return, return_expr);
            self.expect(Token::Semicolon);
            let r#return = self.close_node(r#return);
            return Some(StmtResult::Statement(r#return));
        }

        if self.eat(Token::Let) {
            let mutable = self.eat(Token::Mut);
            let mut r#let =
                self.alloc_node_from(stmt_start, NodeKind::LetStmt { mutable, typed: false });

            let name = self.expect_ident();
            self.push_child(&mut r#let, name);

            if self.eat(Token::Colon) {
                self.update_kind(r#let, NodeKind::LetStmt { mutable, typed: true });
                let type_expr = self.parse_expr(ParseExprMode::AllowAll);
                self.push_child(&mut r#let, type_expr);
            }

            self.expect(Token::Equals);

            let assign = self.parse_expr(ParseExprMode::AllowAll);
            self.push_child(&mut r#let, assign);

            self.expect(Token::Semicolon);

            let r#let = self.close_node(r#let);
            return Some(StmtResult::Statement(r#let));
        }

        let Some(expr) = self.try_parse_expr(ParseExprMode::AllowAll) else {
            // Undo tokens added to `expected` by our check/eat calls (while, return, let, etc.)
            // so the caller's emit_unexpected reports its own expected tokens, not ours.
            self.expected.truncate(expected_checkpoint);
            return None;
        };

        if self.eat(Token::Equals) {
            let mut assign = self.alloc_node_from(stmt_start, NodeKind::AssignStmt);
            self.push_child(&mut assign, expr);
            let rhs = self.parse_expr(ParseExprMode::AllowAll);
            self.push_child(&mut assign, rhs);
            self.expect(Token::Semicolon);
            return Some(StmtResult::Statement(self.close_node(assign)));
        }

        if self.eat(Token::Semicolon) {
            return Some(StmtResult::Statement(expr));
        }

        let expr_kind = self.nodes[expr].kind;
        let requires_semi = expr_kind
            .expr_requires_semi_as_stmt()
            .unwrap_or_else(|| panic!("`try_parse_expr` returned non-expr node {:?}", expr_kind));

        if requires_semi {
            Some(StmtResult::EndExpr(expr))
        } else {
            Some(StmtResult::EndExprOrStmt(expr))
        }
    }

    fn parse_block(&mut self, block_start: TokenIdx, block_kind: NodeKind) -> NodeIdx {
        let mut block = self.alloc_node_from(block_start, block_kind);

        self.expect(Token::LeftCurly);

        let mut statements_list = self.alloc_node(NodeKind::StatementsList);
        let mut end_expr = None;

        while !self.check(Token::RightCurly) {
            let Some(result) = self.try_parse_stmt() else {
                self.emit_unexpected();
                break;
            };
            self.expected.clear();

            if let Some(prev_end) = end_expr.take() {
                self.push_child(&mut statements_list, prev_end);
            }

            match result {
                StmtResult::Statement(stmt) => self.push_child(&mut statements_list, stmt),
                StmtResult::EndExprOrStmt(expr) => end_expr = Some(expr),
                StmtResult::EndExpr(expr) => {
                    if self.check(Token::RightCurly) || self.eof() {
                        // Expression without semicolon is the block's tail expression.
                        end_expr = Some(expr);
                        break;
                    } else {
                        // Otherwise the `;` is missing.
                        self.emit_missing_specific(Token::Semicolon, self.last_src_span);
                        self.push_child(&mut statements_list, expr);
                    }
                }
            }
        }

        let statements_list = self.close_node(statements_list);
        self.push_child(&mut block, statements_list);
        if let Some(end_expr) = end_expr {
            self.nodes[statements_list].tokens.end = self.nodes[end_expr].tokens.start;
            self.push_child(&mut block, end_expr);
        }

        self.expect(Token::RightCurly);

        self.close_node(block)
    }

    // ======================== TOP-LEVEL DECLARATIONS ========================

    fn parse_file(&mut self) -> NodeIdx {
        let mut file = self.alloc_node(NodeKind::File);

        while !self.eof() {
            let new_decl = self.parse_decl();
            self.push_child(&mut file, new_decl);
        }

        self.close_node(file)
    }

    fn parse_decl(&mut self) -> NodeIdx {
        let start = self.tokens.current();
        if self.eat(Token::Init) {
            self.parse_block(start, NodeKind::InitBlock)
        } else if self.eat(Token::Run) {
            self.parse_block(start, NodeKind::RunBlock)
        } else if self.eat(Token::Const) {
            self.parse_const_decl(start)
        } else if self.eat(Token::Import) {
            self.parse_import_decl(start)
        } else {
            self.emit_unexpected();
            self.skip_until_decl_start();
            let node = self.alloc_node_from(start, NodeKind::Error);
            self.close_node(node)
        }
    }

    fn parse_import_decl(&mut self, start: TokenIdx) -> NodeIdx {
        let mut import_path = self.alloc_node_from(start, NodeKind::ImportDecl { glob: false });
        let path_start = self.expect_ident();
        self.push_child(&mut import_path, path_start);

        while self.eat(Token::DoubleColon) {
            if self.eat(Token::Star) {
                self.update_kind(import_path, NodeKind::ImportDecl { glob: true });
                self.expect(Token::Semicolon);
                return self.close_node(import_path);
            }

            let ident = self.expect_ident();
            self.push_child(&mut import_path, ident);
        }

        if self.eat(Token::Semicolon) {
            return self.close_node(import_path);
        }

        self.update_kind(import_path, NodeKind::ImportPath);
        let import_path = self.close_node(import_path);
        let mut import = self.alloc_node_from(start, NodeKind::ImportAsDecl);
        self.push_child(&mut import, import_path);

        self.expect(Token::As);
        let as_name = self.expect_ident();
        self.push_child(&mut import, as_name);
        self.expect(Token::Semicolon);

        self.close_node(import)
    }

    fn parse_const_decl(&mut self, start: TokenIdx) -> NodeIdx {
        let mut r#const = self.alloc_node_from(start, NodeKind::ConstDecl { typed: false });
        let name = self.expect_ident();
        self.push_child(&mut r#const, name);

        // Optional type annotation
        if self.eat(Token::Colon) {
            self.update_kind(r#const, NodeKind::ConstDecl { typed: true });
            let type_expr = self.parse_expr(ParseExprMode::AllowAll);
            self.push_child(&mut r#const, type_expr);
        }

        // = value
        self.expect_check_recovery(Token::Equals, CONST_DEF_EXPR_RECOVERY);

        let expr = self.parse_expr(ParseExprMode::AllowAll);
        self.push_child(&mut r#const, expr);

        self.expect_check_recovery(Token::Semicolon, CONST_DEF_EXPR_RECOVERY);

        self.close_node(r#const)
    }
}

pub fn parse(
    session: &mut Session,
    lexed: &Lexed,
    source: &str,
    source_id: SourceId,
) -> ConcreteSyntaxTree {
    let mut parser = Parser::new(session, lexed, source, source_id);

    let file = parser.parse_file();
    assert_eq!(file, ConcreteSyntaxTree::FILE_IDX);

    parser.assert_complete();

    ConcreteSyntaxTree { nodes: parser.nodes, num_lit_limbs: parser.num_lit_limbs }
}
