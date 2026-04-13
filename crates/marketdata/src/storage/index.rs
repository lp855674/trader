use std::collections::HashMap;

// ── IndexEntry ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct IndexEntry {
    pub ts_ms: i64,
    pub instrument: String,
    pub partition_id: usize,
}

// ── StorageIndex ──────────────────────────────────────────────────────────────

#[derive(Default)]
pub struct StorageIndex {
    pub by_instrument: HashMap<String, Vec<IndexEntry>>,
}

impl StorageIndex {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn index(&mut self, entry: IndexEntry) {
        self.by_instrument
            .entry(entry.instrument.clone())
            .or_default()
            .push(entry);
    }

    pub fn lookup(&self, instrument: &str, start_ms: i64, end_ms: i64) -> Vec<&IndexEntry> {
        self.by_instrument
            .get(instrument)
            .map(|entries| {
                entries
                    .iter()
                    .filter(|e| e.ts_ms >= start_ms && e.ts_ms <= end_ms)
                    .collect()
            })
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn index_and_lookup() {
        let mut idx = StorageIndex::new();
        idx.index(IndexEntry {
            ts_ms: 1000,
            instrument: "BTC".to_string(),
            partition_id: 0,
        });
        idx.index(IndexEntry {
            ts_ms: 2000,
            instrument: "BTC".to_string(),
            partition_id: 0,
        });
        idx.index(IndexEntry {
            ts_ms: 5000,
            instrument: "BTC".to_string(),
            partition_id: 1,
        });

        let result = idx.lookup("BTC", 1000, 3000);
        assert_eq!(result.len(), 2);

        let result2 = idx.lookup("ETH", 0, 9999);
        assert_eq!(result2.len(), 0);
    }
}
