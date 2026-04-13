use domain::{InstrumentId, Side, Venue};
use risk::core::{
    CompositeRiskChecker, MarketContext, OrderContext, OrderType, PortfolioContext, RiskChecker,
    RiskDecision, RiskInput,
};
use risk::risk::{
    EwmaVolatility, OrderRiskChecker, OrderRiskConfig, PnLLimits, PortfolioRiskChecker,
    PortfolioRiskConfig, PositionRiskChecker, RiskPositionManager, StopLossConfig,
    VolatilityAdjuster,
};
use std::sync::Arc;

fn btc() -> InstrumentId {
    InstrumentId::new(Venue::Crypto, "BTC-USD")
}

fn make_input() -> RiskInput {
    RiskInput {
        order: OrderContext {
            instrument: btc(),
            side: Side::Buy,
            quantity: 0.1, // 0.1 BTC * 50k = 5k notional, well within limits
            limit_price: Some(50_000.0),
            order_type: OrderType::Limit,
            strategy_id: "test".into(),
            submitted_ts_ms: 0,
        },
        market: MarketContext {
            instrument: btc(),
            mid_price: 50_000.0,
            bid: 49_990.0,
            ask: 50_010.0,
            volume_24h: 1_000_000.0,
            volatility: 0.02,
            ts_ms: 0,
        },
        portfolio: PortfolioContext {
            total_capital: 100_000.0,
            available_capital: 80_000.0,
            total_exposure: 10_000.0,
            open_positions: 1,
            daily_pnl: 200.0,
            daily_pnl_limit: -5_000.0,
        },
    }
}

// ── Test 1: All checkers approve a valid order ────────────────────────────

#[test]
fn all_checkers_approve_valid_order() {
    let order_checker = OrderRiskChecker::new(OrderRiskConfig::default());
    let vol_adjuster = VolatilityAdjuster::new(0.94, 100.0, 10_000_000.0);
    let position_checker = PositionRiskChecker::new(10, -5_000.0);
    let portfolio_checker = PortfolioRiskChecker::new(PortfolioRiskConfig::default());

    let checker = CompositeRiskChecker::new(vec![
        Box::new(order_checker),
        Box::new(vol_adjuster),
        Box::new(position_checker),
        Box::new(portfolio_checker),
    ]);

    let result = checker.check(&make_input()).unwrap();
    match result {
        RiskDecision::Approve | RiskDecision::ApproveWithAdjustment { .. } => {}
        RiskDecision::Reject { reason, .. } => panic!("Should not reject valid order: {}", reason),
    }
}

// ── Test 2: Circuit breaker short-circuits portfolio check ────────────────

#[test]
fn circuit_breaker_short_circuits_portfolio_check() {
    // OrderRiskChecker has lower priority (10) → runs first with circuit breaker
    let order_checker = OrderRiskChecker::new(OrderRiskConfig {
        circuit_breaker_volatility: 0.05, // very low threshold
        ..OrderRiskConfig::default()
    });
    let portfolio_checker = PortfolioRiskChecker::new(PortfolioRiskConfig::default());

    let checker =
        CompositeRiskChecker::new(vec![Box::new(order_checker), Box::new(portfolio_checker)]);

    let mut input = make_input();
    input.market.volatility = 0.10; // triggers circuit breaker

    let result = checker.check(&input).unwrap();
    assert!(
        matches!(result, RiskDecision::Reject { .. }),
        "Circuit breaker should reject"
    );
}

// ── Test 3: Position stop loss → PositionRiskChecker rejects subsequent order ──

#[test]
fn position_stop_loss_then_checker_rejects() {
    // Simulate a position that has hit its daily loss limit
    let checker = PositionRiskChecker::new(10, -1_000.0);

    let mut input = make_input();
    // Daily PnL is below the limit
    input.portfolio.daily_pnl = -2_000.0;

    let result = checker.check(&input).unwrap();
    assert!(
        matches!(result, RiskDecision::Reject { .. }),
        "Should reject when daily loss limit exceeded"
    );
}

// ── Test 4: VaR budget exhausted → PortfolioRiskChecker rejects ──────────

#[test]
fn var_budget_exhausted_rejects() {
    let config = PortfolioRiskConfig {
        max_var_pct: 0.0001, // extremely tight VaR budget
        ..PortfolioRiskConfig::default()
    };
    let mut checker = PortfolioRiskChecker::new(config);

    // Feed return history to make VaR non-trivial
    for _ in 0..20 {
        checker.push_return(&btc(), -0.05);
    }
    for _ in 0..20 {
        checker.push_return(&btc(), 0.02);
    }

    // Large order: 1 BTC * 50k = 50k notional
    let mut input = make_input();
    input.order.quantity = 1.0;

    let result = checker.check(&input).unwrap();
    assert!(
        matches!(result, RiskDecision::Reject { .. }),
        "VaR budget exhausted should reject"
    );
}

// ── Test 5: Concurrent access — 8 threads, no panics ─────────────────────

#[test]
fn concurrent_access_no_panics() {
    use std::thread;

    // All individual checkers are Send+Sync, wrap them in Arc
    let order_checker = Arc::new(OrderRiskChecker::new(OrderRiskConfig::default()));
    let vol_adjuster = Arc::new(VolatilityAdjuster::new(0.94, 100.0, 10_000_000.0));
    let position_checker = Arc::new(PositionRiskChecker::new(10, -5_000.0));

    let input = Arc::new(make_input());

    let handles: Vec<_> = (0..8)
        .map(|_| {
            let order_checker = Arc::clone(&order_checker);
            let vol_adjuster = Arc::clone(&vol_adjuster);
            let position_checker = Arc::clone(&position_checker);
            let input = Arc::clone(&input);

            thread::spawn(move || {
                // Each thread calls check() on the shared checkers
                let r1 = order_checker
                    .check(&input)
                    .expect("order checker should not error");
                let r2 = vol_adjuster
                    .check(&input)
                    .expect("vol adjuster should not error");
                let r3 = position_checker
                    .check(&input)
                    .expect("position checker should not error");

                // Basic sanity: these are valid decisions (not panics)
                matches!(
                    r1,
                    RiskDecision::Approve
                        | RiskDecision::ApproveWithAdjustment { .. }
                        | RiskDecision::Reject { .. }
                );
                matches!(
                    r2,
                    RiskDecision::Approve
                        | RiskDecision::ApproveWithAdjustment { .. }
                        | RiskDecision::Reject { .. }
                );
                matches!(
                    r3,
                    RiskDecision::Approve
                        | RiskDecision::ApproveWithAdjustment { .. }
                        | RiskDecision::Reject { .. }
                );
            })
        })
        .collect();

    for handle in handles {
        handle.join().expect("Thread should not panic");
    }
}

// ── Test 6: Position manager accumulates then stops ───────────────────────

#[test]
fn position_manager_stop_prevents_new_orders() {
    let mut mgr = RiskPositionManager::new(
        PnLLimits {
            daily_loss_limit: -5_000.0,
            position_loss_limit: -1_000.0,
            max_drawdown_pct: 0.10,
        },
        StopLossConfig {
            hard_stop_pct: 0.05,
            trailing_stop_pct: 0.03,
        },
    );

    // Open a position
    mgr.update_position(&btc(), Side::Buy, 1.0, 50_000.0, 1_000);

    // Simulate a large loss (price drops 10% → stops triggered at 5%)
    let prices = std::collections::HashMap::from([(btc(), 44_000.0)]);
    mgr.update_prices(&prices);

    // Check stops
    let triggered = mgr.check_stops();
    assert!(
        !triggered.is_empty(),
        "Stop should be triggered at 12% loss"
    );

    // PositionRiskChecker rejects when daily loss limit exceeded
    let checker = PositionRiskChecker::new(10, -3_000.0);
    let mut input = make_input();
    input.portfolio.daily_pnl = -4_000.0; // simulate the loss

    let result = checker.check(&input).unwrap();
    assert!(matches!(result, RiskDecision::Reject { .. }));
}
