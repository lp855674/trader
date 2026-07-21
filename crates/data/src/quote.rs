use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MarketDataSource {
    Ibkr,
    Longbridge,
    Other(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MarketDataKind {
    Realtime,
    Frozen,
    Delayed,
    DelayedFrozen,
    Unknown,
}

impl MarketDataKind {
    pub fn from_provider_name(value: &str) -> Self {
        match value {
            "realtime" => Self::Realtime,
            "frozen" => Self::Frozen,
            "delayed" => Self::Delayed,
            "delayed_frozen" => Self::DelayedFrozen,
            _ => Self::Unknown,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Realtime => "realtime",
            Self::Frozen => "frozen",
            Self::Delayed => "delayed",
            Self::DelayedFrozen => "delayed_frozen",
            Self::Unknown => "unknown",
        }
    }
}

impl fmt::Display for MarketDataKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Quote {
    pub symbol: String,
    pub bid: Option<Decimal>,
    pub ask: Option<Decimal>,
    pub last: Option<Decimal>,
    pub exchange_ts_ms: Option<i64>,
    pub received_ts_ms: i64,
    pub source: MarketDataSource,
    pub kind: MarketDataKind,
}

impl Quote {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        symbol: impl Into<String>,
        bid: Option<Decimal>,
        ask: Option<Decimal>,
        last: Option<Decimal>,
        exchange_ts_ms: Option<i64>,
        received_ts_ms: i64,
        source: MarketDataSource,
        kind: MarketDataKind,
    ) -> Self {
        Self {
            symbol: symbol.into(),
            bid,
            ask,
            last,
            exchange_ts_ms,
            received_ts_ms,
            source,
            kind,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn provider_market_data_names_map_to_stable_kinds() {
        assert_eq!(
            MarketDataKind::from_provider_name("realtime"),
            MarketDataKind::Realtime
        );
        assert_eq!(
            MarketDataKind::from_provider_name("delayed_frozen"),
            MarketDataKind::DelayedFrozen
        );
        assert_eq!(
            MarketDataKind::from_provider_name("vendor_specific"),
            MarketDataKind::Unknown
        );
    }

    #[test]
    fn quote_preserves_source_and_provider_timestamps() {
        let quote = Quote::new(
            "US:NASDAQ:AAPL:EQUITY",
            Some(dec!(195.9)),
            Some(dec!(196)),
            Some(dec!(195.95)),
            Some(10_000),
            10_010,
            MarketDataSource::Longbridge,
            MarketDataKind::Realtime,
        );

        assert_eq!(quote.source, MarketDataSource::Longbridge);
        assert_eq!(quote.exchange_ts_ms, Some(10_000));
        assert_eq!(quote.received_ts_ms, 10_010);
        assert_eq!(quote.kind.as_str(), "realtime");
    }
}
