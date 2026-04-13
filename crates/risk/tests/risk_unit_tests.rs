use domain::{InstrumentId, Side, Venue};
use risk::core::{
    CompositeRiskChecker, MarketContext, OrderContext, OrderType, PortfolioContext, RiskChecker,
    RiskDecision, RiskInput,
};
use risk::risk::{
    EwmaVolatility, OrderRiskChecker, OrderRiskConfig, RiskRule, RuleAction, RuleCondition,
    RuleEngine, VolatilityAdjuster,
};

fn make_input() -> RiskInput {
    RiskInput {
        order: OrderContext {
            instrument: InstrumentId::new(Venue::Crypto, "BTC-USD"),
            side: Side::Buy,
            quantity: 1.0,
            limit_price: Some(50_000.0),
            order_type: OrderType::Limit,
            strategy_id: "test".into(),
            submitted_ts_ms: 0,
        },
        market: MarketContext {
            instrument: InstrumentId::new(Venue::Crypto, "BTC-USD"),
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
            total_exposure: 20_000.0,
            open_positions: 2,
            daily_pnl: 500.0,
            daily_pnl_limit: -5_000.0,
        },
    }
}

// ── Test 1: CompositeRiskChecker approves clean order ─────────────────────

#[test]
fn composite_approves_clean_order() {
    let order_checker = OrderRiskChecker::new(OrderRiskConfig::default());
    let vol_adjuster = VolatilityAdjuster::new(0.94, 100.0, 10_000_000.0);

    let checker = CompositeRiskChecker::new(vec![Box::new(order_checker), Box::new(vol_adjuster)]);

    let result = checker.check(&make_input()).unwrap();
    // Clean order with normal volatility should approve (or approve-with-adjustment for vol adjuster)
    match result {
        RiskDecision::Approve | RiskDecision::ApproveWithAdjustment { .. } => {}
        RiskDecision::Reject { reason, .. } => panic!("Should not reject: {}", reason),
    }
}

// ── Test 2: CompositeRiskChecker rejects when any checker rejects ──────────

#[test]
fn composite_rejects_when_any_checker_rejects() {
    let order_checker = OrderRiskChecker::new(OrderRiskConfig::default());
    let vol_adjuster = VolatilityAdjuster::new(0.94, 100.0, 10_000_000.0);

    let checker = CompositeRiskChecker::new(vec![Box::new(order_checker), Box::new(vol_adjuster)]);

    let mut input = make_input();
    // Trigger circuit breaker
    input.market.volatility = 0.50;

    let result = checker.check(&input).unwrap();
    assert!(
        matches!(result, RiskDecision::Reject { .. }),
        "Expected reject but got {:?}",
        result
    );
}

// ── Test 3: RuleEngine + OrderRiskChecker: JSON rule overrides static ─────

#[test]
fn rule_engine_json_loaded_rule_overrides() {
    let json = r#"[
        {
            "id": "qty-limit",
            "name": "Quantity Limit",
            "priority": 5,
            "condition": {"type": "quantity_exceeds", "threshold": 0.5},
            "action": {"type": "reject", "reason": "quantity exceeds JSON rule limit"},
            "enabled": true
        }
    ]"#;

    let rules = RuleEngine::load_from_json(json).unwrap();
    let engine = RuleEngine::new(rules);

    let input = make_input(); // quantity = 1.0 > 0.5 threshold
    let result = engine.check(&input).unwrap();
    assert!(
        matches!(result, RiskDecision::Reject { .. }),
        "Expected JSON rule to reject"
    );
}

// ── Test 4: Circuit breaker + EWMA: volatile market reduces orders ─────────

#[test]
fn circuit_breaker_and_ewma_under_volatile_market() {
    let mut vol_adjuster = VolatilityAdjuster::new(0.94, 100.0, 10_000_000.0);

    // Feed high returns to drive EWMA volatility up
    for _ in 0..50 {
        vol_adjuster.update_volatility(0.08); // 8% returns
    }

    let order_checker = OrderRiskChecker::new(OrderRiskConfig {
        circuit_breaker_volatility: 0.05,
        ..OrderRiskConfig::default()
    });

    let checker = CompositeRiskChecker::new(vec![Box::new(order_checker)]);

    let mut input = make_input();
    input.market.volatility = 0.10; // above 5% circuit breaker threshold

    let result = checker.check(&input).unwrap();
    assert!(
        matches!(result, RiskDecision::Reject { .. }),
        "Expected circuit breaker to trigger"
    );

    // Check EWMA itself shows elevated volatility
    let ewma_vol = {
        let mut ewma = EwmaVolatility::new(0.94, 0.0004);
        for _ in 0..50 {
            ewma.update(0.08);
        }
        ewma.volatility()
    };
    assert!(
        ewma_vol > 0.02,
        "EWMA should show elevated vol: {}",
        ewma_vol
    );
}
