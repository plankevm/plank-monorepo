use hashbrown::HashMap;
use sensei_core::{Idx, IncIterable, IndexVec, Span, list_of_lists::ListOfLists, newtype_index};
use sensei_parser::{
    SourceId, StrId,
    ast::{self, Statement, TopLevelDef},
    cst::{NodeIdx, NumLitId},
    lexer::TokenIdx,
    project::{FileImport, ImportKind, ParsedProject, Source},
};

pub use sensei_values;
use sensei_values::TypeId;

pub mod builtins;
pub mod display;

pub use sensei_values::{BigNumId, BigNumInterner};

use crate::builtins::Builtin;

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
pub enum Expr {
    // References
    ConstRef(ConstId),
    LocalRef(LocalId),
    FnDef(FnDefId),
    // Literals
    Bool(bool),
    Void,
    BigNum(BigNumId),
    Type(TypeId),
    // Compound expressions
    Call { callee: LocalId, args: CallArgsId },
    BuiltinCall { builtin: Builtin, args: CallArgsId },
    Member { object: LocalId, member: StrId },
    StructLit { ty: LocalId, fields: FieldsId },
    StructDef(StructDefId),
}

#[derive(Debug, Clone, Copy)]
pub enum Instruction {
    /// Define local, can be present on multiple branches. Indicates type unification is
    /// allowed.
    Set {
        local: LocalId,
        expr: Expr,
    },
    /// Mutate local. Must adhere to existing type.
    Assign {
        target: LocalId,
        value: Expr,
    },
    AssertType {
        value: LocalId,
        of_type: LocalId,
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
}

#[derive(Debug, Clone, Copy)]
pub struct ParamInfo {
    pub is_comptime: bool,
    pub value: LocalId,
    pub r#type: LocalId,
}

#[derive(Debug, Clone, Copy)]
pub struct CaptureInfo {
    pub outer_local: LocalId,
    pub inner_local: LocalId,
}

#[derive(Debug, Clone, Copy)]
pub struct FieldInfo {
    pub name: StrId,
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
}

#[derive(Debug, Clone, Copy)]
pub struct StructDef {
    pub source: NodeIdx,
    pub type_index: LocalId,
    pub fields: FieldsId,
}

#[derive(Debug, Clone, Copy)]
pub struct ConstDef {
    pub name: StrId,
    pub source: Span<TokenIdx>,
    pub body: BlockId,
    pub result: LocalId,
}

#[derive(Debug, Clone)]
pub struct Hir {
    // Entry points
    pub init: BlockId,
    pub run: Option<BlockId>,

    pub blocks: ListOfLists<BlockId, Instruction>,
    pub consts: IndexVec<ConstId, ConstDef>,

    pub call_args: ListOfLists<CallArgsId, LocalId>,
    pub fields: ListOfLists<FieldsId, FieldInfo>,
    pub struct_defs: IndexVec<StructDefId, StructDef>,

    // Function definition data
    pub fns: IndexVec<FnDefId, FnDef>,
    pub fn_params: ListOfLists<FnDefId, ParamInfo>,
    pub fn_captures: ListOfLists<FnDefId, CaptureInfo>,
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

struct BlockLowerer<'a> {
    consts: HashMap<StrId, ConstId>,
    num_lit_limbs: &'a ListOfLists<NumLitId, u32>,

    big_nums: &'a mut BigNumInterner,
    builder: &'a mut HirBuilder,
    scoped_locals_stack: Vec<(StrId, LocalId)>,
    fn_scope_start: usize,
    fn_captures_start: usize,
    next_local_id: LocalId,

    instructions_buf: Vec<Instruction>,
    locals_buf: Vec<LocalId>,
    field_buf: Vec<FieldInfo>,
    captures_buf: Vec<CaptureInfo>,
}

impl<'a> BlockLowerer<'a> {
    fn reset(&mut self) {
        self.next_local_id = LocalId::ZERO;
        self.scoped_locals_stack.clear();

        debug_assert_eq!(self.fn_scope_start, 0);
        debug_assert_eq!(self.fn_captures_start, 0);

        debug_assert!(self.instructions_buf.is_empty());
        debug_assert!(self.locals_buf.is_empty());
        debug_assert!(self.field_buf.is_empty());
        debug_assert!(self.captures_buf.is_empty());
    }

    fn alloc_local(&mut self, name: StrId) -> LocalId {
        if TypeId::resolve_primitive(name).is_some() {
            todo!("diagnostic: shadowing primitive type");
        }

        if Builtin::from_str_id(name).is_some() {
            todo!("diagnostic: shadowing builtin");
        }

        let id = self.next_local_id.get_and_inc();
        self.scoped_locals_stack.push((name, id));
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

    fn lookup_scope(scope: &[(StrId, LocalId)], name: StrId) -> Option<LocalId> {
        for &(bound_name, id) in scope.iter().rev() {
            if bound_name == name {
                return Some(id);
            }
        }
        None
    }

    fn lookup_local(&self, name: StrId) -> Option<LocalId> {
        Self::lookup_scope(&self.scoped_locals_stack[self.fn_scope_start..], name)
    }

    fn lookup_capture(&mut self, name: StrId) -> Option<LocalId> {
        let outer_local =
            Self::lookup_scope(&self.scoped_locals_stack[..self.fn_scope_start], name)?;

        for capture in &self.captures_buf[self.fn_captures_start..] {
            if capture.outer_local == outer_local {
                return Some(capture.inner_local);
            }
        }

        let inner_local = self.alloc_local(name);
        self.captures_buf.push(CaptureInfo { outer_local, inner_local });
        Some(inner_local)
    }

    fn emit(&mut self, instr: Instruction) {
        self.instructions_buf.push(instr);
    }

    fn flush_instructions_from(&mut self, start: usize) -> BlockId {
        self.builder.blocks.push_iter(self.instructions_buf.drain(start..))
    }

    fn resolve_name(&mut self, name: StrId) -> Expr {
        if let Some(ty) = TypeId::resolve_primitive(name) {
            return Expr::Type(ty);
        }

        if Builtin::from_str_id(name).is_some() {
            todo!("diagnostic: non-call reference to builtin");
        }

        if let Some(local_id) = self.lookup_local(name) {
            return Expr::LocalRef(local_id);
        }

        if let Some(capture_local) = self.lookup_capture(name) {
            return Expr::LocalRef(capture_local);
        }

        if let Some(&const_id) = self.consts.get(&name) {
            return Expr::ConstRef(const_id);
        }

        todo!("diagnostic: unresolved identifier");
    }

    fn lower_expr(&mut self, expr: ast::Expr<'_>) -> Expr {
        match expr {
            ast::Expr::Ident(name) => self.resolve_name(name),
            ast::Expr::Block(block) => self.lower_scope(block),
            ast::Expr::BoolLiteral(b) => Expr::Bool(b),
            ast::Expr::NumLiteral { negative, id } => {
                let limbs = &self.num_lit_limbs[id];
                let value = sensei_core::bigint::limbs_to_u256(limbs, negative)
                    .expect("number literal out of range");
                let big_num_id = self.big_nums.intern(value);
                Expr::BigNum(big_num_id)
            }
            ast::Expr::Member(member_expr) => {
                let object = self.lower_expr_to_local(member_expr.object());
                Expr::Member { object, member: member_expr.member }
            }
            ast::Expr::Call(call_expr) => {
                let callee = call_expr.callee();
                if let ast::Expr::Ident(name) = callee
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
        self.alloc_local(param.name)
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

        let fn_params_id = self.builder.fn_params.push_iter(
            self.locals_buf[param_locals_start..].chunks(2).zip(fn_def.params()).map(
                |(type_value_chunk, param)| {
                    let &[r#type, value] = type_value_chunk else { unreachable!() };
                    ParamInfo { is_comptime: param.is_comptime, value, r#type }
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
                let local_id = self.alloc_local(let_stmt.name);
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
                let ast::Expr::Ident(name) = assign_stmt.target() else {
                    panic!("complex assignment targets not yet supported")
                };
                let target = self.lookup_local(name).expect("unresolved assignment target");
                let value = self.lower_expr(assign_stmt.value());
                self.emit(Instruction::Assign { target, value });
            }
            Statement::While(while_stmt) => {
                if while_stmt.inline {
                    panic!("inline while not yet supported");
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

pub fn lower(project: &ParsedProject, big_nums: &mut BigNumInterner) -> Hir {
    let (mut consts, source_consts) = register_consts(&project.sources);

    let mut builder = HirBuilder::new();
    let mut init = None;
    let mut run = None;

    let mut lowerer = BlockLowerer {
        consts: HashMap::new(),
        num_lit_limbs: &project.sources[SourceId::ROOT].cst.num_lit_limbs,

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
    };

    for (source_id, source) in project.sources.enumerate_idx() {
        build_file_scope(source_id, &source_consts, &project.imports, &mut lowerer.consts);
        lowerer.num_lit_limbs = &source.cst.num_lit_limbs;

        let file = source.cst.as_file();
        for def in file.iter_defs() {
            lowerer.reset();
            match def {
                TopLevelDef::Const(const_def) => {
                    let id = lowerer.consts[&const_def.name];
                    let hir_def = &mut consts[id];
                    hir_def.result = lowerer.alloc_local(const_def.name);
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
                    if source_id != SourceId::ROOT {
                        todo!("diagnostic: init only allowed in entry file");
                    }
                    if init.is_some() {
                        todo!("diagnostic: multiple init blocks");
                    }
                    init = Some(lowerer.lower_body_to_block(init_def.body()));
                }
                TopLevelDef::Run(run_def) => {
                    if source_id != SourceId::ROOT {
                        todo!("diagnostic: run only allowed in entry file");
                    }
                    if run.is_some() {
                        todo!("diagnostic: multiple run blocks");
                    }
                    run = Some(lowerer.lower_body_to_block(run_def.body()));
                }
                TopLevelDef::Import(_) => {}
            }
        }
    }

    // TODO: Diagnostic for missing init block
    let init = init.expect("missing init block");

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
        run,
    }
}

fn register_consts(
    sources: &IndexVec<SourceId, Source>,
) -> (IndexVec<ConstId, ConstDef>, ListOfLists<SourceId, (StrId, ConstId)>) {
    let mut consts: IndexVec<ConstId, ConstDef> = IndexVec::new();
    let mut source_consts: ListOfLists<SourceId, (StrId, ConstId)> = ListOfLists::new();

    let mut seen = HashMap::new();
    for source in sources.iter() {
        let file = source.cst.as_file();
        seen.clear();
        source_consts.push_with(|mut list| {
            for def in file.iter_defs() {
                let TopLevelDef::Const(const_def) = def else { continue };
                let const_id = consts.push(ConstDef {
                    name: const_def.name,
                    source: const_def.span(),
                    body: BlockId::ZERO,
                    result: LocalId::ZERO,
                });
                if seen.insert(const_def.name, const_id).is_some() {
                    todo!("diagnostic: duplicate const def");
                }
                list.push((const_def.name, const_id));
            }
        });
    }

    (consts, source_consts)
}

fn build_file_scope(
    source_id: SourceId,
    source_consts: &ListOfLists<SourceId, (StrId, ConstId)>,
    imports: &ListOfLists<SourceId, FileImport>,
    scope: &mut HashMap<StrId, ConstId>,
) {
    scope.clear();
    for &(name, const_id) in &source_consts[source_id] {
        scope.insert(name, const_id);
    }
    for import in &imports[source_id] {
        match import.kind {
            ImportKind::Specific { selected_name, imported_as } => {
                let const_id = source_consts[import.target_source]
                    .iter()
                    .find_map(|&(name, const_id)| (name == selected_name).then_some(const_id))
                    .expect("imported const not found");
                if scope.insert(imported_as, const_id).is_some() {
                    todo!("diagnostic: name collision on import");
                }
            }
            ImportKind::All => {
                for &(name, const_id) in &source_consts[import.target_source] {
                    if scope.insert(name, const_id).is_some() {
                        todo!("diagnostic: name collision on glob import");
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests;
