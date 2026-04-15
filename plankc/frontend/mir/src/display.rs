use crate::{ArgsId, BlockId, Expr, FnId, Instruction, LocalId, Mir};
use plank_core::Idx;
use plank_session::Session;
use plank_values::{TypeId, Value, ValueId, ValueInterner, uint};
use std::fmt::{self, Display, Formatter};

pub struct DisplayMir<'a> {
    mir: &'a Mir,
    values: &'a ValueInterner,
    session: &'a Session,
}

const PAD: &str = "    ";

impl<'a> DisplayMir<'a> {
    pub fn new(mir: &'a Mir, values: &'a ValueInterner, session: &'a Session) -> Self {
        Self { mir, values, session }
    }

    fn fmt_type(&self, f: &mut Formatter<'_>, ty: TypeId) -> fmt::Result {
        write!(f, "{}", self.mir.types.format(self.session, ty))
    }

    fn fmt_args(&self, f: &mut Formatter<'_>, args_id: ArgsId) -> fmt::Result {
        let args = &self.mir.args[args_id];
        write!(f, "(")?;
        for (i, &local) in args.iter().enumerate() {
            if i > 0 {
                write!(f, ", ")?;
            }
            self.fmt_local(f, local)?;
        }
        write!(f, ")")
    }

    fn fmt_local(&self, f: &mut Formatter<'_>, local: LocalId) -> fmt::Result {
        write!(f, "%{}", local.get())
    }

    fn fmt_value(&self, f: &mut Formatter<'_>, vid: ValueId, indent: usize) -> fmt::Result {
        let pad = PAD.repeat(indent);
        match self.values.lookup(vid) {
            Value::Bool(b) => write!(f, "{}", b),
            Value::BigNum(x) => {
                if x < uint!(100_000_U256) {
                    write!(f, "{x}")
                } else {
                    write!(f, "{x:x}")
                }
            }
            Value::Void => write!(f, "void_unit"),
            Value::StructVal { ty, fields } => {
                write!(f, "struct#{} {{", ty.get())?;
                if !fields.is_empty() {
                    writeln!(f)?;
                }
                for &field in fields {
                    write!(f, "{pad}{PAD}")?;
                    self.fmt_value(f, field, indent + 1)?;
                    writeln!(f, ",")?;
                }
                write!(f, "{pad}}}")
            }
            Value::Type(_) | Value::Closure { .. } => {
                unreachable!("comptime-only value in MIR")
            }
        }
    }

    fn fmt_expr(&self, f: &mut Formatter<'_>, expr: Expr) -> fmt::Result {
        match expr {
            Expr::LocalRef(local) => self.fmt_local(f, local),
            Expr::Const(vid) => self.fmt_value(f, vid, 1),
            Expr::Call { callee, args } => {
                write!(f, "call @fn{}", callee.get())?;
                self.fmt_args(f, args)
            }
            Expr::BuiltinCall { builtin, args } => {
                write!(f, "{builtin}")?;
                self.fmt_args(f, args)
            }
            Expr::FieldAccess { object, field_index } => {
                self.fmt_local(f, object)?;
                write!(f, ".{field_index}")
            }
            Expr::StructLit { ty, fields } => {
                self.fmt_type(f, ty)?;
                write!(f, " {{")?;
                let args = &self.mir.args[fields];
                for (i, &local) in args.iter().enumerate() {
                    if i > 0 {
                        write!(f, ",")?;
                    }
                    write!(f, " ")?;
                    self.fmt_local(f, local)?;
                }
                if !args.is_empty() {
                    write!(f, " ")?;
                }
                write!(f, "}}")
            }
        }
    }

    fn fmt_instr(
        &self,
        f: &mut Formatter<'_>,
        fn_id: FnId,
        instr: Instruction,
        indent: usize,
    ) -> fmt::Result {
        let pad = PAD.repeat(indent);
        match instr {
            Instruction::Set { target: local, expr } => {
                write!(f, "{pad}")?;
                self.fmt_local(f, local)?;
                write!(f, " : ")?;
                self.fmt_type(f, self.mir.fn_locals[fn_id][local.idx()])?;
                write!(f, " = ")?;
                self.fmt_expr(f, expr)?;
                writeln!(f)
            }
            Instruction::Return(value) => {
                write!(f, "{pad}ret ")?;
                self.fmt_local(f, value)?;
                writeln!(f)
            }
            Instruction::If { condition, then_block, else_block } => {
                write!(f, "{pad}if ")?;
                self.fmt_local(f, condition)?;
                writeln!(f, " {{")?;
                self.fmt_block(f, fn_id, then_block, indent + 1)?;
                writeln!(f, "{pad}}} else {{")?;
                self.fmt_block(f, fn_id, else_block, indent + 1)?;
                writeln!(f, "{pad}}}")
            }
            Instruction::While { condition_block, condition, body } => {
                writeln!(f, "{pad}while {{")?;
                writeln!(f, "{pad}  cond:")?;
                self.fmt_block(f, fn_id, condition_block, indent + 2)?;
                write!(f, "{pad}  test ")?;
                self.fmt_local(f, condition)?;
                writeln!(f)?;
                writeln!(f, "{pad}  body:")?;
                self.fmt_block(f, fn_id, body, indent + 2)?;
                writeln!(f, "{pad}}}")
            }
        }
    }

    fn fmt_block(
        &self,
        f: &mut Formatter<'_>,
        fn_id: FnId,
        block_id: BlockId,
        indent: usize,
    ) -> fmt::Result {
        let instructions = &self.mir.blocks[block_id];
        for &instr in instructions {
            self.fmt_instr(f, fn_id, instr, indent)?;
        }
        Ok(())
    }

    fn fmt_fn(&self, f: &mut Formatter<'_>, fn_id: FnId) -> fmt::Result {
        let fn_def = &self.mir.fns[fn_id];
        let locals = &self.mir.fn_locals[fn_id];

        write!(f, "@fn{}(", fn_id.get())?;
        for i in 0..fn_def.param_count {
            if i > 0 {
                write!(f, ", ")?;
            }
            write!(f, "%{i}: ")?;
            self.fmt_type(f, locals[i as usize])?;
        }
        write!(f, ") -> ")?;
        self.fmt_type(f, fn_def.return_type)?;
        writeln!(f, " {{")?;

        self.fmt_block(f, fn_id, fn_def.body, 1)?;
        writeln!(f, "}}")
    }
}

impl Display for DisplayMir<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        writeln!(f, "==== Functions ====")?;
        for (fn_id, _) in self.mir.fns.enumerate_idx() {
            let is_init = fn_id == self.mir.init;
            let is_run = self.mir.run == Some(fn_id);
            if is_init {
                writeln!(f, "; init")?;
            }
            if is_run {
                writeln!(f, "; run")?;
            }
            self.fmt_fn(f, fn_id)?;
            writeln!(f)?;
        }
        Ok(())
    }
}
