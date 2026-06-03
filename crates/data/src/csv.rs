use std::path::Path;

use rust_decimal::Decimal;
use serde::Deserialize;
use thiserror::Error;

use crate::Bar;

#[derive(Debug, Error)]
pub enum DataError {
    #[error("failed to read csv: {0}")]
    Csv(#[from] csv::Error),
    #[error("failed to access data file: {0}")]
    Io(#[from] std::io::Error),
    #[error("failed to read or write parquet: {0}")]
    Parquet(#[from] polars::error::PolarsError),
    #[error("failed to parse decimal value {value}: {source}")]
    Decimal {
        value: String,
        source: rust_decimal::Error,
    },
    #[error("parquet column {0} is missing")]
    MissingColumn(&'static str),
    #[error("parquet row {row} column {column} is null")]
    NullValue { row: usize, column: &'static str },
    #[error("unsupported data source {0}")]
    UnsupportedSource(String),
}

#[derive(Debug, Deserialize)]
struct CsvBar {
    ts_ms: i64,
    open: Decimal,
    high: Decimal,
    low: Decimal,
    close: Decimal,
    volume: Decimal,
}

pub fn load_bars_from_csv(path: impl AsRef<Path>) -> Result<Vec<Bar>, DataError> {
    let mut reader = csv::Reader::from_path(path)?;
    let mut bars = Vec::new();
    for row in reader.deserialize::<CsvBar>() {
        let row = row?;
        bars.push(Bar::new(
            row.ts_ms, row.open, row.high, row.low, row.close, row.volume,
        ));
    }
    Ok(bars)
}
