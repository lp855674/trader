use execution::order_for_target_delta;
use portfolio::TargetPosition;
use rust_decimal::Decimal;
use trader_core::OrderSide;

#[test]
fn creates_buy_order_for_positive_delta() {
    let target = target(Decimal::from(10));
    let order = order_for_target_delta(&target, Decimal::from(4), "paper").unwrap();

    assert_eq!(order.side, OrderSide::Buy);
    assert_eq!(order.qty, Decimal::from(6));
}

#[test]
fn creates_sell_order_for_negative_delta() {
    let target = target(Decimal::from(3));
    let order = order_for_target_delta(&target, Decimal::from(10), "paper").unwrap();

    assert_eq!(order.side, OrderSide::Sell);
    assert_eq!(order.qty, Decimal::from(7));
}

#[test]
fn returns_none_when_target_already_met() {
    let target = target(Decimal::from(10));

    assert!(order_for_target_delta(&target, Decimal::from(10), "paper").is_none());
}

fn target(target_qty: Decimal) -> TargetPosition {
    TargetPosition {
        symbol: "US:NASDAQ:AAPL:EQUITY".to_string(),
        target_qty,
    }
}
