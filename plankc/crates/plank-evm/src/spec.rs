use alloy_primitives::U256;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum EvmSpecId {
    Cancun,
    Prague,
    #[default]
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
