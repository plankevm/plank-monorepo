use alloy_primitives::U256;

// EVM Versions
// Sourced from https://github.com/argotorg/solidity/blob/develop/liblangutil/EVMVersion.h
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd)]
pub enum EvmVersion {
    Homestead = 0,
    TangerineWhistle = 1,
    SpuriousDragon = 2,
    Byzantium = 3,
    Constantinople = 4,
    Petersburg = 5,
    Istanbul = 6,
    Berlin = 7,
    London = 8,
    Paris = 9,
    Shanghai = 10,
    Cancun = 11,
    Prague = 12,
    #[default]
    Osaka = 13,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Feature {
    // Count leading zeros: https://eips.ethereum.org/EIPS/eip-7939
    Clz,
}

impl EvmVersion {
    pub fn supports(self, feature: Feature) -> bool {
        match feature {
            Feature::Clz => self >= Self::Osaka,
        }
    }
}

impl From<EvmVersion> for U256 {
    fn from(value: EvmVersion) -> Self {
        U256::from(value as u32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evm_spec_supports_feature() {
        assert!(!EvmVersion::Cancun.supports(Feature::Clz));
        assert!(EvmVersion::Osaka.supports(Feature::Clz));
    }
}
