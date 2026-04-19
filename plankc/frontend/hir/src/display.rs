use crate::*;
use plank_core::Idx;
use plank_session::Session;
use plank_values::{U256, Value, ValueInterner, uint};
use std::fmt::{self, Display, Formatter};

const DISPLAY_AS_HEX_THRESHOLD: U256 = uint!(100_000_U256);

pub struct DisplayHir<'a> {
    hir: &'a Hir,
    values: &'a ValueInterner,
    session: &'a Session,
}

impl<'a> DisplayHir<'a> {
    pub fn new(hir: &'a Hir, values: &'a ValueInterner, session: &'a Session) -> Self {
        Self { hir, values, session }
    }

    fn fmt_local(&self, f: &mut Formatter<'_>, local: LocalId) -> fmt::Result {
        write!(f, "%{}", local.get())
    }

    fn fmt_const_ref(&self, f: &mut Formatter<'_>, const_id: ConstId) -> fmt::Result {
        write!(f, "${}", const_id.get())
    }

    fn fmt_fn_ref(&self, f: &mut Formatter<'_>, fn_id: FnDefId) -> fmt::Result {
        write!(f, "@fn{}", fn_id.get())
    }

    fn fmt_struct_ref(&self, f: &mut Formatter<'_>, struct_id: StructDefId) -> fmt::Result {
        let r#struct = &self.hir.struct_defs[struct_id];
        let (line, col) =
            self.session.offset_to_line_col(r#struct.source_id, r#struct.source_span.start);
        let source = self.session.get_source(r#struct.source_id);
        write!(f, "struct#{} {}:{}:{}", struct_id.get(), source.path.to_str().unwrap(), line, col)
    }

    fn fmt_args(&self, f: &mut Formatter<'_>, args_id: CallArgsId) -> fmt::Result {
        let args = &self.hir.call_args[args_id];
        write!(f, "(")?;
        for (i, &local) in args.iter().enumerate() {
            if i > 0 {
                write!(f, ", ")?;
            }
            self.fmt_local(f, local)?;
        }
        write!(f, ")")
    }

    fn fmt_expr(&self, f: &mut Formatter<'_>, expr: Expr) -> fmt::Result {
        use ExprKind as Expr;
        match expr.kind {
            Expr::ConstRef(id) => self.fmt_const_ref(f, id),
            Expr::LocalRef(id) => self.fmt_local(f, id),
            Expr::FnDef(id) => self.fmt_fn_ref(f, id),
            Expr::Value(Err(Poisoned)) => write!(f, "<poison>"),
            Expr::Value(Ok(vid)) => match self.values.lookup(vid) {
                Value::Bool(b) => write!(f, "{b}"),
                Value::Void => write!(f, "void"),
                Value::BigNum(x) => {
                    if x < DISPLAY_AS_HEX_THRESHOLD {
                        write!(f, "{x}")
                    } else {
                        write!(f, "0x{x:x}")
                    }
                }
                Value::Type(id) => write!(f, "type:{}", id.as_primitive().unwrap().name()),
                other @ (Value::Closure { .. } | Value::StructVal { .. }) => {
                    unreachable!("unexpected value in HIR: {other:?}")
                }
            },
            Expr::Call { callee, args } => {
                write!(f, "call ")?;
                self.fmt_local(f, callee)?;
                self.fmt_args(f, args)
            }
            Expr::BuiltinCall { builtin, args } => {
                write!(f, "{builtin}")?;
                self.fmt_args(f, args)
            }
            Expr::Member { object, member, .. } => {
                self.fmt_local(f, object)?;
                let name = &self.session.lookup_name(member);
                write!(f, ".{name}")
            }
            Expr::StructLit { ty, fields } => {
                self.fmt_local(f, ty)?;
                write!(f, " {{")?;
                let field_infos = &self.hir.fields[fields];
                for (i, field) in field_infos.iter().enumerate() {
                    if i > 0 {
                        write!(f, ",")?;
                    }
                    let name = self.session.lookup_name(field.name);
                    write!(f, " {name}: ")?;
                    self.fmt_local(f, field.value)?;
                }
                if !field_infos.is_empty() {
                    write!(f, " ")?;
                }
                write!(f, "}}")
            }
            Expr::StructDef(id) => self.fmt_struct_ref(f, id),
            Expr::LogicalNot { input } => {
                write!(f, "logical_not ")?;
                self.fmt_local(f, input)
            }
            Expr::UnaryOpCall { op, input } => {
                write!(f, "({}) ", op.symbol())?;
                self.fmt_local(f, input)
            }
            Expr::BinaryOpCall { op, lhs, rhs } => {
                write!(f, "({}) ", op.symbol())?;
                self.fmt_local(f, lhs)?;
                write!(f, " ")?;
                self.fmt_local(f, rhs)
            }
        }
    }

    fn fmt_set(
        &self,
        f: &mut Formatter<'_>,
        indent: usize,
        local: LocalId,
        r#type: Option<LocalId>,
        expr: Expr,
        mutable: bool,
    ) -> fmt::Result {
        let pad = "    ".repeat(indent);
        write!(f, "{pad}")?;
        self.fmt_local(f, local)?;
        if let Some(r#type) = r#type {
            write!(f, " : ")?;
            self.fmt_local(f, r#type)?;
        }
        write!(f, " {}= ", if mutable { "[mut]" } else { "" })?;
        self.fmt_expr(f, expr)?;
        writeln!(f)
    }

    fn fmt_instr(
        &self,
        f: &mut Formatter<'_>,
        instr: InstructionKind,
        indent: usize,
    ) -> fmt::Result {
        let pad = "    ".repeat(indent);
        match instr {
            InstructionKind::Set { local, r#type, expr } => {
                self.fmt_set(f, indent, local, r#type, expr, false)
            }
            InstructionKind::SetMut { local, r#type, expr } => {
                self.fmt_set(f, indent, local, r#type, expr, true)
            }
            InstructionKind::BranchSet { local, expr } => {
                write!(f, "{pad}")?;
                self.fmt_local(f, local)?;
                write!(f, " [br]= ")?;
                self.fmt_expr(f, expr)?;
                writeln!(f)
            }
            InstructionKind::Assign { target, expr: value } => {
                write!(f, "{pad}")?;
                self.fmt_local(f, target)?;
                write!(f, " := ")?;
                self.fmt_expr(f, value)?;
                writeln!(f)
            }
            InstructionKind::Eval(expr) => {
                write!(f, "{pad}eval ")?;
                self.fmt_expr(f, expr)?;
                writeln!(f)
            }
            InstructionKind::Return(expr) => {
                write!(f, "{pad}ret ")?;
                self.fmt_expr(f, expr)?;
                writeln!(f)
            }
            InstructionKind::If { condition, then_block, else_block } => {
                write!(f, "{pad}if ")?;
                self.fmt_local(f, condition)?;
                writeln!(f, " {{")?;
                self.fmt_block(f, then_block, indent + 1)?;
                writeln!(f, "{pad}}} else {{")?;
                self.fmt_block(f, else_block, indent + 1)?;
                writeln!(f, "{pad}}}")
            }
            InstructionKind::While { condition_block, condition, body } => {
                writeln!(f, "{pad}while {{")?;
                writeln!(f, "{pad}    cond:")?;
                self.fmt_block(f, condition_block, indent + 2)?;
                write!(f, "{pad}    test ")?;
                self.fmt_local(f, condition)?;
                writeln!(f)?;
                writeln!(f, "{pad}    body:")?;
                self.fmt_block(f, body, indent + 2)?;
                writeln!(f, "{pad}}}")
            }
            InstructionKind::ComptimeBlock { body } => {
                writeln!(f, "{pad}comptime {{")?;
                self.fmt_block(f, body, indent + 1)?;
                writeln!(f, "{pad}}}")
            }
            InstructionKind::Param { comptime, arg, r#type, idx } => {
                write!(f, "{pad}")?;
                if comptime {
                    write!(f, "[comptime] ")?;
                }
                write!(f, "param#{idx} ")?;
                self.fmt_local(f, arg)?;
                write!(f, " : ")?;
                self.fmt_local(f, r#type)?;
                writeln!(f)
            }
        }
    }

    fn fmt_block(&self, f: &mut Formatter<'_>, block_id: BlockId, indent: usize) -> fmt::Result {
        let instructions = &self.hir.block_instrs[block_id];
        for &instr in instructions {
            self.fmt_instr(f, instr.kind, indent)?;
        }
        Ok(())
    }

    fn fmt_const(&self, f: &mut Formatter<'_>, const_id: ConstId) -> fmt::Result {
        let const_def = &self.hir.consts[const_id];
        let const_name = self.session.lookup_name(const_def.name);
        writeln!(f, "{const_id:?} ({const_name:?}) result={:?} {{", const_def.result)?;
        self.fmt_block(f, const_def.body, 1)?;
        writeln!(f, "}}")
    }

    fn fmt_fn_def(&self, f: &mut Formatter<'_>, fn_def_id: FnDefId) -> fmt::Result {
        let fn_def = &self.hir.fns[fn_def_id];
        let params = &self.hir.fn_params[fn_def_id];
        let captures = &self.hir.fn_captures[fn_def_id];

        write!(f, "@fn{}(", fn_def_id.get())?;
        for (i, param) in params.iter().enumerate() {
            if i > 0 {
                write!(f, ", ")?;
            }
            if param.is_comptime {
                write!(f, "comptime ")?;
            }
            self.fmt_local(f, param.value)?;
            write!(f, ": ")?;
            self.fmt_local(f, param.r#type)?;
        }
        write!(f, ") -> ")?;
        self.fmt_local(f, fn_def.return_type)?;
        writeln!(f, " {{")?;

        if !captures.is_empty() {
            write!(f, "    captures: [")?;
            for (i, capture) in captures.iter().enumerate() {
                if i > 0 {
                    write!(f, ", ")?;
                }
                self.fmt_local(f, capture.outer_local)?;
                write!(f, " -> ")?;
                self.fmt_local(f, capture.inner_local)?;
            }
            writeln!(f, "]")?;
        }

        writeln!(f, "    preamble:")?;
        self.fmt_block(f, fn_def.type_preamble, 2)?;
        writeln!(f, "    body:")?;
        self.fmt_block(f, fn_def.body, 2)?;
        writeln!(f, "}}")
    }

    fn fmt_struct_def(&self, f: &mut Formatter<'_>, struct_def_id: StructDefId) -> fmt::Result {
        let struct_def = &self.hir.struct_defs[struct_def_id];
        let fields = &self.hir.fields[struct_def.fields];

        write!(f, "@struct{}[index: ", struct_def_id.get())?;
        self.fmt_local(f, struct_def.type_index)?;
        write!(f, "] {{")?;
        for (i, field) in fields.iter().enumerate() {
            if i > 0 {
                write!(f, ",")?;
            }
            let name = self.session.lookup_name(field.name);
            write!(f, " {name}: ")?;
            self.fmt_local(f, field.value)?;
        }
        if !fields.is_empty() {
            write!(f, " ")?;
        }
        writeln!(f, "}}")
    }
}

impl Display for DisplayHir<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        writeln!(f, "==== Constants ====")?;
        for const_id in self.hir.consts.iter_idx() {
            self.fmt_const(f, const_id)?;
        }

        if !self.hir.fns.is_empty() {
            writeln!(f, "\n==== Functions ====")?;
            for fn_def_id in self.hir.fns.iter_idx() {
                self.fmt_fn_def(f, fn_def_id)?;
            }
        }

        if !self.hir.struct_defs.is_empty() {
            writeln!(f, "\n==== Structs ====")?;
            for struct_def_id in self.hir.struct_defs.iter_idx() {
                self.fmt_struct_def(f, struct_def_id)?;
            }
        }

        writeln!(f, "\n==== Init ====")?;
        self.fmt_block(f, self.hir.init, 0)?;

        if let Some(run_block) = self.hir.run {
            writeln!(f, "\n==== Run ====")?;
            self.fmt_block(f, run_block, 0)?;
        }

        Ok(())
    }
}
