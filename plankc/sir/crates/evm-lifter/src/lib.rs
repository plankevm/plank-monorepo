use alloy_primitives as _;
use plank_core as _;
use sir_data as _;
use sir_passes as _;

pub mod instructions;
pub mod opcode;

pub use opcode::{Opcode, StackIO};

#[cfg(test)]
mod tests {
    use plank_test_utils as _;
    use pretty_assertions as _;
}
