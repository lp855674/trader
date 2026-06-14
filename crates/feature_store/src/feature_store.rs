#![forbid(unsafe_code)]

use polars::prelude::*;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
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

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct FeatureManifestInput {
    pub symbol: String,
    pub source: String,
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bar_count: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub first_ts_ms: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_ts_ms: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeatureBuildContract {
    pub builder: String,
    pub indicator: String,
    pub value_column: String,
    pub period: usize,
    pub run_id: String,
    pub feature_name: String,
    pub version: String,
    pub inputs: Vec<FeatureManifestInput>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FeatureBuildContractExpectation {
    pub indicator: Option<String>,
    pub value_column: Option<String>,
    pub period: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeatureManifest {
    pub schema_version: u32,
    pub parquet_path: String,
    pub record_count: usize,
    pub run_ids: Vec<String>,
    pub symbols: Vec<String>,
    pub feature_names: Vec<String>,
    pub versions: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub build_contract: Option<FeatureBuildContract>,
}

impl FeatureManifest {
    pub const CURRENT_SCHEMA_VERSION: u32 = 1;
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
    #[error("failed to read or write feature manifest json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("failed to parse decimal feature value {value}: {source}")]
    Decimal {
        value: String,
        source: rust_decimal::Error,
    },
    #[error("feature parquet column {0} is missing")]
    MissingColumn(&'static str),
    #[error("feature parquet row {row} column {column} is null")]
    NullValue { row: usize, column: &'static str },
    #[error("feature manifest schema_version {found} is unsupported; expected {expected}")]
    UnsupportedManifestSchema { found: u32, expected: u32 },
    #[error("feature manifest does not contain run_id {0}")]
    ManifestMissingRunId(String),
    #[error("feature manifest does not contain symbol {0}")]
    ManifestMissingSymbol(String),
    #[error("feature manifest does not contain feature_name {0}")]
    ManifestMissingFeatureName(String),
    #[error("feature manifest does not contain version {0}")]
    ManifestMissingVersion(String),
    #[error(
        "feature manifest parquet_path {found} does not match configured feature path {expected}"
    )]
    ManifestParquetPathMismatch { found: String, expected: String },
    #[error(
        "feature manifest build inputs do not match configured data inputs; expected {expected:?}, found {found:?}"
    )]
    ManifestBuildInputsMismatch {
        expected: Vec<FeatureManifestInput>,
        found: Vec<FeatureManifestInput>,
    },
    #[error(
        "feature manifest build input {field} does not match expected value {expected}; found {found}"
    )]
    ManifestBuildInputSnapshotMismatch {
        field: &'static str,
        expected: String,
        found: String,
    },
    #[error(
        "feature manifest build contract {field} does not match expected value {expected}; found {found}"
    )]
    ManifestBuildContractMismatch {
        field: &'static str,
        expected: String,
        found: String,
    },
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

pub fn build_feature_manifest(
    parquet_path: impl AsRef<Path>,
    records: &[FeatureRecord],
) -> FeatureManifest {
    let mut run_ids = BTreeSet::new();
    let mut symbols = BTreeSet::new();
    let mut feature_names = BTreeSet::new();
    let mut versions = BTreeSet::new();

    for record in records {
        run_ids.insert(record.key.run_id.clone());
        symbols.insert(record.key.symbol.clone());
        feature_names.insert(record.key.name.clone());
        versions.insert(record.version.clone());
    }

    FeatureManifest {
        schema_version: FeatureManifest::CURRENT_SCHEMA_VERSION,
        parquet_path: parquet_path.as_ref().display().to_string(),
        record_count: records.len(),
        run_ids: run_ids.into_iter().collect(),
        symbols: symbols.into_iter().collect(),
        feature_names: feature_names.into_iter().collect(),
        versions: versions.into_iter().collect(),
        build_contract: None,
    }
}

pub fn build_feature_manifest_with_contract(
    parquet_path: impl AsRef<Path>,
    records: &[FeatureRecord],
    build_contract: FeatureBuildContract,
) -> FeatureManifest {
    let mut manifest = build_feature_manifest(parquet_path, records);
    manifest.build_contract = Some(build_contract);
    manifest
}

pub fn write_feature_manifest(
    path: impl AsRef<Path>,
    manifest: &FeatureManifest,
) -> Result<(), FeatureStoreError> {
    let file = File::create(path)?;
    serde_json::to_writer_pretty(file, manifest)?;
    Ok(())
}

pub fn load_feature_manifest(path: impl AsRef<Path>) -> Result<FeatureManifest, FeatureStoreError> {
    let file = File::open(path)?;
    Ok(serde_json::from_reader(file)?)
}

pub fn validate_feature_manifest_for_gate(
    manifest: &FeatureManifest,
    expected_parquet_path: &str,
    run_id: &str,
    symbols: &[String],
    feature_name: &str,
    version: Option<&str>,
) -> Result<(), FeatureStoreError> {
    validate_feature_manifest_for_contract(
        manifest,
        expected_parquet_path,
        run_id,
        symbols,
        feature_name,
        version,
    )
}

pub fn validate_feature_manifest_for_contract(
    manifest: &FeatureManifest,
    expected_parquet_path: &str,
    run_id: &str,
    symbols: &[String],
    feature_name: &str,
    version: Option<&str>,
) -> Result<(), FeatureStoreError> {
    if manifest.schema_version != FeatureManifest::CURRENT_SCHEMA_VERSION {
        return Err(FeatureStoreError::UnsupportedManifestSchema {
            found: manifest.schema_version,
            expected: FeatureManifest::CURRENT_SCHEMA_VERSION,
        });
    }
    if normalize_path_string(&manifest.parquet_path) != normalize_path_string(expected_parquet_path)
    {
        return Err(FeatureStoreError::ManifestParquetPathMismatch {
            found: manifest.parquet_path.clone(),
            expected: expected_parquet_path.to_string(),
        });
    }
    if !contains_value(&manifest.run_ids, run_id) {
        return Err(FeatureStoreError::ManifestMissingRunId(run_id.to_string()));
    }
    for symbol in symbols {
        if !contains_value(&manifest.symbols, symbol) {
            return Err(FeatureStoreError::ManifestMissingSymbol(symbol.clone()));
        }
    }
    if !contains_value(&manifest.feature_names, feature_name) {
        return Err(FeatureStoreError::ManifestMissingFeatureName(
            feature_name.to_string(),
        ));
    }
    if let Some(version) = version
        && !contains_value(&manifest.versions, version)
    {
        return Err(FeatureStoreError::ManifestMissingVersion(
            version.to_string(),
        ));
    }
    Ok(())
}

pub fn validate_feature_manifest_for_input_contract(
    manifest: &FeatureManifest,
    expected_inputs: &[FeatureManifestInput],
) -> Result<(), FeatureStoreError> {
    let Some(build_contract) = &manifest.build_contract else {
        return Ok(());
    };
    let expected = normalized_manifest_inputs(expected_inputs);
    let found = normalized_manifest_inputs(&build_contract.inputs);
    if input_contract_keys(&found) != input_contract_keys(&expected) {
        return Err(FeatureStoreError::ManifestBuildInputsMismatch { expected, found });
    }
    for found_input in &found {
        let Some(expected_input) = expected
            .iter()
            .find(|input| input.same_source_as(found_input))
        else {
            return Err(FeatureStoreError::ManifestBuildInputsMismatch { expected, found });
        };
        validate_optional_input_snapshot(
            "content_hash",
            found_input.content_hash.as_deref(),
            expected_input.content_hash.as_deref(),
        )?;
        validate_optional_input_snapshot(
            "bar_count",
            found_input
                .bar_count
                .map(|value| value.to_string())
                .as_deref(),
            expected_input
                .bar_count
                .map(|value| value.to_string())
                .as_deref(),
        )?;
        validate_optional_input_snapshot(
            "first_ts_ms",
            found_input
                .first_ts_ms
                .map(|value| value.to_string())
                .as_deref(),
            expected_input
                .first_ts_ms
                .map(|value| value.to_string())
                .as_deref(),
        )?;
        validate_optional_input_snapshot(
            "last_ts_ms",
            found_input
                .last_ts_ms
                .map(|value| value.to_string())
                .as_deref(),
            expected_input
                .last_ts_ms
                .map(|value| value.to_string())
                .as_deref(),
        )?;
    }
    Ok(())
}

pub fn validate_feature_manifest_for_build_contract(
    manifest: &FeatureManifest,
    expectation: &FeatureBuildContractExpectation,
) -> Result<(), FeatureStoreError> {
    let Some(build_contract) = &manifest.build_contract else {
        return Ok(());
    };
    if let Some(expected) = &expectation.indicator
        && build_contract.indicator != *expected
    {
        return Err(FeatureStoreError::ManifestBuildContractMismatch {
            field: "indicator",
            expected: expected.clone(),
            found: build_contract.indicator.clone(),
        });
    }
    if let Some(expected) = &expectation.value_column
        && build_contract.value_column != *expected
    {
        return Err(FeatureStoreError::ManifestBuildContractMismatch {
            field: "value_column",
            expected: expected.clone(),
            found: build_contract.value_column.clone(),
        });
    }
    if let Some(expected) = expectation.period
        && build_contract.period != expected
    {
        return Err(FeatureStoreError::ManifestBuildContractMismatch {
            field: "period",
            expected: expected.to_string(),
            found: build_contract.period.to_string(),
        });
    }
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

fn contains_value(values: &[String], expected: &str) -> bool {
    values.iter().any(|value| value == expected)
}

fn normalize_path_string(path: &str) -> String {
    path.replace('\\', "/")
}

impl FeatureManifestInput {
    fn same_source_as(&self, other: &Self) -> bool {
        self.symbol == other.symbol && self.source == other.source && self.path == other.path
    }
}

fn normalized_manifest_inputs(inputs: &[FeatureManifestInput]) -> Vec<FeatureManifestInput> {
    let mut normalized = inputs
        .iter()
        .map(|input| FeatureManifestInput {
            symbol: input.symbol.clone(),
            source: input.source.clone(),
            path: normalize_path_string(&input.path),
            content_hash: input.content_hash.clone(),
            bar_count: input.bar_count,
            first_ts_ms: input.first_ts_ms,
            last_ts_ms: input.last_ts_ms,
        })
        .collect::<Vec<_>>();
    normalized.sort();
    normalized
}

fn input_contract_keys(inputs: &[FeatureManifestInput]) -> Vec<FeatureManifestInput> {
    inputs
        .iter()
        .map(|input| FeatureManifestInput {
            symbol: input.symbol.clone(),
            source: input.source.clone(),
            path: input.path.clone(),
            content_hash: None,
            bar_count: None,
            first_ts_ms: None,
            last_ts_ms: None,
        })
        .collect()
}

fn validate_optional_input_snapshot(
    field: &'static str,
    found: Option<&str>,
    expected: Option<&str>,
) -> Result<(), FeatureStoreError> {
    if let Some(found) = found
        && Some(found) != expected
    {
        return Err(FeatureStoreError::ManifestBuildInputSnapshotMismatch {
            field,
            expected: expected.unwrap_or("<missing>").to_string(),
            found: found.to_string(),
        });
    }
    Ok(())
}
