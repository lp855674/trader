use crate::core::OrderRequest;

#[derive(Debug, Clone)]
pub struct BatchConfig {
    pub max_batch_size: usize,
    pub flush_interval_ms: u64,
    pub rate_limit_per_sec: u32,
}

impl Default for BatchConfig {
    fn default() -> Self {
        Self {
            max_batch_size: 100,
            flush_interval_ms: 100,
            rate_limit_per_sec: 10,
        }
    }
}

pub struct BatchExecutionQueue {
    pub pending: Vec<(OrderRequest, i64)>,
    pub config: BatchConfig,
    pub last_flush_ts: i64,
    /// Token bucket for rate limiting (tokens replenish at rate_limit_per_sec per second).
    pub tokens: f64,
}

impl BatchExecutionQueue {
    pub fn new(config: BatchConfig) -> Self {
        let initial_tokens = config.rate_limit_per_sec as f64;
        Self {
            pending: Vec::new(),
            config,
            last_flush_ts: 0,
            tokens: initial_tokens,
        }
    }

    pub fn push(&mut self, req: OrderRequest, ts_ms: i64) {
        self.pending.push((req, ts_ms));
    }

    /// Flush and return batch respecting rate limit and max_batch_size.
    pub fn flush(&mut self, ts_ms: i64) -> Vec<OrderRequest> {
        // Replenish tokens based on elapsed time
        let elapsed_secs = (ts_ms - self.last_flush_ts).max(0) as f64 / 1000.0;
        self.tokens = (self.tokens + elapsed_secs * self.config.rate_limit_per_sec as f64)
            .min(self.config.rate_limit_per_sec as f64);
        self.last_flush_ts = ts_ms;

        let available = self.tokens.floor() as usize;
        let take = available
            .min(self.config.max_batch_size)
            .min(self.pending.len());
        if take == 0 {
            return Vec::new();
        }
        self.tokens -= take as f64;
        let batch: Vec<OrderRequest> = self.pending.drain(..take).map(|(r, _)| r).collect();
        batch
    }

    /// Flush if interval has elapsed.
    pub fn tick(&mut self, ts_ms: i64) -> Vec<OrderRequest> {
        let elapsed = (ts_ms - self.last_flush_ts) as u64;
        if elapsed >= self.config.flush_interval_ms {
            self.flush(ts_ms)
        } else {
            Vec::new()
        }
    }
}

#[cfg(test)]
mod tests {
    use domain::{InstrumentId, Side, Venue};

    use super::*;
    use crate::core::types::{OrderKind, TimeInForce};

    fn make_req(n: u32) -> OrderRequest {
        OrderRequest {
            client_order_id: format!("c{}", n),
            instrument: InstrumentId::new(Venue::Crypto, "BTC-USD"),
            side: Side::Buy,
            quantity: 1.0,
            kind: OrderKind::Market,
            tif: TimeInForce::GTC,
            flags: vec![],
            strategy_id: "s".to_string(),
            submitted_ts_ms: 0,
        }
    }

    #[test]
    fn batch_queue_respects_rate_limit() {
        let config = BatchConfig {
            max_batch_size: 100,
            flush_interval_ms: 100,
            rate_limit_per_sec: 5,
        };
        let mut queue = BatchExecutionQueue::new(config);
        // Push 10 orders
        for i in 0..10 {
            queue.push(make_req(i), 0);
        }
        queue.last_flush_ts = 0;
        // Initial tokens = 5; flush at ts=0 → can take 5
        let batch = queue.flush(0);
        assert_eq!(batch.len(), 5);
        // No tokens left immediately
        let batch2 = queue.flush(0);
        assert_eq!(batch2.len(), 0);
        // After 1 second, replenished
        let batch3 = queue.flush(1000);
        assert_eq!(batch3.len(), 5);
    }
}
