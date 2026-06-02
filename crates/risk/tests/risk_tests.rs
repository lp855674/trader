use risk::{RiskError, RiskPolicy};
use rust_decimal::Decimal;
use trader_core::{OrderRequest, OrderSide, OrderType};

#[test]
fn rejects_order_quantity_above_limit() {
    let policy = RiskPolicy::new(Decimal::from(10), Decimal::from(1_000), Decimal::from(500));
    let order = buy_order(Decimal::from(11));

    assert_eq!(
        policy
            .check_order(&order, Decimal::from(100), Decimal::from(10_000), false)
            .unwrap_err(),
        RiskError::MaxOrderQuantity
    );
}

#[test]
fn rejects_order_notional_above_limit() {
    let policy = RiskPolicy::new(Decimal::from(100), Decimal::from(1_000), Decimal::from(500));
    let order = buy_order(Decimal::from(11));

    assert_eq!(
        policy
            .check_order(&order, Decimal::from(100), Decimal::from(10_000), false)
            .unwrap_err(),
        RiskError::MaxOrderNotional
    );
}

#[test]
fn rejects_buy_when_cash_is_insufficient() {
    let policy = RiskPolicy::new(
        Decimal::from(100),
        Decimal::from(10_000),
        Decimal::from(500),
    );
    let order = buy_order(Decimal::from(6));

    assert_eq!(
        policy
            .check_order(&order, Decimal::from(100), Decimal::from(500), false)
            .unwrap_err(),
        RiskError::InsufficientCash
    );
}

#[test]
fn rejects_when_trading_halted() {
    let policy = RiskPolicy::new(
        Decimal::from(100),
        Decimal::from(10_000),
        Decimal::from(500),
    );

    assert_eq!(
        policy
            .check_order(
                &buy_order(Decimal::ONE),
                Decimal::from(100),
                Decimal::from(500),
                true
            )
            .unwrap_err(),
        RiskError::TradingHalted
    );
}

fn buy_order(qty: Decimal) -> OrderRequest {
    OrderRequest {
        symbol: "US:NASDAQ:AAPL:EQUITY".to_string(),
        side: OrderSide::Buy,
        order_type: OrderType::Market,
        qty,
        price: None,
        account_id: "paper".to_string(),
    }
}
