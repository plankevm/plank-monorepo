use std::cell::RefCell;

use hashbrown::HashMap;
use plank_core::{
    Idx, IncIterable, IndexVec, SourceId, SourceSpan, Span, list_of_lists::ListOfLists,
};
use plank_diagnostics::DiagnosticsContext;
use plank_parser::{
    PlankInterner, StrId,
    ast::{self, Statement, TopLevelDef},
    cst::NumLitId,
    lexer::{Lexed, TokenIdx},
};
use plank_source::{
    Source,
    project::{FileImport, ImportKind},
};
use plank_values::{BigNumInterner, TypeId};

mod diagnostics;

use plank_source::ParsedProject;

use crate::{
    BlockId, CallArgsId, CaptureInfo, ConstDef, ConstId, Expr, FieldInfo, FieldsId, FnDef, FnDefId,
    Hir, Instruction, LocalId, ParamInfo, StructDef, StructDefId, builtins::Builtin,
};

#[derive(Clone, Copy)]
struct ScopedLocal {
    name: StrId,
    id: LocalId,
    mutable: bool,
    span: Option<Span<TokenIdx>>,
}

struct HirBuilder {
    blocks: ListOfLists<BlockId, Instruction>,

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
            blocks: ListOfLists::new(),
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

struct BlockLowerer<'a, D: DiagnosticsContext> {
    consts: HashMap<StrId, ScopedConst>,
    num_lit_limbs: &'a ListOfLists<NumLitId, u32>,
    diag_ctx: RefCell<&'a mut D>,

    big_nums: &'a mut BigNumInterner,
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
    interner: &'a PlankInterner,
}

impl<'a, D> BlockLowerer<'a, D>
where
    D: DiagnosticsContext,
{
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
                        span: self.lexed.tokens_src_span(name_span),
                        imported: true,
                    };
                    let Some(prev) = self.consts.insert(imported_as, entry) else { continue };
                    self.error_import_collision(
                        imported_as,
                        name_span,
                        prev.source_id,
                        prev.span,
                        prev.imported,
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
                        self.error_import_collision(
                            name,
                            import.span,
                            prev.source_id,
                            prev.span,
                            prev.imported,
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

    fn alloc_local(&mut self, name: StrId, mutable: bool, span: Span<TokenIdx>) -> LocalId {
        if TypeId::resolve_primitive(name).is_some() {
            self.error_shadowing_primitive_type(name, span);
        } else if Builtin::from_str_id(name).is_some() {
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

    fn lower_expr_to_local(&mut self, expr: ast::Expr<'_>) -> LocalId {
        let value = self.lower_expr(expr);
        let local = self.alloc_temp();
        self.emit(Instruction::Set { local, expr: value });
        local
    }

    fn create_sub_block(&mut self, f: impl FnOnce(&mut Self)) -> BlockId {
        self.create_sub_block_with(f).0
    }

    fn create_sub_block_with<R>(&mut self, f: impl FnOnce(&mut Self) -> R) -> (BlockId, R) {
        let locals_start = self.scoped_locals_stack.len();
        let block_start = self.instructions_buf.len();
        let result = f(self);
        self.scoped_locals_stack.truncate(locals_start);
        (self.flush_instructions_from(block_start), result)
    }

    fn lower_body_to_block(&mut self, block: ast::BlockExpr<'_>) -> BlockId {
        self.create_sub_block(|lowerer| {
            for stmt in block.statements() {
                lowerer.lower_statement(stmt);
            }
            if let Some(e) = block.end_expr() {
                let value = lowerer.lower_expr(e);
                lowerer.emit(Instruction::Eval(value));
            }
        })
    }

    fn lower_body_to_block_with_result(
        &mut self,
        block: ast::BlockExpr<'_>,
        result: LocalId,
    ) -> BlockId {
        self.create_sub_block(|lowerer| {
            for stmt in block.statements() {
                lowerer.lower_statement(stmt);
            }
            let value = block.end_expr().map(|e| lowerer.lower_expr(e)).unwrap_or(Expr::Void);
            lowerer.emit(Instruction::Set { local: result, expr: value });
        })
    }

    fn lower_fn_body_block(&mut self, block: ast::BlockExpr<'_>) -> BlockId {
        self.create_sub_block(|lowerer| {
            for stmt in block.statements() {
                lowerer.lower_statement(stmt);
            }
            let value = block.end_expr().map(|e| lowerer.lower_expr(e)).unwrap_or(Expr::Void);
            lowerer.emit(Instruction::Return(value));
        })
    }

    fn find_in_scope(scope: &[ScopedLocal], name: StrId) -> Option<ScopedLocal> {
        scope.iter().rev().find(|entry| entry.name == name).copied()
    }

    fn find_local(&self, name: StrId) -> Option<ScopedLocal> {
        Self::find_in_scope(&self.scoped_locals_stack[self.fn_scope_start..], name)
    }

    fn lookup_capture(&mut self, name: StrId) -> Option<LocalId> {
        let outer_local =
            Self::find_in_scope(&self.scoped_locals_stack[..self.fn_scope_start], name)?.id;

        for capture in &self.captures_buf[self.fn_captures_start..] {
            if capture.outer_local == outer_local {
                return Some(capture.inner_local);
            }
        }

        let inner_local = self.alloc_anonymous_local(name);
        self.captures_buf.push(CaptureInfo { outer_local, inner_local });
        Some(inner_local)
    }

    fn emit(&mut self, instr: Instruction) {
        self.instructions_buf.push(instr);
    }

    fn flush_instructions_from(&mut self, start: usize) -> BlockId {
        self.builder.blocks.push_iter(self.instructions_buf.drain(start..))
    }

    fn resolve_name(&mut self, name: StrId, span: Span<TokenIdx>) -> Expr {
        if let Some(ty) = TypeId::resolve_primitive(name) {
            return Expr::Type(ty);
        }

        if Builtin::from_str_id(name).is_some() {
            self.error_non_call_reference_to_builtin(name, span);
            return Expr::Error;
        }

        if let Some(entry) = self.find_local(name) {
            return Expr::LocalRef(entry.id);
        }

        if let Some(capture_local) = self.lookup_capture(name) {
            return Expr::LocalRef(capture_local);
        }

        if let Some(entry) = self.consts.get(&name) {
            return Expr::ConstRef(entry.const_id);
        }

        self.error_unresolved_identifier(name, span);
        Expr::Error
    }

    fn lower_expr(&mut self, expr: ast::Expr<'_>) -> Expr {
        match expr {
            ast::Expr::Ident { name, span } => self.resolve_name(name, span),
            ast::Expr::Block(block) => self.lower_scope(block),
            ast::Expr::BoolLiteral(b) => Expr::Bool(b),
            ast::Expr::NumLiteral { negative, id, span } => {
                let limbs = &self.num_lit_limbs[id];
                match plank_core::bigint::limbs_to_u256(limbs, negative) {
                    Some(value) => {
                        let big_num_id = self.big_nums.intern(value);
                        Expr::BigNum(big_num_id)
                    }
                    None => {
                        self.error_number_out_of_range(span);
                        Expr::Error
                    }
                }
            }
            ast::Expr::Member(member_expr) => {
                let object = self.lower_expr_to_local(member_expr.object());
                Expr::Member { object, member: member_expr.member }
            }
            ast::Expr::Call(call_expr) => {
                let callee = call_expr.callee();
                if let ast::Expr::Ident { name, span: _ } = callee
                    && let Some(builtin) = Builtin::from_str_id(name)
                {
                    let buf_start = self.locals_buf.len();
                    for arg in call_expr.args() {
                        let local = self.lower_expr_to_local(arg);
                        self.locals_buf.push(local);
                    }
                    let args = self.builder.call_args.push_iter(self.locals_buf.drain(buf_start..));
                    Expr::BuiltinCall { builtin, args }
                } else {
                    let callee = self.lower_expr_to_local(callee);
                    let buf_start = self.locals_buf.len();
                    for arg in call_expr.args() {
                        let local = self.lower_expr_to_local(arg);
                        self.locals_buf.push(local);
                    }
                    let args = self.builder.call_args.push_iter(self.locals_buf.drain(buf_start..));
                    Expr::Call { callee, args }
                }
            }
            ast::Expr::StructLit(struct_lit) => {
                let ty = self.lower_expr_to_local(struct_lit.type_expr());
                let buf_start = self.field_buf.len();
                for field in struct_lit.fields() {
                    let value = self.lower_expr_to_local(field.value());
                    self.field_buf.push(FieldInfo { name: field.name, value });
                }
                let fields = self.builder.fields.push_iter(self.field_buf.drain(buf_start..));
                Expr::StructLit { ty, fields }
            }
            ast::Expr::StructDef(struct_def) => {
                let source = struct_def.node().idx();
                let type_index = struct_def
                    .index_expr()
                    .map(|expr| self.lower_expr_to_local(expr))
                    .unwrap_or_else(|| {
                        let local = self.alloc_temp();
                        self.emit(Instruction::Set { local, expr: Expr::Void });
                        local
                    });
                let buf_start = self.field_buf.len();
                for field in struct_def.fields() {
                    let value = self.lower_expr_to_local(field.type_expr());
                    self.field_buf.push(FieldInfo { name: field.name, value });
                }
                let fields = self.builder.fields.push_iter(self.field_buf.drain(buf_start..));
                let struct_def_id =
                    self.builder.struct_defs.push(StructDef { source, type_index, fields });
                Expr::StructDef(struct_def_id)
            }
            ast::Expr::FnDef(fn_def) => Expr::FnDef(self.lower_fn_def(fn_def)),
            ast::Expr::If(if_expr) => {
                let result = self.alloc_temp();
                let condition = self.lower_expr_to_local(if_expr.condition());
                let then_block = self.lower_body_to_block_with_result(if_expr.body(), result);
                let else_block =
                    self.lower_else_chain(result, if_expr.else_if_branches(), if_expr.else_body());
                self.emit(Instruction::If { condition, then_block, else_block });
                Expr::LocalRef(result)
            }
            ast::Expr::ComptimeBlock(_) => {
                todo!("comptime block lowering requires extra HIR instructions")
            }
            ast::Expr::Binary(binary) => {
                panic!("binary expression lowering not yet implemented (op: {:?})", binary.op)
            }
            ast::Expr::Unary(unary) => {
                panic!("unary expression lowering not yet implemented (op: {:?})", unary.op)
            }
        }
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
            for param in fn_def.params() {
                let param_type = self.lower_expr_to_local(param.type_expr());
                self.locals_buf.push(param_type);
                let param_value = self.add_param_to_scope_as_local(param);
                self.locals_buf.push(param_value);
            }
            return_type = self.lower_expr_to_local(fn_def.return_type());
            self.flush_instructions_from(preamble_block_start)
        };

        let body = self.lower_fn_body_block(fn_def.body());
        let fn_def_id = self.builder.fns.push(FnDef { type_preamble, body, return_type });

        let (type_value_pairs, []) = self.locals_buf[param_locals_start..].as_chunks() else {
            unreachable!("not only pairs?")
        };
        let fn_params_id = self.builder.fn_params.push_iter(
            type_value_pairs.iter().zip(fn_def.params()).map(|(&[r#type, value], param)| {
                ParamInfo { is_comptime: param.is_comptime, value, r#type }
            }),
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
        self.scoped(|lowerer| {
            for stmt in block.statements() {
                lowerer.lower_statement(stmt);
            }

            match block.end_expr() {
                Some(expr) => lowerer.lower_expr(expr),
                None => Expr::Void,
            }
        })
    }

    fn lower_else_chain<'cst>(
        &mut self,
        result: LocalId,
        mut branches: impl Iterator<Item = ast::ElseIfBranch<'cst>>,
        else_body: Option<ast::BlockExpr<'cst>>,
    ) -> BlockId {
        if let Some(first) = branches.next() {
            self.create_sub_block(|lowerer| {
                let condition = lowerer.lower_expr_to_local(first.condition());
                let then_block = lowerer.lower_body_to_block_with_result(first.body(), result);
                let else_block = lowerer.lower_else_chain(result, branches, else_body);
                lowerer.emit(Instruction::If { condition, then_block, else_block });
            })
        } else if let Some(body) = else_body {
            self.lower_body_to_block_with_result(body, result)
        } else {
            self.create_sub_block(|lowerer| {
                lowerer.emit(Instruction::Set { local: result, expr: Expr::Void });
            })
        }
    }

    fn lower_statement(&mut self, stmt: Statement<'_>) {
        match stmt {
            Statement::Let(let_stmt) => {
                let type_local = let_stmt.type_expr().map(|t| self.lower_expr_to_local(t));
                let value = self.lower_expr(let_stmt.value());
                let local_id =
                    self.alloc_local(let_stmt.name, let_stmt.mutable, let_stmt.name_span);
                self.emit(Instruction::Set { local: local_id, expr: value });
                if let Some(type_local) = type_local {
                    self.emit(Instruction::AssertType { value: local_id, of_type: type_local });
                }
            }
            Statement::Expr(expr) => {
                let value = self.lower_expr(expr);
                self.emit(Instruction::Eval(value));
            }
            Statement::Return(return_stmt) => {
                let value = self.lower_expr(return_stmt.value());
                self.emit(Instruction::Return(value));
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
                self.emit(Instruction::Assign { target, value });
            }
            Statement::While(while_stmt) => {
                if while_stmt.inline {
                    self.error_not_yet_implemented("inline while", while_stmt.node().span());
                    return;
                }
                let (condition_block, condition) = self.create_sub_block_with(|lowerer| {
                    lowerer.lower_expr_to_local(while_stmt.condition())
                });
                let body = self.lower_body_to_block(while_stmt.body());
                self.emit(Instruction::While { condition_block, condition, body });
            }
        }
    }
}

pub fn lower(
    project: &ParsedProject,
    big_nums: &mut BigNumInterner,
    interner: &PlankInterner,
    diag_ctx: &mut impl DiagnosticsContext,
) -> Hir {
    let (mut consts, source_consts) = register_consts(&project.sources, interner, diag_ctx);

    let mut builder = HirBuilder::new();
    let mut init = None;
    let mut run = None;

    let mut lowerer = BlockLowerer {
        consts: HashMap::new(),
        num_lit_limbs: &project.sources[SourceId::ROOT].cst.num_lit_limbs,
        diag_ctx: RefCell::new(diag_ctx),

        big_nums,
        builder: &mut builder,
        scoped_locals_stack: Vec::new(),
        fn_scope_start: 0,
        fn_captures_start: 0,
        next_local_id: LocalId::ZERO,

        instructions_buf: Vec::new(),
        locals_buf: Vec::new(),
        field_buf: Vec::new(),
        captures_buf: Vec::new(),

        lexed: &project.sources[SourceId::ROOT].lexed,
        source_id: SourceId::ROOT,
        interner,
    };

    for (source_id, source) in project.sources.enumerate_idx() {
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
                    hir_def.result =
                        lowerer.alloc_local(const_def.name, false, const_def.name_span());
                    hir_def.body = lowerer.create_sub_block(|l| {
                        if let Some(type_expr) = const_def.r#type {
                            let type_local = l.lower_expr_to_local(type_expr);
                            let assign = l.lower_expr(const_def.assign);
                            l.emit(Instruction::Set { local: hir_def.result, expr: assign });
                            l.emit(Instruction::AssertType {
                                value: hir_def.result,
                                of_type: type_local,
                            });
                        } else {
                            let assign = l.lower_expr(const_def.assign);
                            l.emit(Instruction::Set { local: hir_def.result, expr: assign });
                        }
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
                // already handled in build_file_scope
                TopLevelDef::Import(_) => {}
            }
        }
    }

    let init = match init {
        Some((id, _)) => id,
        None => {
            lowerer.error_missing_init_block();
            builder.blocks.push_iter(std::iter::empty())
        }
    };

    Hir {
        blocks: builder.blocks,
        call_args: builder.call_args,
        fields: builder.fields,
        consts,
        fns: builder.fns,
        fn_params: builder.fn_params,
        fn_captures: builder.fn_captures,
        struct_defs: builder.struct_defs,
        init,
        run: run.map(|(id, _)| id),
    }
}

fn register_consts(
    sources: &IndexVec<SourceId, Source>,
    interner: &PlankInterner,
    diag_ctx: &mut impl DiagnosticsContext,
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
                        &interner[const_def.name],
                        id,
                        source_span,
                        &consts[prev],
                        diag_ctx,
                    );
                } else {
                    list.push((const_def.name, const_id));
                }
            }
        });
    }

    (consts, source_consts)
}
