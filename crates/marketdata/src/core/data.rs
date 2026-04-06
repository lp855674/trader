use std::collections::HashMap;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use domain::NormalizedBar;

// ── DataItem ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DataItem {
    Bar(NormalizedBar),
    Tick {
        ts_ms: i64,
        bid: f64,
        ask: f64,
        last: f64,
        volume: f64,
    },
    OrderBook {
        ts_ms: i64,
        bids: Vec<(f64, f64)>,
        asks: Vec<(f64, f64)>,
    },
}

impl DataItem {
    pub fn ts_ms(&self) -> i64 {
        match self {
            DataItem::Bar(b) => b.ts_ms,
            DataItem::Tick { ts_ms, .. } => *ts_ms,
            DataItem::OrderBook { ts_ms, .. } => *ts_ms,
        }
    }

    pub fn instrument_hint(&self) -> Option<&str> {
        None
    }
}

// ── Granularity ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Granularity {
    Tick,
    Seconds(u32),
    Minutes(u32),
    Hours(u32),
    Days(u32),
}

impl Granularity {
    pub fn to_ms(&self) -> Option<u64> {
        match self {
            Granularity::Tick => None,
            Granularity::Seconds(n) => Some(*n as u64 * 1_000),
            Granularity::Minutes(n) => Some(*n as u64 * 60_000),
            Granularity::Hours(n) => Some(*n as u64 * 3_600_000),
            Granularity::Days(n) => Some(*n as u64 * 86_400_000),
        }
    }

    pub fn from_ms(ms: u64) -> Self {
        if ms == 0 {
            return Granularity::Tick;
        }
        if ms % 86_400_000 == 0 {
            Granularity::Days((ms / 86_400_000) as u32)
        } else if ms % 3_600_000 == 0 {
            Granularity::Hours((ms / 3_600_000) as u32)
        } else if ms % 60_000 == 0 {
            Granularity::Minutes((ms / 60_000) as u32)
        } else if ms % 1_000 == 0 {
            Granularity::Seconds((ms / 1_000) as u32)
        } else {
            Granularity::Seconds(1)
        }
    }
}

// ── DataQuery ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct DataQuery {
    pub instrument: String,
    pub start_ts_ms: i64,
    pub end_ts_ms: i64,
    pub granularity: Granularity,
    pub limit: Option<usize>,
}

impl DataQuery {
    pub fn new(instrument: &str, start: i64, end: i64) -> Self {
        Self {
            instrument: instrument.to_string(),
            start_ts_ms: start,
            end_ts_ms: end,
            granularity: Granularity::Minutes(1),
            limit: None,
        }
    }

    pub fn with_granularity(mut self, g: Granularity) -> Self {
        self.granularity = g;
        self
    }

    pub fn with_limit(mut self, limit: usize) -> Self {
        self.limit = Some(limit);
        self
    }
}

// ── DataSourceError ───────────────────────────────────────────────────────────

#[derive(Debug, Error)]
pub enum DataSourceError {
    #[error("IO error: {0}")]
    Io(String),

    #[error("Parse error: {0}")]
    Parse(String),

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Rate limited")]
    RateLimited,
}

// ── DataSource trait ──────────────────────────────────────────────────────────

pub trait DataSource: Send + Sync {
    fn name(&self) -> &str;
    fn query(&self, query: &DataQuery) -> Result<Vec<DataItem>, DataSourceError>;
    fn supports_granularity(&self, g: &Granularity) -> bool;
}

// ── InMemoryDataSource ────────────────────────────────────────────────────────

pub struct InMemoryDataSource {
    name: String,
    items: Vec<DataItem>,
}

impl InMemoryDataSource {
    pub fn new(name: &str, items: Vec<DataItem>) -> Self {
        Self {
            name: name.to_string(),
            items,
        }
    }
}

impl DataSource for InMemoryDataSource {
    fn name(&self) -> &str {
        &self.name
    }

    fn query(&self, query: &DataQuery) -> Result<Vec<DataItem>, DataSourceError> {
        let mut result: Vec<DataItem> = self
            .items
            .iter()
            .filter(|item| {
                let ts = item.ts_ms();
                ts >= query.start_ts_ms && ts <= query.end_ts_ms
            })
            .cloned()
            .collect();

        if let Some(limit) = query.limit {
            result.truncate(limit);
        }

        Ok(result)
    }

    fn supports_granularity(&self, _g: &Granularity) -> bool {
        true
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_bar(ts_ms: i64) -> DataItem {
        DataItem::Bar(NormalizedBar {
            ts_ms,
            open: 1.0,
            high: 2.0,
            low: 0.5,
            close: 1.5,
            volume: 100.0,
        })
    }

    #[test]
    fn data_query_builder() {
        let q = DataQuery::new("BTC", 1000, 2000)
            .with_granularity(Granularity::Minutes(5))
            .with_limit(10);
        assert_eq!(q.instrument, "BTC");
        assert_eq!(q.granularity, Granularity::Minutes(5));
        assert_eq!(q.limit, Some(10));
    }

    #[test]
    fn in_memory_filter_by_range() {
        let items = vec![make_bar(500), make_bar(1000), make_bar(1500), make_bar(2500)];
        let source = InMemoryDataSource::new("test", items);
        let q = DataQuery::new("BTC", 900, 2000);
        let result = source.query(&q).unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].ts_ms(), 1000);
        assert_eq!(result[1].ts_ms(), 1500);
    }

    #[test]
    fn in_memory_respects_limit() {
        let items = vec![make_bar(1000), make_bar(1001), make_bar(1002), make_bar(1003)];
        let source = InMemoryDataSource::new("test", items);
        let q = DataQuery::new("BTC", 0, 9999).with_limit(2);
        let result = source.query(&q).unwrap();
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn granularity_to_ms() {
        assert_eq!(Granularity::Seconds(30).to_ms(), Some(30_000));
        assert_eq!(Granularity::Minutes(1).to_ms(), Some(60_000));
        assert_eq!(Granularity::Hours(1).to_ms(), Some(3_600_000));
        assert_eq!(Granularity::Days(1).to_ms(), Some(86_400_000));
        assert_eq!(Granularity::Tick.to_ms(), None);
    }
}
