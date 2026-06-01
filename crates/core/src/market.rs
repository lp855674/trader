use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Market {
    Cn,
    Hk,
    Us,
    Crypto,
}

impl Market {
    pub fn code(self) -> &'static str {
        match self {
            Self::Cn => "CN",
            Self::Hk => "HK",
            Self::Us => "US",
            Self::Crypto => "CRYPTO",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AssetClass {
    Equity,
    CryptoSpot,
    CryptoPerp,
    CryptoFuture,
}

impl AssetClass {
    pub fn code(self) -> &'static str {
        match self {
            Self::Equity => "EQUITY",
            Self::CryptoSpot => "CRYPTO_SPOT",
            Self::CryptoPerp => "CRYPTO_PERP",
            Self::CryptoFuture => "CRYPTO_FUTURE",
        }
    }
}
