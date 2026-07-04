use risk::{
    DailyLossGuard, MarketDataFreshnessGuard, OrderThrottleGuard, PriceDeviationGuard,
    StrategyCircuitBreaker, TradingSessionGuard,
};
use rust_decimal_macros::dec;

#[test]
fn rejects_order_when_market_data_is_stale() {
    let guard = MarketDataFreshnessGuard::new(5_000);
    let error = guard.check(1_000_000, 1_006_000).unwrap_err();

    assert_eq!(error.risk_type, "stale_market_data");
}

#[test]
fn allows_order_when_market_data_is_fresh() {
    let guard = MarketDataFreshnessGuard::new(5_000);

    assert!(guard.check(1_000_000, 1_004_999).is_ok());
}

#[test]
fn rejects_order_when_limit_price_deviates_from_reference() {
    let guard = PriceDeviationGuard::new(dec!(50));
    let error = guard.check(dec!(101), dec!(100)).unwrap_err();

    assert_eq!(error.risk_type, "price_deviation");
}

#[test]
fn rejects_after_daily_loss_limit_is_breached() {
    let guard = DailyLossGuard::new(dec!(50));
    let error = guard.check(dec!(10000), dec!(9949.99)).unwrap_err();

    assert_eq!(error.risk_type, "daily_loss_limit");
}

#[test]
fn rejects_after_order_attempt_limit_is_reached() {
    let guard = OrderThrottleGuard::new(Some(20), Some(5));
    let error = guard.check_attempts(20).unwrap_err();

    assert_eq!(error.risk_type, "max_order_attempts");
}

#[test]
fn rejects_after_order_failure_limit_is_reached() {
    let guard = OrderThrottleGuard::new(Some(20), Some(5));
    let error = guard.check_failures(5).unwrap_err();

    assert_eq!(error.risk_type, "max_order_failures");
}

#[test]
fn rejects_after_strategy_loss_limit_is_reached() {
    let guard = StrategyCircuitBreaker::new(Some(3), Some(2));
    let error = guard.check(3, 0).unwrap_err();

    assert_eq!(error.risk_type, "strategy_loss_circuit_breaker");
}

#[test]
fn rejects_after_strategy_error_limit_is_reached() {
    let guard = StrategyCircuitBreaker::new(Some(3), Some(2));
    let error = guard.check(0, 2).unwrap_err();

    assert_eq!(error.risk_type, "strategy_error_circuit_breaker");
}

#[test]
fn rejects_when_outside_trading_session() {
    let guard = TradingSessionGuard::new(9 * 60 + 30, 16 * 60);
    let error = guard.check(true, 8 * 60 + 45).unwrap_err();

    assert_eq!(error.risk_type, "trading_session_closed");
}

#[test]
fn allows_when_inside_trading_session() {
    let guard = TradingSessionGuard::new(9 * 60 + 30, 16 * 60);

    assert!(guard.check(true, 10 * 60).is_ok());
}
