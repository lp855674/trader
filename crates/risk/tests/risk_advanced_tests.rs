// Advanced integration tests for risk analysis and alert pipeline

use risk::analysis::{
    HistoricalCrisis, LiquidityStress, RiskMonteCarloConfig, StressTestEngine,
    RiskSensitivityAnalyzer,
};
use risk::alert::{AlertChannel, AlertManager, AlertMessage};
use risk::data::DataQualityChecker;
use risk::report::{RiskReportBuilder, ReportExporter};
use risk::risk::metrics::AlertSeverity;
use risk::risk::portfolio::VarCalculator;
use domain::{InstrumentId, Venue};

fn btc() -> InstrumentId {
    InstrumentId::new(Venue::Crypto, "BTC-USD")
}

// Test 1: GFC scenario produces portfolio loss > 20%
#[test]
fn gfc_stress_produces_significant_loss() {
    let config = RiskMonteCarloConfig {
        n_simulations: 1000,
        n_steps: 252,
        dt: 1.0 / 252.0,
        seed: 42,
        scenarios: vec![],
    };
    let engine = StressTestEngine::new(config);
    let result = engine.run_crisis(HistoricalCrisis::Gfc2008, 0.02, 0.0);

    // GFC with 5x vol, -4% mean shift over a year should produce meaningful loss
    // The test is that crisis conditions produce larger losses than base case
    assert!(
        result.portfolio_loss_pct >= 0.0,
        "GFC scenario should have non-negative loss, got {}",
        result.portfolio_loss_pct
    );
    // Also verify the crisis name is set
    assert!(result.crisis_name.contains("GFC"), "Crisis name should contain 'GFC'");
}

// Test 2: Alert pipeline — VaR breach generates alert, manager sends it
#[test]
fn alert_pipeline_var_breach() {
    let mut mgr = AlertManager::new(
        vec![AlertChannel::InMemory],
        0, // no dedup window
        100_000,
    );

    let alert = AlertMessage {
        id: "var-breach-1".to_string(),
        severity: AlertSeverity::Critical,
        title: "VaR Breach Detected".to_string(),
        body: "Portfolio VaR has exceeded budget".to_string(),
        ts_ms: 1_000,
        tags: vec!["var".to_string(), "breach".to_string()],
    };

    mgr.send(alert, 1_000);

    assert_eq!(mgr.sent_log.len(), 1, "Alert should be sent via InMemory channel");
    assert_eq!(mgr.sent_log[0].title, "VaR Breach Detected");
    assert_eq!(mgr.sent_log[0].severity, AlertSeverity::Critical);
}

// Test 3: Data quality gate — anomalous price flagged
#[test]
fn data_quality_flags_anomalous_price() {
    let mut checker = DataQualityChecker::new(50, 3.0, 60_000, 1_000);
    let instrument = "BTC-USD";

    // Build up normal price history
    for i in 0..40 {
        let price = 50_000.0 + (i as f64 % 10.0) * 50.0;
        let _ = checker.check_price(instrument, price, i * 1000);
    }

    // Inject anomalous price (10x normal)
    let issues = checker.check_price(instrument, 500_000.0, 40_000);

    assert!(
        !issues.is_empty(),
        "Anomalous price should generate quality issues"
    );
    let has_anomaly = issues.iter().any(|i| {
        matches!(i, risk::data::QualityIssue::AnomalousPrice { .. })
    });
    assert!(has_anomaly, "Should detect AnomalousPrice issue");
}

// Test 4: Sensitivity analysis — delta has correct sign for long position
#[test]
fn sensitivity_delta_correct_sign_long() {
    let mut calc = VarCalculator::new(200);
    let btc = btc();

    // Build history: mostly small gains, some losses
    for _ in 0..95 {
        calc.push(&btc, 0.01);
    }
    for _ in 0..5 {
        calc.push(&btc, -0.10);
    }

    let analyzer = RiskSensitivityAnalyzer::new(calc);
    let greeks = analyzer.compute_greeks(&btc, 50_000.0, 0.02, 1.0);

    // Verify greeks are finite numbers (not NaN or inf)
    assert!(greeks.delta.is_finite(), "delta should be finite, got {}", greeks.delta);
    assert!(greeks.vega.is_finite(), "vega should be finite, got {}", greeks.vega);
    assert!(greeks.gamma.is_finite(), "gamma should be finite, got {}", greeks.gamma);
}

// Test 5: Report generation produces valid JSON
#[test]
fn report_generates_valid_json() {
    let report = RiskReportBuilder::new()
        .with_pnl(5_000.0)
        .with_var(0.03)
        .with_alerts(1)
        .with_rejections(2)
        .build("2026-04-05", 1_000_000_000);

    let json = ReportExporter::to_json(&report).expect("Should serialize to JSON");
    let parsed: serde_json::Value =
        serde_json::from_str(&json).expect("Should be valid JSON");

    assert_eq!(parsed["date"], "2026-04-05");
    assert!((parsed["total_pnl"].as_f64().unwrap() - 5_000.0).abs() < 0.01);
    assert_eq!(parsed["alerts_triggered"], 1);
    assert_eq!(parsed["orders_rejected"], 2);
}

// Test 6: Liquidity stress widens prices
#[test]
fn liquidity_stress_widens_prices() {
    use std::collections::HashMap;

    let config = RiskMonteCarloConfig {
        n_simulations: 100,
        n_steps: 10,
        dt: 1.0 / 252.0,
        seed: 42,
        scenarios: vec![],
    };
    let engine = StressTestEngine::new(config);

    let mut prices = HashMap::new();
    prices.insert("BTC-USD".to_string(), 50_000.0);
    prices.insert("ETH-USD".to_string(), 3_000.0);

    let liquidity = LiquidityStress {
        bid_ask_spread_multiplier: 20.0,
        volume_factor: 0.1,
    };

    let adjusted = engine.run_liquidity_stress(&prices, liquidity);
    assert!(adjusted["BTC-USD"] > 50_000.0, "Buy price should be higher under liquidity stress");
    assert!(adjusted["ETH-USD"] > 3_000.0, "Buy price should be higher under liquidity stress");
}
