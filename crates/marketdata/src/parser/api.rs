use domain::NormalizedBar;
use crate::core::{DataItem, DataSourceError};

// ── RateLimiter ───────────────────────────────────────────────────────────────

pub struct RateLimiter {
    pub requests_per_second: f64,
    pub tokens: f64,
    pub last_refill_ts_ms: i64,
}

impl RateLimiter {
    pub fn new(rps: f64) -> Self {
        Self {
            requests_per_second: rps,
            tokens: rps,
            last_refill_ts_ms: i64::MIN, // sentinel: never refilled
        }
    }

    pub fn try_acquire(&mut self, ts_ms: i64) -> bool {
        // Refill tokens based on elapsed time
        if self.last_refill_ts_ms != i64::MIN {
            let elapsed_ms = (ts_ms - self.last_refill_ts_ms).max(0) as f64;
            let elapsed_s = elapsed_ms / 1_000.0;
            let new_tokens = elapsed_s * self.requests_per_second;
            self.tokens = (self.tokens + new_tokens).min(self.requests_per_second);
        }
        self.last_refill_ts_ms = ts_ms;

        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            true
        } else {
            false
        }
    }

    pub fn tokens_available(&self) -> f64 {
        self.tokens
    }
}

// ── RetryConfig ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct RetryConfig {
    pub max_retries: u32,
    pub base_delay_ms: u64,
    pub backoff_factor: f64,
}

impl RetryConfig {
    pub fn delay_for_attempt(&self, attempt: u32) -> u64 {
        (self.base_delay_ms as f64 * self.backoff_factor.powi(attempt as i32)) as u64
    }
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            base_delay_ms: 100,
            backoff_factor: 2.0,
        }
    }
}

// ── ApiParser ─────────────────────────────────────────────────────────────────

pub struct ApiParser {
    pub rate_limiter: RateLimiter,
    pub retry_config: RetryConfig,
}

impl ApiParser {
    pub fn new(rps: f64, retry_config: RetryConfig) -> Self {
        Self {
            rate_limiter: RateLimiter::new(rps),
            retry_config,
        }
    }

    pub fn parse_bar_json(json: &str) -> Result<NormalizedBar, DataSourceError> {
        let v: serde_json::Value = serde_json::from_str(json)
            .map_err(|e| DataSourceError::Parse(e.to_string()))?;

        let get_f64 = |key: &str| -> Result<f64, DataSourceError> {
            v[key]
                .as_f64()
                .ok_or_else(|| DataSourceError::Parse(format!("missing or invalid field: {}", key)))
        };
        let get_i64 = |key: &str| -> Result<i64, DataSourceError> {
            v[key]
                .as_i64()
                .ok_or_else(|| DataSourceError::Parse(format!("missing or invalid field: {}", key)))
        };

        Ok(NormalizedBar {
            ts_ms: get_i64("ts_ms")?,
            open: get_f64("open")?,
            high: get_f64("high")?,
            low: get_f64("low")?,
            close: get_f64("close")?,
            volume: get_f64("volume")?,
        })
    }

    pub fn parse_bars_array(json: &str) -> Result<Vec<NormalizedBar>, DataSourceError> {
        let arr: Vec<serde_json::Value> = serde_json::from_str(json)
            .map_err(|e| DataSourceError::Parse(e.to_string()))?;

        arr.iter()
            .map(|v| {
                let s = serde_json::to_string(v)
                    .map_err(|e| DataSourceError::Parse(e.to_string()))?;
                Self::parse_bar_json(&s)
            })
            .collect()
    }

    pub fn parse_tick_json(json: &str) -> Result<DataItem, DataSourceError> {
        let v: serde_json::Value = serde_json::from_str(json)
            .map_err(|e| DataSourceError::Parse(e.to_string()))?;

        let get_f64 = |key: &str| -> Result<f64, DataSourceError> {
            v[key]
                .as_f64()
                .ok_or_else(|| DataSourceError::Parse(format!("missing or invalid field: {}", key)))
        };
        let get_i64 = |key: &str| -> Result<i64, DataSourceError> {
            v[key]
                .as_i64()
                .ok_or_else(|| DataSourceError::Parse(format!("missing or invalid field: {}", key)))
        };

        Ok(DataItem::Tick {
            ts_ms: get_i64("ts_ms")?,
            bid: get_f64("bid")?,
            ask: get_f64("ask")?,
            last: get_f64("last")?,
            volume: get_f64("volume")?,
        })
    }

    pub fn can_request(&mut self, ts_ms: i64) -> bool {
        self.rate_limiter.try_acquire(ts_ms)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rate_limiter_depletes() {
        let mut rl = RateLimiter::new(2.0);
        assert!(rl.try_acquire(0));
        assert!(rl.try_acquire(0));
        assert!(!rl.try_acquire(0)); // depleted
    }

    #[test]
    fn rate_limiter_refills() {
        let mut rl = RateLimiter::new(2.0);
        assert!(rl.try_acquire(0));
        assert!(rl.try_acquire(0));
        assert!(!rl.try_acquire(0));
        // After 1 second, should have 2 more tokens
        assert!(rl.try_acquire(1_000));
        assert!(rl.try_acquire(1_000));
        assert!(!rl.try_acquire(1_000));
    }

    #[test]
    fn retry_delay_exponential() {
        let cfg = RetryConfig {
            max_retries: 3,
            base_delay_ms: 100,
            backoff_factor: 2.0,
        };
        assert_eq!(cfg.delay_for_attempt(0), 100);
        assert_eq!(cfg.delay_for_attempt(1), 200);
        assert_eq!(cfg.delay_for_attempt(2), 400);
    }

    #[test]
    fn parse_bar_json_valid() {
        let json = r#"{"ts_ms":1000,"open":1.0,"high":2.0,"low":0.5,"close":1.5,"volume":100.0}"#;
        let bar = ApiParser::parse_bar_json(json).unwrap();
        assert_eq!(bar.ts_ms, 1000);
        assert!((bar.close - 1.5).abs() < 1e-9);
    }

    #[test]
    fn parse_bars_array_valid() {
        let json = r#"[
            {"ts_ms":1000,"open":1.0,"high":2.0,"low":0.5,"close":1.5,"volume":100.0},
            {"ts_ms":2000,"open":2.0,"high":3.0,"low":1.5,"close":2.5,"volume":200.0}
        ]"#;
        let bars = ApiParser::parse_bars_array(json).unwrap();
        assert_eq!(bars.len(), 2);
        assert_eq!(bars[1].ts_ms, 2000);
    }

    #[test]
    fn parse_tick_json_valid() {
        let json = r#"{"ts_ms":5000,"bid":1.0,"ask":1.1,"last":1.05,"volume":50.0}"#;
        let item = ApiParser::parse_tick_json(json).unwrap();
        match item {
            DataItem::Tick { ts_ms, bid, .. } => {
                assert_eq!(ts_ms, 5000);
                assert!((bid - 1.0).abs() < 1e-9);
            }
            _ => panic!("expected Tick"),
        }
    }
}
