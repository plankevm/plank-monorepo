use alloy_primitives as _;
use plank_core as _;
use sir_data as _;
use sir_passes as _;

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
