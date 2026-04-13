use domain::{InstrumentId, Side, Venue};
use std::collections::HashMap;
use std::sync::Arc;
use strategy::backtest::{BacktestConfig, BacktestEngine};
use strategy::core::r#trait::Strategy as CoreStrategy;
use strategy::core::r#trait::{Kline, Signal, StrategyContext, StrategyError};

// ─── helpers ────────────────────────────────────────────────────────────────

fn make_instrument() -> InstrumentId {
    InstrumentId::new(Venue::Crypto, "BTC")
}

fn make_kline(instrument: InstrumentId, open: f64, close: f64, ts: i64) -> Kline {
    Kline {
        instrument,
        open_ts_ms: ts - 60_000,
        close_ts_ms: ts,
        open,
        high: close.max(open),
        low: close.min(open),
        close,
        volume: 1000.0,
    }
}

fn make_config(instrument: InstrumentId, capital: f64) -> BacktestConfig {
    BacktestConfig {
        start_ts_ms: 0,
        end_ts_ms: 3_600_000,
        initial_capital: capital,
        instruments: vec![instrument],
        granularity_ms: 60_000,
        max_positions: 5,
        commission_rate: 0.001,
    }
}

// ─── strategies ─────────────────────────────────────────────────────────────

/// Buy when close > open (bullish bar)
struct CloseAboveOpenStrategy;

impl CoreStrategy for CloseAboveOpenStrategy {
    fn evaluate(&self, ctx: &StrategyContext) -> Result<Option<Signal>, StrategyError> {
        // Use last_bar_close as proxy; the engine sets this to close price.
        // For the "close > open" check we use the close value provided in context.
        // Since StrategyContext only carries close, we always buy when a bar is present.
        if let Some(close) = ctx.last_bar_close {
            Ok(Some(Signal::new(
                ctx.instrument.clone(),
                Side::Buy,
                1.0,
                None,
                ctx.ts_ms,
                "close_above_open".to_string(),
                HashMap::new(),
            )))
        } else {
            Ok(None)
        }
    }
    fn name(&self) -> &str {
        "close_above_open"
    }
}

// ─── 1. Reproducibility test ─────────────────────────────────────────────────

#[test]
fn test_determinism_same_result_two_runs() {
    let instrument = make_instrument();
    let config1 = make_config(instrument.clone(), 10_000.0);
    let config2 = make_config(instrument.clone(), 10_000.0);

    let bars: Vec<Kline> = (1..=10)
        .map(|i| {
            make_kline(
                instrument.clone(),
                100.0 * i as f64,
                110.0 * i as f64,
                i * 60_000,
            )
        })
        .collect();

    let mut engine1 = BacktestEngine::new(config1, Arc::new(CloseAboveOpenStrategy));
    let state1 = engine1.run(bars.clone()).unwrap();

    let mut engine2 = BacktestEngine::new(config2, Arc::new(CloseAboveOpenStrategy));
    let state2 = engine2.run(bars).unwrap();

    assert_eq!(
        state1.equity_curve.len(),
        state2.equity_curve.len(),
        "equity curve lengths must match"
    );
    assert_eq!(
        state1.trade_count, state2.trade_count,
        "trade counts must match"
    );

    for (i, ((ts1, eq1), (ts2, eq2))) in state1
        .equity_curve
        .iter()
        .zip(state2.equity_curve.iter())
        .enumerate()
    {
        assert_eq!(ts1, ts2, "timestamp mismatch at index {i}");
        assert!(
            (eq1 - eq2).abs() < 1e-9,
            "equity mismatch at index {i}: {eq1} vs {eq2}"
        );
    }
}

// ─── 2. Edge case: empty bars ────────────────────────────────────────────────

#[test]
fn test_empty_bars_preserves_capital() {
    let instrument = make_instrument();
    let initial_capital = 5_000.0;
    let config = make_config(instrument.clone(), initial_capital);
    let mut engine = BacktestEngine::new(config, Arc::new(CloseAboveOpenStrategy));

    let state = engine.run(vec![]).unwrap();

    assert_eq!(
        state.equity_curve.len(),
        0,
        "equity curve should be empty with no bars"
    );
    assert!(state.positions.is_empty(), "no positions should exist");
    assert!(
        (state.capital - initial_capital).abs() < 1e-9,
        "capital should be unchanged: expected {initial_capital}, got {}",
        state.capital
    );
}

// ─── 3. Edge case: single bar ────────────────────────────────────────────────

#[test]
fn test_single_bar_equity_snapshot() {
    let instrument = make_instrument();
    let config = make_config(instrument.clone(), 10_000.0);
    let mut engine = BacktestEngine::new(config, Arc::new(CloseAboveOpenStrategy));

    let bar = make_kline(instrument.clone(), 90.0, 100.0, 60_000);
    let state = engine.run(vec![bar]).unwrap();

    assert!(
        !state.equity_curve.is_empty(),
        "equity curve should have at least one snapshot after a single bar"
    );
}

// ─── 4. Edge case: insufficient capital ──────────────────────────────────────

struct LargeOrderStrategy;

impl CoreStrategy for LargeOrderStrategy {
    fn evaluate(&self, ctx: &StrategyContext) -> Result<Option<Signal>, StrategyError> {
        Ok(Some(Signal::new(
            ctx.instrument.clone(),
            Side::Buy,
            1000.0, // quantity of 1000 units
            None,
            ctx.ts_ms,
            "large_order".to_string(),
            HashMap::new(),
        )))
    }
    fn name(&self) -> &str {
        "large_order"
    }
}

#[test]
fn test_insufficient_capital_position_not_opened() {
    let instrument = make_instrument();
    let mut config = make_config(instrument.clone(), 1.0); // only 1.0 capital
    config.commission_rate = 0.0; // no commission to keep math simple
    let mut engine = BacktestEngine::new(config, Arc::new(LargeOrderStrategy));

    // price=100, qty=1000 => cost=100_000; capital=1.0 => insufficient
    let bar = make_kline(instrument.clone(), 100.0, 100.0, 60_000);
    let result = engine.step(&bar);

    match result {
        Err(strategy::backtest::engine::BacktestError::InsufficientCapital) => {
            // Expected error path
        }
        Ok(_) => {
            // Engine chose to skip the order silently — verify capital is preserved
            assert!(
                engine.state.positions.is_empty(),
                "position should not be opened with insufficient capital"
            );
            assert!(
                (engine.state.capital - 1.0).abs() < 1e-9,
                "capital should be unchanged"
            );
        }
        Err(other) => panic!("unexpected error: {other:?}"),
    }
}

// ─── 5. Concurrency test ─────────────────────────────────────────────────────

#[tokio::test]
async fn test_concurrent_runs_identical_results() {
    let instrument = make_instrument();

    let bars: Vec<Kline> = (1..=20)
        .map(|i| make_kline(instrument.clone(), 100.0, 100.0 + i as f64, i * 60_000))
        .collect();

    let bars = Arc::new(bars);

    let handles: Vec<_> = (0..4)
        .map(|_| {
            let instrument = instrument.clone();
            let bars = Arc::clone(&bars);
            tokio::spawn(async move {
                let config = make_config(instrument.clone(), 10_000.0);
                let mut engine = BacktestEngine::new(config, Arc::new(CloseAboveOpenStrategy));
                let state = engine.run((*bars).clone()).unwrap();
                state.total_equity()
            })
        })
        .collect();

    let results: Vec<f64> = futures::future::join_all(handles)
        .await
        .into_iter()
        .map(|r| r.expect("task panicked"))
        .collect();

    let first = results[0];
    for (i, &equity) in results.iter().enumerate() {
        assert!(
            (equity - first).abs() < 1e-9,
            "run {i} equity {equity} differs from run 0 equity {first}"
        );
    }
}
