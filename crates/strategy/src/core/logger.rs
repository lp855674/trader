// Structured strategy logging layer.
//
// Provides `StructuredLogger` (mpsc-based) and `LoggingStrategy` wrapper that
// intercepts `evaluate()` calls and emits structured `LogEvent` values.

use std::sync::Arc;

use tokio::sync::mpsc;

use domain::Side;

use super::{
    combinators::StrategyStats,
    r#trait::{Signal, StrategyContext, StrategyError, Strategy},
};

// ─── LogEvent ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum LogEvent {
    SignalGenerated {
        strategy_id: String,
        side: Side,
        quantity: f64,
        limit_price: Option<f64>,
        ts_ms: i64,
    },
    SignalSuppressed {
        strategy_id: String,
        reason: String,
        ts_ms: i64,
    },
    EvaluationError {
        strategy_id: String,
        error: String,
        ts_ms: i64,
    },
    PerformanceSnapshot {
        strategy_id: String,
        win_rate: f64,
        avg_pnl: f64,
        ts_ms: i64,
    },
}

// ─── StructuredLogger ─────────────────────────────────────────────────────────

/// Sends `LogEvent` values over a channel.  Callers hold a `Receiver` to
/// consume them asynchronously.  All send operations are fire-and-forget; a
/// full channel or dropped receiver is silently ignored.
#[derive(Clone)]
pub struct StructuredLogger {
    tx: mpsc::Sender<LogEvent>,
}

impl StructuredLogger {
    /// Create a logger and its associated receiver.
    pub fn new() -> (Self, mpsc::Receiver<LogEvent>) {
        let (tx, rx) = mpsc::channel(256);
        (Self { tx }, rx)
    }

    /// Emit a `SignalGenerated` event for the given signal.
    pub fn log_signal(&self, signal: &Signal) {
        tracing::debug!(
            strategy_id = %signal.strategy_id,
            side = ?signal.side,
            qty = signal.quantity,
            "signal generated"
        );
        let event = LogEvent::SignalGenerated {
            strategy_id: signal.strategy_id.clone(),
            side: signal.side,
            quantity: signal.quantity,
            limit_price: signal.limit_price,
            ts_ms: signal.timestamp_ms,
        };
        let _ = self.tx.try_send(event);
    }

    /// Emit a `SignalSuppressed` event.
    pub fn log_suppressed(&self, strategy_id: &str, reason: &str, ts_ms: i64) {
        tracing::debug!(strategy_id = %strategy_id, reason = %reason, "signal suppressed");
        let event = LogEvent::SignalSuppressed {
            strategy_id: strategy_id.to_owned(),
            reason: reason.to_owned(),
            ts_ms,
        };
        let _ = self.tx.try_send(event);
    }

    /// Emit an `EvaluationError` event.
    pub fn log_error(&self, strategy_id: &str, err: &StrategyError, ts_ms: i64) {
        tracing::warn!(strategy_id = %strategy_id, error = %err, "evaluation error");
        let event = LogEvent::EvaluationError {
            strategy_id: strategy_id.to_owned(),
            error: err.to_string(),
            ts_ms,
        };
        let _ = self.tx.try_send(event);
    }

    /// Emit a `PerformanceSnapshot` event.
    pub fn log_performance(&self, strategy_id: &str, stats: &StrategyStats, ts_ms: i64) {
        let win_rate = stats.win_rate();
        let avg_pnl = stats.rolling_avg_pnl();
        tracing::debug!(
            strategy_id = %strategy_id,
            win_rate = win_rate,
            avg_pnl = avg_pnl,
            "performance snapshot"
        );
        let event = LogEvent::PerformanceSnapshot {
            strategy_id: strategy_id.to_owned(),
            win_rate,
            avg_pnl,
            ts_ms,
        };
        let _ = self.tx.try_send(event);
    }
}

// ─── LoggingStrategy ─────────────────────────────────────────────────────────

/// Wraps an inner `Strategy` and logs all `evaluate()` outcomes via a
/// `StructuredLogger`.
pub struct LoggingStrategy {
    inner: Box<dyn Strategy>,
    logger: Arc<StructuredLogger>,
}

impl LoggingStrategy {
    pub fn new(inner: Box<dyn Strategy>, logger: Arc<StructuredLogger>) -> Self {
        Self { inner, logger }
    }
}

impl Strategy for LoggingStrategy {
    fn evaluate(&self, ctx: &StrategyContext) -> Result<Option<Signal>, StrategyError> {
        match self.inner.evaluate(ctx) {
            Ok(Some(signal)) => {
                self.logger.log_signal(&signal);
                Ok(Some(signal))
            }
            Ok(None) => {
                self.logger.log_suppressed(self.inner.name(), "no signal", ctx.ts_ms);
                Ok(None)
            }
            Err(err) => {
                self.logger.log_error(self.inner.name(), &err, ctx.ts_ms);
                Err(err)
            }
        }
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
        StrategyContext::new(InstrumentId::new(Venue::Crypto, "BTC"), 1_000)
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
            Err(StrategyError::DataSource("boom".into()))
        }
        fn name(&self) -> &str { "erroring" }
    }

    // ── tests ─────────────────────────────────────────────────────────────────

    #[test]
    fn logging_strategy_passes_signal_through() {
        let (logger, mut rx) = StructuredLogger::new();
        let strat = LoggingStrategy::new(Box::new(AlwaysBuy), Arc::new(logger));
        let result = strat.evaluate(&ctx()).unwrap().unwrap();
        assert_eq!(result.side, Side::Buy);

        // Receiver should have gotten a SignalGenerated event
        let event = rx.try_recv().expect("expected log event");
        assert!(matches!(event, LogEvent::SignalGenerated { .. }));
    }

    #[test]
    fn logging_strategy_logs_suppression() {
        let (logger, mut rx) = StructuredLogger::new();
        let strat = LoggingStrategy::new(Box::new(AlwaysSilent), Arc::new(logger));
        assert!(strat.evaluate(&ctx()).unwrap().is_none());

        let event = rx.try_recv().expect("expected suppression event");
        assert!(matches!(event, LogEvent::SignalSuppressed { .. }));
    }

    #[test]
    fn logging_strategy_logs_error_and_returns_err() {
        let (logger, mut rx) = StructuredLogger::new();
        let strat = LoggingStrategy::new(Box::new(AlwaysError), Arc::new(logger));
        assert!(strat.evaluate(&ctx()).is_err());

        let event = rx.try_recv().expect("expected error event");
        assert!(matches!(event, LogEvent::EvaluationError { .. }));
    }

    #[test]
    fn structured_logger_emits_performance_snapshot() {
        use crate::core::combinators::StrategyStats;

        let (logger, mut rx) = StructuredLogger::new();
        let mut stats = StrategyStats::new(10);
        stats.record(1.0);
        logger.log_performance("strat_a", &stats, 9999);

        let event = rx.try_recv().expect("expected perf snapshot");
        assert!(matches!(
            event,
            LogEvent::PerformanceSnapshot { win_rate, .. } if win_rate > 0.0
        ));
    }
}
