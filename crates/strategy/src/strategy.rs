//! Trading strategies.

use async_trait::async_trait;
use domain::{InstrumentId, Side, Signal};

pub struct StrategyContext {
    pub instrument: InstrumentId,
    pub instrument_db_id: i64,
    pub last_bar_close: Option<f64>,
    pub ts_ms: i64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ScoredCandidate {
    pub symbol: String,
    pub score: f64,
    pub confidence: f64,
}

/// `Send + Sync` so pipeline callers can hold `Arc<dyn Strategy>` across `await`.
#[async_trait]
pub trait Strategy: Send + Sync {
    async fn evaluate(&self, context: &StrategyContext) -> Option<Signal>;

    async fn evaluate_candidate(
        &self,
        _context: &StrategyContext,
    ) -> Result<Option<ScoredCandidate>, String> {
        Err("strategy does not support universe scoring".to_string())
    }
}

pub struct NoOpStrategy;

#[async_trait]
impl Strategy for NoOpStrategy {
    async fn evaluate(&self, _context: &StrategyContext) -> Option<Signal> {
        None
    }
}

pub struct AlwaysLongOne;

#[async_trait]
impl Strategy for AlwaysLongOne {
    async fn evaluate(&self, context: &StrategyContext) -> Option<Signal> {
        let limit_price = context.last_bar_close?;
        Some(Signal {
            strategy_id: "always_long_one".to_string(),
            instrument: context.instrument.clone(),
            instrument_db_id: context.instrument_db_id,
            side: Side::Buy,
            qty: 1.0,
            limit_price,
            ts_ms: context.ts_ms,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{AlwaysLongOne, Strategy, StrategyContext};
    use domain::{InstrumentId, Venue};

    #[tokio::test]
    async fn long_one_when_bar_present() {
        let strategy = AlwaysLongOne;
        let context = StrategyContext {
            instrument: InstrumentId::new(Venue::Crypto, "X"),
            instrument_db_id: 7,
            last_bar_close: Some(42.0),
            ts_ms: 99,
        };
        let signal = strategy.evaluate(&context).await.expect("signal");
        assert_eq!(signal.qty, 1.0);
        assert_eq!(signal.limit_price, 42.0);
    }

    #[tokio::test]
    async fn no_signal_without_bar() {
        let strategy = AlwaysLongOne;
        let context = StrategyContext {
            instrument: InstrumentId::new(Venue::Crypto, "X"),
            instrument_db_id: 7,
            last_bar_close: None,
            ts_ms: 99,
        };
        assert!(strategy.evaluate(&context).await.is_none());
    }
}
