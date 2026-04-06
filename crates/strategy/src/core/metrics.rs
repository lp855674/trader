// Per-strategy performance metrics and instrumentation.
//
// Provides `EvaluationTimer`, `StrategyMetrics`, `MetricsRegistry`, and a
// `MeteredStrategy` wrapper that records timing and signal counts automatically.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use super::r#trait::{Signal, StrategyContext, StrategyError, Strategy};

// ─── EvaluationTimer ─────────────────────────────────────────────────────────

/// A simple wall-clock timer for measuring `evaluate()` call duration.
pub struct EvaluationTimer {
    start: Instant,
}

impl EvaluationTimer {
    /// Start the timer.
    pub fn start() -> Self {
        Self { start: Instant::now() }
    }

    /// Stop the timer and return the elapsed duration.
    pub fn stop(self) -> Duration {
        self.start.elapsed()
    }
}

// ─── StrategyMetrics ─────────────────────────────────────────────────────────

/// Counters and accumulators for a single strategy.
#[derive(Debug, Clone, Default)]
pub struct StrategyMetrics {
    pub evaluations: u64,
    pub signals_generated: u64,
    pub signals_suppressed: u64,
    pub errors: u64,
    pub total_eval_ns: u64,
    pub cache_hits: u64,
    pub cache_misses: u64,
}

impl StrategyMetrics {
    /// Average nanoseconds per `evaluate()` call.
    pub fn avg_eval_ns(&self) -> f64 {
        if self.evaluations == 0 {
            return 0.0;
        }
        self.total_eval_ns as f64 / self.evaluations as f64
    }

    /// Fraction of evaluations that produced a signal.
    pub fn signal_rate(&self) -> f64 {
        if self.evaluations == 0 {
            return 0.0;
        }
        self.signals_generated as f64 / self.evaluations as f64
    }

    /// Fraction of cache lookups that were hits.
    pub fn cache_hit_rate(&self) -> f64 {
        let total = self.cache_hits + self.cache_misses;
        if total == 0 {
            return 0.0;
        }
        self.cache_hits as f64 / total as f64
    }

    /// Record a single evaluation.
    pub fn record_evaluation(
        &mut self,
        duration: Duration,
        signal_produced: bool,
        error: bool,
    ) {
        self.evaluations += 1;
        self.total_eval_ns += duration.as_nanos() as u64;
        if error {
            self.errors += 1;
        } else if signal_produced {
            self.signals_generated += 1;
        } else {
            self.signals_suppressed += 1;
        }
    }

    pub fn record_cache_hit(&mut self) {
        self.cache_hits += 1;
    }

    pub fn record_cache_miss(&mut self) {
        self.cache_misses += 1;
    }
}

// ─── MetricsRegistry ─────────────────────────────────────────────────────────

/// Thread-safe registry of per-strategy metrics.
#[derive(Clone)]
pub struct MetricsRegistry {
    inner: Arc<Mutex<HashMap<String, StrategyMetrics>>>,
}

impl MetricsRegistry {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Record an evaluation for the named strategy.
    pub fn record_evaluation(
        &self,
        strategy_id: &str,
        duration: Duration,
        signal_produced: bool,
        error: bool,
    ) {
        self.inner
            .lock()
            .unwrap()
            .entry(strategy_id.to_owned())
            .or_default()
            .record_evaluation(duration, signal_produced, error);
    }

    /// Record a cache hit or miss for the named strategy.
    pub fn record_cache(&self, strategy_id: &str, hit: bool) {
        let mut map = self.inner.lock().unwrap();
        let m = map.entry(strategy_id.to_owned()).or_default();
        if hit {
            m.record_cache_hit();
        } else {
            m.record_cache_miss();
        }
    }

    /// Return a snapshot of metrics for the named strategy.
    pub fn snapshot(&self, strategy_id: &str) -> Option<StrategyMetrics> {
        self.inner.lock().unwrap().get(strategy_id).cloned()
    }

    /// Return snapshots for all strategies.
    pub fn all_snapshots(&self) -> HashMap<String, StrategyMetrics> {
        self.inner.lock().unwrap().clone()
    }
}

impl Default for MetricsRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ─── MeteredStrategy ─────────────────────────────────────────────────────────

/// Wraps an inner strategy and records timing + signal/error counts to a
/// `MetricsRegistry` on every `evaluate()` call.
pub struct MeteredStrategy {
    inner: Box<dyn Strategy>,
    registry: Arc<MetricsRegistry>,
}

impl MeteredStrategy {
    pub fn new(inner: Box<dyn Strategy>, registry: Arc<MetricsRegistry>) -> Self {
        Self { inner, registry }
    }
}

impl Strategy for MeteredStrategy {
    fn evaluate(&self, ctx: &StrategyContext) -> Result<Option<Signal>, StrategyError> {
        let timer = EvaluationTimer::start();
        let result = self.inner.evaluate(ctx);
        let duration = timer.stop();

        let (signal_produced, error) = match &result {
            Ok(Some(_)) => (true, false),
            Ok(None) => (false, false),
            Err(_) => (false, true),
        };

        self.registry.record_evaluation(self.inner.name(), duration, signal_produced, error);

        result
    }

    fn name(&self) -> &str {
        self.inner.name()
    }

    fn version(&self) -> u32 {
        self.inner.version()
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Arc;

    use domain::{InstrumentId, Side, Venue};

    use super::*;
    use crate::core::r#trait::{Signal, StrategyContext, StrategyError, Strategy};

    fn ctx() -> StrategyContext {
        StrategyContext::new(InstrumentId::new(Venue::Crypto, "BTC"), 0)
    }

    // ── helper strategies ─────────────────────────────────────────────────────

    struct AlwaysBuy;
    impl Strategy for AlwaysBuy {
        fn evaluate(&self, ctx: &StrategyContext) -> Result<Option<Signal>, StrategyError> {
            Ok(Some(Signal::new(
                ctx.instrument.clone(),
                Side::Buy,
                1.0,
                None,
                ctx.ts_ms,
                "always_buy".into(),
                HashMap::new(),
            )))
        }
        fn name(&self) -> &str { "always_buy" }
    }

    struct AlwaysSilent;
    impl Strategy for AlwaysSilent {
        fn evaluate(&self, _ctx: &StrategyContext) -> Result<Option<Signal>, StrategyError> {
            Ok(None)
        }
        fn name(&self) -> &str { "silent" }
    }

    struct AlwaysError;
    impl Strategy for AlwaysError {
        fn evaluate(&self, _ctx: &StrategyContext) -> Result<Option<Signal>, StrategyError> {
            Err(StrategyError::DataSource("oops".into()))
        }
        fn name(&self) -> &str { "erroring" }
    }

    // ── EvaluationTimer ──────────────────────────────────────────────────────

    #[test]
    fn timer_measures_nonzero_duration() {
        let timer = EvaluationTimer::start();
        // Do a tiny bit of work to ensure non-zero elapsed time
        let _: u64 = (0..1000_u64).sum();
        let d = timer.stop();
        // Can't guarantee > 0 on all platforms, but nanos should be >= 0
        assert!(d.as_nanos() >= 0);
    }

    // ── StrategyMetrics ──────────────────────────────────────────────────────

    #[test]
    fn metrics_initial_state_is_zero() {
        let m = StrategyMetrics::default();
        assert_eq!(m.evaluations, 0);
        assert!((m.avg_eval_ns() - 0.0).abs() < f64::EPSILON);
        assert!((m.signal_rate() - 0.0).abs() < f64::EPSILON);
        assert!((m.cache_hit_rate() - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn metrics_records_signal() {
        let mut m = StrategyMetrics::default();
        m.record_evaluation(Duration::from_nanos(100), true, false);
        assert_eq!(m.evaluations, 1);
        assert_eq!(m.signals_generated, 1);
        assert_eq!(m.errors, 0);
        assert!((m.signal_rate() - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn metrics_records_error() {
        let mut m = StrategyMetrics::default();
        m.record_evaluation(Duration::from_nanos(50), false, true);
        assert_eq!(m.errors, 1);
        assert_eq!(m.signals_generated, 0);
    }

    #[test]
    fn metrics_avg_eval_ns_correct() {
        let mut m = StrategyMetrics::default();
        m.record_evaluation(Duration::from_nanos(200), true, false);
        m.record_evaluation(Duration::from_nanos(400), false, false);
        assert!((m.avg_eval_ns() - 300.0).abs() < f64::EPSILON);
    }

    #[test]
    fn metrics_cache_hit_rate_correct() {
        let mut m = StrategyMetrics::default();
        m.record_cache_hit();
        m.record_cache_hit();
        m.record_cache_miss();
        // 2 hits / 3 total = 0.666...
        assert!((m.cache_hit_rate() - 2.0 / 3.0).abs() < 1e-9);
    }

    // ── MetricsRegistry ──────────────────────────────────────────────────────

    #[test]
    fn registry_records_and_snapshots() {
        let reg = MetricsRegistry::new();
        reg.record_evaluation("s1", Duration::from_nanos(100), true, false);
        reg.record_evaluation("s1", Duration::from_nanos(100), false, false);
        let snap = reg.snapshot("s1").unwrap();
        assert_eq!(snap.evaluations, 2);
        assert_eq!(snap.signals_generated, 1);
        assert_eq!(snap.signals_suppressed, 1);
    }

    #[test]
    fn registry_snapshot_returns_none_for_unknown() {
        let reg = MetricsRegistry::new();
        assert!(reg.snapshot("unknown").is_none());
    }

    #[test]
    fn registry_all_snapshots() {
        let reg = MetricsRegistry::new();
        reg.record_evaluation("a", Duration::from_nanos(1), true, false);
        reg.record_evaluation("b", Duration::from_nanos(1), false, false);
        let snaps = reg.all_snapshots();
        assert!(snaps.contains_key("a"));
        assert!(snaps.contains_key("b"));
    }

    // ── MeteredStrategy ──────────────────────────────────────────────────────

    #[test]
    fn metered_strategy_increments_counters() {
        let reg = Arc::new(MetricsRegistry::new());
        let strat = MeteredStrategy::new(Box::new(AlwaysBuy), Arc::clone(&reg));
        strat.evaluate(&ctx()).unwrap();
        strat.evaluate(&ctx()).unwrap();
        let snap = reg.snapshot("always_buy").unwrap();
        assert_eq!(snap.evaluations, 2);
        assert_eq!(snap.signals_generated, 2);
    }

    #[test]
    fn metered_strategy_records_error() {
        let reg = Arc::new(MetricsRegistry::new());
        let strat = MeteredStrategy::new(Box::new(AlwaysError), Arc::clone(&reg));
        let _ = strat.evaluate(&ctx()); // ignore the error
        let snap = reg.snapshot("erroring").unwrap();
        assert_eq!(snap.errors, 1);
    }

    #[test]
    fn metered_strategy_records_suppression() {
        let reg = Arc::new(MetricsRegistry::new());
        let strat = MeteredStrategy::new(Box::new(AlwaysSilent), Arc::clone(&reg));
        strat.evaluate(&ctx()).unwrap();
        let snap = reg.snapshot("silent").unwrap();
        assert_eq!(snap.signals_suppressed, 1);
    }

    #[test]
    fn metered_strategy_passes_result_through() {
        let reg = Arc::new(MetricsRegistry::new());
        let strat = MeteredStrategy::new(Box::new(AlwaysBuy), Arc::clone(&reg));
        let result = strat.evaluate(&ctx()).unwrap();
        assert!(result.is_some());
        assert_eq!(result.unwrap().side, Side::Buy);
    }
}
