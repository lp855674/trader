// Advanced combinator implementations: dynamic weight adjustment, per-strategy
// performance tracking, and signal normalization.
//
// This module builds on the basic combinators in `combinator.rs` and adds
// runtime adaptivity:
//
//  * `PerformanceTracker`        – records signal outcomes and exposes rolling stats.
//  * `DynamicWeightedAverage`    – re-computes blend weights from live performance data.
//  * `PerformanceAwareRoundRobin`– skips strategies whose win-rate falls below a threshold.
//  * `SignalNormalizer`           – `SignalFilter` that clamps/scales signal quantity.
//  * `MinQuantityFilter`          – `SignalFilter` that suppresses low-confidence signals.

use std::{
    collections::{HashMap, VecDeque},
    sync::{Arc, Mutex},
};

use domain::Side;

use super::{
    combinator::SignalFilter,
    r#trait::{Signal, StrategyContext, StrategyError, Strategy},
};

// ─── StrategyStats ───────────────────────────────────────────────────────────

/// Rolling performance statistics for a single strategy.
#[derive(Debug, Clone)]
pub struct StrategyStats {
    pub total_signals: u64,
    pub win_count: u64,
    pub loss_count: u64,
    pub total_pnl: f64,
    recent_pnl: VecDeque<f64>,
    window_size: usize,
}

impl StrategyStats {
    pub fn new(window_size: usize) -> Self {
        Self {
            total_signals: 0,
            win_count: 0,
            loss_count: 0,
            total_pnl: 0.0,
            recent_pnl: VecDeque::with_capacity(window_size),
            window_size,
        }
    }

    /// Fraction of signals that resulted in a profit.
    pub fn win_rate(&self) -> f64 {
        if self.total_signals == 0 {
            return 0.5; // neutral prior
        }
        self.win_count as f64 / self.total_signals as f64
    }

    /// Average PnL over the rolling window.  Returns 0 if no data yet.
    pub fn rolling_avg_pnl(&self) -> f64 {
        if self.recent_pnl.is_empty() {
            return 0.0;
        }
        self.recent_pnl.iter().sum::<f64>() / self.recent_pnl.len() as f64
    }

    /// Sharpe-like ratio: mean / std_dev of recent PnL.  Returns 0 when
    /// there is insufficient data (< 2 samples) or zero variance.
    pub fn rolling_sharpe(&self) -> f64 {
        let n = self.recent_pnl.len();
        if n < 2 {
            return 0.0;
        }
        let mean = self.rolling_avg_pnl();
        let variance = self
            .recent_pnl
            .iter()
            .map(|x| (x - mean).powi(2))
            .sum::<f64>()
            / (n - 1) as f64;
        let std_dev = variance.sqrt();
        if std_dev < f64::EPSILON {
            return 0.0;
        }
        mean / std_dev
    }

    /// Adaptive weight: positive win-rate advantage times rolling avg PnL,
    /// floored at a small positive value so a strategy is never fully ignored
    /// before it has enough history.
    pub fn adaptive_weight(&self) -> f64 {
        let wr_advantage = self.win_rate() - 0.5; // range [-0.5, +0.5]
        let avg = self.rolling_avg_pnl();
        // Give equal weight (1.0) while the strategy has no history.
        if self.total_signals == 0 {
            return 1.0;
        }
        // Combine win-rate advantage with average PnL, then floor.
        let raw = 1.0 + wr_advantage * 2.0 + avg.max(-0.5).min(0.5);
        raw.max(0.05) // never drop below 5% of equal weight
    }

    pub fn record(&mut self, pnl: f64) {
        self.total_signals += 1;
        self.total_pnl += pnl;
        if pnl > 0.0 {
            self.win_count += 1;
        } else {
            self.loss_count += 1;
        }
        if self.recent_pnl.len() >= self.window_size {
            self.recent_pnl.pop_front();
        }
        self.recent_pnl.push_back(pnl);
    }
}

// ─── SignalOutcome ────────────────────────────────────────────────────────────

/// Feedback record supplied externally once the market has resolved a signal.
#[derive(Debug, Clone)]
pub struct SignalOutcome {
    /// Must match the `strategy_id` field of the originating `Signal`.
    pub strategy_id: String,
    /// Realised profit / loss for this signal (positive = profitable).
    pub pnl: f64,
    /// When the outcome was observed (ms since epoch).
    pub ts_ms: i64,
}

// ─── PerformanceTracker ───────────────────────────────────────────────────────

/// Thread-safe registry that maps strategy IDs to their rolling `StrategyStats`.
///
/// Callers are responsible for feeding outcomes back via [`record`].  In a live
/// system this would be wired to the execution layer; in backtests it is driven
/// by the backtesting engine.
#[derive(Debug, Clone)]
pub struct PerformanceTracker {
    stats: HashMap<String, StrategyStats>,
    window_size: usize,
}

impl PerformanceTracker {
    pub fn new(window_size: usize) -> Self {
        Self {
            stats: HashMap::new(),
            window_size,
        }
    }

    /// Record the outcome of a signal produced by the named strategy.
    pub fn record(&mut self, outcome: SignalOutcome) {
        self.stats
            .entry(outcome.strategy_id)
            .or_insert_with(|| StrategyStats::new(self.window_size))
            .record(outcome.pnl);
    }

    /// Return the current stats for a strategy, or a fresh (neutral) record.
    pub fn stats(&self, strategy_id: &str) -> StrategyStats {
        self.stats
            .get(strategy_id)
            .cloned()
            .unwrap_or_else(|| StrategyStats::new(self.window_size))
    }

    /// Compute adaptive weights for a slice of strategy IDs.
    /// Strategies without recorded history receive weight 1.0 (equal share).
    pub fn weights(&self, ids: &[&str]) -> Vec<f64> {
        ids.iter()
            .map(|id| self.stats(id).adaptive_weight())
            .collect()
    }
}

// ─── DynamicWeightedAverage ───────────────────────────────────────────────────

/// A `WeightedAverage`-style combinator whose blend weights are recomputed on
/// every call from a shared `PerformanceTracker`.
///
/// Each inner strategy **must** have a stable, unique ID (returned by
/// `Strategy::name()`).
pub struct DynamicWeightedAverage {
    strategies: Vec<Box<dyn Strategy>>,
    tracker: Arc<Mutex<PerformanceTracker>>,
    name: String,
}

impl DynamicWeightedAverage {
    pub fn new(
        strategies: Vec<Box<dyn Strategy>>,
        tracker: Arc<Mutex<PerformanceTracker>>,
        name: impl Into<String>,
    ) -> Self {
        Self {
            strategies,
            tracker,
            name: name.into(),
        }
    }

    /// Convenience constructor that creates a fresh `PerformanceTracker` with
    /// the given rolling-window size and returns both the combinator and a
    /// shared handle to the tracker so the caller can feed outcomes back.
    pub fn with_new_tracker(
        strategies: Vec<Box<dyn Strategy>>,
        window_size: usize,
        name: impl Into<String>,
    ) -> (Self, Arc<Mutex<PerformanceTracker>>) {
        let tracker = Arc::new(Mutex::new(PerformanceTracker::new(window_size)));
        let this = Self::new(strategies, Arc::clone(&tracker), name);
        (this, tracker)
    }
}

impl Strategy for DynamicWeightedAverage {
    fn evaluate(&self, ctx: &StrategyContext) -> Result<Option<Signal>, StrategyError> {
        let ids: Vec<&str> = self.strategies.iter().map(|s| s.name()).collect();
        let weights = self
            .tracker
            .lock()
            .map_err(|_| StrategyError::DataSource("tracker lock poisoned".into()))?
            .weights(&ids);

        let mut buy_weight = 0.0_f64;
        let mut sell_weight = 0.0_f64;
        let mut price_weight_sum = 0.0_f64;
        let mut price_sum = 0.0_f64;
        let mut contributing_ids: Vec<String> = Vec::new();

        for (strategy, weight) in self.strategies.iter().zip(&weights) {
            if let Some(signal) = strategy.evaluate(ctx)? {
                match signal.side {
                    Side::Buy => buy_weight += weight,
                    Side::Sell => sell_weight += weight,
                }
                if let Some(price) = signal.limit_price {
                    price_sum += price * weight;
                    price_weight_sum += weight;
                }
                contributing_ids.push(signal.strategy_id.clone());
            }
        }

        let net = buy_weight - sell_weight;
        if net.abs() < f64::EPSILON {
            return Ok(None);
        }

        let side = if net > 0.0 { Side::Buy } else { Side::Sell };
        let quantity = net.abs();
        let limit_price = if price_weight_sum > 0.0 {
            Some(price_sum / price_weight_sum)
        } else {
            None
        };

        Ok(Some(Signal::new(
            ctx.instrument.clone(),
            side,
            quantity,
            limit_price,
            ctx.ts_ms,
            contributing_ids.join("+"),
            Default::default(),
        )))
    }

    fn name(&self) -> &str {
        &self.name
    }
}

// ─── PerformanceAwareRoundRobin ───────────────────────────────────────────────

/// Rotates through strategies like `RoundRobin` but skips any strategy whose
/// win-rate has fallen below `min_win_rate`.  Falls back to the first available
/// strategy if all are below threshold.
pub struct PerformanceAwareRoundRobin {
    strategies: Vec<Box<dyn Strategy>>,
    tracker: Arc<Mutex<PerformanceTracker>>,
    min_win_rate: f64,
    index: std::sync::atomic::AtomicUsize,
    name: String,
}

impl PerformanceAwareRoundRobin {
    pub fn new(
        strategies: Vec<Box<dyn Strategy>>,
        tracker: Arc<Mutex<PerformanceTracker>>,
        min_win_rate: f64,
        name: impl Into<String>,
    ) -> Self {
        Self {
            strategies,
            tracker,
            min_win_rate,
            index: std::sync::atomic::AtomicUsize::new(0),
            name: name.into(),
        }
    }
}

impl Strategy for PerformanceAwareRoundRobin {
    fn evaluate(&self, ctx: &StrategyContext) -> Result<Option<Signal>, StrategyError> {
        if self.strategies.is_empty() {
            return Ok(None);
        }

        let tracker = self
            .tracker
            .lock()
            .map_err(|_| StrategyError::DataSource("tracker lock poisoned".into()))?;

        let n = self.strategies.len();
        let start = self
            .index
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            % n;

        // Try each candidate in round-robin order; skip those below threshold.
        for offset in 0..n {
            let idx = (start + offset) % n;
            let strategy = &self.strategies[idx];
            let win_rate = tracker.stats(strategy.name()).win_rate();
            // Accept if: no history yet (win_rate == 0.5 default) OR above threshold.
            if win_rate >= self.min_win_rate || tracker.stats(strategy.name()).total_signals == 0 {
                drop(tracker);
                return strategy.evaluate(ctx);
            }
        }

        // All strategies are below threshold – fall back to round-robin candidate.
        drop(tracker);
        self.strategies[start].evaluate(ctx)
    }

    fn name(&self) -> &str {
        &self.name
    }
}

// ─── SignalNormalizer ─────────────────────────────────────────────────────────

/// Clamps signal `quantity` to `[min_qty, max_qty]` and optionally rescales it
/// to a target quantity.  Useful as the final stage in a `Pipeline` to enforce
/// position-size limits regardless of what upstream combinators produce.
pub struct SignalNormalizer {
    min_qty: f64,
    max_qty: f64,
    /// When `Some(target)`, the clamped quantity is further scaled so its
    /// magnitude equals `target` (direction is preserved).
    target_qty: Option<f64>,
    name: String,
}

impl SignalNormalizer {
    /// Clamp only – quantity is kept within `[min_qty, max_qty]`.
    pub fn clamp(min_qty: f64, max_qty: f64, name: impl Into<String>) -> Self {
        Self {
            min_qty,
            max_qty,
            target_qty: None,
            name: name.into(),
        }
    }

    /// Clamp then normalise to a fixed `target_qty`.
    pub fn normalize(min_qty: f64, max_qty: f64, target_qty: f64, name: impl Into<String>) -> Self {
        Self {
            min_qty,
            max_qty,
            target_qty: Some(target_qty),
            name: name.into(),
        }
    }
}

impl SignalFilter for SignalNormalizer {
    fn filter(
        &self,
        signal: Option<Signal>,
        _ctx: &StrategyContext,
    ) -> Result<Option<Signal>, StrategyError> {
        Ok(signal.and_then(|mut s| {
            if s.quantity < self.min_qty {
                return None; // below minimum – suppress signal
            }
            s.quantity = if let Some(target) = self.target_qty {
                target
            } else {
                s.quantity.min(self.max_qty) // cap to maximum
            };
            Some(s)
        }))
    }

    fn name(&self) -> &str {
        &self.name
    }
}

// ─── MinQuantityFilter ────────────────────────────────────────────────────────

/// Suppresses signals whose `quantity` is below `min_qty`.  Complements
/// `SignalNormalizer` when you only want a lower bound without clamping the top.
pub struct MinQuantityFilter {
    min_qty: f64,
    name: String,
}

impl MinQuantityFilter {
    pub fn new(min_qty: f64, name: impl Into<String>) -> Self {
        Self {
            min_qty,
            name: name.into(),
        }
    }
}

impl SignalFilter for MinQuantityFilter {
    fn filter(
        &self,
        signal: Option<Signal>,
        _ctx: &StrategyContext,
    ) -> Result<Option<Signal>, StrategyError> {
        Ok(signal.filter(|s| s.quantity >= self.min_qty))
    }

    fn name(&self) -> &str {
        &self.name
    }
}

// ─── VotingPolicy & Ensemble ─────────────────────────────────────────────────

/// Voting policy for `Ensemble` combinator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VotingPolicy {
    /// Emit signal only if ALL strategies agree on the same side.
    Unanimous,
    /// Emit signal if >50% weight agrees on one side.
    Majority,
    /// Emit signal if at least one strategy fires (first-wins).
    AnyOne,
}

/// Multi-strategy combinator that combines signals using a `VotingPolicy`.
pub struct Ensemble {
    strategies: Vec<Box<dyn Strategy>>,
    policy: VotingPolicy,
    name: String,
}

impl Ensemble {
    pub fn new(
        strategies: Vec<Box<dyn Strategy>>,
        policy: VotingPolicy,
        name: impl Into<String>,
    ) -> Self {
        Self {
            strategies,
            policy,
            name: name.into(),
        }
    }
}

impl Strategy for Ensemble {
    fn evaluate(&self, ctx: &StrategyContext) -> Result<Option<Signal>, StrategyError> {
        if self.strategies.is_empty() {
            return Ok(None);
        }

        match self.policy {
            VotingPolicy::AnyOne => {
                // First-wins: return first non-None signal
                for strategy in &self.strategies {
                    if let Some(signal) = strategy.evaluate(ctx)? {
                        return Ok(Some(signal));
                    }
                }
                Ok(None)
            }

            VotingPolicy::Unanimous => {
                // All strategies must agree on the same side
                let mut agreed_side: Option<Side> = None;
                let mut total_qty = 0.0_f64;
                let mut price_sum = 0.0_f64;
                let mut price_count = 0u32;
                let mut contributing_ids: Vec<String> = Vec::new();

                for strategy in &self.strategies {
                    match strategy.evaluate(ctx)? {
                        None => return Ok(None), // any silence breaks unanimity
                        Some(signal) => {
                            match agreed_side {
                                None => agreed_side = Some(signal.side),
                                Some(s) if s != signal.side => return Ok(None), // disagreement
                                _ => {}
                            }
                            total_qty += signal.quantity;
                            if let Some(p) = signal.limit_price {
                                price_sum += p;
                                price_count += 1;
                            }
                            contributing_ids.push(signal.strategy_id.clone());
                        }
                    }
                }

                let side = match agreed_side {
                    Some(s) => s,
                    None => return Ok(None),
                };

                let limit_price = if price_count > 0 {
                    Some(price_sum / price_count as f64)
                } else {
                    None
                };

                Ok(Some(Signal::new(
                    ctx.instrument.clone(),
                    side,
                    total_qty / self.strategies.len() as f64,
                    limit_price,
                    ctx.ts_ms,
                    contributing_ids.join("+"),
                    Default::default(),
                )))
            }

            VotingPolicy::Majority => {
                // >50% by count agrees on one side
                let mut buy_count = 0u32;
                let mut sell_count = 0u32;
                let mut buy_qty = 0.0_f64;
                let mut sell_qty = 0.0_f64;
                let mut buy_price_sum = 0.0_f64;
                let mut buy_price_count = 0u32;
                let mut sell_price_sum = 0.0_f64;
                let mut sell_price_count = 0u32;
                let mut contributing_ids: Vec<String> = Vec::new();

                for strategy in &self.strategies {
                    if let Some(signal) = strategy.evaluate(ctx)? {
                        match signal.side {
                            Side::Buy => {
                                buy_count += 1;
                                buy_qty += signal.quantity;
                                if let Some(p) = signal.limit_price {
                                    buy_price_sum += p;
                                    buy_price_count += 1;
                                }
                            }
                            Side::Sell => {
                                sell_count += 1;
                                sell_qty += signal.quantity;
                                if let Some(p) = signal.limit_price {
                                    sell_price_sum += p;
                                    sell_price_count += 1;
                                }
                            }
                        }
                        contributing_ids.push(signal.strategy_id.clone());
                    }
                }

                let total = buy_count + sell_count;
                if total == 0 {
                    return Ok(None);
                }

                let (side, qty, price_sum, price_count) = if buy_count as f64 > total as f64 / 2.0 {
                    (Side::Buy, buy_qty / buy_count as f64, buy_price_sum, buy_price_count)
                } else if sell_count as f64 > total as f64 / 2.0 {
                    (Side::Sell, sell_qty / sell_count as f64, sell_price_sum, sell_price_count)
                } else {
                    return Ok(None); // tied — no majority
                };

                let limit_price = if price_count > 0 {
                    Some(price_sum / price_count as f64)
                } else {
                    None
                };

                Ok(Some(Signal::new(
                    ctx.instrument.clone(),
                    side,
                    qty,
                    limit_price,
                    ctx.ts_ms,
                    contributing_ids.join("+"),
                    Default::default(),
                )))
            }
        }
    }

    fn name(&self) -> &str {
        &self.name
    }
}

// ─── StopLossFilter ───────────────────────────────────────────────────────────

/// Suppresses signals when the current price has moved adversely past a stop threshold.
///
/// For Buy signals: suppresses if `last_bar_close < entry_price * (1 - stop_loss_pct)`.
/// For Sell signals: suppresses if `last_bar_close > entry_price * (1 + stop_loss_pct)`.
pub struct StopLossFilter {
    entry_price: f64,
    stop_loss_pct: f64,
    name: String,
}

impl StopLossFilter {
    pub fn new(entry_price: f64, stop_loss_pct: f64, name: impl Into<String>) -> Self {
        Self {
            entry_price,
            stop_loss_pct,
            name: name.into(),
        }
    }
}

impl SignalFilter for StopLossFilter {
    fn filter(
        &self,
        signal: Option<Signal>,
        ctx: &StrategyContext,
    ) -> Result<Option<Signal>, StrategyError> {
        let Some(sig) = signal else { return Ok(None) };
        let current_price = ctx.last_bar_close.unwrap_or(self.entry_price);

        let should_suppress = match sig.side {
            Side::Buy => current_price < self.entry_price * (1.0 - self.stop_loss_pct),
            Side::Sell => current_price > self.entry_price * (1.0 + self.stop_loss_pct),
        };

        if should_suppress {
            Ok(None)
        } else {
            Ok(Some(sig))
        }
    }

    fn name(&self) -> &str {
        &self.name
    }
}

// ─── PositionSizingFilter ─────────────────────────────────────────────────────

/// Scales signal quantity based on available capital and risk per trade.
///
/// Formula: `qty = (capital * risk_pct) / price`
/// where `price = limit_price.unwrap_or(ctx.last_bar_close.unwrap_or(1.0))`.
/// Quantity is clamped to `[0.01, capital / price]`.
pub struct PositionSizingFilter {
    capital: f64,
    risk_pct: f64,
    name: String,
}

impl PositionSizingFilter {
    pub fn new(capital: f64, risk_pct: f64, name: impl Into<String>) -> Self {
        Self {
            capital,
            risk_pct,
            name: name.into(),
        }
    }
}

impl SignalFilter for PositionSizingFilter {
    fn filter(
        &self,
        signal: Option<Signal>,
        ctx: &StrategyContext,
    ) -> Result<Option<Signal>, StrategyError> {
        let Some(mut sig) = signal else { return Ok(None) };

        let price = sig
            .limit_price
            .or(ctx.last_bar_close)
            .unwrap_or(1.0);

        if price <= 0.0 {
            return Ok(None);
        }

        let raw_qty = (self.capital * self.risk_pct) / price;
        let max_qty = self.capital / price;
        let qty = raw_qty.clamp(0.01, max_qty);

        sig.quantity = qty;
        Ok(Some(sig))
    }

    fn name(&self) -> &str {
        &self.name
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use domain::{InstrumentId, Side, Venue};

    use super::*;
    use crate::core::r#trait::{Signal, StrategyContext, StrategyError, Strategy};

    // ── helpers ──────────────────────────────────────────────────────────────

    fn ctx() -> StrategyContext {
        StrategyContext::new(InstrumentId::new(Venue::Crypto, "ETH"), 2_000_000)
    }

    fn outcome(id: &str, pnl: f64) -> SignalOutcome {
        SignalOutcome {
            strategy_id: id.into(),
            pnl,
            ts_ms: 0,
        }
    }

    struct Fixed {
        side: Side,
        qty: f64,
        price: Option<f64>,
        id: String,
    }

    impl Fixed {
        fn buy(qty: f64, id: &str) -> Box<Self> {
            Box::new(Self { side: Side::Buy, qty, price: None, id: id.into() })
        }

        fn sell(qty: f64, id: &str) -> Box<Self> {
            Box::new(Self { side: Side::Sell, qty, price: None, id: id.into() })
        }
    }

    impl Strategy for Fixed {
        fn evaluate(&self, ctx: &StrategyContext) -> Result<Option<Signal>, StrategyError> {
            Ok(Some(Signal::new(
                ctx.instrument.clone(),
                self.side,
                self.qty,
                self.price,
                ctx.ts_ms,
                self.id.clone(),
                HashMap::new(),
            )))
        }

        fn name(&self) -> &str {
            &self.id
        }
    }

    // ── StrategyStats ─────────────────────────────────────────────────────────

    #[test]
    fn stats_win_rate_neutral_with_no_history() {
        let stats = StrategyStats::new(10);
        assert!((stats.win_rate() - 0.5).abs() < 1e-9);
    }

    #[test]
    fn stats_win_rate_after_wins_and_losses() {
        let mut stats = StrategyStats::new(10);
        stats.record(1.0);
        stats.record(1.0);
        stats.record(-1.0);
        assert!((stats.win_rate() - 2.0 / 3.0).abs() < 1e-9);
    }

    #[test]
    fn stats_rolling_avg_pnl_respects_window() {
        let mut stats = StrategyStats::new(3);
        for pnl in [1.0, 2.0, 3.0, 4.0] {
            stats.record(pnl);
        }
        // window = [2, 3, 4], avg = 3.0
        assert!((stats.rolling_avg_pnl() - 3.0).abs() < 1e-9);
    }

    #[test]
    fn stats_adaptive_weight_positive_performer_above_one() {
        let mut stats = StrategyStats::new(10);
        for _ in 0..8 {
            stats.record(0.2);
        }
        for _ in 0..2 {
            stats.record(-0.1);
        }
        // win_rate = 0.8 → wr_advantage = 0.3 → contributes +0.6 to weight
        assert!(stats.adaptive_weight() > 1.0);
    }

    #[test]
    fn stats_adaptive_weight_floored_for_poor_performer() {
        let mut stats = StrategyStats::new(10);
        for _ in 0..10 {
            stats.record(-1.0);
        }
        assert!(stats.adaptive_weight() >= 0.05);
    }

    // ── PerformanceTracker ───────────────────────────────────────────────────

    #[test]
    fn tracker_records_and_retrieves() {
        let mut tracker = PerformanceTracker::new(10);
        tracker.record(outcome("s1", 1.0));
        tracker.record(outcome("s1", -0.5));
        let stats = tracker.stats("s1");
        assert_eq!(stats.total_signals, 2);
        assert_eq!(stats.win_count, 1);
    }

    #[test]
    fn tracker_weights_returns_one_per_strategy() {
        let tracker = PerformanceTracker::new(10);
        let weights = tracker.weights(&["a", "b", "c"]);
        assert_eq!(weights.len(), 3);
        // all neutral (no history)
        for w in &weights {
            assert!((w - 1.0).abs() < 1e-9);
        }
    }

    // ── DynamicWeightedAverage ───────────────────────────────────────────────

    #[test]
    fn dynamic_wa_equal_weights_no_history() {
        let (dwa, _tracker) = DynamicWeightedAverage::with_new_tracker(
            vec![
                Fixed::buy(1.0, "a") as Box<dyn Strategy>,
                Fixed::buy(1.0, "b"),
            ],
            10,
            "dwa",
        );
        let signal = dwa.evaluate(&ctx()).unwrap().unwrap();
        assert_eq!(signal.side, Side::Buy);
        // both weight 1.0 → net qty = 2.0
        assert!((signal.quantity - 2.0).abs() < 1e-9);
    }

    #[test]
    fn dynamic_wa_better_performer_gets_higher_weight() {
        let (dwa, tracker) = DynamicWeightedAverage::with_new_tracker(
            vec![
                Fixed::buy(1.0, "good") as Box<dyn Strategy>,
                Fixed::sell(1.0, "bad"),
            ],
            10,
            "dwa",
        );

        // Give "good" a strong track record, "bad" a poor one.
        {
            let mut t = tracker.lock().unwrap();
            for _ in 0..9 {
                t.record(outcome("good", 1.0));
            }
            t.record(outcome("good", -0.1));
            for _ in 0..9 {
                t.record(outcome("bad", -1.0));
            }
            t.record(outcome("bad", 0.1));
        }

        let signal = dwa.evaluate(&ctx()).unwrap().unwrap();
        // "good" buy weight >> "bad" sell weight → net Buy
        assert_eq!(signal.side, Side::Buy);
    }

    // ── PerformanceAwareRoundRobin ───────────────────────────────────────────

    #[test]
    fn parr_skips_low_win_rate_strategy() {
        let tracker = Arc::new(Mutex::new(PerformanceTracker::new(10)));

        // Give "bad" a 0% win rate.
        {
            let mut t = tracker.lock().unwrap();
            for _ in 0..5 {
                t.record(outcome("bad", -1.0));
            }
        }

        let parr = PerformanceAwareRoundRobin::new(
            vec![
                // index 0 will be tried first on first call
                Fixed::sell(1.0, "bad") as Box<dyn Strategy>,
                Fixed::buy(1.0, "good"),
            ],
            Arc::clone(&tracker),
            0.4, // min_win_rate = 40%
            "parr",
        );

        // "bad" (win_rate=0%) is below 40% → should skip to "good"
        let signal = parr.evaluate(&ctx()).unwrap().unwrap();
        assert_eq!(signal.side, Side::Buy);
    }

    // ── SignalNormalizer ──────────────────────────────────────────────────────

    fn make_signal(qty: f64) -> Signal {
        Signal::new(
            InstrumentId::new(Venue::Crypto, "ETH"),
            Side::Buy,
            qty,
            None,
            0,
            "s".into(),
            HashMap::new(),
        )
    }

    #[test]
    fn normalizer_clamp_caps_large_qty() {
        let norm = SignalNormalizer::clamp(0.1, 5.0, "n");
        let out = norm.filter(Some(make_signal(10.0)), &ctx()).unwrap().unwrap();
        assert!((out.quantity - 5.0).abs() < 1e-9);
    }

    #[test]
    fn normalizer_clamp_suppresses_below_min() {
        let norm = SignalNormalizer::clamp(1.0, 5.0, "n");
        assert!(norm.filter(Some(make_signal(0.0)), &ctx()).unwrap().is_none());
    }

    #[test]
    fn normalizer_normalize_sets_target_qty() {
        let norm = SignalNormalizer::normalize(0.1, 10.0, 3.0, "n");
        let out = norm
            .filter(Some(make_signal(7.0)), &ctx())
            .unwrap()
            .unwrap();
        assert!((out.quantity - 3.0).abs() < 1e-9);
    }

    #[test]
    fn normalizer_passes_none_through() {
        let norm = SignalNormalizer::clamp(0.1, 10.0, "n");
        assert!(norm.filter(None, &ctx()).unwrap().is_none());
    }

    // ── MinQuantityFilter ─────────────────────────────────────────────────────

    #[test]
    fn min_qty_filter_passes_adequate_signal() {
        let f = MinQuantityFilter::new(0.5, "mq");
        assert!(f.filter(Some(make_signal(1.0)), &ctx()).unwrap().is_some());
    }

    #[test]
    fn min_qty_filter_suppresses_small_signal() {
        let f = MinQuantityFilter::new(1.0, "mq");
        assert!(f.filter(Some(make_signal(0.5)), &ctx()).unwrap().is_none());
    }

    // ── Ensemble ──────────────────────────────────────────────────────────────

    #[test]
    fn ensemble_unanimous_all_agree_buy() {
        let e = Ensemble::new(
            vec![
                Fixed::buy(1.0, "a") as Box<dyn Strategy>,
                Fixed::buy(1.0, "b"),
                Fixed::buy(1.0, "c"),
            ],
            VotingPolicy::Unanimous,
            "ens",
        );
        let signal = e.evaluate(&ctx()).unwrap().unwrap();
        assert_eq!(signal.side, Side::Buy);
    }

    #[test]
    fn ensemble_unanimous_disagreement_returns_none() {
        let e = Ensemble::new(
            vec![
                Fixed::buy(1.0, "a") as Box<dyn Strategy>,
                Fixed::sell(1.0, "b"),
            ],
            VotingPolicy::Unanimous,
            "ens",
        );
        assert!(e.evaluate(&ctx()).unwrap().is_none());
    }

    #[test]
    fn ensemble_majority_buy_wins() {
        let e = Ensemble::new(
            vec![
                Fixed::buy(1.0, "a") as Box<dyn Strategy>,
                Fixed::buy(1.0, "b"),
                Fixed::sell(1.0, "c"),
            ],
            VotingPolicy::Majority,
            "ens",
        );
        let signal = e.evaluate(&ctx()).unwrap().unwrap();
        assert_eq!(signal.side, Side::Buy);
    }

    #[test]
    fn ensemble_majority_tie_returns_none() {
        let e = Ensemble::new(
            vec![
                Fixed::buy(1.0, "a") as Box<dyn Strategy>,
                Fixed::sell(1.0, "b"),
            ],
            VotingPolicy::Majority,
            "ens",
        );
        assert!(e.evaluate(&ctx()).unwrap().is_none());
    }

    #[test]
    fn ensemble_any_one_first_wins() {
        let e = Ensemble::new(
            vec![
                Fixed::buy(1.0, "a") as Box<dyn Strategy>,
                Fixed::sell(1.0, "b"),
            ],
            VotingPolicy::AnyOne,
            "ens",
        );
        let signal = e.evaluate(&ctx()).unwrap().unwrap();
        assert_eq!(signal.side, Side::Buy);
    }

    // ── StopLossFilter ────────────────────────────────────────────────────────

    #[test]
    fn stop_loss_filter_passes_buy_above_threshold() {
        let f = StopLossFilter::new(100.0, 0.05, "sl");
        // last_bar_close = 96 which is > 100 * 0.95 = 95 → should pass
        let mut c = ctx();
        c.last_bar_close = Some(96.0);
        assert!(f.filter(Some(make_signal(1.0)), &c).unwrap().is_some());
    }

    #[test]
    fn stop_loss_filter_suppresses_buy_below_threshold() {
        let f = StopLossFilter::new(100.0, 0.05, "sl");
        // last_bar_close = 94 which is < 100 * 0.95 = 95 → should suppress
        let mut c = ctx();
        c.last_bar_close = Some(94.0);
        assert!(f.filter(Some(make_signal(1.0)), &c).unwrap().is_none());
    }

    #[test]
    fn stop_loss_filter_suppresses_sell_above_threshold() {
        use std::collections::HashMap as HMap;
        let f = StopLossFilter::new(100.0, 0.05, "sl");
        // For Sell: suppress if last_bar_close > entry * (1 + pct) = 105
        let mut c = ctx();
        c.last_bar_close = Some(106.0);
        let sell_signal = Signal::new(
            c.instrument.clone(),
            Side::Sell,
            1.0,
            None,
            c.ts_ms,
            "s".into(),
            HMap::new(),
        );
        assert!(f.filter(Some(sell_signal), &c).unwrap().is_none());
    }

    // ── PositionSizingFilter ──────────────────────────────────────────────────

    #[test]
    fn position_sizing_scales_qty_from_bar_close() {
        let f = PositionSizingFilter::new(10_000.0, 0.02, "ps");
        let mut c = ctx();
        c.last_bar_close = Some(200.0);
        // qty = (10000 * 0.02) / 200 = 1.0
        let out = f.filter(Some(make_signal(999.0)), &c).unwrap().unwrap();
        assert!((out.quantity - 1.0).abs() < 1e-9);
    }

    #[test]
    fn position_sizing_prefers_limit_price_over_bar_close() {
        use std::collections::HashMap as HMap;
        let f = PositionSizingFilter::new(10_000.0, 0.02, "ps");
        let c = ctx();
        let sig = Signal::new(
            c.instrument.clone(),
            Side::Buy,
            999.0,
            Some(250.0),
            c.ts_ms,
            "s".into(),
            HMap::new(),
        );
        // qty = (10000 * 0.02) / 250 = 0.8
        let out = f.filter(Some(sig), &c).unwrap().unwrap();
        assert!((out.quantity - 0.8).abs() < 1e-9);
    }

    #[test]
    fn position_sizing_clamps_to_min() {
        let f = PositionSizingFilter::new(10_000.0, 0.000001, "ps");
        let mut c = ctx();
        c.last_bar_close = Some(100.0);
        // raw qty = (10000 * 0.000001) / 100 = 0.0001 → clamp to 0.01
        let out = f.filter(Some(make_signal(1.0)), &c).unwrap().unwrap();
        assert!((out.quantity - 0.01).abs() < 1e-9);
    }
}
