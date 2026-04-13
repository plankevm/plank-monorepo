use std::cell::RefCell;

use hashbrown::HashMap;
use plank_core::{Idx, IncIterable, IndexVec, list_of_lists::ListOfLists};
use plank_parser::{
    ast::{self, Statement, TopLevelDef},
    cst::{self, NumLitId},
    lexer::{Lexed, TokenSpan},
};
use plank_session::{EvmBuiltin, Poisoned, Session, SourceId, SourceSpan, StrId, TypeId};
use plank_source::project::{FileImport, ImportKind};
use plank_values::ValueInterner;

use crate::operators as hir_ops;

mod diagnostics;

use plank_source::ParsedProject;

use crate::*;

#[derive(Clone, Copy)]
struct ScopedLocal {
    name: StrId,
    id: LocalId,
    mutable: bool,
    span: Option<TokenSpan>,
}

struct HirBuilder {
    block_instrs: ListOfLists<BlockId, Instruction>,
    block_spans: IndexVec<BlockId, MaybePoisoned<SourceSpan>>,

    call_args: ListOfLists<CallArgsId, LocalId>,
    fields: ListOfLists<FieldsId, FieldInfo>,
    struct_defs: IndexVec<StructDefId, StructDef>,

    fns: IndexVec<FnDefId, FnDef>,
    fn_params: ListOfLists<FnDefId, ParamInfo>,
    fn_captures: ListOfLists<FnDefId, CaptureInfo>,
}

impl HirBuilder {
    fn new() -> Self {
        Self {
            block_instrs: ListOfLists::new(),
            block_spans: IndexVec::new(),
            call_args: ListOfLists::new(),
            fields: ListOfLists::new(),
            fns: IndexVec::new(),
            fn_params: ListOfLists::new(),
            fn_captures: ListOfLists::new(),
            struct_defs: IndexVec::new(),
        }
    }
}

#[derive(Clone, Copy)]
struct ScopedConst {
    const_id: ConstId,
    source_id: SourceId,
    span: SourceSpan,
    imported: bool,
}

struct BlockLowerer<'a> {
    consts: HashMap<StrId, ScopedConst>,
    num_lit_limbs: &'a ListOfLists<NumLitId, u32>,
    session: RefCell<&'a mut Session>,

    values: &'a mut ValueInterner,
    builder: &'a mut HirBuilder,
    scoped_locals_stack: Vec<ScopedLocal>,
    fn_scope_start: usize,
    fn_captures_start: usize,
    next_local_id: LocalId,

    instructions_buf: Vec<Instruction>,
    locals_buf: Vec<LocalId>,
    field_buf: Vec<FieldInfo>,
    captures_buf: Vec<CaptureInfo>,

    lexed: &'a Lexed,
    source_id: SourceId,
}
enum ShortCircuitOp {
    And,
    Or,
}

impl BlockLowerer<'_> {
    fn build_file_scope(
        &mut self,
        source_consts: &ListOfLists<SourceId, (StrId, ConstId)>,
        imports: &ListOfLists<SourceId, FileImport>,
        const_defs: &IndexVec<ConstId, ConstDef>,
    ) {
        self.consts.clear();
        for &(name, const_id) in &source_consts[self.source_id] {
            let def = &const_defs[const_id];
            self.consts.insert(
                name,
                ScopedConst {
                    const_id,
                    source_id: def.source_id,
                    span: def.source_span,
                    imported: false,
                },
            );
        }
        for import in &imports[self.source_id] {
            let import_source_id = self.source_id;
            let import_source_span = self.lexed.tokens_src_span(import.span);
            match import.kind {
                ImportKind::Specific { selected_name, imported_as, name_span } => {
                    let Some(const_id) = source_consts[import.target_source]
                        .iter()
                        .find_map(|&(name, const_id)| (name == selected_name).then_some(const_id))
                    else {
                        self.error_unresolved_import(
                            selected_name,
                            name_span,
                            import.target_source,
                        );
                        continue;
                    };
                    let entry = ScopedConst {
                        const_id,
                        source_id: import_source_id,
                        span: import_source_span,
                        imported: true,
                    };
                    let Some(prev) = self.consts.insert(imported_as, entry) else { continue };
                    self.error_import_collision(
                        imported_as,
                        import.span,
                        prev.source_id,
                        prev.span,
                        prev.imported,
                        None,
                    );
                }
                ImportKind::All => {
                    for &(name, const_id) in &source_consts[import.target_source] {
                        let entry = ScopedConst {
                            const_id,
                            source_id: import_source_id,
                            span: import_source_span,
                            imported: true,
                        };
                        let Some(prev) = self.consts.insert(name, entry) else { continue };
                        let def = &const_defs[const_id];
                        self.error_import_collision(
                            name,
                            import.span,
                            prev.source_id,
                            prev.span,
                            prev.imported,
                            Some((def.source_id, def.source_span)),
                        );
                    }
                }
            }
        }
    }

    fn reset_scope(&mut self) {
        self.next_local_id = LocalId::ZERO;
        self.scoped_locals_stack.clear();

        debug_assert_eq!(self.fn_scope_start, 0);
        debug_assert_eq!(self.fn_captures_start, 0);
        debug_assert!(self.instructions_buf.is_empty());
        debug_assert!(self.locals_buf.is_empty());
        debug_assert!(self.field_buf.is_empty());
        debug_assert!(self.captures_buf.is_empty());
    }

    fn alloc_local(&mut self, name: StrId, mutable: bool, span: TokenSpan) -> LocalId {
        if TypeId::resolve_primitive(name).is_some() {
            self.error_shadowing_primitive_type(name, span);
        } else if EvmBuiltin::from_str_id(name).is_some() {
            self.error_shadowing_builtin(name, span);
        }

        let id = self.next_local_id.get_and_inc();
        self.scoped_locals_stack.push(ScopedLocal { name, id, mutable, span: Some(span) });
        id
    }

    fn alloc_anonymous_local(&mut self, name: StrId) -> LocalId {
        let id = self.next_local_id.get_and_inc();
        self.scoped_locals_stack.push(ScopedLocal { name, id, mutable: false, span: None });
        id
    }

    fn alloc_temp(&mut self) -> LocalId {
        self.next_local_id.get_and_inc()
    }

    fn expr(&self, kind: ExprKind, span: TokenSpan) -> Expr {
        Expr { kind, span: self.lexed.tokens_src_span(span) }
    }

    fn lower_expr_to_local(&mut self, expr: ast::Expr<'_>) -> LocalId {
        let expr = self.lower_expr(expr);
        let local = self.alloc_temp();
        self.emit(InstructionKind::Set { local, r#type: None, expr });
        local
    }

    fn create_sub_block(&mut self, span: TokenSpan, f: impl FnOnce(&mut Self)) -> BlockId {
        self.create_sub_block_with(span, f).0
    }

    fn create_sub_block_with<R>(
        &mut self,
        span: TokenSpan,
        f: impl FnOnce(&mut Self) -> R,
    ) -> (BlockId, R) {
        let locals_start = self.scoped_locals_stack.len();
        let block_start = self.instructions_buf.len();
        let result = f(self);
        self.scoped_locals_stack.truncate(locals_start);
        let src_span = self.lexed.tokens_src_span(span);
        (self.flush_instructions_from(block_start, src_span), result)
    }

    fn lower_body_to_block(&mut self, block: ast::BlockExpr<'_>) -> BlockId {
        self.create_sub_block(block.node().span(), |this| {
            for stmt in block.statements() {
                this.lower_statement(stmt);
            }
            if let Some(e) = block.end_expr() {
                let value = this.lower_expr(e);
                this.emit(InstructionKind::Eval(value));
            }
        })
    }

    fn lower_branch_body(&mut self, block: ast::BlockExpr<'_>, result: LocalId) -> BlockId {
        self.create_sub_block(block.node().span(), |this| {
            for stmt in block.statements() {
                this.lower_statement(stmt);
            }
            let expr = match block.end_expr() {
                Some(e) => this.lower_expr(e),
                None => {
                    let span = block.node().span();
                    this.expr(ExprKind::VOID, span)
                }
            };
            this.emit(InstructionKind::BranchSet { local: result, expr });
        })
    }

    fn lower_fn_body_block(&mut self, block: ast::BlockExpr<'_>) -> BlockId {
        self.create_sub_block(block.node().span(), |this| {
            for stmt in block.statements() {
                this.lower_statement(stmt);
            }
            let value = match block.end_expr() {
                Some(e) => this.lower_expr(e),
                None => {
                    let span = block.node().span();
                    this.expr(ExprKind::VOID, span)
                }
            };
            this.emit(InstructionKind::Return(value));
        })
    }

    fn find_in_scope(scope: &[ScopedLocal], name: StrId) -> Option<ScopedLocal> {
        scope.iter().rev().find(|entry| entry.name == name).copied()
    }

    fn find_local(&self, name: StrId) -> Option<ScopedLocal> {
        Self::find_in_scope(&self.scoped_locals_stack[self.fn_scope_start..], name)
    }

    fn lookup_capture(&mut self, name: StrId, use_span: TokenSpan) -> Option<LocalId> {
        let outer_local =
            Self::find_in_scope(&self.scoped_locals_stack[..self.fn_scope_start], name)?.id;

        for capture in &self.captures_buf[self.fn_captures_start..] {
            if capture.outer_local == outer_local {
                return Some(capture.inner_local);
            }
        }

        let use_span = self.lexed.tokens_src_span(use_span);
        let inner_local = self.alloc_anonymous_local(name);
        self.captures_buf.push(CaptureInfo { outer_local, inner_local, use_span });
        Some(inner_local)
    }

    fn emit(&mut self, kind: InstructionKind) {
        self.instructions_buf.push(Instruction { kind });
    }

    fn flush_instructions_from(&mut self, start: usize, span: SourceSpan) -> BlockId {
        let block_id = self.builder.block_instrs.push_iter(self.instructions_buf.drain(start..));
        let span_id = self.builder.block_spans.push(Ok(span));
        assert_eq!(block_id, span_id, "block_instrs and block_spans out of sync");
        block_id
    }

    fn resolve_name(&mut self, name: StrId, span: TokenSpan) -> ExprKind {
        if let Some(ty) = TypeId::resolve_primitive(name) {
            return ExprKind::Value(Ok(self.values.intern_type(ty)));
        }

        if EvmBuiltin::from_str_id(name).is_some() {
            self.error_non_call_reference_to_builtin(name, span);
            return ExprKind::POISON;
        }

        if let Some(entry) = self.find_local(name) {
            return ExprKind::LocalRef(entry.id);
        }

        if let Some(capture_local) = self.lookup_capture(name, span) {
            return ExprKind::LocalRef(capture_local);
        }

        if let Some(entry) = self.consts.get(&name) {
            return ExprKind::ConstRef(entry.const_id);
        }

        self.error_unresolved_identifier(name, span);
        ExprKind::POISON
    }

    fn lower_expr(&mut self, expr: ast::Expr<'_>) -> Expr {
        let kind = match expr {
            ast::Expr::Block(block) => return self.lower_scope(block),
            ast::Expr::Error { .. } => ExprKind::Value(Err(Poisoned)),
            ast::Expr::Ident { name, span } => self.resolve_name(name, span),
            ast::Expr::BoolLiteral { value, .. } => ExprKind::Value(Ok(value.into())),
            ast::Expr::NumLiteral { id, span } => {
                let limbs = &self.num_lit_limbs[id];
                match plank_core::bigint::limbs_to_u256(limbs) {
                    Some(value) => ExprKind::Value(Ok(self.values.intern_num(value))),
                    None => {
                        self.error_number_out_of_range(span);
                        ExprKind::POISON
                    }
                }
            }
            ast::Expr::Member(member_expr) => {
                let object = self.lower_expr_to_local(member_expr.object());
                ExprKind::Member { object, member: member_expr.member }
            }
            ast::Expr::Call(call_expr) => {
                let callee = call_expr.callee();
                if let ast::Expr::Ident { name, span: _ } = callee
                    && let Some(builtin) = EvmBuiltin::from_str_id(name)
                {
                    let buf_start = self.locals_buf.len();
                    for arg in call_expr.args() {
                        let local = self.lower_expr_to_local(arg);
                        self.locals_buf.push(local);
                    }
                    let args = self.builder.call_args.push_iter(self.locals_buf.drain(buf_start..));
                    ExprKind::EvmBuiltinCall { builtin, args }
                } else {
                    let callee = self.lower_expr_to_local(callee);
                    let buf_start = self.locals_buf.len();
                    for arg in call_expr.args() {
                        let local = self.lower_expr_to_local(arg);
                        self.locals_buf.push(local);
                    }
                    let args = self.builder.call_args.push_iter(self.locals_buf.drain(buf_start..));
                    ExprKind::Call { callee, args }
                }
            }
            ast::Expr::StructLit(struct_lit) => {
                let ty = self.lower_expr_to_local(struct_lit.type_expr());
                let buf_start = self.field_buf.len();
                for result in struct_lit.fields() {
                    let Ok(field) = result else { continue };
                    let value = self.lower_expr_to_local(field.value());
                    let name_offset = self.lexed.tokens_src_span(field.name_span()).start;
                    self.field_buf.push(FieldInfo { name: field.name, name_offset, value });
                }
                let fields = self.builder.fields.push_iter(self.field_buf.drain(buf_start..));
                ExprKind::StructLit { ty, fields }
            }
            ast::Expr::StructDef(struct_def) => {
                let source_id = self.source_id;
                let span = struct_def.node().span();
                let source_span = self.lexed.tokens_src_span(span);
                let type_index = match struct_def.index_expr() {
                    Some(expr) => self.lower_expr_to_local(expr),
                    None => {
                        let local = self.alloc_temp();
                        let expr = self.expr(ExprKind::VOID, struct_def.node().span());
                        self.emit(InstructionKind::Set { local, r#type: None, expr });
                        local
                    }
                };
                let buf_start = self.field_buf.len();
                for result in struct_def.fields() {
                    let Ok(field) = result else { continue };
                    let value = self.lower_expr_to_local(field.type_expr());
                    let name_offset = self.lexed.tokens_src_span(field.name_span()).start;
                    self.field_buf.push(FieldInfo { name: field.name, name_offset, value });
                }
                let fields = self.builder.fields.push_iter(self.field_buf.drain(buf_start..));
                let struct_def_id = self.builder.struct_defs.push(StructDef {
                    source_id,
                    source_span,
                    type_index,
                    fields,
                });
                ExprKind::StructDef(struct_def_id)
            }
            ast::Expr::FnDef(fn_def) => ExprKind::FnDef(self.lower_fn_def(fn_def)),
            ast::Expr::If(if_expr) => {
                let result = self.alloc_temp();
                let condition = self.lower_expr_to_local(if_expr.condition());
                let then_block = self.lower_branch_body(if_expr.body(), result);
                let else_block = self.lower_else_chain(
                    result,
                    if_expr.else_if_branches(),
                    if_expr.else_body().ok_or_else(|| if_expr.body().node().span()),
                );
                self.emit(InstructionKind::If { condition, then_block, else_block });
                ExprKind::LocalRef(result)
            }
            ast::Expr::ComptimeBlock(block) => {
                let result = self.alloc_temp();
                let body = self.create_sub_block(block.node().span(), |this| {
                    for stmt in block.statements() {
                        this.lower_statement(stmt);
                    }
                    let expr = match block.end_expr() {
                        Some(e) => this.lower_expr(e),
                        None => {
                            let span = block.node().span();
                            this.expr(ExprKind::VOID, span)
                        }
                    };
                    this.emit(InstructionKind::Set { local: result, r#type: None, expr });
                });

                self.emit(InstructionKind::ComptimeBlock { body });
                ExprKind::LocalRef(result)
            }
            ast::Expr::Binary(binary) => 'binary: {
                let op = match binary.op {
                    // Logical (short-circuit, handled specially)
                    cst::BinaryOp::And => {
                        break 'binary self.lower_short_circuit_op(binary, ShortCircuitOp::And);
                    }
                    cst::BinaryOp::Or => {
                        break 'binary self.lower_short_circuit_op(binary, ShortCircuitOp::Or);
                    }
                    // Comparison
                    cst::BinaryOp::DoubleEquals => hir_ops::BinaryOp::NotEquals,
                    cst::BinaryOp::BangEquals => hir_ops::BinaryOp::Equals,
                    cst::BinaryOp::LessThan => hir_ops::BinaryOp::LessThan,
                    cst::BinaryOp::GreaterThan => hir_ops::BinaryOp::GreaterThan,
                    cst::BinaryOp::LessEquals => hir_ops::BinaryOp::LessEquals,
                    cst::BinaryOp::GreaterEquals => hir_ops::BinaryOp::GreaterEquals,
                    // Bitwise
                    cst::BinaryOp::Pipe => hir_ops::BinaryOp::BitwiseOr,
                    cst::BinaryOp::Caret => hir_ops::BinaryOp::BitwiseXor,
                    cst::BinaryOp::Ampersand => hir_ops::BinaryOp::BitwiseAnd,
                    cst::BinaryOp::ShiftLeft => hir_ops::BinaryOp::ShiftLeft,
                    cst::BinaryOp::ShiftRight => hir_ops::BinaryOp::ShiftRight,
                    // Arithmetic (additive)
                    cst::BinaryOp::Plus => hir_ops::BinaryOp::Add,
                    cst::BinaryOp::Minus => hir_ops::BinaryOp::Subtract,
                    cst::BinaryOp::PlusPercent => hir_ops::BinaryOp::AddWrap,
                    cst::BinaryOp::MinusPercent => hir_ops::BinaryOp::SubtractWrap,
                    // Arithmetic (multiplicative)
                    cst::BinaryOp::Star => hir_ops::BinaryOp::Mul,
                    cst::BinaryOp::Slash => {
                        self.emit_lone_slash_not_supported(binary.op_span());
                        hir_ops::BinaryOp::DivRoundToZero
                    }
                    cst::BinaryOp::Percent => hir_ops::BinaryOp::Mod,
                    cst::BinaryOp::StarPercent => hir_ops::BinaryOp::MulWrap,
                    cst::BinaryOp::PlusSlash => hir_ops::BinaryOp::DivRoundPos,
                    cst::BinaryOp::MinusSlash => hir_ops::BinaryOp::DivRoundNeg,
                    cst::BinaryOp::LessSlash => hir_ops::BinaryOp::DivRoundToZero,
                    cst::BinaryOp::GreaterSlash => hir_ops::BinaryOp::DivRoundAwayFromZero,
                };
                let lhs = self.lower_expr_to_local(binary.lhs());
                let rhs = self.lower_expr_to_local(binary.rhs());
                ExprKind::BinaryOpCall { op, lhs, rhs }
            }
            ast::Expr::Unary(unary) => {
                let input = self.lower_expr_to_local(unary.operand());
                match unary.op {
                    cst::UnaryOp::Bang => ExprKind::LogicalNot { input },
                    cst::UnaryOp::Minus => {
                        ExprKind::UnaryOpCall { op: hir_ops::UnaryOp::Negate, input }
                    }
                    cst::UnaryOp::Tilde => {
                        ExprKind::UnaryOpCall { op: hir_ops::UnaryOp::BitwiseNot, input }
                    }
                }
            }
        };
        self.expr(kind, expr.span())
    }

    fn add_param_to_scope_as_local(&mut self, param: ast::Param<'_>) -> LocalId {
        self.alloc_local(param.name, false, param.name_span())
    }

    fn lower_fn_def(&mut self, fn_def: ast::FnDef<'_>) -> FnDefId {
        let saved_next_local = std::mem::replace(&mut self.next_local_id, LocalId::ZERO);
        let saved_fn_scope_start =
            std::mem::replace(&mut self.fn_scope_start, self.scoped_locals_stack.len());
        let saved_captures_start =
            std::mem::replace(&mut self.fn_captures_start, self.captures_buf.len());

        let param_locals_start = self.locals_buf.len();
        let return_type;
        let type_preamble = {
            let preamble_block_start = self.instructions_buf.len();
            for (idx, param) in fn_def.params().filter_map(Result::ok).enumerate() {
                let param_type = self.lower_expr_to_local(param.type_expr());
                self.locals_buf.push(param_type);
                let param_value = self.add_param_to_scope_as_local(param);
                self.locals_buf.push(param_value);
                self.emit(InstructionKind::Param {
                    comptime: param.is_comptime,
                    arg: param_value,
                    r#type: param_type,
                    idx: idx as u32,
                });
            }
            return_type = self.lower_expr_to_local(fn_def.return_type());
            let preamble_span = self.lexed.tokens_src_span(fn_def.param_list_span());
            self.flush_instructions_from(preamble_block_start, preamble_span)
        };

        let body = self.lower_fn_body_block(fn_def.body());
        let param_list_span = self.lexed.tokens_src_span(fn_def.param_list_span());
        let fn_def_id = self.builder.fns.push(FnDef {
            type_preamble,
            body,
            return_type,
            source: self.source_id,
            param_list_span,
        });

        let (type_value_pairs, []) = self.locals_buf[param_locals_start..].as_chunks() else {
            unreachable!("not only pairs?")
        };
        let fn_params_id = self.builder.fn_params.push_iter(
            type_value_pairs.iter().zip(fn_def.params().flatten()).map(
                |(&[r#type, value], param)| {
                    let span = self.lexed.tokens_src_span(param.node().span());
                    ParamInfo { is_comptime: param.is_comptime, value, r#type, span }
                },
            ),
        );
        self.locals_buf.truncate(param_locals_start);
        let fn_captures_id =
            self.builder.fn_captures.push_iter(self.captures_buf.drain(self.fn_captures_start..));
        assert_eq!(fn_def_id, fn_params_id, "fn and fn_params out of sync");
        assert_eq!(fn_def_id, fn_captures_id, "fn and fn_captures out of sync");

        self.scoped_locals_stack.truncate(self.fn_scope_start);
        self.next_local_id = saved_next_local;
        self.fn_scope_start = saved_fn_scope_start;
        self.fn_captures_start = saved_captures_start;

        fn_def_id
    }

    fn scoped<R>(&mut self, f: impl FnOnce(&mut Self) -> R) -> R {
        let scope_start = self.scoped_locals_stack.len();
        let result = f(self);
        self.scoped_locals_stack.truncate(scope_start);
        result
    }

    fn lower_scope(&mut self, block: ast::BlockExpr<'_>) -> Expr {
        self.scoped(|this| {
            for stmt in block.statements() {
                this.lower_statement(stmt);
            }

            match block.end_expr() {
                Some(expr) => this.lower_expr(expr),
                None => this.expr(ExprKind::VOID, block.node().span()),
            }
        })
    }

    fn lower_else_chain<'cst>(
        &mut self,
        result: LocalId,
        mut branches: impl Iterator<Item = Result<ast::ElseIfBranch<'cst>, TokenSpan>>,
        else_body: Result<ast::BlockExpr<'cst>, TokenSpan>,
    ) -> BlockId {
        while let Some(next) = branches.next() {
            let Ok(first) = next else { continue };
            return self.create_sub_block(first.node().span(), |this| {
                let condition = this.lower_expr_to_local(first.condition());
                let then_block = this.lower_branch_body(first.body(), result);
                let else_body = else_body.map_err(|_| first.body().node().span());
                let else_block = this.lower_else_chain(result, branches, else_body);
                this.emit(InstructionKind::If { condition, then_block, else_block });
            });
        }
        match else_body {
            Ok(body) => self.lower_branch_body(body, result),
            Err(empty_else_span) => self.create_sub_block(empty_else_span, |this| {
                let expr = this.expr(ExprKind::VOID, empty_else_span);
                this.emit(InstructionKind::BranchSet { local: result, expr });
            }),
        }
    }

    /// Desugars short-circuit boolean operators.
    /// Lowers OR to: `if <lhs> { true } else { <rhs> }`
    /// Lowers AND to: `if <lhs> { <rhs> } else { false }`
    fn lower_short_circuit_op(
        &mut self,
        binary: ast::BinaryExpr<'_>,
        op: ShortCircuitOp,
    ) -> ExprKind {
        let op_result_local = self.alloc_temp();
        let op_lhs_as_condition = self.lower_expr_to_local(binary.lhs());

        // Creates `{ <rhs> }` block.
        let rhs_span = binary.rhs().span();
        let eval_op_rhs_block = self.create_sub_block(rhs_span, |this| {
            let expr = this.lower_expr(binary.rhs());
            this.emit(InstructionKind::BranchSet { local: op_result_local, expr });
        });

        // Creates `{ false }` / `{ true }` block.
        let span = binary.node().span();
        let short_circuit_block = self.create_sub_block(span, |this| {
            let short_circuit_value = match op {
                ShortCircuitOp::And => false,
                ShortCircuitOp::Or => true,
            };
            let expr = this.expr(ExprKind::Value(Ok(short_circuit_value.into())), span);
            this.emit(InstructionKind::BranchSet { local: op_result_local, expr });
        });

        let (then_block, else_block) = match op {
            ShortCircuitOp::Or => (short_circuit_block, eval_op_rhs_block),
            ShortCircuitOp::And => (eval_op_rhs_block, short_circuit_block),
        };
        let r#if = InstructionKind::If { condition: op_lhs_as_condition, then_block, else_block };
        self.emit(r#if);
        ExprKind::LocalRef(op_result_local)
    }

    fn lower_statement(&mut self, stmt: Statement<'_>) {
        match stmt {
            Statement::Let(let_stmt) => {
                let expr = self.lower_expr(let_stmt.value());
                // Local allocated *after* to ensure it's not visible to `lower_expr`.
                let local = self.alloc_local(let_stmt.name, let_stmt.mutable, let_stmt.name_span);
                let r#type =
                    let_stmt.type_expr().map(|type_expr| self.lower_expr_to_local(type_expr));
                self.emit(if let_stmt.mutable {
                    InstructionKind::SetMut { local, r#type, expr }
                } else {
                    InstructionKind::Set { local, r#type, expr }
                });
            }
            Statement::Expr(expr) => {
                let value = self.lower_expr(expr);
                self.emit(InstructionKind::Eval(value));
            }
            Statement::Return(return_stmt) => {
                let value = self.lower_expr(return_stmt.value());
                self.emit(InstructionKind::Return(value));
            }
            Statement::Assign(assign_stmt) => {
                let ast::Expr::Ident { name, span } = assign_stmt.target() else {
                    panic!("complex assignment targets not yet supported")
                };
                let Some(entry) = self.find_local(name) else {
                    self.error_unresolved_identifier(name, span);
                    return;
                };
                if !entry.mutable {
                    self.error_assignment_to_immutable(
                        name,
                        span,
                        entry.span.expect("named locals always have a span"),
                    );
                    return;
                }
                let target = entry.id;
                let value = self.lower_expr(assign_stmt.value());
                self.emit(InstructionKind::Assign { target, expr: value });
            }
            Statement::While(while_stmt) => {
                let span = while_stmt.node().span();
                if while_stmt.inline {
                    self.error_not_yet_implemented("inline while", span);
                    return;
                }
                let (condition_block, condition) = self
                    .create_sub_block_with(while_stmt.condition().span(), |this| {
                        this.lower_expr_to_local(while_stmt.condition())
                    });
                let body = self.lower_body_to_block(while_stmt.body());
                self.emit(InstructionKind::While { condition_block, condition, body });
            }
            Statement::Error { .. } => {}
        }
    }
}

pub fn lower(project: &ParsedProject, values: &mut ValueInterner, session: &mut Session) -> Hir {
    let (mut consts, source_consts) = register_consts(&project.parsed_sources, session);

    let mut builder = HirBuilder::new();
    let mut init = None;
    let mut run = None;

    let mut lowerer = BlockLowerer {
        consts: HashMap::new(),
        num_lit_limbs: &project.parsed_sources[SourceId::ROOT].cst.num_lit_limbs,
        session: RefCell::new(session),

        values,
        builder: &mut builder,
        scoped_locals_stack: Vec::new(),
        fn_scope_start: 0,
        fn_captures_start: 0,
        next_local_id: LocalId::ZERO,

        instructions_buf: Vec::new(),
        locals_buf: Vec::new(),
        field_buf: Vec::new(),
        captures_buf: Vec::new(),

        lexed: &project.parsed_sources[SourceId::ROOT].lexed,
        source_id: SourceId::ROOT,
    };

    for (source_id, source) in project.parsed_sources.enumerate_idx() {
        lowerer.num_lit_limbs = &source.cst.num_lit_limbs;
        lowerer.source_id = source_id;
        lowerer.lexed = &source.lexed;
        lowerer.build_file_scope(&source_consts, &project.imports, &consts);

        let file = source.cst.as_file();
        for def in file.iter_defs() {
            lowerer.reset_scope();
            match def {
                TopLevelDef::Const(const_def) => {
                    let id = lowerer.consts[&const_def.name].const_id;
                    let hir_def = &mut consts[id];
                    hir_def.result = lowerer.alloc_temp();
                    hir_def.body = lowerer.create_sub_block(const_def.span(), |this| {
                        let r#type =
                            const_def.r#type.map(|type_expr| this.lower_expr_to_local(type_expr));
                        let expr = this.lower_expr(const_def.assign);
                        this.emit(InstructionKind::Set { local: hir_def.result, r#type, expr });
                    });
                }
                TopLevelDef::Init(init_def) => {
                    let span = init_def.node().span();
                    if source_id != SourceId::ROOT {
                        lowerer.error_init_outside_entry(span);
                    } else if let Some((_, prev_span)) = init {
                        lowerer.error_multiple_init_blocks(span, prev_span);
                    } else {
                        init = Some((lowerer.lower_body_to_block(init_def.body()), span));
                    }
                }
                TopLevelDef::Run(run_def) => {
                    let span = run_def.node().span();
                    if source_id != SourceId::ROOT {
                        lowerer.error_run_outside_entry(span);
                    } else if let Some((_, prev_span)) = run {
                        lowerer.error_multiple_run_blocks(span, prev_span);
                    } else {
                        run = Some((lowerer.lower_body_to_block(run_def.body()), span));
                    }
                }
                TopLevelDef::Import(_) => {}
                TopLevelDef::Error { .. } => {}
            }
        }
    }

    let init = match init {
        Some((id, _)) => id,
        None => {
            lowerer.error_missing_init_block();
            let block_id = builder.block_instrs.push_iter(std::iter::empty());
            builder.block_spans.push(Err(Poisoned));
            block_id
        }
    };

    Hir {
        entry_source: SourceId::ROOT,
        init,
        run: run.map(|(id, _)| id),

        block_instrs: builder.block_instrs,
        block_spans: builder.block_spans,
        consts,

        call_args: builder.call_args,
        fields: builder.fields,
        struct_defs: builder.struct_defs,

        fns: builder.fns,
        fn_params: builder.fn_params,
        fn_captures: builder.fn_captures,
    }
}

fn register_consts(
    sources: &IndexVec<SourceId, plank_source::project::ParsedSource>,
    session: &mut Session,
) -> (IndexVec<ConstId, ConstDef>, ListOfLists<SourceId, (StrId, ConstId)>) {
    let mut consts: IndexVec<ConstId, ConstDef> = IndexVec::new();
    let mut source_consts: ListOfLists<SourceId, (StrId, ConstId)> = ListOfLists::new();

    let mut seen = HashMap::new();
    for (id, source) in sources.enumerate_idx() {
        let file = source.cst.as_file();
        seen.clear();
        source_consts.push_with(|mut list| {
            for def in file.iter_defs() {
                let TopLevelDef::Const(const_def) = def else { continue };
                let source_span = source.lexed.tokens_src_span(const_def.span());
                let const_id = consts.push(ConstDef {
                    name: const_def.name,
                    source_id: id,
                    source_span,
                    body: BlockId::ZERO,
                    result: LocalId::ZERO,
                });
                if let Some(prev) = seen.insert(const_def.name, const_id) {
                    diagnostics::error_duplicate_const(
                        session,
                        id,
                        const_def.name,
                        source_span,
                        &consts[prev],
                    );
                } else {
                    list.push((const_def.name, const_id));
                }
            }
        });
    }

    (consts, source_consts)
}
