#![forbid(unsafe_code)]

use polars::prelude::*;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs::File,
    path::{Path, PathBuf},
    str::FromStr,
};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct FeatureKey {
    pub run_id: String,
    pub symbol: String,
    pub name: String,
}

impl FeatureKey {
    pub fn new(
        run_id: impl Into<String>,
        symbol: impl Into<String>,
        name: impl Into<String>,
    ) -> Self {
        Self {
            run_id: run_id.into(),
            symbol: symbol.into(),
            name: name.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeatureRecord {
    pub key: FeatureKey,
    pub ts_ms: i64,
    pub value: Decimal,
    pub version: String,
}

impl FeatureRecord {
    pub fn new(
        run_id: impl Into<String>,
        symbol: impl Into<String>,
        ts_ms: i64,
        name: impl Into<String>,
        value: Decimal,
        version: impl Into<String>,
    ) -> Self {
        Self {
            key: FeatureKey::new(run_id, symbol, name),
            ts_ms,
            value,
            version: version.into(),
        }
    }
}

pub trait FeatureStore {
    fn insert(&mut self, record: FeatureRecord);
    fn latest(&self, key: &FeatureKey) -> Option<&FeatureRecord>;
    fn range(&self, key: &FeatureKey, start_ts_ms: i64, end_ts_ms: i64) -> Vec<&FeatureRecord>;
}

#[derive(Debug, Default, Clone)]
pub struct InMemoryFeatureStore {
    records: BTreeMap<FeatureKey, BTreeMap<i64, FeatureRecord>>,
}

impl FeatureStore for InMemoryFeatureStore {
    fn insert(&mut self, record: FeatureRecord) {
        self.records
            .entry(record.key.clone())
            .or_default()
            .insert(record.ts_ms, record);
    }

    fn latest(&self, key: &FeatureKey) -> Option<&FeatureRecord> {
        self.records
            .get(key)?
            .last_key_value()
            .map(|(_, record)| record)
    }

    fn range(&self, key: &FeatureKey, start_ts_ms: i64, end_ts_ms: i64) -> Vec<&FeatureRecord> {
        self.records
            .get(key)
            .into_iter()
            .flat_map(|records| records.range(start_ts_ms..=end_ts_ms))
            .map(|(_, record)| record)
            .collect()
    }
}

impl InMemoryFeatureStore {
    pub fn records(&self) -> Vec<&FeatureRecord> {
        self.records
            .values()
            .flat_map(BTreeMap::values)
            .collect::<Vec<_>>()
    }
}

#[derive(Debug, Error)]
pub enum FeatureStoreError {
    #[error("failed to access feature parquet file: {0}")]
    Io(#[from] std::io::Error),
    #[error("failed to read or write feature parquet file: {0}")]
    Parquet(#[from] polars::error::PolarsError),
    #[error("failed to parse decimal feature value {value}: {source}")]
    Decimal {
        value: String,
        source: rust_decimal::Error,
    },
    #[error("feature parquet column {0} is missing")]
    MissingColumn(&'static str),
    #[error("feature parquet row {row} column {column} is null")]
    NullValue { row: usize, column: &'static str },
}

#[derive(Debug, Clone)]
pub struct ParquetFeatureStore {
    path: PathBuf,
    inner: InMemoryFeatureStore,
}

impl ParquetFeatureStore {
    pub fn create(path: impl AsRef<Path>) -> Self {
        Self {
            path: path.as_ref().to_path_buf(),
            inner: InMemoryFeatureStore::default(),
        }
    }

    pub fn open(path: impl AsRef<Path>) -> Result<Self, FeatureStoreError> {
        let records = load_feature_records_from_parquet(&path)?;
        let mut inner = InMemoryFeatureStore::default();
        for record in records {
            inner.insert(record);
        }
        Ok(Self {
            path: path.as_ref().to_path_buf(),
            inner,
        })
    }

    pub fn flush(&self) -> Result<(), FeatureStoreError> {
        let records = self
            .inner
            .records()
            .into_iter()
            .cloned()
            .collect::<Vec<_>>();
        write_feature_records_to_parquet(&self.path, &records)
    }
}

impl FeatureStore for ParquetFeatureStore {
    fn insert(&mut self, record: FeatureRecord) {
        self.inner.insert(record);
    }

    fn latest(&self, key: &FeatureKey) -> Option<&FeatureRecord> {
        self.inner.latest(key)
    }

    fn range(&self, key: &FeatureKey, start_ts_ms: i64, end_ts_ms: i64) -> Vec<&FeatureRecord> {
        self.inner.range(key, start_ts_ms, end_ts_ms)
    }
}

pub fn write_feature_records_to_parquet(
    path: impl AsRef<Path>,
    records: &[FeatureRecord],
) -> Result<(), FeatureStoreError> {
    let run_id = records
        .iter()
        .map(|record| record.key.run_id.as_str())
        .collect::<Vec<_>>();
    let symbol = records
        .iter()
        .map(|record| record.key.symbol.as_str())
        .collect::<Vec<_>>();
    let name = records
        .iter()
        .map(|record| record.key.name.as_str())
        .collect::<Vec<_>>();
    let ts_ms = records
        .iter()
        .map(|record| record.ts_ms)
        .collect::<Vec<_>>();
    let value = records
        .iter()
        .map(|record| record.value.to_string())
        .collect::<Vec<_>>();
    let version = records
        .iter()
        .map(|record| record.version.as_str())
        .collect::<Vec<_>>();

    let mut dataframe = DataFrame::new(vec![
        Series::new("run_id".into(), run_id).into(),
        Series::new("symbol".into(), symbol).into(),
        Series::new("name".into(), name).into(),
        Series::new("ts_ms".into(), ts_ms).into(),
        Series::new("value".into(), value).into(),
        Series::new("version".into(), version).into(),
    ])?;
    let file = File::create(path)?;
    ParquetWriter::new(file).finish(&mut dataframe)?;
    Ok(())
}

pub fn load_feature_records_from_parquet(
    path: impl AsRef<Path>,
) -> Result<Vec<FeatureRecord>, FeatureStoreError> {
    let file = File::open(path)?;
    let dataframe = ParquetReader::new(file).finish()?;
    let run_id = string_column(&dataframe, "run_id")?;
    let symbol = string_column(&dataframe, "symbol")?;
    let name = string_column(&dataframe, "name")?;
    let ts_ms = dataframe
        .column("ts_ms")
        .map_err(|_| FeatureStoreError::MissingColumn("ts_ms"))?
        .i64()?;
    let value = string_column(&dataframe, "value")?;
    let version = string_column(&dataframe, "version")?;

    let mut records = Vec::with_capacity(dataframe.height());
    for row in 0..dataframe.height() {
        records.push(FeatureRecord::new(
            parse_string(run_id.get(row), row, "run_id")?,
            parse_string(symbol.get(row), row, "symbol")?,
            ts_ms.get(row).ok_or(FeatureStoreError::NullValue {
                row,
                column: "ts_ms",
            })?,
            parse_string(name.get(row), row, "name")?,
            parse_decimal(value.get(row), row, "value")?,
            parse_string(version.get(row), row, "version")?,
        ));
    }
    Ok(records)
}

fn string_column<'a>(
    dataframe: &'a DataFrame,
    column: &'static str,
) -> Result<&'a StringChunked, FeatureStoreError> {
    Ok(dataframe
        .column(column)
        .map_err(|_| FeatureStoreError::MissingColumn(column))?
        .str()?)
}

fn parse_string(
    value: Option<&str>,
    row: usize,
    column: &'static str,
) -> Result<String, FeatureStoreError> {
    Ok(value
        .ok_or(FeatureStoreError::NullValue { row, column })?
        .to_string())
}

fn parse_decimal(
    value: Option<&str>,
    row: usize,
    column: &'static str,
) -> Result<Decimal, FeatureStoreError> {
    let value = value.ok_or(FeatureStoreError::NullValue { row, column })?;
    Decimal::from_str(value).map_err(|source| FeatureStoreError::Decimal {
        value: value.to_string(),
        source,
    })
}
