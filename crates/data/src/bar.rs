use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Bar {
    pub ts_ms: i64,
    pub open: Decimal,
    pub high: Decimal,
    pub low: Decimal,
    pub close: Decimal,
    pub volume: Decimal,
}

impl Bar {
    pub fn new(
        ts_ms: i64,
        open: Decimal,
        high: Decimal,
        low: Decimal,
        close: Decimal,
        volume: Decimal,
    ) -> Self {
        Self {
            ts_ms,
            open,
            high,
            low,
            close,
            volume,
        }
    }

    pub fn close_return(&self, previous: &Self) -> Decimal {
        (self.close - previous.close) / previous.close
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SymbolBar {
    pub symbol: String,
    pub bar: Bar,
}

impl SymbolBar {
    pub fn new(symbol: impl Into<String>, bar: Bar) -> Self {
        Self {
            symbol: symbol.into(),
            bar,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MarketSlice {
    pub ts_ms: i64,
    bars: BTreeMap<String, Bar>,
}

impl MarketSlice {
    pub fn new(ts_ms: i64, bars: impl IntoIterator<Item = SymbolBar>) -> Self {
        Self {
            ts_ms,
            bars: bars
                .into_iter()
                .map(|symbol_bar| (symbol_bar.symbol, symbol_bar.bar))
                .collect(),
        }
    }

    pub fn single(symbol: impl Into<String>, bar: Bar) -> Self {
        let ts_ms = bar.ts_ms;
        Self::new(ts_ms, vec![SymbolBar::new(symbol, bar)])
    }

    pub fn bar(&self, symbol: &str) -> Option<&Bar> {
        self.bars.get(symbol)
    }

    pub fn symbols(&self) -> Vec<String> {
        self.bars.keys().cloned().collect()
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, &Bar)> {
        self.bars.iter().map(|(symbol, bar)| (symbol.as_str(), bar))
    }
}
