use std::path::Path;

use rust_decimal::Decimal;
use serde::Deserialize;
use thiserror::Error;

use crate::Bar;

#[derive(Debug, Error)]
pub enum DataError {
    #[error("failed to read csv: {0}")]
    Csv(#[from] csv::Error),
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
