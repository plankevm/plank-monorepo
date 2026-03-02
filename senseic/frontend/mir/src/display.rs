use crate::{ArgsId, BlockId, Expr, FnId, Instruction, LocalId, Mir};
use sensei_core::Idx;
use sensei_values::{BigNumInterner, Type, TypeId};
use std::fmt::{self, Display, Formatter};

pub struct DisplayMir<'a> {
    mir: &'a Mir,
    big_nums: &'a BigNumInterner,
}

impl<'a> DisplayMir<'a> {
    pub fn new(mir: &'a Mir, big_nums: &'a BigNumInterner) -> Self {
        Self { mir, big_nums }
    }

    fn fmt_type(&self, f: &mut Formatter<'_>, type_id: TypeId) -> fmt::Result {
        match self.mir.types.lookup(type_id) {
            Type::Void => write!(f, "void"),
            Type::Int => write!(f, "u256"),
            Type::Bool => write!(f, "bool"),
            Type::MemoryPointer => write!(f, "memptr"),
            Type::Type => write!(f, "type"),
            Type::Function => write!(f, "function"),
            Type::Struct(info) => {
                write!(f, "struct#{}", info.type_index.get())
            }
        }
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
            Expr::Void => write!(f, "void"),
            Expr::BigNum(id) => write!(f, "{}", self.big_nums[id]),
            Expr::Call { callee, args } => {
                write!(f, "call @fn{}", callee.get())?;
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

    fn fmt_instr(&self, f: &mut Formatter<'_>, instr: Instruction, indent: usize) -> fmt::Result {
        let pad = "    ".repeat(indent);
        match instr {
            Instruction::Set { local, expr } => {
                write!(f, "{pad}")?;
                self.fmt_local(f, local)?;
                write!(f, " = ")?;
                self.fmt_expr(f, expr)?;
                writeln!(f)
            }
            Instruction::Assign { target, value } => {
                write!(f, "{pad}")?;
                self.fmt_local(f, target)?;
                write!(f, " := ")?;
                self.fmt_expr(f, value)?;
                writeln!(f)
            }
            Instruction::Eval(expr) => {
                write!(f, "{pad}eval ")?;
                self.fmt_expr(f, expr)?;
                writeln!(f)
            }
            Instruction::Return(expr) => {
                write!(f, "{pad}ret ")?;
                self.fmt_expr(f, expr)?;
                writeln!(f)
            }
            Instruction::If { condition, then_block, else_block } => {
                write!(f, "{pad}if ")?;
                self.fmt_local(f, condition)?;
                writeln!(f, " {{")?;
                self.fmt_block(f, then_block, indent + 1)?;
                writeln!(f, "{pad}}} else {{")?;
                self.fmt_block(f, else_block, indent + 1)?;
                writeln!(f, "{pad}}}")
            }
            Instruction::While { condition_block, condition, body } => {
                writeln!(f, "{pad}while {{")?;
                writeln!(f, "{pad}  cond:")?;
                self.fmt_block(f, condition_block, indent + 2)?;
                write!(f, "{pad}  test ")?;
                self.fmt_local(f, condition)?;
                writeln!(f)?;
                writeln!(f, "{pad}  body:")?;
                self.fmt_block(f, body, indent + 2)?;
                writeln!(f, "{pad}}}")
            }
        }
    }

    fn fmt_block(&self, f: &mut Formatter<'_>, block_id: BlockId, indent: usize) -> fmt::Result {
        let instructions = &self.mir.blocks[block_id];
        for &instr in instructions {
            self.fmt_instr(f, instr, indent)?;
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

        if fn_def.param_count < locals.len() as u32 {
            writeln!(f, "    ; locals")?;
            for (i, &ty) in locals.iter().enumerate().skip(fn_def.param_count as usize) {
                write!(f, "    ; %{i}: ")?;
                self.fmt_type(f, ty)?;
                writeln!(f)?;
            }
            writeln!(f)?;
        }

        self.fmt_block(f, fn_def.body, 1)?;
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
