#![forbid(unsafe_code)]

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

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
