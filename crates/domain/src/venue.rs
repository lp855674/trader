#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Venue {
    UsEquity,
    HkEquity,
    Crypto,
    Polymarket,
}

impl Venue {
    pub fn as_str(self) -> &'static str {
        match self {
            Venue::UsEquity => "US_EQUITY",
            Venue::HkEquity => "HK_EQUITY",
            Venue::Crypto => "CRYPTO",
            Venue::Polymarket => "POLYMARKET",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "US_EQUITY" => Some(Venue::UsEquity),
            "HK_EQUITY" => Some(Venue::HkEquity),
            "CRYPTO" => Some(Venue::Crypto),
            "POLYMARKET" => Some(Venue::Polymarket),
            _ => None,
        }
    }
}
