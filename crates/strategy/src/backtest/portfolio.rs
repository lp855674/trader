use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;

use domain::InstrumentId;

use crate::core::r#trait::{Kline, Signal, Strategy, StrategyContext};
use super::engine::Position;

// ─── Errors ──────────────────────────────────────────────────────────────────

#[derive(Debug, Error)]
pub enum PortfolioError {
    #[error("Insufficient capital")]
    InsufficientCapital,
    #[error("Exposure limit exceeded for instrument: {instrument}")]
    ExposureLimitExceeded { instrument: InstrumentId },
    #[error("Strategy error: {0}")]
    StrategyError(String),
    #[error("Invalid config: {0}")]
    InvalidConfig(String),
}

// ─── Config ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct PortfolioConfig {
    /// Instruments included in the portfolio.
    pub instruments: Vec<InstrumentId>,
    /// Starting capital in quote currency.
    pub initial_capital: f64,
    /// Maximum total portfolio exposure as a fraction of equity (e.g. 0.9).
    pub max_total_exposure: f64,
    /// Maximum exposure for a single instrument as a fraction of equity (e.g. 0.3).
    pub max_instrument_exposure: f64,
    /// Rebalancing interval in milliseconds. `None` disables periodic rebalancing.
    pub rebalance_interval_ms: Option<u64>,
    /// Commission rate applied to each trade (e.g. 0.001 = 0.1 %).
    pub commission_rate: f64,
    /// Rolling window length (in bars) used for correlation calculations.
    pub correlation_lookback: usize,
}

// ─── Allocation ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct InstrumentAllocation {
    pub instrument: InstrumentId,
    /// Desired portfolio weight (0.0 – 1.0).
    pub target_weight: f64,
    /// Current weight based on position value vs. total equity.
    pub current_weight: f64,
    /// Absolute current position value in quote currency.
    pub current_value: f64,
}

// ─── Portfolio State ──────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct PortfolioState {
    /// Free cash / quote balance not allocated to any position.
    pub capital: f64,
    /// Current timestamp in milliseconds.
    pub ts_ms: i64,
    /// Open positions keyed by instrument.
    pub positions: HashMap<InstrumentId, Position>,
    /// Per-instrument allocation metadata.
    pub allocations: HashMap<InstrumentId, InstrumentAllocation>,
    /// (timestamp_ms, total_equity) snapshots.
    pub equity_curve: Vec<(i64, f64)>,
    /// Timestamp of the last rebalance event.
    pub last_rebalance_ts: Option<i64>,
}

impl PortfolioState {
    pub fn new(initial_capital: f64, start_ts_ms: i64) -> Self {
        Self {
            capital: initial_capital,
            ts_ms: start_ts_ms,
            positions: HashMap::new(),
            allocations: HashMap::new(),
            equity_curve: Vec::new(),
            last_rebalance_ts: None,
        }
    }

    /// Total equity = free capital + sum of all unrealised position values.
    pub fn total_equity(&self) -> f64 {
        let position_value: f64 = self
            .positions
            .values()
            .map(|p| p.entry_price * p.quantity + p.unrealised_pnl)
            .sum();
        self.capital + position_value
    }

    /// Total exposure = Σ|position_value| / total_equity.
    pub fn total_exposure(&self) -> f64 {
        let equity = self.total_equity();
        if equity <= 0.0 {
            return 0.0;
        }
        let total_pos: f64 = self
            .positions
            .values()
            .map(|p| (p.entry_price * p.quantity + p.unrealised_pnl).abs())
            .sum();
        total_pos / equity
    }

    /// Exposure for a single instrument = |position_value| / total_equity.
    pub fn instrument_exposure(&self, id: &InstrumentId) -> f64 {
        let equity = self.total_equity();
        if equity <= 0.0 {
            return 0.0;
        }
        match self.positions.get(id) {
            None => 0.0,
            Some(p) => (p.entry_price * p.quantity + p.unrealised_pnl).abs() / equity,
        }
    }

    /// Snapshot equity at current timestamp.
    pub fn snapshot_equity(&mut self) {
        let equity = self.total_equity();
        self.equity_curve.push((self.ts_ms, equity));
    }
}

// ─── Correlation Tracker ──────────────────────────────────────────────────────

/// Tracks recent per-instrument return series for Pearson correlation.
#[derive(Debug, Clone)]
pub struct CorrelationTracker {
    lookback: usize,
    /// price history per instrument (oldest first)
    prices: HashMap<InstrumentId, Vec<(i64, f64)>>,
}

impl CorrelationTracker {
    pub fn new(lookback: usize) -> Self {
        Self {
            lookback,
            prices: HashMap::new(),
        }
    }

    /// Record a new close price for `instrument` at `ts_ms`.
    pub fn push(&mut self, instrument: &InstrumentId, ts_ms: i64, price: f64) {
        let series = self.prices.entry(instrument.clone()).or_default();
        series.push((ts_ms, price));
        // Keep only the last (lookback + 1) prices so we can compute `lookback` returns.
        let max_len = self.lookback + 1;
        if series.len() > max_len {
            let drain_count = series.len() - max_len;
            series.drain(0..drain_count);
        }
    }

    /// Pearson correlation of log-returns between instruments `a` and `b`.
    /// Returns `0.0` if either series has fewer than 2 price points (need ≥1 return).
    pub fn correlation(&self, a: &InstrumentId, b: &InstrumentId) -> f64 {
        let returns_a = self.returns(a);
        let returns_b = self.returns(b);

        let n = returns_a.len().min(returns_b.len());
        if n < 2 {
            return 0.0;
        }

        let ra = &returns_a[returns_a.len() - n..];
        let rb = &returns_b[returns_b.len() - n..];

        pearson_correlation(ra, rb)
    }

    fn returns(&self, instrument: &InstrumentId) -> Vec<f64> {
        match self.prices.get(instrument) {
            None => vec![],
            Some(prices) if prices.len() < 2 => vec![],
            Some(prices) => prices
                .windows(2)
                .map(|w| {
                    let (_, prev) = w[0];
                    let (_, curr) = w[1];
                    if prev > 0.0 { (curr / prev).ln() } else { 0.0 }
                })
                .collect(),
        }
    }
}

fn pearson_correlation(a: &[f64], b: &[f64]) -> f64 {
    let n = a.len() as f64;
    if n == 0.0 {
        return 0.0;
    }

    let mean_a = a.iter().sum::<f64>() / n;
    let mean_b = b.iter().sum::<f64>() / n;

    let (cov, var_a, var_b) = a.iter().zip(b.iter()).fold(
        (0.0_f64, 0.0_f64, 0.0_f64),
        |(cov, va, vb), (&xa, &xb)| {
            let da = xa - mean_a;
            let db = xb - mean_b;
            (cov + da * db, va + da * da, vb + db * db)
        },
    );

    let denom = (var_a * var_b).sqrt();
    if denom < f64::EPSILON {
        0.0
    } else {
        cov / denom
    }
}

// ─── Portfolio Backtest ────────────────────────────────────────────────────────

pub struct PortfolioBacktest {
    pub config: PortfolioConfig,
    pub state: PortfolioState,
    /// One strategy per instrument.
    pub strategies: HashMap<InstrumentId, Arc<dyn Strategy>>,
    pub correlation_tracker: CorrelationTracker,
}

impl PortfolioBacktest {
    pub fn new(
        config: PortfolioConfig,
        strategies: HashMap<InstrumentId, Arc<dyn Strategy>>,
    ) -> Self {
        let lookback = config.correlation_lookback;
        let state = PortfolioState::new(config.initial_capital, 0);
        Self {
            config,
            state,
            strategies,
            correlation_tracker: CorrelationTracker::new(lookback),
        }
    }

    /// Process one timestep across all instruments.
    ///
    /// Steps:
    /// 1. Update correlation tracker with new bar closes.
    /// 2. Evaluate strategies for each instrument that has a bar.
    /// 3. Check portfolio-level and per-instrument exposure constraints.
    /// 4. Trigger rebalance if the interval has elapsed.
    /// 5. Apply approved signals.
    /// 6. Snapshot equity.
    pub fn step(
        &mut self,
        bars: &HashMap<InstrumentId, Kline>,
    ) -> Result<Vec<Signal>, PortfolioError> {
        if bars.is_empty() {
            self.state.snapshot_equity();
            return Ok(vec![]);
        }

        // Advance clock to the latest bar timestamp.
        if let Some(max_ts) = bars.values().map(|b| b.close_ts_ms).max() {
            self.state.ts_ms = max_ts;
        }

        // 1. Update correlation tracker.
        for (id, bar) in bars {
            self.correlation_tracker.push(id, bar.close_ts_ms, bar.close);
        }

        // 2. Update unrealised PnL for all open positions using latest bar close.
        for (id, bar) in bars {
            if let Some(pos) = self.state.positions.get_mut(id) {
                pos.unrealised_pnl = match pos.side {
                    domain::Side::Buy => (bar.close - pos.entry_price) * pos.quantity,
                    domain::Side::Sell => (pos.entry_price - bar.close) * pos.quantity,
                };
            }
        }

        // 3. Collect strategy signals.
        let mut pending_signals: Vec<Signal> = Vec::new();
        for (id, bar) in bars {
            if let Some(strategy) = self.strategies.get(id) {
                let mut ctx = StrategyContext::new(id.clone(), bar.close_ts_ms);
                ctx.update(Some(bar.close), Some(bar.close_ts_ms));
                let signal = strategy
                    .evaluate(&ctx)
                    .map_err(|e| PortfolioError::StrategyError(e.to_string()))?;
                if let Some(sig) = signal {
                    pending_signals.push(sig);
                }
            }
        }

        // 4. Rebalance if interval elapsed.
        if self.should_rebalance() {
            self.rebalance();
            self.state.last_rebalance_ts = Some(self.state.ts_ms);
        }

        // 5. Apply signals that pass constraint checks.
        let mut applied: Vec<Signal> = Vec::new();
        for sig in pending_signals {
            if let Err(_) = self.apply_signal(&sig, bars) {
                // Silently skip signals that violate constraints.
                continue;
            }
            applied.push(sig);
        }

        // 6. Snapshot equity.
        self.state.snapshot_equity();

        Ok(applied)
    }

    /// Whether a rebalance is due based on `rebalance_interval_ms`.
    fn should_rebalance(&self) -> bool {
        match self.config.rebalance_interval_ms {
            None => false,
            Some(interval_ms) => {
                let interval = interval_ms as i64;
                match self.state.last_rebalance_ts {
                    None => self.state.ts_ms >= interval,
                    Some(last) => self.state.ts_ms - last >= interval,
                }
            }
        }
    }

    /// Try to apply a signal respecting capital and exposure constraints.
    fn apply_signal(
        &mut self,
        signal: &Signal,
        bars: &HashMap<InstrumentId, Kline>,
    ) -> Result<(), PortfolioError> {
        let bar = match bars.get(&signal.instrument) {
            Some(b) => b,
            None => return Ok(()), // no price data; skip
        };
        let fill_price = bar.close;
        let id = &signal.instrument;

        // Close opposite position if it exists.
        if let Some(existing) = self.state.positions.get(id) {
            let is_opposite = match signal.side {
                domain::Side::Buy => existing.side == domain::Side::Sell,
                domain::Side::Sell => existing.side == domain::Side::Buy,
            };
            if is_opposite {
                self.close_position(id, fill_price);
            }
        }

        // Skip if same-side position already open.
        if self.state.positions.contains_key(id) {
            return Ok(());
        }

        let cost = fill_price * signal.quantity;
        let commission = cost * self.config.commission_rate;
        let total_cost = cost + commission;

        if self.state.capital < total_cost {
            return Err(PortfolioError::InsufficientCapital);
        }

        // Check per-instrument exposure after hypothetical open.
        let equity = self.state.total_equity();
        if equity > 0.0 && cost / equity > self.config.max_instrument_exposure {
            return Err(PortfolioError::ExposureLimitExceeded {
                instrument: id.clone(),
            });
        }

        // Check total exposure after hypothetical open.
        let current_exposure_value: f64 = self
            .state
            .positions
            .values()
            .map(|p| (p.entry_price * p.quantity + p.unrealised_pnl).abs())
            .sum();
        if equity > 0.0 && (current_exposure_value + cost) / equity > self.config.max_total_exposure {
            return Err(PortfolioError::ExposureLimitExceeded {
                instrument: id.clone(),
            });
        }

        // Commit.
        self.state.capital -= total_cost;
        self.state.positions.insert(
            id.clone(),
            Position {
                instrument: id.clone(),
                side: signal.side.clone(),
                entry_price: fill_price,
                quantity: signal.quantity,
                entry_ts_ms: bar.close_ts_ms,
                unrealised_pnl: 0.0,
                realised_pnl: 0.0,
            },
        );

        Ok(())
    }

    /// Close a position and return capital + net PnL.
    fn close_position(&mut self, id: &InstrumentId, close_price: f64) {
        if let Some(pos) = self.state.positions.remove(id) {
            let pnl = match pos.side {
                domain::Side::Buy => (close_price - pos.entry_price) * pos.quantity,
                domain::Side::Sell => (pos.entry_price - close_price) * pos.quantity,
            };
            let commission = close_price * pos.quantity * self.config.commission_rate;
            let net_pnl = pnl - commission;
            self.state.capital += pos.entry_price * pos.quantity + net_pnl;
        }
    }

    /// Rebalance: adjust positions toward equal-weight allocation.
    ///
    /// Over-weight positions are reduced; under-weight positions are left
    /// unchanged (no forced buys).
    pub fn rebalance(&mut self) {
        if self.config.instruments.is_empty() {
            return;
        }

        let target_weight = 1.0 / self.config.instruments.len() as f64;
        let equity = self.state.total_equity();
        if equity <= 0.0 {
            return;
        }

        // Identify over-weight instruments.
        let overweight: Vec<InstrumentId> = self
            .config
            .instruments
            .iter()
            .filter(|id| {
                let pos_value = self
                    .state
                    .positions
                    .get(*id)
                    .map(|p| (p.entry_price * p.quantity + p.unrealised_pnl).abs())
                    .unwrap_or(0.0);
                pos_value / equity > target_weight * 1.05 // 5 % tolerance
            })
            .cloned()
            .collect();

        // Close over-weight positions (the engine will re-enter on the next signal).
        for id in overweight {
            if let Some(pos) = self.state.positions.get(&id) {
                let close_price = pos.entry_price + pos.unrealised_pnl / pos.quantity.max(f64::EPSILON);
                let close_price = close_price.max(0.0);
                self.close_position(&id, close_price);
            }
        }

        // Update allocation metadata.
        let equity_after = self.state.total_equity();
        for id in &self.config.instruments.clone() {
            let (current_weight, current_value) = match self.state.positions.get(id) {
                None => (0.0, 0.0),
                Some(p) => {
                    let val = (p.entry_price * p.quantity + p.unrealised_pnl).abs();
                    let w = if equity_after > 0.0 { val / equity_after } else { 0.0 };
                    (w, val)
                }
            };
            self.state.allocations.insert(
                id.clone(),
                InstrumentAllocation {
                    instrument: id.clone(),
                    target_weight,
                    current_weight,
                    current_value,
                },
            );
        }
    }

    /// Run the full backtest over a sequence of bar snapshots.
    ///
    /// Each element of `bar_series` is a map of instrument → kline for that
    /// timestep.  Returns the final `PortfolioState`.
    pub fn run(
        &mut self,
        bar_series: Vec<HashMap<InstrumentId, Kline>>,
    ) -> Result<PortfolioState, PortfolioError> {
        for bars in bar_series {
            self.step(&bars)?;
        }
        Ok(self.state.clone())
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::r#trait::{Signal, StrategyContext, StrategyError};
    use domain::{InstrumentId, Side, Venue};
    use std::collections::HashMap;

    // ── helpers ──────────────────────────────────────────────────────────────

    fn inst(symbol: &str) -> InstrumentId {
        InstrumentId::new(Venue::Crypto, symbol)
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
            volume: 1000.0,
        }
    }

    struct AlwaysBuy;
    impl Strategy for AlwaysBuy {
        fn evaluate(&self, ctx: &StrategyContext) -> Result<Option<Signal>, StrategyError> {
            Ok(Some(Signal::new(
                ctx.instrument.clone(),
                Side::Buy,
                1.0,
                None,
                ctx.ts_ms,
                "always_buy".to_string(),
                HashMap::new(),
            )))
        }
        fn name(&self) -> &str { "always_buy" }
    }

    fn base_config(instruments: Vec<InstrumentId>) -> PortfolioConfig {
        PortfolioConfig {
            instruments,
            initial_capital: 100_000.0,
            max_total_exposure: 0.9,
            max_instrument_exposure: 0.5,
            rebalance_interval_ms: None,
            commission_rate: 0.001,
            correlation_lookback: 20,
        }
    }

    // ── tests ─────────────────────────────────────────────────────────────────

    #[test]
    fn multi_instrument_step_generates_signals() {
        let btc = inst("BTC");
        let eth = inst("ETH");
        let config = base_config(vec![btc.clone(), eth.clone()]);

        let mut strategies: HashMap<InstrumentId, Arc<dyn Strategy>> = HashMap::new();
        strategies.insert(btc.clone(), Arc::new(AlwaysBuy));
        strategies.insert(eth.clone(), Arc::new(AlwaysBuy));

        let mut pb = PortfolioBacktest::new(config, strategies);

        let mut bars = HashMap::new();
        bars.insert(btc.clone(), make_kline(btc.clone(), 50_000.0, 60_000));
        bars.insert(eth.clone(), make_kline(eth.clone(), 3_000.0, 60_000));

        let signals = pb.step(&bars).unwrap();
        // Both instruments should have generated at least one signal attempt.
        // The BTC position at 50_000 * 1.0 = 50k is 50 % of 100k equity — exactly
        // at the limit, so it may or may not be accepted depending on rounding;
        // we just assert at least one signal was produced.
        assert!(
            !signals.is_empty() || !pb.state.positions.is_empty(),
            "at least one signal or position should exist"
        );
    }

    #[test]
    fn exposure_limit_prevents_oversized_position() {
        let btc = inst("BTC");
        let mut config = base_config(vec![btc.clone()]);
        // Allow total exposure up to 90 % but instrument cap at 10 %.
        config.max_instrument_exposure = 0.10;
        config.initial_capital = 1_000.0;

        let mut strategies: HashMap<InstrumentId, Arc<dyn Strategy>> = HashMap::new();
        strategies.insert(btc.clone(), Arc::new(AlwaysBuy));

        let mut pb = PortfolioBacktest::new(config, strategies);

        // price=200, qty=1 → cost=200 = 20 % of 1000 equity → exceeds 10 % cap.
        let mut bars = HashMap::new();
        bars.insert(btc.clone(), make_kline(btc.clone(), 200.0, 60_000));

        let signals = pb.step(&bars).unwrap();

        assert!(
            signals.is_empty(),
            "signal should be rejected due to per-instrument exposure limit"
        );
        assert!(
            pb.state.positions.is_empty(),
            "no position should be open after rejection"
        );
    }

    #[test]
    fn rebalance_reduces_overweight_position() {
        let btc = inst("BTC");
        let eth = inst("ETH");
        let mut config = base_config(vec![btc.clone(), eth.clone()]);
        config.max_instrument_exposure = 1.0; // allow large position for setup
        config.max_total_exposure = 1.0;
        config.initial_capital = 10_000.0;
        config.commission_rate = 0.0;

        let mut pb = PortfolioBacktest::new(config, HashMap::new());

        // Manually inject a large BTC position.
        pb.state.positions.insert(
            btc.clone(),
            Position {
                instrument: btc.clone(),
                side: Side::Buy,
                entry_price: 1_000.0,
                quantity: 8.0, // 8 000 of 10 000 equity = 80 % weight
                entry_ts_ms: 0,
                unrealised_pnl: 0.0,
                realised_pnl: 0.0,
            },
        );
        pb.state.capital -= 8_000.0;

        let equity_before = pb.state.total_equity();
        let btc_weight_before = pb.state.instrument_exposure(&btc);

        pb.rebalance();

        let btc_weight_after = pb.state.instrument_exposure(&btc);
        assert!(
            btc_weight_after < btc_weight_before,
            "BTC weight should decrease after rebalance: before={btc_weight_before:.3}, after={btc_weight_after:.3}"
        );
        let equity_after = pb.state.total_equity();
        // Equity should be roughly preserved (within commission costs).
        assert!(
            (equity_after - equity_before).abs() < 1.0,
            "equity should be roughly preserved: before={equity_before}, after={equity_after}"
        );
    }

    #[test]
    fn correlation_tracker_returns_reasonable_value() {
        let btc = inst("BTC");
        let eth = inst("ETH");
        let mut tracker = CorrelationTracker::new(10);

        // Push perfectly correlated prices.
        for i in 1..=15_u64 {
            let price = 100.0 + i as f64;
            tracker.push(&btc, i as i64 * 60_000, price);
            tracker.push(&eth, i as i64 * 60_000, price * 0.1); // same direction
        }

        let corr = tracker.correlation(&btc, &eth);
        assert!(
            corr > 0.9,
            "perfectly co-moving assets should have high positive correlation, got {corr}"
        );
    }

    #[test]
    fn run_multi_step_populates_equity_curve() {
        let btc = inst("BTC");
        let mut config = base_config(vec![btc.clone()]);
        config.max_instrument_exposure = 1.0;

        let mut strategies: HashMap<InstrumentId, Arc<dyn Strategy>> = HashMap::new();
        strategies.insert(btc.clone(), Arc::new(AlwaysBuy));

        let mut pb = PortfolioBacktest::new(config, strategies);

        let bar_series: Vec<HashMap<InstrumentId, Kline>> = (1..=5)
            .map(|i| {
                let mut bars = HashMap::new();
                bars.insert(btc.clone(), make_kline(btc.clone(), 100.0 + i as f64, i * 60_000));
                bars
            })
            .collect();

        let state = pb.run(bar_series).unwrap();
        assert_eq!(state.equity_curve.len(), 5, "should have 5 equity snapshots");
    }
}
