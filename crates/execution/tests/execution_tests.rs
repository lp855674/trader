use execution::{
    ExecutionIntent, ReduceOnlyIntent, TimeSlicedIntent, WeightedIntent, expand_execution_intent,
    order_for_target_delta,
};
use portfolio::TargetPosition;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use trader_core::{OrderRequest, OrderSide, OrderType};

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

#[test]
fn immediate_intent_returns_single_market_order() {
    let orders = expand_execution_intent(ExecutionIntent::Immediate(order(dec!(6)))).unwrap();

    assert_eq!(orders.len(), 1);
    assert_eq!(orders[0].qty, dec!(6));
    assert_eq!(orders[0].order_type, OrderType::Market);
}

#[test]
fn twap_intent_splits_order_into_equal_slices() {
    let orders = expand_execution_intent(ExecutionIntent::Twap(TimeSlicedIntent {
        order: order(dec!(9)),
        slices: 3,
    }))
    .unwrap();

    assert_eq!(orders.len(), 3);
    assert!(orders.iter().all(|order| order.qty == dec!(3)));
}

#[test]
fn vwap_intent_splits_order_by_volume_weights() {
    let orders = expand_execution_intent(ExecutionIntent::Vwap(WeightedIntent {
        order: order(dec!(8)),
        weights: vec![dec!(1), dec!(2), dec!(1)],
    }))
    .unwrap();

    assert_eq!(
        orders.iter().map(|order| order.qty).collect::<Vec<_>>(),
        vec![dec!(2), dec!(4), dec!(2)]
    );
}

#[test]
fn post_only_intent_creates_post_only_limit_order() {
    let orders =
        expand_execution_intent(ExecutionIntent::PostOnly(order(dec!(5)), dec!(12.34))).unwrap();

    assert_eq!(orders.len(), 1);
    assert_eq!(orders[0].order_type, OrderType::PostOnly);
    assert_eq!(orders[0].price, Some(dec!(12.34)));
}

#[test]
fn reduce_only_intent_clips_order_to_existing_position() {
    let mut base = order(dec!(10));
    base.side = OrderSide::Sell;

    let orders = expand_execution_intent(ExecutionIntent::ReduceOnly(ReduceOnlyIntent {
        order: base,
        current_qty: dec!(4),
    }))
    .unwrap();

    assert_eq!(orders.len(), 1);
    assert_eq!(orders[0].qty, dec!(4));
    assert_eq!(orders[0].side, OrderSide::Sell);
}

#[test]
fn reduce_only_intent_returns_no_order_when_it_would_increase_exposure() {
    let orders = expand_execution_intent(ExecutionIntent::ReduceOnly(ReduceOnlyIntent {
        order: order(dec!(4)),
        current_qty: dec!(4),
    }))
    .unwrap();

    assert!(orders.is_empty());
}

fn target(target_qty: Decimal) -> TargetPosition {
    TargetPosition {
        symbol: "US:NASDAQ:AAPL:EQUITY".to_string(),
        target_qty,
    }
}

fn order(qty: Decimal) -> OrderRequest {
    OrderRequest {
        symbol: "US:NASDAQ:AAPL:EQUITY".to_string(),
        side: OrderSide::Buy,
        order_type: OrderType::Market,
        qty,
        price: None,
        account_id: "paper".to_string(),
    }
}
