use std::collections::HashMap;

// ── LruEntry ──────────────────────────────────────────────────────────────────

struct LruEntry<V> {
    value: V,
    size_bytes: usize,
    access_order: u64,
}

// ── MarketDataLru ─────────────────────────────────────────────────────────────

pub struct MarketDataLru<K, V> {
    pub capacity_bytes: usize,
    pub current_bytes: usize,
    entries: HashMap<K, LruEntry<V>>,
    counter: u64,
    hits: u64,
    misses: u64,
}

impl<K, V> MarketDataLru<K, V>
where
    K: std::hash::Hash + Eq + Clone,
{
    pub fn new(capacity_bytes: usize) -> Self {
        Self {
            capacity_bytes,
            current_bytes: 0,
            entries: HashMap::new(),
            counter: 0,
            hits: 0,
            misses: 0,
        }
    }

    pub fn get(&mut self, key: &K) -> Option<&V> {
        if let Some(entry) = self.entries.get_mut(key) {
            self.counter += 1;
            entry.access_order = self.counter;
            self.hits += 1;
            // Safety: we know entries exists
            Some(&self.entries[key].value)
        } else {
            self.misses += 1;
            None
        }
    }

    pub fn insert(&mut self, key: K, value: V, size_bytes: usize) {
        // If key already exists, remove old size
        if let Some(old) = self.entries.remove(&key) {
            self.current_bytes -= old.size_bytes;
        }
        self.counter += 1;
        self.current_bytes += size_bytes;
        self.entries.insert(
            key,
            LruEntry {
                value,
                size_bytes,
                access_order: self.counter,
            },
        );
        // Evict if over capacity
        while self.current_bytes > self.capacity_bytes && !self.entries.is_empty() {
            self.evict_lru();
        }
    }

    pub fn evict_lru(&mut self) {
        if self.entries.is_empty() {
            return;
        }
        // Find the key with lowest access_order
        let lru_key = self
            .entries
            .iter()
            .min_by_key(|(_, e)| e.access_order)
            .map(|(k, _)| k.clone())
            .unwrap();
        if let Some(evicted) = self.entries.remove(&lru_key) {
            self.current_bytes -= evicted.size_bytes;
        }
    }

    pub fn hit_rate(&self) -> f64 {
        let total = self.hits + self.misses;
        if total == 0 {
            0.0
        } else {
            self.hits as f64 / total as f64
        }
    }

    pub fn memory_pressure(&self) -> f64 {
        if self.capacity_bytes == 0 {
            0.0
        } else {
            self.current_bytes as f64 / self.capacity_bytes as f64
        }
    }

    pub fn shrink(&mut self, target_bytes: usize) {
        while self.current_bytes > target_bytes && !self.entries.is_empty() {
            self.evict_lru();
        }
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evicts_on_capacity() {
        let mut lru: MarketDataLru<String, Vec<u8>> = MarketDataLru::new(100);
        lru.insert("a".to_string(), vec![0u8; 60], 60);
        lru.insert("b".to_string(), vec![0u8; 60], 60);
        // Inserting "b" should evict "a"
        assert!(lru.len() <= 2);
        assert!(lru.current_bytes <= 100);
    }

    #[test]
    fn hit_rate_tracks_correctly() {
        let mut lru: MarketDataLru<String, i32> = MarketDataLru::new(1000);
        lru.insert("key".to_string(), 42, 10);
        lru.get(&"key".to_string());
        lru.get(&"missing".to_string());
        assert!((lru.hit_rate() - 0.5).abs() < 1e-9);
    }

    #[test]
    fn memory_pressure() {
        let mut lru: MarketDataLru<String, i32> = MarketDataLru::new(100);
        lru.insert("key".to_string(), 1, 50);
        assert!((lru.memory_pressure() - 0.5).abs() < 1e-9);
    }

    #[test]
    fn shrink_works() {
        let mut lru: MarketDataLru<String, i32> = MarketDataLru::new(1000);
        lru.insert("a".to_string(), 1, 100);
        lru.insert("b".to_string(), 2, 100);
        lru.insert("c".to_string(), 3, 100);
        lru.shrink(150);
        assert!(lru.current_bytes <= 150);
    }
}
