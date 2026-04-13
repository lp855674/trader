// Strategy combinator traits for composing multiple strategies.
//
// Combinators wrap one or more `Strategy` impls and return a combined signal:
//
//  * `WeightedAverage` – evaluates all inner strategies and blends their signals by weight.
//  * `RoundRobin`       – rotates through inner strategies one per call.
//  * `Conditional`      – routes to a then/else branch based on a runtime predicate.
//  * `Pipeline`         – passes a signal through a chain of `SignalFilter` stages.

use std::sync::atomic::{AtomicUsize, Ordering};

use super::r#trait::{Signal, StrategyContext, StrategyError};
use crate::core::r#trait::Strategy;
use domain::Side;

// ─── SignalFilter ─────────────────────────────────────────────────────────────

/// A single stage in a `Pipeline`.  Receives the (possibly absent) signal
/// produced by the previous stage, optionally transforms it, and returns it.
pub trait SignalFilter: Send + Sync {
    fn filter(
        &self,
        signal: Option<Signal>,
        ctx: &StrategyContext,
    ) -> Result<Option<Signal>, StrategyError>;

    fn name(&self) -> &str;
}

// ─── WeightedAverage ─────────────────────────────────────────────────────────

/// Evaluates every inner strategy, collects the signals that fire, and blends
/// them into a single signal weighted by the supplied weights.
///
/// Blending rules
/// ──────────────
/// * Separate signals into Buy and Sell buckets.
/// * Compute `buy_weight = Σ(weight_i for Buy signals)` and likewise for Sell.
/// * The winning side is the one with greater total weight.
/// * Blended `quantity` = |buy_weight − sell_weight| (net directional weight).
/// * Blended `limit_price` = weighted-average of limit prices across *all* firing
///   signals (market-order signals, i.e. `limit_price == None`, are excluded from
///   the price average; if no limit price exists the blended signal is a market order).
/// * `strategy_id` encodes the contributing strategy IDs joined by `+`.
pub struct WeightedAverage {
    strategies: Vec<(Box<dyn Strategy>, f64)>,
    name: String,
}

impl WeightedAverage {
    /// `strategies` is a list of `(strategy, weight)` pairs.
    /// Weights need not sum to 1 – only their relative magnitudes matter.
    pub fn new(strategies: Vec<(Box<dyn Strategy>, f64)>, name: impl Into<String>) -> Self {
        Self {
            strategies,
            name: name.into(),
        }
    }
}

impl Strategy for WeightedAverage {
    fn evaluate(&self, ctx: &StrategyContext) -> Result<Option<Signal>, StrategyError> {
        let mut buy_weight = 0.0_f64;
        let mut sell_weight = 0.0_f64;
        let mut price_weight_sum = 0.0_f64;
        let mut price_sum = 0.0_f64;
        let mut contributing_ids: Vec<String> = Vec::new();

        for (strategy, weight) in &self.strategies {
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

// ─── RoundRobin ──────────────────────────────────────────────────────────────

/// Rotates through inner strategies in order, delegating each call to the next
/// strategy in the ring.  Thread-safe via `AtomicUsize`.
pub struct RoundRobin {
    strategies: Vec<Box<dyn Strategy>>,
    index: AtomicUsize,
    name: String,
}

impl RoundRobin {
    pub fn new(strategies: Vec<Box<dyn Strategy>>, name: impl Into<String>) -> Self {
        Self {
            strategies,
            index: AtomicUsize::new(0),
            name: name.into(),
        }
    }
}

impl Strategy for RoundRobin {
    fn evaluate(&self, ctx: &StrategyContext) -> Result<Option<Signal>, StrategyError> {
        if self.strategies.is_empty() {
            return Ok(None);
        }
        let idx = self.index.fetch_add(1, Ordering::Relaxed) % self.strategies.len();
        self.strategies[idx].evaluate(ctx)
    }

    fn name(&self) -> &str {
        &self.name
    }
}

// ─── Conditional ─────────────────────────────────────────────────────────────

/// Routes to `then_strategy` when the predicate returns `true`, otherwise to
/// `else_strategy` (if provided).
pub struct Conditional {
    predicate: Box<dyn Fn(&StrategyContext) -> bool + Send + Sync>,
    then_strategy: Box<dyn Strategy>,
    else_strategy: Option<Box<dyn Strategy>>,
    name: String,
}

impl Conditional {
    pub fn new(
        predicate: impl Fn(&StrategyContext) -> bool + Send + Sync + 'static,
        then_strategy: Box<dyn Strategy>,
        else_strategy: Option<Box<dyn Strategy>>,
        name: impl Into<String>,
    ) -> Self {
        Self {
            predicate: Box::new(predicate),
            then_strategy,
            else_strategy,
            name: name.into(),
        }
    }
}

impl Strategy for Conditional {
    fn evaluate(&self, ctx: &StrategyContext) -> Result<Option<Signal>, StrategyError> {
        if (self.predicate)(ctx) {
            self.then_strategy.evaluate(ctx)
        } else if let Some(else_strategy) = &self.else_strategy {
            else_strategy.evaluate(ctx)
        } else {
            Ok(None)
        }
    }

    fn name(&self) -> &str {
        &self.name
    }
}

// ─── Pipeline ────────────────────────────────────────────────────────────────

/// Evaluates a base `Strategy` then passes the resulting signal through an
/// ordered list of `SignalFilter` stages.  Any stage may suppress the signal by
/// returning `Ok(None)`.
pub struct Pipeline {
    source: Box<dyn Strategy>,
    filters: Vec<Box<dyn SignalFilter>>,
    name: String,
}

impl Pipeline {
    pub fn new(
        source: Box<dyn Strategy>,
        filters: Vec<Box<dyn SignalFilter>>,
        name: impl Into<String>,
    ) -> Self {
        Self {
            source,
            filters,
            name: name.into(),
        }
    }
}

impl Strategy for Pipeline {
    fn evaluate(&self, ctx: &StrategyContext) -> Result<Option<Signal>, StrategyError> {
        let mut signal = self.source.evaluate(ctx)?;
        for filter in &self.filters {
            signal = filter.filter(signal, ctx)?;
        }
        Ok(signal)
    }

    fn name(&self) -> &str {
        &self.name
    }
}

// ─── Built-in filters ────────────────────────────────────────────────────────

/// Scales the signal quantity by a constant factor.  Useful as the final stage
/// in a pipeline to apply position-sizing rules.
pub struct QuantityScaler {
    factor: f64,
    name: String,
}

impl QuantityScaler {
    pub fn new(factor: f64, name: impl Into<String>) -> Self {
        Self {
            factor,
            name: name.into(),
        }
    }
}

impl SignalFilter for QuantityScaler {
    fn filter(
        &self,
        signal: Option<Signal>,
        _ctx: &StrategyContext,
    ) -> Result<Option<Signal>, StrategyError> {
        Ok(signal.map(|mut s| {
            s.quantity *= self.factor;
            s
        }))
    }

    fn name(&self) -> &str {
        &self.name
    }
}

/// Passes only signals whose side matches the configured side.
pub struct SideFilter {
    allowed_side: Side,
    name: String,
}

impl SideFilter {
    pub fn new(allowed_side: Side, name: impl Into<String>) -> Self {
        Self {
            allowed_side,
            name: name.into(),
        }
    }
}

impl SignalFilter for SideFilter {
    fn filter(
        &self,
        signal: Option<Signal>,
        _ctx: &StrategyContext,
    ) -> Result<Option<Signal>, StrategyError> {
        Ok(signal.filter(|s| s.side == self.allowed_side))
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
    use crate::core::r#trait::{Signal, Strategy, StrategyContext, StrategyError};

    // ── helpers ──────────────────────────────────────────────────────────────

    fn ctx() -> StrategyContext {
        StrategyContext::new(InstrumentId::new(Venue::Crypto, "BTC"), 1_000_000)
    }

    /// Strategy that always emits a signal with the given side and quantity.
    struct Fixed {
        side: Side,
        qty: f64,
        price: Option<f64>,
        id: String,
    }

    impl Fixed {
        fn buy(qty: f64, price: Option<f64>) -> Box<Self> {
            Box::new(Self {
                side: Side::Buy,
                qty,
                price,
                id: "buy".into(),
            })
        }

        fn sell(qty: f64, price: Option<f64>) -> Box<Self> {
            Box::new(Self {
                side: Side::Sell,
                qty,
                price,
                id: "sell".into(),
            })
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

    /// Strategy that always returns `None`.
    struct Silent;

    impl Strategy for Silent {
        fn evaluate(&self, _ctx: &StrategyContext) -> Result<Option<Signal>, StrategyError> {
            Ok(None)
        }

        fn name(&self) -> &str {
            "silent"
        }
    }

    // ── WeightedAverage ──────────────────────────────────────────────────────

    #[test]
    fn weighted_average_net_buy() {
        let wa = WeightedAverage::new(
            vec![
                (Fixed::buy(1.0, Some(100.0)), 3.0),
                (Fixed::sell(1.0, Some(90.0)), 1.0),
            ],
            "wa",
        );
        let signal = wa.evaluate(&ctx()).unwrap().unwrap();
        assert_eq!(signal.side, Side::Buy);
        // net qty = 3 - 1 = 2
        assert!((signal.quantity - 2.0).abs() < 1e-9);
        // weighted price = (100*3 + 90*1) / 4 = 97.5
        assert!((signal.limit_price.unwrap() - 97.5).abs() < 1e-9);
    }

    #[test]
    fn weighted_average_no_signal_when_balanced() {
        let wa = WeightedAverage::new(
            vec![(Fixed::buy(1.0, None), 1.0), (Fixed::sell(1.0, None), 1.0)],
            "wa",
        );
        assert!(wa.evaluate(&ctx()).unwrap().is_none());
    }

    #[test]
    fn weighted_average_silent_strategies_produce_none() {
        let wa = WeightedAverage::new(vec![(Box::new(Silent) as Box<dyn Strategy>, 1.0)], "wa");
        assert!(wa.evaluate(&ctx()).unwrap().is_none());
    }

    // ── RoundRobin ───────────────────────────────────────────────────────────

    #[test]
    fn round_robin_rotates() {
        let rr = RoundRobin::new(
            vec![Fixed::buy(1.0, None) as Box<dyn Strategy>, Box::new(Silent)],
            "rr",
        );
        // first call → Fixed (index 0 → Buy signal)
        assert!(rr.evaluate(&ctx()).unwrap().is_some());
        // second call → Silent (index 1 → None)
        assert!(rr.evaluate(&ctx()).unwrap().is_none());
        // wraps around
        assert!(rr.evaluate(&ctx()).unwrap().is_some());
    }

    #[test]
    fn round_robin_empty_returns_none() {
        let rr = RoundRobin::new(vec![], "rr");
        assert!(rr.evaluate(&ctx()).unwrap().is_none());
    }

    // ── Conditional ──────────────────────────────────────────────────────────

    #[test]
    fn conditional_routes_to_then() {
        let cond = Conditional::new(
            |_ctx| true,
            Fixed::buy(2.0, None),
            Some(Fixed::sell(2.0, None)),
            "cond",
        );
        let signal = cond.evaluate(&ctx()).unwrap().unwrap();
        assert_eq!(signal.side, Side::Buy);
    }

    #[test]
    fn conditional_routes_to_else() {
        let cond = Conditional::new(
            |_ctx| false,
            Fixed::buy(2.0, None),
            Some(Fixed::sell(2.0, None)),
            "cond",
        );
        let signal = cond.evaluate(&ctx()).unwrap().unwrap();
        assert_eq!(signal.side, Side::Sell);
    }

    #[test]
    fn conditional_no_else_returns_none_on_false() {
        let cond = Conditional::new(|_ctx| false, Fixed::buy(1.0, None), None, "cond");
        assert!(cond.evaluate(&ctx()).unwrap().is_none());
    }

    // ── Pipeline ─────────────────────────────────────────────────────────────

    #[test]
    fn pipeline_applies_filters_in_order() {
        let pipeline = Pipeline::new(
            Fixed::buy(4.0, None),
            vec![
                Box::new(QuantityScaler::new(0.5, "half")) as Box<dyn SignalFilter>,
                Box::new(SideFilter::new(Side::Buy, "buy_only")),
            ],
            "pipeline",
        );
        let signal = pipeline.evaluate(&ctx()).unwrap().unwrap();
        assert_eq!(signal.side, Side::Buy);
        assert!((signal.quantity - 2.0).abs() < 1e-9);
    }

    #[test]
    fn pipeline_side_filter_suppresses_wrong_side() {
        let pipeline = Pipeline::new(
            Fixed::sell(1.0, None),
            vec![Box::new(SideFilter::new(Side::Buy, "buy_only")) as Box<dyn SignalFilter>],
            "pipeline",
        );
        assert!(pipeline.evaluate(&ctx()).unwrap().is_none());
    }

    #[test]
    fn pipeline_suppressed_signal_skips_remaining_filters() {
        // If the source returns None the filters should propagate None without panicking.
        let pipeline = Pipeline::new(
            Box::new(Silent),
            vec![Box::new(QuantityScaler::new(10.0, "scale")) as Box<dyn SignalFilter>],
            "pipeline",
        );
        assert!(pipeline.evaluate(&ctx()).unwrap().is_none());
    }

    // ── QuantityScaler ───────────────────────────────────────────────────────

    #[test]
    fn quantity_scaler_passes_none_through() {
        let scaler = QuantityScaler::new(2.0, "scaler");
        assert!(scaler.filter(None, &ctx()).unwrap().is_none());
    }

    // ── SideFilter ───────────────────────────────────────────────────────────

    #[test]
    fn side_filter_allows_matching_side() {
        let filter = SideFilter::new(Side::Sell, "sell_only");
        let signal = Signal::new(
            ctx().instrument.clone(),
            Side::Sell,
            1.0,
            None,
            0,
            "s".into(),
            HashMap::new(),
        );
        assert!(filter.filter(Some(signal), &ctx()).unwrap().is_some());
    }
}
