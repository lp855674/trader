// Full system integration test (4.8)

use std::sync::Arc;
use risk::alert::{AlertChannel, AlertManager, AlertMessage};
use risk::analysis::{HistoricalCrisis, RiskMonteCarloConfig, StressTestEngine};
use risk::config::{RiskConfigLoader, RiskSystemConfig};
use risk::core::{CompositeRiskChecker, RiskDecision, RiskInput,
    MarketContext, OrderContext, OrderType, PortfolioContext};
use risk::execution::{LiveConfig, LiveExecutionMode};
use risk::report::{RiskReportBuilder, ReportExporter};
use risk::risk::metrics::AlertSeverity;
use risk::risk::order::{OrderRiskChecker, OrderRiskConfig};
use risk::risk::portfolio::{PortfolioRiskChecker, PortfolioRiskConfig};
use risk::risk::position::PositionRiskChecker;
use domain::{InstrumentId, Side, Venue};

fn btc() -> InstrumentId {
    InstrumentId::new(Venue::Crypto, "BTC-USD")
}

fn make_valid_input() -> RiskInput {
    // Use small notional: 0.1 BTC @ 50,000 = 5,000 notional
    // max_single_position_pct = 0.20 → 20,000 limit; 5,000 < 20,000 ✓
    // total_exposure 20,000 + 5,000 = 25,000 < 90,000 ✓
    RiskInput {
        order: OrderContext {
            instrument: btc(),
            side: Side::Buy,
            quantity: 0.1,
            limit_price: Some(50_000.0),
            order_type: OrderType::Limit,
            strategy_id: "sys-test".into(),
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
            total_exposure: 20_000.0,
            open_positions: 2,
            daily_pnl: 500.0,
            daily_pnl_limit: -5_000.0,
        },
    }
}

fn make_invalid_input() -> RiskInput {
    let mut input = make_valid_input();
    // Over max quantity → will be rejected
    input.order.quantity = 10_000.0;
    input
}

// Step 1: Load RiskSystemConfig from JSON
#[test]
fn load_config_from_json() {
    let config = RiskSystemConfig::default();
    let json = serde_json::to_string(&config).expect("serialize config");
    let loaded = RiskConfigLoader::from_json(&json).expect("load config");
    assert!(RiskConfigLoader::validate(&loaded).is_ok());
}

// Step 2: Build CompositeRiskChecker from config
fn build_composite() -> Arc<CompositeRiskChecker> {
    let order_checker = OrderRiskChecker::new(OrderRiskConfig::default());
    let portfolio_checker = PortfolioRiskChecker::new(PortfolioRiskConfig::default());
    let position_checker = PositionRiskChecker::new(10, -5_000.0);

    Arc::new(CompositeRiskChecker::new(vec![
        Box::new(order_checker),
        Box::new(portfolio_checker),
        Box::new(position_checker),
    ]))
}

// Step 3-5: Live execution mode processes orders and circuit breaker trips
#[test]
fn circuit_breaker_trips_after_consecutive_rejections() {
    let checker = build_composite();
    let config = LiveConfig {
        circuit_breaker_threshold: 3,
        circuit_breaker_window_ms: 60_000,
        ..LiveConfig::default()
    };
    let mut live = LiveExecutionMode::new(checker, config);

    // Process 10 orders: first 3 invalid (will be rejected), rest valid
    let invalid_input = make_invalid_input();
    let valid_input = make_valid_input();

    let mut reject_count = 0;
    for i in 0..3 {
        let result = live.check_and_submit(&invalid_input, i * 1000).unwrap();
        if matches!(result, RiskDecision::Reject { .. }) {
            reject_count += 1;
        }
    }

    assert_eq!(reject_count, 3, "All 3 invalid orders should be rejected");
    assert!(
        live.is_circuit_breaker_open(),
        "Circuit breaker should be open after 3 rejections"
    );

    // Next order (even valid) should be blocked by circuit breaker
    let result = live.check_and_submit(&valid_input, 10_000).unwrap();
    assert!(
        matches!(result, RiskDecision::Reject { ref reason, .. } if reason.contains("circuit breaker")),
        "Circuit breaker should block all orders when open"
    );
}

// Step 6: StressTestEngine GFC scenario
#[test]
fn stress_test_gfc_scenario() {
    let config = RiskMonteCarloConfig {
        n_simulations: 500,
        n_steps: 100,
        dt: 1.0 / 252.0,
        seed: 42,
        scenarios: vec![],
    };
    let engine = StressTestEngine::new(config);
    let result = engine.run_crisis(HistoricalCrisis::Gfc2008, 0.02, 0.0);

    // Verify the result has a valid name and non-negative loss
    assert!(result.crisis_name.len() > 0);
    assert!(result.portfolio_loss_pct >= 0.0);
}

// Step 7: Report generation produces valid JSON
#[test]
fn report_generation_valid_json() {
    let report = RiskReportBuilder::new()
        .with_pnl(-1_500.0)
        .with_var(0.05)
        .with_alerts(3)
        .with_rejections(5)
        .build("2026-04-05", 1_714_000_000_000);

    let json = ReportExporter::to_json(&report).expect("JSON serialization should succeed");
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("Should be valid JSON");

    assert_eq!(parsed["date"], "2026-04-05");
    assert!(parsed["total_pnl"].as_f64().is_some());
    assert!(parsed["var_95"].as_f64().is_some());
    assert_eq!(parsed["alerts_triggered"], 3);
    assert_eq!(parsed["orders_rejected"], 5);
}

// Step 8: AlertManager fires alert on VaR breach
#[test]
fn alert_manager_fires_on_var_breach() {
    let mut mgr = AlertManager::new(
        vec![AlertChannel::InMemory],
        0,       // no dedup
        100_000, // escalate after 100s
    );

    let alert = AlertMessage {
        id: "var-001".to_string(),
        severity: AlertSeverity::Critical,
        title: "VaR Limit Exceeded".to_string(),
        body: "Current VaR 8% exceeds budget 5%".to_string(),
        ts_ms: 0,
        tags: vec!["var".to_string()],
    };

    mgr.send(alert, 0);

    assert_eq!(mgr.sent_log.len(), 1, "Alert should be sent via InMemory");
    assert_eq!(mgr.sent_log[0].id, "var-001");
    assert_eq!(mgr.sent_log[0].severity, AlertSeverity::Critical);
}

// Full pipeline test: config → checker → live mode → stress → report → alert
#[test]
fn full_pipeline_integration() {
    // 1. Load config
    let config = RiskSystemConfig::default();
    let json = serde_json::to_string(&config).unwrap();
    let loaded_config = RiskConfigLoader::from_json(&json).unwrap();
    assert!(RiskConfigLoader::validate(&loaded_config).is_ok());

    // 2. Build checker
    let checker = build_composite();

    // 3. Process orders through live mode
    let live_config = LiveConfig {
        circuit_breaker_threshold: 5,
        circuit_breaker_window_ms: 60_000,
        ..LiveConfig::default()
    };
    let mut live = LiveExecutionMode::new(checker, live_config);
    let valid = make_valid_input();
    let result = live.check_and_submit(&valid, 0).unwrap();
    assert!(
        matches!(result, RiskDecision::Approve | RiskDecision::ApproveWithAdjustment { .. }),
        "Valid input should be approved: {:?}",
        result
    );

    // 4. Run stress test
    let mc_config = RiskMonteCarloConfig {
        n_simulations: 200,
        n_steps: 50,
        dt: 1.0 / 252.0,
        seed: 1,
        scenarios: vec![],
    };
    let engine = StressTestEngine::new(mc_config);
    let stress = engine.run_crisis(HistoricalCrisis::Covid2020, 0.02, 0.0);
    assert!(stress.crisis_name.len() > 0);

    // 5. Generate report
    let report = RiskReportBuilder::new()
        .with_pnl(1_000.0)
        .with_var(0.04)
        .with_alerts(0)
        .with_rejections(0)
        .build("2026-04-05", 0);

    let json = ReportExporter::to_json(&report).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed["date"], "2026-04-05");

    // 6. Alert manager
    let mut alert_mgr = AlertManager::new(vec![AlertChannel::InMemory], 0, 50_000);
    alert_mgr.send(
        AlertMessage {
            id: "test-001".to_string(),
            severity: AlertSeverity::Warning,
            title: "System OK".to_string(),
            body: "All systems nominal".to_string(),
            ts_ms: 0,
            tags: vec![],
        },
        0,
    );
    assert_eq!(alert_mgr.sent_log.len(), 1);
}
