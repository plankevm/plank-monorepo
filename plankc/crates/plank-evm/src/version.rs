use alloy_primitives::U256;

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

impl EvmVersion {
    pub fn name(self) -> &'static str {
        match self {
            EvmVersion::Homestead => "homestead",
            EvmVersion::TangerineWhistle => "tangerineWhistle",
            EvmVersion::SpuriousDragon => "spuriousDragon",
            EvmVersion::Byzantium => "byzantium",
            EvmVersion::Constantinople => "constantinople",
            EvmVersion::Petersburg => "petersburg",
            EvmVersion::Istanbul => "istanbul",
            EvmVersion::Berlin => "berlin",
            EvmVersion::London => "london",
            EvmVersion::Paris => "paris",
            EvmVersion::Shanghai => "shanghai",
            EvmVersion::Cancun => "cancun",
            EvmVersion::Prague => "prague",
            EvmVersion::Osaka => "osaka",
        }
    }
}

impl std::fmt::Display for EvmVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name())
    }
}

impl From<EvmVersion> for U256 {
    fn from(value: EvmVersion) -> Self {
        U256::from(value as u32)
    }
}
