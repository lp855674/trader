#![forbid(unsafe_code)]

mod bar;
mod csv;
pub mod ingestion;
mod parquet;
mod quote;

use std::collections::{BTreeMap, BTreeSet};

pub use bar::*;
pub use csv::*;
pub use parquet::*;
pub use quote::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BarInput {
    pub symbol: String,
    pub source: String,
    pub path: String,
}

impl BarInput {
    pub fn new(
        symbol: impl Into<String>,
        source: impl Into<String>,
        path: impl Into<String>,
    ) -> Self {
        Self {
            symbol: symbol.into(),
            source: source.into(),
            path: path.into(),
        }
    }
}

pub fn load_bars(source: &str, path: impl AsRef<std::path::Path>) -> Result<Vec<Bar>, DataError> {
    match source {
        "csv" => load_bars_from_csv(path),
        "parquet" => load_bars_from_parquet(path),
        other => Err(DataError::UnsupportedSource(other.to_string())),
    }
}

pub fn load_market_slices(inputs: &[BarInput]) -> Result<Vec<MarketSlice>, DataError> {
    if inputs.is_empty() {
        return Err(DataError::EmptyBarInputs);
    }

    let mut grouped_bars = BTreeMap::<i64, Vec<SymbolBar>>::new();
    let mut seen_symbol_timestamps = BTreeSet::<(String, i64)>::new();
    for input in inputs {
        for bar in load_bars(&input.source, &input.path)? {
            if !seen_symbol_timestamps.insert((input.symbol.clone(), bar.ts_ms)) {
                return Err(DataError::DuplicateSymbolTimestamp {
                    symbol: input.symbol.clone(),
                    ts_ms: bar.ts_ms,
                });
            }
            grouped_bars
                .entry(bar.ts_ms)
                .or_default()
                .push(SymbolBar::new(input.symbol.clone(), bar));
        }
    }

    Ok(grouped_bars
        .into_iter()
        .map(|(ts_ms, bars)| MarketSlice::new(ts_ms, bars))
        .collect())
}
