use crate::{Opcode, instructions::InstructionView};
use std::collections::VecDeque;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Value {
    Constant(u32),
    Symbolic,
    FunctionInput(u32),
}

#[derive(Debug, Clone)]
pub struct AbstractStack {
    stack: VecDeque<Value>,
    function_inputs: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Control {
    Step,
    JumpTo(Value),
    JumpIfUnknownTo(Value),
    Terminate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvmError {
    StackOverflow,
}

const MAX_EVM_STACK_DEPTH: usize = 1024;

impl AbstractStack {
    pub fn new() -> AbstractStack {
        AbstractStack { stack: VecDeque::new(), function_inputs: 0 }
    }

    pub fn clear(&mut self) {
        self.stack.clear();
        self.function_inputs = 0;
    }

    fn pop(&mut self) -> Value {
        if let Some(value) = self.stack.pop_back() {
            return value;
        }
        let fn_inp = self.function_inputs;
        self.function_inputs += 1;
        Value::FunctionInput(fn_inp)
    }

    fn push(&mut self, value: Value) -> Result<(), EvmError> {
        if self.stack.len() == MAX_EVM_STACK_DEPTH {
            return Err(EvmError::StackOverflow);
        }
        self.stack.push_back(value);
        Ok(())
    }

    pub fn execute(&mut self, instr: InstructionView<'_>) -> Result<Control, EvmError> {
        let Ok(op) = instr.op else { return Ok(Control::Terminate) };
        let control = match op {
            Opcode::Jump => {
                let destination = self.pop();
                Control::JumpTo(destination)
            }
            Opcode::JumpI => {
                let destination = self.pop();
                let condition = self.pop();
                match condition {
                    Value::Constant(known_condition) => {
                        if known_condition == 0 {
                            Control::Step
                        } else {
                            Control::JumpTo(destination)
                        }
                    }
                    Value::Symbolic | Value::FunctionInput(_) => {
                        Control::JumpIfUnknownTo(destination)
                    }
                }
            }
            Opcode::Push0 => {
                self.push(Value::Constant(0))?;
                Control::Step
            }
            op if op.is_push().is_some() => {
                let imm = instr.immediate().expect("push without immediate");
                match u32::try_from(imm) {
                    Ok(x) => self.push(Value::Constant(x))?,
                    Err(_) => self.push(Value::Symbolic)?,
                };
                Control::Step
            }
            op if op.is_terminating() => {
                for _ in 0..op.stack_io().inputs {
                    let _ = self.pop();
                }
                Control::Terminate
            }
            op => {
                for _ in 0..op.stack_io().inputs {
                    let _ = self.pop();
                }
                for _ in 0..op.stack_io().outputs {
                    self.push(Value::Symbolic)?;
                }
                Control::Step
            }
        };
        Ok(control)
    }
}
