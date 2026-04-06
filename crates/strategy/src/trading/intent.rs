// Order Intent Processor — converts strategy Signals into aggregated order
// intents with size caps and time-window aggregation.

use std::collections::HashMap;

use domain::{InstrumentId, Side};
use thiserror::Error;

use crate::core::r#trait::Signal;

// ─── IntentError ─────────────────────────────────────────────────────────────

#[derive(Debug, Error)]
pub enum IntentError {
    #[error("Quantity {qty} below minimum {min}")]
    BelowMinSize { qty: f64, min: f64 },

    #[error("Quantity {qty} exceeds maximum {max}")]
    ExceedsMaxSize { qty: f64, max: f64 },

    #[error("Invalid signal: {0}")]
    InvalidSignal(String),
}

// ─── IntentConfig ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct IntentConfig {
    pub max_order_size: f64,
    pub min_order_size: f64,
    pub default_slippage_bps: f64,
    pub aggregate_window_ms: u64,
}

impl Default for IntentConfig {
    fn default() -> Self {
        Self {
            max_order_size: 1_000_000.0,
            min_order_size: 0.0001,
            default_slippage_bps: 5.0,
            aggregate_window_ms: 1_000,
        }
    }
}

// ─── StrategyOrderIntent ─────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct StrategyOrderIntent {
    pub strategy_id: String,
    pub instrument: InstrumentId,
    pub side: Side,
    pub raw_quantity: f64,
    pub max_quantity: f64,
    pub limit_price: Option<f64>,
    /// 0.0–1.0, higher = more aggressive pricing
    pub urgency: f64,
    pub ts_ms: i64,
}

// ─── SignalBatch ──────────────────────────────────────────────────────────────

pub struct SignalBatch {
    pub signals: Vec<Signal>,
    pub ts_ms: i64,
}

// ─── IntentProcessor ─────────────────────────────────────────────────────────

pub struct IntentProcessor {
    pub config: IntentConfig,
    pub pending: Vec<StrategyOrderIntent>,
    pub last_flush_ts: i64,
}

impl IntentProcessor {
    pub fn new(config: IntentConfig) -> Self {
        Self {
            config,
            pending: Vec::new(),
            last_flush_ts: 0,
        }
    }

    /// Convert a signal to an intent and add it to the pending queue, applying
    /// size caps.  Silently drops intents below min size.
    pub fn ingest(&mut self, signal: Signal, ts_ms: i64) {
        if signal.quantity <= 0.0 {
            return;
        }
        let qty = signal.quantity.min(self.config.max_order_size);
        if qty < self.config.min_order_size {
            return;
        }
        let intent = StrategyOrderIntent {
            strategy_id: signal.strategy_id,
            instrument: signal.instrument,
            side: signal.side,
            raw_quantity: qty,
            max_quantity: self.config.max_order_size,
            limit_price: signal.limit_price,
            urgency: 0.5,
            ts_ms,
        };
        self.pending.push(intent);
    }

    /// Flush if the aggregation window has elapsed.
    pub fn flush(&mut self, ts_ms: i64) -> Vec<StrategyOrderIntent> {
        let window = self.config.aggregate_window_ms as i64;
        if ts_ms - self.last_flush_ts >= window {
            self.last_flush_ts = ts_ms;
            let pending = std::mem::take(&mut self.pending);
            Self::aggregate(pending)
        } else {
            Vec::new()
        }
    }

    /// Always flush regardless of window.
    pub fn force_flush(&mut self, ts_ms: i64) -> Vec<StrategyOrderIntent> {
        self.last_flush_ts = ts_ms;
        let pending = std::mem::take(&mut self.pending);
        Self::aggregate(pending)
    }

    /// Group intents by (instrument, side) and merge them: sum qty, avg
    /// limit_price (weighted by qty), keep first strategy_id.
    pub fn aggregate(intents: Vec<StrategyOrderIntent>) -> Vec<StrategyOrderIntent> {
        // Key: (instrument, side discriminant)
        let mut groups: HashMap<(InstrumentId, String), StrategyOrderIntent> = HashMap::new();

        for intent in intents {
            let side_key = format!("{:?}", intent.side);
            let key = (intent.instrument.clone(), side_key);
            groups
                .entry(key)
                .and_modify(|acc| {
                    // Weighted average limit_price
                    let new_lp = match (acc.limit_price, intent.limit_price) {
                        (Some(ap), Some(bp)) => {
                            let total_qty = acc.raw_quantity + intent.raw_quantity;
                            Some((ap * acc.raw_quantity + bp * intent.raw_quantity) / total_qty)
                        }
                        (Some(ap), None) => Some(ap),
                        (None, Some(bp)) => Some(bp),
                        (None, None) => None,
                    };
                    acc.raw_quantity += intent.raw_quantity;
                    acc.limit_price = new_lp;
                    // urgency: keep max
                    if intent.urgency > acc.urgency {
                        acc.urgency = intent.urgency;
                    }
                })
                .or_insert(intent);
        }

        groups.into_values().collect()
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use domain::{InstrumentId, Side, Venue};
    use std::collections::HashMap;

    fn btc() -> InstrumentId {
        InstrumentId::new(Venue::Crypto, "BTC")
    }

    fn eth() -> InstrumentId {
        InstrumentId::new(Venue::Crypto, "ETH")
    }

    fn buy_signal(instrument: InstrumentId, qty: f64, ts: i64) -> Signal {
        Signal::new(instrument, Side::Buy, qty, None, ts, "s1".into(), HashMap::new())
    }

    fn sell_signal(instrument: InstrumentId, qty: f64, ts: i64) -> Signal {
        Signal::new(instrument, Side::Sell, qty, None, ts, "s1".into(), HashMap::new())
    }

    #[test]
    fn ingest_and_force_flush() {
        let cfg = IntentConfig {
            max_order_size: 10.0,
            min_order_size: 0.01,
            default_slippage_bps: 5.0,
            aggregate_window_ms: 1000,
        };
        let mut proc = IntentProcessor::new(cfg);
        proc.ingest(buy_signal(btc(), 1.0, 0), 0);
        proc.ingest(buy_signal(btc(), 2.0, 0), 0);

        let intents = proc.force_flush(0);
        assert_eq!(intents.len(), 1);
        assert!((intents[0].raw_quantity - 3.0).abs() < 1e-9);
    }

    #[test]
    fn window_aggregation() {
        let cfg = IntentConfig {
            aggregate_window_ms: 1000,
            ..IntentConfig::default()
        };
        let mut proc = IntentProcessor::new(cfg);
        proc.ingest(buy_signal(btc(), 1.0, 0), 0);

        // Not yet time
        let intents = proc.flush(500);
        assert!(intents.is_empty());

        // Window elapsed
        let intents = proc.flush(1000);
        assert_eq!(intents.len(), 1);
    }

    #[test]
    fn max_size_cap() {
        let cfg = IntentConfig {
            max_order_size: 5.0,
            min_order_size: 0.01,
            ..IntentConfig::default()
        };
        let mut proc = IntentProcessor::new(cfg);
        proc.ingest(buy_signal(btc(), 100.0, 0), 0);
        let intents = proc.force_flush(0);
        // qty is capped at 5.0
        assert!((intents[0].raw_quantity - 5.0).abs() < 1e-9);
    }

    #[test]
    fn min_size_filters() {
        let cfg = IntentConfig {
            min_order_size: 1.0,
            ..IntentConfig::default()
        };
        let mut proc = IntentProcessor::new(cfg);
        proc.ingest(buy_signal(btc(), 0.001, 0), 0);
        let intents = proc.force_flush(0);
        assert!(intents.is_empty());
    }

    #[test]
    fn different_instruments_not_merged() {
        let cfg = IntentConfig::default();
        let mut proc = IntentProcessor::new(cfg);
        proc.ingest(buy_signal(btc(), 1.0, 0), 0);
        proc.ingest(buy_signal(eth(), 1.0, 0), 0);
        let intents = proc.force_flush(0);
        assert_eq!(intents.len(), 2);
    }

    #[test]
    fn same_instrument_opposite_sides_not_merged() {
        let cfg = IntentConfig::default();
        let mut proc = IntentProcessor::new(cfg);
        proc.ingest(buy_signal(btc(), 1.0, 0), 0);
        proc.ingest(sell_signal(btc(), 1.0, 0), 0);
        let intents = proc.force_flush(0);
        assert_eq!(intents.len(), 2);
    }

    #[test]
    fn aggregate_weighted_avg_price() {
        let a = StrategyOrderIntent {
            strategy_id: "s1".into(),
            instrument: btc(),
            side: Side::Buy,
            raw_quantity: 2.0,
            max_quantity: 100.0,
            limit_price: Some(100.0),
            urgency: 0.5,
            ts_ms: 0,
        };
        let b = StrategyOrderIntent {
            strategy_id: "s1".into(),
            instrument: btc(),
            side: Side::Buy,
            raw_quantity: 1.0,
            max_quantity: 100.0,
            limit_price: Some(200.0),
            urgency: 0.5,
            ts_ms: 0,
        };
        let merged = IntentProcessor::aggregate(vec![a, b]);
        assert_eq!(merged.len(), 1);
        let m = &merged[0];
        assert!((m.raw_quantity - 3.0).abs() < 1e-9);
        // (2*100 + 1*200) / 3 = 133.33...
        let expected = (2.0 * 100.0 + 1.0 * 200.0) / 3.0;
        assert!((m.limit_price.unwrap() - expected).abs() < 1e-9);
    }
}
