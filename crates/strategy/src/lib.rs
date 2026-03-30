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

pub struct AlwaysLongOne;

impl Strategy for AlwaysLongOne {
    fn evaluate(&self, context: &StrategyContext) -> Option<Signal> {
        context.last_bar_close?;
        Some(Signal {
            strategy_id: "always_long_one".to_string(),
            instrument: context.instrument.clone(),
            instrument_db_id: context.instrument_db_id,
            side: Side::Buy,
            qty: 1.0,
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
