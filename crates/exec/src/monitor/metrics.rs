use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct LatencyBucket {
    pub p50_us: f64,
    pub p95_us: f64,
    pub p99_us: f64,
    pub max_us: f64,
}

impl LatencyBucket {
    pub fn from_samples(samples: &[u64]) -> Self {
        if samples.is_empty() {
            return Self { p50_us: 0.0, p95_us: 0.0, p99_us: 0.0, max_us: 0.0 };
        }
        let mut sorted = samples.to_vec();
        sorted.sort_unstable();
        let len = sorted.len();
        let p50 = sorted[(len * 50 / 100).min(len - 1)] as f64;
        let p95 = sorted[(len * 95 / 100).min(len - 1)] as f64;
        let p99 = sorted[(len * 99 / 100).min(len - 1)] as f64;
        let max = *sorted.last().unwrap() as f64;
        Self { p50_us: p50, p95_us: p95, p99_us: p99, max_us: max }
    }

    pub fn zero() -> Self {
        Self { p50_us: 0.0, p95_us: 0.0, p99_us: 0.0, max_us: 0.0 }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct MetricsSnapshot {
    pub ts_ms: i64,
    pub orders_submitted: u64,
    pub orders_filled: u64,
    pub orders_rejected: u64,
    pub orders_cancelled: u64,
    pub fill_rate: f64,
    pub rejection_rate: f64,
    pub queue_depth: usize,
    pub submit_latency: LatencyBucket,
    pub fill_latency: LatencyBucket,
}

pub struct ExecutionMetrics {
    orders_submitted: u64,
    orders_filled: u64,
    orders_rejected: u64,
    orders_cancelled: u64,
    queue_depth: usize,
    submit_latency_samples: Vec<u64>,
    fill_latency_samples: Vec<u64>,
}

impl ExecutionMetrics {
    pub fn new() -> Self {
        Self {
            orders_submitted: 0,
            orders_filled: 0,
            orders_rejected: 0,
            orders_cancelled: 0,
            queue_depth: 0,
            submit_latency_samples: Vec::new(),
            fill_latency_samples: Vec::new(),
        }
    }

    pub fn record_submit(&mut self, latency_us: u64) {
        self.orders_submitted += 1;
        self.submit_latency_samples.push(latency_us);
    }

    pub fn record_fill(&mut self, latency_us: u64) {
        self.orders_filled += 1;
        self.fill_latency_samples.push(latency_us);
    }

    pub fn record_rejection(&mut self) {
        self.orders_rejected += 1;
    }

    pub fn record_cancellation(&mut self) {
        self.orders_cancelled += 1;
    }

    pub fn set_queue_depth(&mut self, depth: usize) {
        self.queue_depth = depth;
    }

    pub fn snapshot(&self, ts_ms: i64) -> MetricsSnapshot {
        let fill_rate = if self.orders_submitted == 0 {
            0.0
        } else {
            self.orders_filled as f64 / self.orders_submitted as f64
        };
        let rejection_rate = if self.orders_submitted == 0 {
            0.0
        } else {
            self.orders_rejected as f64 / self.orders_submitted as f64
        };
        MetricsSnapshot {
            ts_ms,
            orders_submitted: self.orders_submitted,
            orders_filled: self.orders_filled,
            orders_rejected: self.orders_rejected,
            orders_cancelled: self.orders_cancelled,
            fill_rate,
            rejection_rate,
            queue_depth: self.queue_depth,
            submit_latency: LatencyBucket::from_samples(&self.submit_latency_samples),
            fill_latency: LatencyBucket::from_samples(&self.fill_latency_samples),
        }
    }

    pub fn reset_counters(&mut self) {
        self.orders_submitted = 0;
        self.orders_filled = 0;
        self.orders_rejected = 0;
        self.orders_cancelled = 0;
        self.submit_latency_samples.clear();
        self.fill_latency_samples.clear();
    }
}

impl Default for ExecutionMetrics {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fill_rate_correct() {
        let mut m = ExecutionMetrics::new();
        m.record_submit(100);
        m.record_submit(200);
        m.record_fill(150);
        let snap = m.snapshot(1000);
        assert_eq!(snap.orders_submitted, 2);
        assert_eq!(snap.orders_filled, 1);
        assert!((snap.fill_rate - 0.5).abs() < 1e-9);
    }

    #[test]
    fn percentiles_computed() {
        let samples: Vec<u64> = (1..=100).collect();
        let bucket = LatencyBucket::from_samples(&samples);
        // p50 index = 100*50/100 = 50, sorted[50] = 51
        assert!((bucket.p50_us - 51.0).abs() < 1.0, "p50={}", bucket.p50_us);
        // p95 index = 100*95/100 = 95, sorted[95] = 96
        assert!((bucket.p95_us - 96.0).abs() < 1.0, "p95={}", bucket.p95_us);
        // p99 index = 100*99/100 = 99, sorted[99] = 100
        assert!((bucket.p99_us - 100.0).abs() < 1.0, "p99={}", bucket.p99_us);
        assert!((bucket.max_us - 100.0).abs() < 1e-9);
    }

    #[test]
    fn snapshot_populated() {
        let mut m = ExecutionMetrics::new();
        m.record_submit(50);
        m.record_fill(100);
        m.record_rejection();
        m.record_cancellation();
        m.set_queue_depth(5);
        let snap = m.snapshot(999);
        assert_eq!(snap.ts_ms, 999);
        assert_eq!(snap.orders_submitted, 1);
        assert_eq!(snap.orders_filled, 1);
        assert_eq!(snap.orders_rejected, 1);
        assert_eq!(snap.orders_cancelled, 1);
        assert_eq!(snap.queue_depth, 5);
        assert!((snap.fill_rate - 1.0).abs() < 1e-9);
    }

    #[test]
    fn reset_counters() {
        let mut m = ExecutionMetrics::new();
        m.record_submit(10);
        m.record_fill(20);
        m.reset_counters();
        let snap = m.snapshot(0);
        assert_eq!(snap.orders_submitted, 0);
        assert_eq!(snap.orders_filled, 0);
    }
}
