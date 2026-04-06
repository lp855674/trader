// Strategy Context implementation with LruCache and memory buffer

use std::collections::HashMap;
use std::sync::Arc;

use super::trait_def::{
    CacheKey, CacheKeyType, CacheValue, DataSourceBundle, DataSourceError, Granularity,
    HistoricalData, InstrumentId, MemoryBuffer, Side, StrategyContext as BaseStrategyContext,
    StrategyLogger, Value,
};

/// StrategyContext with enhanced functionality
#[derive(Debug, Clone)]
pub struct StrategyContext {
    pub instrument: InstrumentId,
    pub instrument_db_id: i64,
    pub ts_ms: i64,
    pub last_bar_close: Option<f64>,
    pub last_bar_ts: Option<i64>,
    pub cache: LruCache<CacheKey, CacheValue>,
    pub memory: MemoryBuffer,
    pub parameters: HashMap<String, Value>,
    pub data_sources: Arc<DataSourceBundle>,
    pub logger: Arc<StrategyLogger>,
    pub sequence_number: u64,
}

impl StrategyContext {
    pub fn new(instrument: InstrumentId, ts_ms: i64) -> Self {
        Self {
            instrument,
            instrument_db_id: 0,
            ts_ms,
            last_bar_close: None,
            last_bar_ts: None,
            cache: LruCache::new(1000),
            memory: MemoryBuffer::new(1000),
            parameters: HashMap::new(),
            data_sources: Arc::new(DataSourceBundle::default()),
            logger: Arc::new(StrategyLogger::new(instrument.clone())),
            sequence_number: 0,
        }
    }

    /// Initialize with database ID
    pub fn init(&mut self, db_id: i64) {
        self.instrument_db_id = db_id;
    }

    /// Update with latest bar data
    pub fn update(&mut self, bar_close: Option<f64>, bar_ts: Option<i64>) {
        self.last_bar_close = bar_close;
        self.last_bar_ts = bar_ts;
    }

    /// Set strategy parameters
    pub fn set_params(&mut self, params: HashMap<String, Value>) {
        self.parameters = params;
    }

    /// Get parameter by name
    pub fn get_param(&self, name: &str) -> Option<&Value> {
        self.parameters.get(name)
    }

    /// Cache a computed value
    pub fn cache_result(&mut self, key: CacheKey, value: f64, confidence: f64) {
        self.cache.insert(
            key,
            CacheValue {
                value,
                timestamp: self.ts_ms,
                confidence,
            },
        );
    }

    /// Get cached value
    pub fn get_cached(&mut self, key: &CacheKey) -> Option<&f64> {
        let entry = self.cache.get(key)?;
        Some(&entry.value)
    }

    /// Clear cache for specific instrument
    pub fn clear_cache(&mut self) {
        self.cache.entries.clear();
    }

    /// Log strategy event
    pub fn log(&self, event: &str, context: &serde_json::Value) {
        self.logger.log(event, context);
    }

    /// Increment sequence number
    pub fn next_sequence(&mut self) -> u64 {
        self.sequence_number += 1;
        self.sequence_number
    }
}

/// LruCache implementation
pub struct LruCache<K, V> {
    entries: HashMap<K, Entry<V>>,
    capacity: usize,
    next_id: usize,
}

#[derive(Debug)]
struct Entry<V> {
    value: V,
    access_order: usize,
    is_valid: bool,
}

impl<K: Eq + Hash + Clone, V: Clone> LruCache<K, V> {
    pub fn new(capacity: usize) -> Self {
        Self {
            entries: HashMap::new(),
            capacity,
            next_id: 0,
        }
    }

    pub fn get(&mut self, key: &K) -> Option<&V> {
        if let Some(entry) = self.entries.get(key) {
            if entry.is_valid {
                if let Some((_, old_entry)) = self.entries.remove_entry(key) {
                    self.entries.insert(
                        key.clone(),
                        Entry {
                            value: old_entry.value.clone(),
                            access_order: self.next_id,
                            is_valid: true,
                        },
                    );
                    self.next_id += 1;
                    return Some(&entry.value);
                }
            }
        }
        None
    }

    pub fn insert(&mut self, key: K, value: V) {
        if self.entries.len() >= self.capacity {
            if let Some((oldest_key, _)) = self.entries.iter().min_by_key(|(_, e)| e.access_order) {
                self.entries.remove(oldest_key);
            }
        }

        self.entries.insert(
            key,
            Entry {
                value: value.clone(),
                access_order: self.next_id,
                is_valid: true,
            },
        );
        self.next_id += 1;
    }

    pub fn remove(&mut self, key: &K) -> Option<V> {
        self.entries.remove(key).map(|e| e.value)
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Memory buffer for historical data
pub struct MemoryBuffer {
    data: Vec<DataPoint>,
    max_size: usize,
}

#[derive(Debug, Clone)]
pub struct DataPoint {
    pub timestamp: i64,
    pub value: f64,
}

impl MemoryBuffer {
    pub fn new(max_size: usize) -> Self {
        Self {
            data: Vec::with_capacity(max_size),
            max_size,
        }
    }

    pub fn push(&mut self, timestamp: i64, value: f64) {
        self.data.push(DataPoint { timestamp, value });

        while self.data.len() > self.max_size {
            self.data.remove(0);
        }
    }

    pub fn get(&self, index: usize) -> Option<&DataPoint> {
        self.data.get(index)
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }

    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    pub fn get_last(&self) -> Option<&DataPoint> {
        self.data.last()
    }
}

/// Cache key and value
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct CacheKey {
    pub instrument: InstrumentId,
    pub key_type: CacheKeyType,
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub enum CacheKeyType {
    MovingAverage { period: u32 },
    Rsi { period: u32 },
    Bollinger { period: u32, std_dev: f64 },
    Custom(String),
}

#[derive(Debug, Clone)]
pub struct CacheValue {
    pub value: f64,
    pub timestamp: i64,
    pub confidence: f64,
}

impl Default for CacheValue {
    fn default() -> Self {
        Self {
            value: 0.0,
            timestamp: 0,
            confidence: 1.0,
        }
    }
}

/// Data source bundle
#[derive(Debug, Clone, Default)]
pub struct DataSourceBundle {
    pub kline_source: Option<Arc<dyn HistoricalData>>,
    pub tick_source: Option<Arc<dyn HistoricalData>>,
}

impl DataSourceBundle {
    pub fn with_kline(source: Arc<dyn HistoricalData>) -> Self {
        Self {
            kline_source: Some(source),
            tick_source: None,
        }
    }

    pub fn with_tick(source: Arc<dyn HistoricalData>) -> Self {
        Self {
            kline_source: None,
            tick_source: Some(source),
        }
    }

    pub fn has_kline(&self) -> bool {
        self.kline_source.is_some()
    }

    pub fn has_tick(&self) -> bool {
        self.tick_source.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_context_initialization() {
        let ctx = StrategyContext::new(
            InstrumentId::new(super::types::Venue::UsEquity, "AAPL"),
            1000,
        );
        assert_eq!(ctx.instrument_db_id, 0);
        assert!(ctx.last_bar_close.is_none());
    }

    #[test]
    fn test_cache_operations() {
        let mut cache = LruCache::new(3);
        cache.insert(1, 100.0);
        cache.insert(2, 200.0);
        cache.insert(3, 300.0);

        assert_eq!(cache.get(&1), Some(&100.0));
        assert_eq!(cache.len(), 3);
    }

    #[test]
    fn test_memory_buffer() {
        let mut buf = MemoryBuffer::new(10);
        buf.push(1000, 100.0);
        buf.push(2000, 200.0);

        assert_eq!(buf.len(), 2);
        assert_eq!(buf.get(0).unwrap().value, 100.0);
    }
}
