use crate::{ArgsId, BlockId, Expr, FnId, Instruction, LocalId, Mir};
use plank_core::Idx;
use plank_session::Session;
use plank_values::{BigNumInterner, TypeId};
use std::fmt::{self, Display, Formatter};

pub struct DisplayMir<'a> {
    mir: &'a Mir,
    big_nums: &'a BigNumInterner,
    session: &'a Session,
}

impl<'a> DisplayMir<'a> {
    pub fn new(mir: &'a Mir, big_nums: &'a BigNumInterner, session: &'a Session) -> Self {
        Self { mir, big_nums, session }
    }

    fn fmt_type(&self, f: &mut Formatter<'_>, type_id: TypeId) -> fmt::Result {
        self.mir.types.fmt_type(f, type_id, self.session)
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

    fn fmt_expr(&self, f: &mut Formatter<'_>, expr: Expr) -> fmt::Result {
        match expr {
            Expr::LocalRef(local) => self.fmt_local(f, local),
            Expr::Bool(b) => write!(f, "{b}"),
            Expr::Void => write!(f, "unit"),
            Expr::Error => write!(f, "<error>"),
            Expr::BigNum(id) => write!(f, "{}", self.big_nums[id]),
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
        let pad = "    ".repeat(indent);
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
