// Data model and type definitions

use serde::{Deserialize, Serialize};
use std::fmt;
use std::hash::{Hash, Hasher};

/// Instrument identifier
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct InstrumentId {
    pub venue: Venue,
    pub symbol: String,
}

impl InstrumentId {
    pub fn new(venue: Venue, symbol: impl Into<String>) -> Self {
        Self {
            venue,
            symbol: symbol.into(),
        }
    }

    pub fn as_str(&self) -> &str {
        &self.symbol
    }

    pub fn display(&self) -> String {
        format!("{}:{}", self.venue.as_str(), self.symbol)
    }
}

impl fmt::Display for InstrumentId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.venue.as_str(), self.symbol)
    }
}

/// Trading venue
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Venue {
    UsEquity,
    HkEquity,
    Crypto,
    Polymarket,
}

impl Venue {
    pub fn as_str(&self) -> &'static str {
        match self {
            Venue::UsEquity => "USE",
            Venue::HkEquity => "HKE",
            Venue::Crypto => "CRYPTO",
            Venue::Polymarket => "POLY",
        }
    }
}

/// Order side
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum Side {
    Buy,
    Sell,
}

impl Side {
    pub fn as_str(&self) -> &'static str {
        match self {
            Side::Buy => "BUY",
            Side::Sell => "SELL",
        }
    }
}

impl fmt::Display for Side {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Granularity for time series data
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Granularity {
    Tick,
    Minute(u32),
    Hour(u32),
    Day,
}

impl Default for Granularity {
    fn default() -> Self {
        Granularity::Minute(1)
    }
}

impl fmt::Display for Granularity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Granularity::Tick => write!(f, "tick"),
            Granularity::Minute(m) => write!(f, "{}m", m),
            Granularity::Hour(h) => write!(f, "{}h", h),
            Granularity::Day => write!(f, "day"),
        }
    }
}

/// Kline (OHLCV) data structure
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Kline {
    pub instrument: InstrumentId,
    pub open_ts_ms: i64,
    pub close_ts_ms: i64,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: f64,
}

impl Kline {
    pub fn new(
        instrument: InstrumentId,
        open_ts_ms: i64,
        close_ts_ms: i64,
        open: f64,
        high: f64,
        low: f64,
        close: f64,
        volume: f64,
    ) -> Self {
        Self {
            instrument,
            open_ts_ms,
            close_ts_ms,
            open,
            high,
            low,
            close,
            volume,
        }
    }

    pub fn is_bullish(&self) -> bool {
        self.close > self.open
    }

    pub fn is_bearish(&self) -> bool {
        self.close < self.open
    }

    pub fn range(&self) -> f64 {
        self.high - self.low
    }

    pub fn body_size(&self) -> f64 {
        (self.close - self.open).abs()
    }
}

/// Tick data structure
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Tick {
    pub instrument: InstrumentId,
    pub ts_ms: i64,
    pub bid_price: f64,
    pub ask_price: f64,
    pub last_price: f64,
    pub volume: f64,
}

impl Tick {
    pub fn new(
        instrument: InstrumentId,
        ts_ms: i64,
        bid_price: f64,
        ask_price: f64,
        last_price: f64,
        volume: f64,
    ) -> Self {
        Self {
            instrument,
            ts_ms,
            bid_price,
            ask_price,
            last_price,
            volume,
        }
    }

    pub fn spread(&self) -> f64 {
        self.ask_price - self.bid_price
    }

    pub fn mid_price(&self) -> f64 {
        (self.bid_price + self.ask_price) / 2.0
    }
}

/// Value type for strategy parameters
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Value {
    Number(f64),
    String(String),
    Boolean(bool),
}

impl Default for Value {
    fn default() -> Self {
        Value::Number(0.0)
    }
}

impl Value {
    pub fn as_f64(&self) -> f64 {
        match self {
            Value::Number(n) => *n,
            Value::String(s) => s.parse().unwrap_or(0.0),
            Value::Boolean(b) => {
                if *b {
                    1.0
                } else {
                    0.0
                }
            }
        }
    }

    pub fn as_string(&self) -> &str {
        match self {
            Value::String(s) => s,
            _ => "",
        }
    }

    pub fn as_bool(&self) -> bool {
        match self {
            Value::Boolean(b) => *b,
            _ => false,
        }
    }
}

/// Order type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum OrderType {
    Market,
    Limit,
    Stop,
    StopLimit,
    Twap,
    Vwap,
    Iceberg,
}

impl Default for OrderType {
    fn default() -> Self {
        OrderType::Limit
    }
}

// Tests
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_instrument_id() {
        let id = InstrumentId::new(Venue::UsEquity, "AAPL");
        assert_eq!(id.as_str(), "AAPL");
        assert_eq!(id.display(), "USE:AAPL");
    }

    #[test]
    fn test_kline_properties() {
        let kline = Kline::new(
            InstrumentId::new(Venue::UsEquity, "AAPL"),
            1000,
            2000,
            100.0,
            105.0,
            98.0,
            103.0,
            1000.0,
        );
        assert!(kline.is_bullish());
        assert_eq!(kline.range(), 7.0);
        assert_eq!(kline.body_size(), 3.0);
    }

    #[test]
    fn test_tick_properties() {
        let tick = Tick::new(
            InstrumentId::new(Venue::UsEquity, "AAPL"),
            1000,
            100.0,
            101.0,
            100.5,
            50.0,
        );
        assert_eq!(tick.spread(), 1.0);
        assert_eq!(tick.mid_price(), 100.5);
    }

    #[test]
    fn test_value_types() {
        let num = Value::Number(42.0);
        assert_eq!(num.as_f64(), 42.0);

        let str_val = Value::String("3.14".to_string());
        assert_eq!(str_val.as_f64(), 3.14);

        let bool_val = Value::Boolean(true);
        assert_eq!(bool_val.as_f64(), 1.0);
    }
}
