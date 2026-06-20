use alloy_primitives::U256;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvmSpecId {
    Cancun,
    Prague,
    Osaka,
}

impl Into<U256> for EvmSpecId {
    fn into(self) -> U256 {
        match self {
            Self::Cancun => U256::from(0),
            Self::Prague => U256::from(1),
            Self::Osaka => U256::from(2),
        }
    }
}
