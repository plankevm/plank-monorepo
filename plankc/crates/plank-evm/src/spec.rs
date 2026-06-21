use alloy_primitives::U256;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum EvmSpecId {
    Cancun,
    Prague,
    #[default]
    Osaka,
}

impl From<EvmSpecId> for U256 {
    fn from(value: EvmSpecId) -> Self {
        match value {
            EvmSpecId::Cancun => U256::from(0),
            EvmSpecId::Prague => U256::from(1),
            EvmSpecId::Osaka => U256::from(2),
        }
    }
}
