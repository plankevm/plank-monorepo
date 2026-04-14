use std::collections::HashMap;

use sir_hir::{self, LocalVariableId};

use crate::dataflow::{self, Effect, Transfer, TransferContext};

pub struct CopyPropagation {
    copies: HashMap<LocalVariableId, LocalVariableId>,
}

impl CopyPropagation {
    pub fn new() -> Self {
        Self {
            copies: HashMap::new(),
        }
    }
}

impl Default for CopyPropagation {
    fn default() -> Self {
        Self::new()
    }
}

impl Transfer for CopyPropagation {
    fn transfer_block(
        &mut self,
        ctx: &mut TransferContext<'_>,
        block_id: sir_hir::BasicBlockId,
    ) -> Effect {
        let block = ctx.body.block(block_id);

        for stmt in block.statements() {
            match stmt {
                sir_hir::Statement::Assign(assign) => {
                    match assign.rhs.as_ref() {
                        sir_hir::Rvalue::Use(op) => {
                            if let sir_hir::Operand::Copy(src) = op {
                                if let Some(replacement) = self.copies.get(src) {
                                    ctx.update(ctx.get_lvalue(&assign.lhs), *replacement);
                                }
                            }
                        }
                        _ => {}
                    }

                    if let sir_hir::Lvalue::Var(target) = assign.lhs.as_ref() {
                        if let sir_hir::Operand::Copy(src) = assign.rhs.as_ref() {
                            self.copies.insert(target.id, src.id);
                        } else if let sir_hir::Rvalue::Use(op) = assign.rhs.as_ref() {
                            if let sir_hir::Operand::Copy(src) = op {
                                if let Some(replacement) = self.copies.get(src) {
                                    self.copies.insert(target.id, *replacement);
                                } else {
                                    self.copies.remove(&target.id);
                                }
                            }
                        } else {
                            self.copies.remove(&target.id);
                        }
                    }
                }
                sir_hir::Statement::Call(call) => {
                    for arg in call.arguments.iter() {
                        if let sir_hir::Operand::Copy(src) = arg {
                            if let Some(replacement) = self.copies.get(src) {
                                ctx.replace_operand(&sir_hir::Operand::Copy(*replacement));
                            }
                        }
                    }

                    if let Some(dest) = call.destination.as_ref() {
                        self.copies.remove(&dest.id);
                    }
                }
                sir_hir::Statement::Switch(switch) => {
                    if let sir_hir::Operand::Copy(src) = &switch.operand {
                        if let Some(replacement) = self.copies.get(src) {
                            ctx.replace_operand(&sir_hir::Operand::Copy(*replacement));
                        }
                    }
                }
                _ => {}
            }
        }

        let terminator = block.terminator();
        match terminator.kind() {
            sir_hir::TerminatorKind::Call(call) => {
                for arg in call.arguments.iter() {
                    if let sir_hir::Operand::Copy(src) = arg {
                        if let Some(replacement) = self.copies.get(src) {
                            ctx.replace_operand(&sir_hir::Operand::Copy(*replacement));
                        }
                    }
                }

                if let Some(dest) = call.destination.as_ref() {
                    self.copies.remove(&dest.id);
                }
            }
            sir_hir::TerminatorKind::Branch(branch) => {
                for arg in branch.arguments.iter() {
                    if let sir_hir::Operand::Copy(src) = arg {
                        if let Some(replacement) = self.copies.get(src) {
                            ctx.replace_operand(&sir_hir::Operand::Copy(*replacement));
                        }
                    }
                }
            }
            sir_hir::TerminatorKind::ConditionalBranch(cond_branch) => {
                if let sir_hir::Operand::Copy(src) = &cond_branch.condition {
                    if let Some(replacement) = self.copies.get(src) {
                        ctx.replace_operand(&sir_hir::Operand::Copy(*replacement));
                    }
                }

                for arg in cond_branch.true_args.iter() {
                    if let sir_hir::Operand::Copy(src) = arg {
                        if let Some(replacement) = self.copies.get(src) {
                            ctx.replace_operand(&sir_hir::Operand::Copy(*replacement));
                        }
                    }
                }

                for arg in cond_branch.false_args.iter() {
                    if let sir_hir::Operand::Copy(src) = arg {
                        if let Some(replacement) = self.copies.get(src) {
                            ctx.replace_operand(&sir_hir::Operand::Copy(*replacement));
                        }
                    }
                }
            }
            _ => {}
        }

        Effect::Continue
    }

    fn transfer_phi(
        &mut self,
        ctx: &mut TransferContext<'_>,
        _block_id: sir_hir::BasicBlockId,
        var: LocalVariableId,
        _values: &[(sir_hir::BasicBlockId, sir_hir::Operand)],
        result: &sir_hir::Operand,
    ) {
        match result {
            sir_hir::Operand::Copy(src) => {
                if let Some(replacement) = self.copies.get(src) {
                    ctx.update_phi_result(var, sir_hir::Operand::Copy(*replacement));
                }
            }
            _ => {
                self.copies.remove(&var);
            }
        }
    }
}

pub fn run(body: &sir_hir::Body) -> HashMap<LocalVariableId, LocalVariableId> {
    let mut propagator = CopyPropagation::new();
    dataflow::iterate(body, &mut propagator, true);
    propagator.copies
}
