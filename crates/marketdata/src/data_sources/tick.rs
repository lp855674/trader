use crate::core::data::{DataItem, DataQuery, DataSource, DataSourceError, Granularity};
use domain::NormalizedBar;

pub struct TickSource {
    instrument: String,
    ticks: Vec<DataItem>,
}

impl TickSource {
    pub fn new_synthetic(
        instrument: &str,
        n_ticks: usize,
        start_ms: i64,
        interval_ms: u64,
        seed: u64,
    ) -> Self {
        let mut state = seed;
        let mut last = 100.0f64;
        let mut ticks = Vec::with_capacity(n_ticks);

        for i in 0..n_ticks {
            let ts_ms = start_ms + i as i64 * interval_ms as i64;

            state = state
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            let r1 = (state >> 11) as f64 / (1u64 << 53) as f64;
            state = state
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            let r2 = (state >> 11) as f64 / (1u64 << 53) as f64;
            state = state
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            let r3 = (state >> 11) as f64 / (1u64 << 53) as f64;
            state = state
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            let r4 = (state >> 11) as f64 / (1u64 << 53) as f64;

            let spread = last * 0.0005;
            let change = (r1 - 0.5) * 0.002;
            last = (last * (1.0 + change)).max(0.01);

            ticks.push(DataItem::Tick {
                ts_ms,
                bid: last - spread / 2.0,
                ask: last + spread / 2.0,
                last,
                volume: 10.0 * (0.5 + r4),
            });
            let _ = (r2, r3);
        }

        Self {
            instrument: instrument.to_string(),
            ticks,
        }
    }
}

impl DataSource for TickSource {
    fn name(&self) -> &str {
        &self.instrument
    }

    fn query(&self, query: &DataQuery) -> Result<Vec<DataItem>, DataSourceError> {
        let mut result: Vec<DataItem> = self
            .ticks
            .iter()
            .filter(|t| t.ts_ms() >= query.start_ts_ms && t.ts_ms() <= query.end_ts_ms)
            .cloned()
            .collect();
        if let Some(limit) = query.limit {
            result.truncate(limit);
        }
        Ok(result)
    }

    fn supports_granularity(&self, g: &Granularity) -> bool {
        matches!(g, Granularity::Tick)
    }
}

// ── TickAggregator ────────────────────────────────────────────────────────────

pub struct TickAggregator {
    pub pending: Vec<DataItem>,
    pub interval_ms: u64,
}

impl TickAggregator {
    pub fn new(interval_ms: u64) -> Self {
        Self {
            pending: Vec::new(),
            interval_ms,
        }
    }

    pub fn push(&mut self, tick: DataItem) {
        self.pending.push(tick);
    }

    /// Aggregate all pending ticks with ts_ms <= ts_ms into a bar.
    pub fn flush(&mut self, ts_ms: i64) -> Option<NormalizedBar> {
        let bucket_end = (ts_ms / self.interval_ms as i64) * self.interval_ms as i64;
        let (ready, remaining): (Vec<DataItem>, Vec<DataItem>) = self
            .pending
            .drain(..)
            .partition(|t| t.ts_ms() <= bucket_end);
        self.pending = remaining;

        if ready.is_empty() {
            return None;
        }

        let bucket_ts = (ready[0].ts_ms() / self.interval_ms as i64) * self.interval_ms as i64;
        let mut open = None;
        let mut high = f64::NEG_INFINITY;
        let mut low = f64::INFINITY;
        let mut close = 0.0;
        let mut volume = 0.0;

        for item in &ready {
            let price = match item {
                DataItem::Tick {
                    last, volume: v, ..
                } => {
                    volume += v;
                    *last
                }
                DataItem::Bar(b) => {
                    volume += b.volume;
                    b.close
                }
                _ => continue,
            };
            if open.is_none() {
                open = Some(price);
            }
            if price > high {
                high = price;
            }
            if price < low {
                low = price;
            }
            close = price;
        }

        let open = open?;
        if high == f64::NEG_INFINITY {
            return None;
        }

        Some(NormalizedBar {
            ts_ms: bucket_ts,
            open,
            high,
            low,
            close,
            volume,
        })
    }

    pub fn flush_all(&mut self) -> Vec<NormalizedBar> {
        if self.pending.is_empty() {
            return Vec::new();
        }
        // Group by interval bucket
        let all: Vec<DataItem> = self.pending.drain(..).collect();
        let mut buckets: std::collections::HashMap<i64, Vec<DataItem>> =
            std::collections::HashMap::new();
        for item in all {
            let bucket = (item.ts_ms() / self.interval_ms as i64) * self.interval_ms as i64;
            buckets.entry(bucket).or_default().push(item);
        }

        let mut result = Vec::new();
        let mut sorted_keys: Vec<i64> = buckets.keys().cloned().collect();
        sorted_keys.sort();

        for bucket_ts in sorted_keys {
            let items = &buckets[&bucket_ts];
            let mut open = None;
            let mut high = f64::NEG_INFINITY;
            let mut low = f64::INFINITY;
            let mut close = 0.0;
            let mut volume = 0.0;

            for item in items {
                let price = match item {
                    DataItem::Tick {
                        last, volume: v, ..
                    } => {
                        volume += v;
                        *last
                    }
                    DataItem::Bar(b) => {
                        volume += b.volume;
                        b.close
                    }
                    _ => continue,
                };
                if open.is_none() {
                    open = Some(price);
                }
                if price > high {
                    high = price;
                }
                if price < low {
                    low = price;
                }
                close = price;
            }

            if let Some(o) = open {
                if high != f64::NEG_INFINITY {
                    result.push(NormalizedBar {
                        ts_ms: bucket_ts,
                        open: o,
                        high,
                        low,
                        close,
                        volume,
                    });
                }
            }
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tick_source_generates_ticks() {
        let src = TickSource::new_synthetic("BTC", 50, 0, 1000, 12345);
        assert_eq!(src.ticks.len(), 50);
        assert!(src.ticks.iter().all(|t| matches!(t, DataItem::Tick { .. })));
    }

    #[test]
    fn aggregator_flush_all_returns_bars() {
        let mut agg = TickAggregator::new(60_000);
        for i in 0..10 {
            agg.push(DataItem::Tick {
                ts_ms: i * 10_000,
                bid: 99.0,
                ask: 101.0,
                last: 100.0,
                volume: 10.0,
            });
        }
        let bars = agg.flush_all();
        assert!(!bars.is_empty());
        assert!(bars.iter().all(|b| b.volume > 0.0));
    }
}
