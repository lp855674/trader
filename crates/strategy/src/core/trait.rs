// Core trait definitions for strategy system
// Pure functional, deterministic strategy evaluation

use domain::{InstrumentId, Side};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::hash::Hash;
use std::sync::Arc;
use thiserror::Error;

/// Strategy evaluation error
#[derive(Debug, Error)]
pub enum StrategyError {
    #[error("Invalid instrument ID: {0}")]
    InvalidInstrument(String),

    #[error("Cache miss for key: {0}")]
    CacheMiss(String),

    #[error("Data source error: {0}")]
    DataSource(String),
}

/// Strategy trait - pure function, deterministic behavior
///
/// Strategies must be pure functions: given the same context, they always
/// return the same Signal (or None). No side effects, no randomness.
pub trait Strategy: Send + Sync {
    /// Evaluate strategy and return signal if any
    ///
    /// # Arguments
    /// * `context` - StrategyContext containing instrument, timestamp, and data sources
    ///
    /// # Returns
    /// * `Option<Signal>` - None means no signal, Some means execute this signal
    fn evaluate(&self, context: &StrategyContext) -> Result<Option<Signal>, StrategyError>;

    /// Get strategy name
    fn name(&self) -> &str;

    /// Get strategy version
    fn version(&self) -> u32 {
        1
    }
}

/// Signal types和序列化（与 `domain::Signal` 独立，供扩展策略上下文使用）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Signal {
    pub instrument: InstrumentId,
    pub side: Side,
    pub quantity: f64,
    pub limit_price: Option<f64>,
    pub timestamp_ms: i64,
    pub strategy_id: String,
    pub params: HashMap<String, serde_json::Value>,
}

impl Signal {
    pub fn new(
        instrument: InstrumentId,
        side: Side,
        quantity: f64,
        limit_price: Option<f64>,
        timestamp_ms: i64,
        strategy_id: String,
        params: HashMap<String, serde_json::Value>,
    ) -> Self {
        Self {
            instrument,
            side,
            quantity,
            limit_price,
            timestamp_ms,
            strategy_id,
            params,
        }
    }

    pub fn is_market_order(&self) -> bool {
        self.limit_price.is_none()
    }
}

/// 带缓存的策略上下文（与 `strategy::StrategyContext` 独立）
#[derive(Clone)]
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
}

impl StrategyContext {
    pub fn new(instrument: InstrumentId, ts_ms: i64) -> Self {
        let logger = Arc::new(StrategyLogger::new(instrument.clone()));
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
            logger,
        }
    }

    pub fn update(&mut self, bar_close: Option<f64>, bar_ts: Option<i64>) {
        self.last_bar_close = bar_close;
        self.last_bar_ts = bar_ts;
    }

    pub fn set_db_id(&mut self, db_id: i64) {
        self.instrument_db_id = db_id;
    }
}

/// Cache key and value for LruCache
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct CacheKey {
    pub instrument: InstrumentId,
    pub key_type: CacheKeyType,
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub enum CacheKeyType {
    MovingAverage {
        period: u32,
    },
    Rsi {
        period: u32,
    },
    /// `std_dev` 的 `f64::to_bits()`，用于 `Hash` / `Eq`
    Bollinger {
        period: u32,
        std_dev_bits: u64,
    },
    Custom(String),
}

#[derive(Debug, Clone)]
pub struct CacheValue {
    pub value: f64,
    pub timestamp: i64,
    pub confidence: f64, // 0.0-1.0
}

/// 简单 LRU（按访问序驱逐）
pub struct LruCache<K, V: Clone> {
    entries: HashMap<K, Entry<V>>,
    capacity: usize,
    next_id: usize,
}

#[derive(Debug, Clone)]
struct Entry<V: Clone> {
    value: V,
    access_order: usize,
    is_valid: bool,
}

impl<K: Eq + Hash + Clone, V: Clone> Clone for LruCache<K, V> {
    fn clone(&self) -> Self {
        Self {
            entries: self.entries.clone(),
            capacity: self.capacity,
            next_id: self.next_id,
        }
    }
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
        let valid = self.entries.get(key).is_some_and(|entry| entry.is_valid);
        if !valid {
            return None;
        }
        let old_entry = self.entries.remove(key)?;
        let value = old_entry.value.clone();
        let order = self.next_id;
        self.next_id += 1;
        self.entries.insert(
            key.clone(),
            Entry {
                value,
                access_order: order,
                is_valid: true,
            },
        );
        self.entries.get(key).map(|entry| &entry.value)
    }

    pub fn insert(&mut self, key: K, value: V) {
        if self.entries.len() >= self.capacity && !self.entries.contains_key(&key) {
            if let Some((oldest_key, _)) = self
                .entries
                .iter()
                .min_by_key(|(_, entry)| entry.access_order)
            {
                let oldest_key = oldest_key.clone();
                self.entries.remove(&oldest_key);
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

/// 内存环形历史缓冲
#[derive(Debug, Clone)]
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

        // Trim if over capacity
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
}

/// 策略参数值
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Value {
    Number(f64),
    String(String),
    Boolean(bool),
}

impl Default for Value {
    fn default() -> Self {
        Value::Number(0.0)
    }
}

impl Value {
    pub fn as_f64(&self) -> f64 {
        match self {
            Value::Number(n) => *n,
            Value::String(s) => s.parse().unwrap_or(0.0),
            Value::Boolean(b) => {
                if *b {
                    1.0
                } else {
                    0.0
                }
            }
        }
    }
}

/// InputSource traits for data access
#[async_trait::async_trait]
pub trait HistoricalData: Send + Sync + 'static {
    /// Get kline data
    async fn get_klines(
        &self,
        instrument: InstrumentId,
        start_ts: i64,
        end_ts: i64,
        granularity: Granularity,
    ) -> Result<Vec<Kline>, DataSourceError>;

    /// Get tick data
    async fn get_ticks(
        &self,
        instrument: InstrumentId,
        start_ts: i64,
        end_ts: i64,
    ) -> Result<Vec<Tick>, DataSourceError>;
}

#[derive(Debug, Clone)]
pub struct Kline {
    pub instrument: InstrumentId,
    pub open_ts_ms: i64,
    pub close_ts_ms: i64,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: f64,
}

#[derive(Debug, Clone)]
pub struct Tick {
    pub instrument: InstrumentId,
    pub ts_ms: i64,
    pub bid_price: f64,
    pub ask_price: f64,
    pub last_price: f64,
    pub volume: f64,
}

#[derive(Debug, Clone)]
pub struct DataSourceError {
    pub source: String,
    pub message: String,
}

impl std::fmt::Display for DataSourceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "DataSourceError[{}]: {}", self.source, self.message)
    }
}

impl std::error::Error for DataSourceError {}

/// Granularity types for time resolution
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Granularity {
    Tick,
    Minute(u32), // 1m, 5m, 15m, etc.
    Hour(u32),   // 1h, 4h, etc.
    Day,
}

impl Default for Granularity {
    fn default() -> Self {
        Granularity::Minute(1)
    }
}

/// Strategy logger for tracing
#[derive(Debug, Clone)]
pub struct StrategyLogger {
    instrument: InstrumentId,
    strategy_id: String,
}

impl StrategyLogger {
    pub fn new(instrument: InstrumentId) -> Self {
        Self {
            instrument,
            strategy_id: "unknown".to_string(),
        }
    }

    pub fn set_strategy_id(&mut self, id: String) {
        self.strategy_id = id;
    }

    pub fn log(&self, event: &str, context: &serde_json::Value) {
        // In production, use tracing::info!
        tracing::info!(
            instrument = %self.instrument,
            strategy = %self.strategy_id,
            event = event,
            context = ?context,
            "strategy_log"
        );
    }
}

/// 数据源集合
#[derive(Clone, Default)]
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
}
