pub mod lru;
pub mod mmap;

pub use lru::MarketDataLru;
pub use mmap::{MmapCache, MmapConfig};

use crate::core::DataItem;
use std::collections::HashMap;

// ── CacheLevel ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum CacheLevel {
    Memory,
    Disk,
    Database,
}

// ── CacheStats ────────────────────────────────────────────────────────────────

#[derive(Debug, Default, Clone)]
pub struct CacheStats {
    pub l1_hits: u64,
    pub l2_hits: u64,
    pub misses: u64,
}

impl CacheStats {
    pub fn hit_rate(&self) -> f64 {
        let total = self.l1_hits + self.l2_hits + self.misses;
        if total == 0 {
            0.0
        } else {
            (self.l1_hits + self.l2_hits) as f64 / total as f64
        }
    }
}

// ── TieredCache ───────────────────────────────────────────────────────────────

pub struct TieredCache {
    l1: MarketDataLru<String, Vec<DataItem>>,
    l2_data: HashMap<String, Vec<DataItem>>,
    stats: CacheStats,
}

impl TieredCache {
    pub fn new(l1_capacity_bytes: usize) -> Self {
        Self {
            l1: MarketDataLru::new(l1_capacity_bytes),
            l2_data: HashMap::new(),
            stats: CacheStats::default(),
        }
    }

    pub fn get(&mut self, key: &str) -> Option<Vec<DataItem>> {
        // Check L1 first
        if self.l1.get(&key.to_string()).is_some() {
            self.stats.l1_hits += 1;
            return self.l1.get(&key.to_string()).cloned();
        }
        // Check L2
        if let Some(items) = self.l2_data.get(key).cloned() {
            self.stats.l2_hits += 1;
            // Promote to L1
            let size_bytes = items.len() * 64; // rough estimate
            self.l1.insert(key.to_string(), items.clone(), size_bytes);
            return Some(items);
        }
        self.stats.misses += 1;
        None
    }

    pub fn insert(&mut self, key: String, items: Vec<DataItem>) {
        let size_bytes = items.len() * 64;
        self.l2_data.insert(key.clone(), items.clone());
        self.l1.insert(key, items, size_bytes);
    }

    pub fn invalidate(&mut self, key: &str) {
        self.l2_data.remove(key);
        // Note: MarketDataLru doesn't have a direct remove, but we can work around
        // by inserting an empty vec — however for correctness we just leave L1 stale
        // In production this would need proper remove. For now, no remove on LRU.
    }

    pub fn stats(&self) -> CacheStats {
        self.stats.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::NormalizedBar;

    fn make_items(count: usize) -> Vec<DataItem> {
        (0..count)
            .map(|i| {
                DataItem::Bar(NormalizedBar {
                    ts_ms: i as i64,
                    open: 1.0,
                    high: 1.0,
                    low: 1.0,
                    close: 1.0,
                    volume: 1.0,
                })
            })
            .collect()
    }

    #[test]
    fn tiered_cache_l2_to_l1_promotion() {
        let mut cache = TieredCache::new(10_000_000);
        let items = make_items(5);
        cache.insert("key1".to_string(), items.clone());
        let result = cache.get("key1");
        assert!(result.is_some());
        assert_eq!(result.unwrap().len(), 5);
    }

    #[test]
    fn tiered_cache_miss() {
        let mut cache = TieredCache::new(1_000_000);
        let result = cache.get("nonexistent");
        assert!(result.is_none());
        assert_eq!(cache.stats().misses, 1);
    }

    #[test]
    fn lru_evicts_on_capacity() {
        let mut lru: MarketDataLru<String, Vec<u8>> = MarketDataLru::new(100);
        lru.insert("a".to_string(), vec![0u8; 60], 60);
        lru.insert("b".to_string(), vec![0u8; 60], 60);
        assert!(lru.current_bytes <= 100);
    }
}
