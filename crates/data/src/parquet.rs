use std::{fs::File, path::Path, str::FromStr};

use polars::prelude::*;
use rust_decimal::Decimal;

use crate::{Bar, DataError};

pub fn write_bars_to_parquet(path: impl AsRef<Path>, bars: &[Bar]) -> Result<(), DataError> {
    let ts_ms = bars.iter().map(|bar| bar.ts_ms).collect::<Vec<_>>();
    let open = bars
        .iter()
        .map(|bar| bar.open.to_string())
        .collect::<Vec<_>>();
    let high = bars
        .iter()
        .map(|bar| bar.high.to_string())
        .collect::<Vec<_>>();
    let low = bars
        .iter()
        .map(|bar| bar.low.to_string())
        .collect::<Vec<_>>();
    let close = bars
        .iter()
        .map(|bar| bar.close.to_string())
        .collect::<Vec<_>>();
    let volume = bars
        .iter()
        .map(|bar| bar.volume.to_string())
        .collect::<Vec<_>>();

    let mut dataframe = DataFrame::new(vec![
        Series::new("ts_ms".into(), ts_ms).into(),
        Series::new("open".into(), open).into(),
        Series::new("high".into(), high).into(),
        Series::new("low".into(), low).into(),
        Series::new("close".into(), close).into(),
        Series::new("volume".into(), volume).into(),
    ])?;
    let file = File::create(path)?;
    ParquetWriter::new(file).finish(&mut dataframe)?;
    Ok(())
}

pub fn load_bars_from_parquet(path: impl AsRef<Path>) -> Result<Vec<Bar>, DataError> {
    let file = File::open(path)?;
    let dataframe = ParquetReader::new(file).finish()?;
    let ts_ms = dataframe
        .column("ts_ms")
        .map_err(|_| DataError::MissingColumn("ts_ms"))?
        .i64()?;
    let open = dataframe
        .column("open")
        .map_err(|_| DataError::MissingColumn("open"))?
        .str()?;
    let high = dataframe
        .column("high")
        .map_err(|_| DataError::MissingColumn("high"))?
        .str()?;
    let low = dataframe
        .column("low")
        .map_err(|_| DataError::MissingColumn("low"))?
        .str()?;
    let close = dataframe
        .column("close")
        .map_err(|_| DataError::MissingColumn("close"))?
        .str()?;
    let volume = dataframe
        .column("volume")
        .map_err(|_| DataError::MissingColumn("volume"))?
        .str()?;

    let mut bars = Vec::with_capacity(dataframe.height());
    for row in 0..dataframe.height() {
        bars.push(Bar::new(
            ts_ms.get(row).ok_or(DataError::NullValue {
                row,
                column: "ts_ms",
            })?,
            parse_decimal(open.get(row), row, "open")?,
            parse_decimal(high.get(row), row, "high")?,
            parse_decimal(low.get(row), row, "low")?,
            parse_decimal(close.get(row), row, "close")?,
            parse_decimal(volume.get(row), row, "volume")?,
        ));
    }
    Ok(bars)
}

fn parse_decimal(
    value: Option<&str>,
    row: usize,
    column: &'static str,
) -> Result<Decimal, DataError> {
    let value = value.ok_or(DataError::NullValue { row, column })?;
    Decimal::from_str(value).map_err(|source| DataError::Decimal {
        value: value.to_string(),
        source,
    })
}
