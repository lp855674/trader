use std::collections::HashMap;
use crate::core::{DataItem, DataQuery};

// ── Partition ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct Partition {
    pub instrument: String,
    pub start_ts_ms: i64,
    pub end_ts_ms: i64,
    pub items: Vec<DataItem>,
}

// ── PartitionedStorage ────────────────────────────────────────────────────────

pub struct PartitionedStorage {
    pub partitions: HashMap<String, Vec<Partition>>,
    pub partition_size_ms: u64,
}

impl PartitionedStorage {
    pub fn new(partition_size_ms: u64) -> Self {
        Self {
            partitions: HashMap::new(),
            partition_size_ms,
        }
    }

    pub fn insert(&mut self, instrument: &str, items: Vec<DataItem>) {
        let list = self.partitions.entry(instrument.to_string()).or_default();
        for item in items {
            let ts = item.ts_ms();
            let bucket = (ts as u64 / self.partition_size_ms * self.partition_size_ms) as i64;
            let bucket_end = bucket + self.partition_size_ms as i64;
            if let Some(p) = list.iter_mut().find(|p| p.start_ts_ms == bucket) {
                if ts < p.start_ts_ms {
                    p.start_ts_ms = ts;
                }
                if ts > p.end_ts_ms {
                    p.end_ts_ms = ts;
                }
                p.items.push(item);
            } else {
                list.push(Partition {
                    instrument: instrument.to_string(),
                    start_ts_ms: bucket,
                    end_ts_ms: bucket_end,
                    items: vec![item],
                });
            }
        }
    }

    pub fn query(&self, query: &DataQuery) -> Vec<DataItem> {
        let mut result = Vec::new();
        if let Some(partitions) = self.partitions.get(&query.instrument) {
            for partition in partitions {
                // Check if partition overlaps with query range
                if partition.end_ts_ms < query.start_ts_ms
                    || partition.start_ts_ms > query.end_ts_ms
                {
                    continue;
                }
                for item in &partition.items {
                    let ts = item.ts_ms();
                    if ts >= query.start_ts_ms && ts <= query.end_ts_ms {
                        result.push(item.clone());
                    }
                }
            }
        }
        result.sort_by_key(|i| i.ts_ms());
        result
    }

    pub fn partition_count(&self) -> usize {
        self.partitions.values().map(|v| v.len()).sum()
    }

    pub fn total_items(&self) -> usize {
        self.partitions
            .values()
            .flat_map(|v| v.iter())
            .map(|p| p.items.len())
            .sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::NormalizedBar;

    fn bar_item(ts_ms: i64) -> DataItem {
        DataItem::Bar(NormalizedBar {
            ts_ms,
            open: 1.0,
            high: 1.0,
            low: 1.0,
            close: 1.0,
            volume: 1.0,
        })
    }

    #[test]
    fn partitioned_storage_query() {
        let mut storage = PartitionedStorage::new(60_000); // 1-minute partitions
        let items: Vec<DataItem> = (0..10).map(|i| bar_item(i * 10_000)).collect();
        storage.insert("BTC", items);

        let q = DataQuery::new("BTC", 20_000, 50_000);
        let result = storage.query(&q);
        assert!(result.iter().all(|i| i.ts_ms() >= 20_000 && i.ts_ms() <= 50_000));
    }

    #[test]
    fn total_items_count() {
        let mut storage = PartitionedStorage::new(100_000);
        let items: Vec<DataItem> = (0..5).map(|i| bar_item(i * 1000)).collect();
        storage.insert("ETH", items);
        assert_eq!(storage.total_items(), 5);
    }
}
