use std::collections::HashMap;

use domain::{InstrumentId, Side, Venue};
use exec::core::order::OrderManager;
use exec::core::position::{ExecPositionManager, FillRecord, TaxLotMethod};
use exec::monitor::alert::{ExecAlertManager, ExecAlertThreshold, ExecAlertType};
use exec::monitor::metrics::ExecutionMetrics;
use exec::monitor::pnl::PnlCalculator;
use exec::monitor::tracing::{ExecutionTracer, SpanKind};
use exec::api::health::{HealthChecker, HealthStatus};

fn btc() -> InstrumentId {
    InstrumentId::new(Venue::Crypto, "BTC-USD")
}

fn fill(order_id: &str, side: Side, qty: f64, price: f64) -> FillRecord {
    FillRecord {
        order_id: order_id.to_string(),
        instrument: btc(),
        side,
        qty,
        price,
        commission: 1.0,
        ts_ms: 1000,
    }
}

#[test]
fn high_rejection_rate_triggers_alert() {
    let thresholds = vec![ExecAlertThreshold {
        alert_type: ExecAlertType::HighRejectionRate,
        threshold: 0.2,
        window_ms: 1000,
    }];
    let mut mgr = ExecAlertManager::new(thresholds);
    let mut metrics = ExecutionMetrics::new();

    for _ in 0..10 {
        metrics.record_submit(100);
    }
    for _ in 0..5 {
        metrics.record_rejection();
    }
    let snap = metrics.snapshot(1000);
    let alerts = mgr.check(&snap, 1000);
    assert!(!alerts.is_empty(), "expected alert to fire for 50% rejection rate");
    assert_eq!(alerts[0].alert_type, ExecAlertType::HighRejectionRate);
}

#[test]
fn tracer_records_full_order_lifecycle() {
    let mut tracer = ExecutionTracer::new(100);

    // Simulate: risk check → route decision → order submit → order fill → persist
    let risk_id = tracer.start_span(SpanKind::RiskCheck, None, 1000);
    tracer.tag_span(&risk_id, "order_id", "o1");
    tracer.finish_span(&risk_id, 1050);

    let route_id = tracer.start_span(SpanKind::RouteDecision, Some(risk_id.clone()), 1050);
    tracer.finish_span(&route_id, 1100);

    let submit_id = tracer.start_span(SpanKind::OrderSubmit, Some(route_id.clone()), 1100);
    tracer.tag_span(&submit_id, "order_id", "o1");
    tracer.finish_span(&submit_id, 1200);

    let fill_id = tracer.start_span(SpanKind::OrderFill, Some(submit_id.clone()), 1500);
    tracer.finish_span(&fill_id, 1600);

    let persist_id = tracer.start_span(SpanKind::PersistOrder, Some(fill_id.clone()), 1600);
    tracer.finish_span(&persist_id, 1650);

    assert_eq!(tracer.spans.len(), 5);

    let submit_spans = tracer.spans_by_kind(&SpanKind::OrderSubmit);
    assert_eq!(submit_spans.len(), 1);
    assert_eq!(submit_spans[0].tags.get("order_id").unwrap(), "o1");
    assert_eq!(submit_spans[0].duration_us(), Some(100));
}

#[test]
fn pnl_calculator_accumulates_correctly() {
    let mut calc = PnlCalculator::new(0.001);
    let mut pos_mgr = ExecPositionManager::new(TaxLotMethod::Fifo);

    let buy = fill("o1", Side::Buy, 10.0, 100.0);
    let sell = fill("o2", Side::Sell, 10.0, 110.0);

    pos_mgr.apply_fill(&buy);
    pos_mgr.apply_fill(&sell);
    calc.record_fill(buy);
    calc.record_fill(sell);

    let prices = HashMap::new();
    let snap = calc.snapshot(&pos_mgr, &prices, 2000);

    // 10*(110-100) = 100, minus 2*1.0 commission = 98
    assert!((snap.realised_pnl - 98.0).abs() < 1e-6, "realised_pnl={}", snap.realised_pnl);
    assert!((snap.commission_total - 2.0).abs() < 1e-6);
    assert_eq!(snap.ts_ms, 2000);
}

#[test]
fn health_report_from_working_order_manager_is_ok() {
    let mgr = OrderManager::new();
    let mut checker = HealthChecker::new();
    checker.add_check(move || HealthChecker::order_manager_check(&mgr));

    let report = checker.check_all(1000);
    assert_eq!(report.overall, HealthStatus::Ok);
    assert!(report.is_ready());
    assert!(report.is_live());
}
