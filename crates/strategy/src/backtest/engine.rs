use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;

use domain::{InstrumentId, Side};

use crate::core::r#trait::{Kline, Strategy, StrategyContext};
use crate::core::r#trait::Signal;

#[derive(Debug, Error)]
pub enum BacktestError {
    #[error("Invalid config: {0}")]
    InvalidConfig(String),
    #[error("Insufficient capital")]
    InsufficientCapital,
    #[error("Max positions reached")]
    MaxPositionsReached,
    #[error("Strategy error: {0}")]
    StrategyError(String),
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct BacktestConfig {
    pub start_ts_ms: i64,
    pub end_ts_ms: i64,
    pub initial_capital: f64,
    pub instruments: Vec<InstrumentId>,
    pub granularity_ms: u64,
    pub max_positions: usize,
    pub commission_rate: f64,
}

#[derive(Debug, Clone)]
pub struct Position {
    pub instrument: InstrumentId,
    pub side: Side,
    pub entry_price: f64,
    pub quantity: f64,
    pub entry_ts_ms: i64,
    pub unrealised_pnl: f64,
    pub realised_pnl: f64,
}

#[derive(Debug, Clone)]
pub struct BacktestState {
    pub ts_ms: i64,
    pub capital: f64,
    pub positions: HashMap<InstrumentId, Position>,
    pub equity_curve: Vec<(i64, f64)>,
    pub trade_count: u64,
}

impl BacktestState {
    pub fn new(initial_capital: f64, start_ts_ms: i64) -> Self {
        Self {
            ts_ms: start_ts_ms,
            capital: initial_capital,
            positions: HashMap::new(),
            equity_curve: Vec::new(),
            trade_count: 0,
        }
    }

    pub fn total_equity(&self) -> f64 {
        let unrealised: f64 = self.positions.values().map(|p| p.unrealised_pnl).sum();
        self.capital + unrealised
    }

    pub fn open_position(
        &mut self,
        instrument: InstrumentId,
        side: Side,
        price: f64,
        qty: f64,
        ts_ms: i64,
    ) {
        let position = Position {
            instrument: instrument.clone(),
            side,
            entry_price: price,
            quantity: qty,
            entry_ts_ms: ts_ms,
            unrealised_pnl: 0.0,
            realised_pnl: 0.0,
        };
        self.positions.insert(instrument, position);
    }

    pub fn close_position(
        &mut self,
        instrument: &InstrumentId,
        close_price: f64,
        commission_rate: f64,
    ) -> Option<f64> {
        let pos = self.positions.remove(instrument)?;
        let pnl = match pos.side {
            Side::Buy => (close_price - pos.entry_price) * pos.quantity,
            Side::Sell => (pos.entry_price - close_price) * pos.quantity,
        };
        let commission = close_price * pos.quantity * commission_rate;
        let net_pnl = pnl - commission;
        // Return capital
        self.capital += pos.entry_price * pos.quantity + net_pnl;
        self.trade_count += 1;
        Some(net_pnl)
    }

    pub fn update_unrealised(&mut self, instrument: &InstrumentId, current_price: f64) {
        if let Some(pos) = self.positions.get_mut(instrument) {
            pos.unrealised_pnl = match pos.side {
                Side::Buy => (current_price - pos.entry_price) * pos.quantity,
                Side::Sell => (pos.entry_price - current_price) * pos.quantity,
            };
        }
    }

    pub fn snapshot_equity(&mut self) {
        let equity = self.total_equity();
        self.equity_curve.push((self.ts_ms, equity));
    }
}

pub struct BacktestEngine {
    pub config: BacktestConfig,
    pub state: BacktestState,
    pub strategy: Arc<dyn Strategy>,
}

impl BacktestEngine {
    pub fn new(config: BacktestConfig, strategy: Arc<dyn Strategy>) -> Self {
        let state = BacktestState::new(config.initial_capital, config.start_ts_ms);
        Self { config, state, strategy }
    }

    pub fn step(&mut self, kline: &Kline) -> Result<Option<Signal>, BacktestError> {
        self.state.ts_ms = kline.close_ts_ms;

        let mut ctx = StrategyContext::new(kline.instrument.clone(), kline.close_ts_ms);
        ctx.update(Some(kline.close), Some(kline.close_ts_ms));

        let signal = self
            .strategy
            .evaluate(&ctx)
            .map_err(|e| BacktestError::StrategyError(e.to_string()))?;

        if let Some(ref sig) = signal {
            self.apply_signal(sig.clone(), kline)?;
        }

        // Update unrealised PnL for all open positions
        for instrument in self.state.positions.keys().cloned().collect::<Vec<_>>() {
            self.state.update_unrealised(&instrument, kline.close);
        }

        self.state.snapshot_equity();

        Ok(signal)
    }

    pub fn apply_signal(&mut self, signal: Signal, kline: &Kline) -> Result<(), BacktestError> {
        let fill_price = kline.close;
        let instrument = &signal.instrument;

        // Close opposite position if exists
        if let Some(existing) = self.state.positions.get(instrument) {
            let opposite = match signal.side {
                Side::Buy => existing.side == Side::Sell,
                Side::Sell => existing.side == Side::Buy,
            };
            if opposite {
                self.state.close_position(instrument, fill_price, self.config.commission_rate);
            }
        }

        // Check if we already have same-side position; if so, skip
        if self.state.positions.contains_key(instrument) {
            return Ok(());
        }

        // Check max positions
        if self.state.positions.len() >= self.config.max_positions {
            return Err(BacktestError::MaxPositionsReached);
        }

        // Check capital
        let cost = fill_price * signal.quantity;
        let commission = cost * self.config.commission_rate;
        if self.state.capital < cost + commission {
            return Err(BacktestError::InsufficientCapital);
        }

        self.state.capital -= cost + commission;
        self.state.open_position(
            instrument.clone(),
            signal.side.clone(),
            fill_price,
            signal.quantity,
            kline.close_ts_ms,
        );

        Ok(())
    }

    pub fn run(&mut self, bars: Vec<Kline>) -> Result<BacktestState, BacktestError> {
        for bar in &bars {
            self.step(bar)?;
        }
        Ok(self.state.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::{InstrumentId, Side, Venue};
    use crate::core::r#trait::{Signal, StrategyError};
    use std::collections::HashMap;

    struct AlwaysBuy;

    impl Strategy for AlwaysBuy {
        fn evaluate(&self, context: &StrategyContext) -> Result<Option<Signal>, StrategyError> {
            Ok(Some(Signal::new(
                context.instrument.clone(),
                Side::Buy,
                1.0,
                None,
                context.ts_ms,
                "always_buy".to_string(),
                HashMap::new(),
            )))
        }
        fn name(&self) -> &str { "always_buy" }
    }

    fn make_kline(instrument: InstrumentId, close: f64, ts: i64) -> Kline {
        Kline {
            instrument,
            open_ts_ms: ts - 60_000,
            close_ts_ms: ts,
            open: close,
            high: close,
            low: close,
            close,
            volume: 100.0,
        }
    }

    fn make_config(instrument: InstrumentId) -> BacktestConfig {
        BacktestConfig {
            start_ts_ms: 0,
            end_ts_ms: 600_000,
            initial_capital: 10_000.0,
            instruments: vec![instrument],
            granularity_ms: 60_000,
            max_positions: 5,
            commission_rate: 0.001,
        }
    }

    #[test]
    fn step_single_bar_produces_signal() {
        let instrument = InstrumentId::new(Venue::Crypto, "BTC");
        let config = make_config(instrument.clone());
        let mut engine = BacktestEngine::new(config, Arc::new(AlwaysBuy));
        let kline = make_kline(instrument, 100.0, 60_000);
        let signal = engine.step(&kline).unwrap();
        assert!(signal.is_some());
        assert_eq!(signal.unwrap().side, Side::Buy);
    }

    #[test]
    fn run_5_bars_produces_equity_curve() {
        let instrument = InstrumentId::new(Venue::Crypto, "BTC");
        let config = make_config(instrument.clone());
        let mut engine = BacktestEngine::new(config, Arc::new(AlwaysBuy));
        let bars: Vec<Kline> = (1..=5)
            .map(|i| make_kline(instrument.clone(), 100.0 + i as f64, i * 60_000))
            .collect();
        let state = engine.run(bars).unwrap();
        assert_eq!(state.equity_curve.len(), 5);
    }

    #[test]
    fn insufficient_capital_returns_error() {
        let instrument = InstrumentId::new(Venue::Crypto, "BTC");
        let mut config = make_config(instrument.clone());
        config.initial_capital = 0.01; // Not enough for any real trade
        let mut engine = BacktestEngine::new(config, Arc::new(AlwaysBuy));
        let kline = make_kline(instrument, 100.0, 60_000);
        let result = engine.step(&kline);
        assert!(matches!(result, Err(BacktestError::InsufficientCapital)));
    }
}
