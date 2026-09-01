use alloy_primitives as _;
use plank_core as _;
use sir_data as _;
use sir_passes as _;

#[cfg(test)]
macro_rules! bytecode {
    ($($byte:tt),* $(,)?) => {
        [$(bytecode!(@byte $byte)),*]
    };
    (@byte $opcode:ident) => {
        $crate::Opcode::$opcode as u8
    };
    (@byte $opcode:path) => {
        $opcode as u8
    };
    (@byte $byte:literal) => {
        $byte as u8
    };
}

mod abs_evm;
pub mod instructions;
pub mod opcode;
pub mod primitive_blocks;

pub use opcode::{Opcode, StackIO};

#[cfg(test)]
mod tests {
    use plank_test_utils as _;
    use pretty_assertions as _;
}
