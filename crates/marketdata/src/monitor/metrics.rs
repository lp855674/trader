use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct DataMetricsSnapshot {
    pub ts_ms: i64,
    pub queries_per_sec: f64,
    pub cache_hit_rate: f64,
    pub avg_query_latency_us: f64,
    pub items_ingested_total: u64,
    pub quality_score: f64,
}

pub struct DataMetricsCollector {
    query_count: u64,
    cache_hits: u64,
    cache_total: u64,
    total_latency_us: u64,
    items_ingested: u64,
    quality_sum: f64,
    quality_count: u64,
    window_start_ms: i64,
}

impl DataMetricsCollector {
    pub fn new() -> Self {
        Self {
            query_count: 0,
            cache_hits: 0,
            cache_total: 0,
            total_latency_us: 0,
            items_ingested: 0,
            quality_sum: 0.0,
            quality_count: 0,
            window_start_ms: 0,
        }
    }

    pub fn record_query(&mut self, latency_us: u64, hit: bool) {
        self.query_count += 1;
        self.cache_total += 1;
        if hit {
            self.cache_hits += 1;
        }
        self.total_latency_us += latency_us;
    }

    pub fn record_ingestion(&mut self, count: u64) {
        self.items_ingested += count;
    }

    pub fn record_quality(&mut self, score: f64) {
        self.quality_sum += score;
        self.quality_count += 1;
    }

    pub fn snapshot(&self, ts_ms: i64) -> DataMetricsSnapshot {
        let elapsed_sec = ((ts_ms - self.window_start_ms) as f64 / 1000.0).max(1.0);
        let cache_hit_rate = if self.cache_total == 0 {
            0.0
        } else {
            self.cache_hits as f64 / self.cache_total as f64
        };
        let avg_latency = if self.query_count == 0 {
            0.0
        } else {
            self.total_latency_us as f64 / self.query_count as f64
        };
        let quality_score = if self.quality_count == 0 {
            1.0
        } else {
            self.quality_sum / self.quality_count as f64
        };

        DataMetricsSnapshot {
            ts_ms,
            queries_per_sec: self.query_count as f64 / elapsed_sec,
            cache_hit_rate,
            avg_query_latency_us: avg_latency,
            items_ingested_total: self.items_ingested,
            quality_score,
        }
    }
}

impl Default for DataMetricsCollector {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_reflects_recorded_data() {
        let mut collector = DataMetricsCollector::new();
        collector.record_query(100, true);
        collector.record_query(200, false);
        collector.record_ingestion(500);
        collector.record_quality(0.9);

        let snap = collector.snapshot(1000);
        assert!((snap.cache_hit_rate - 0.5).abs() < 1e-9);
        assert!((snap.avg_query_latency_us - 150.0).abs() < 1e-9);
        assert_eq!(snap.items_ingested_total, 500);
        assert!((snap.quality_score - 0.9).abs() < 1e-9);
    }
}
