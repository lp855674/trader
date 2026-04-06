//! Trading strategies.

use domain::{InstrumentId, Side, Signal};

pub struct StrategyContext {
    pub instrument: InstrumentId,
    pub instrument_db_id: i64,
    pub last_bar_close: Option<f64>,
    pub ts_ms: i64,
}

/// `Send + Sync` so pipeline callers can hold `&dyn Strategy` across `await` (Axum handlers require `Send`).
pub trait Strategy: Send + Sync {
    fn evaluate(&self, context: &StrategyContext) -> Option<Signal>;
}

/// 不下单、不产出信号；用于生产默认，避免演示策略自动成交。
pub struct NoOpStrategy;

impl Strategy for NoOpStrategy {
    fn evaluate(&self, _context: &StrategyContext) -> Option<Signal> {
        None
    }
}

/// 仅用于单元测试与集成测试：有 bar 则做多 1 手。
pub struct AlwaysLongOne;

impl Strategy for AlwaysLongOne {
    fn evaluate(&self, context: &StrategyContext) -> Option<Signal> {
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

    #[test]
    fn long_one_when_bar_present() {
        let strategy = AlwaysLongOne;
        let context = StrategyContext {
            instrument: InstrumentId::new(Venue::Crypto, "X"),
            instrument_db_id: 7,
            last_bar_close: Some(42.0),
            ts_ms: 99,
        };
        let signal = strategy.evaluate(&context).expect("signal");
        assert_eq!(signal.qty, 1.0);
        assert_eq!(signal.limit_price, 42.0);
    }

    #[test]
    fn no_signal_without_bar() {
        let strategy = AlwaysLongOne;
        let context = StrategyContext {
            instrument: InstrumentId::new(Venue::Crypto, "X"),
            instrument_db_id: 7,
            last_bar_close: None,
            ts_ms: 99,
        };
        assert!(strategy.evaluate(&context).is_none());
    }
}
