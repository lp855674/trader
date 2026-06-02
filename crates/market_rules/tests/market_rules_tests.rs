use market_rules::{MarketRuleError, MarketRuleSet};
use rust_decimal::Decimal;
use trader_core::{OrderRequest, OrderSide, OrderType};

#[test]
fn rejects_quantity_below_lot_size() {
    let rules = MarketRuleSet::us_equity();
    let order = market_order(Decimal::new(5, 1));

    assert_eq!(
        rules
            .validate_order(&order, Decimal::from(100))
            .unwrap_err(),
        MarketRuleError::InvalidLotSize
    );
}

#[test]
fn rejects_limit_price_off_tick_size() {
    let rules = MarketRuleSet::us_equity();
    let mut order = market_order(Decimal::ONE);
    order.order_type = OrderType::Limit;
    order.price = Some(Decimal::new(100_001, 3));

    assert_eq!(
        rules
            .validate_order(&order, Decimal::from(100))
            .unwrap_err(),
        MarketRuleError::InvalidTickSize
    );
}

#[test]
fn rejects_notional_below_minimum() {
    let rules = MarketRuleSet {
        lot_size: Decimal::ONE,
        tick_size: Decimal::new(1, 2),
        min_qty: Decimal::ONE,
        min_notional: Decimal::from(100),
        allow_market_orders: true,
    };
    let order = market_order(Decimal::ONE);

    assert_eq!(
        rules.validate_order(&order, Decimal::from(50)).unwrap_err(),
        MarketRuleError::MinNotional
    );
}

#[test]
fn accepts_valid_us_equity_market_order() {
    let rules = MarketRuleSet::us_equity();
    rules
        .validate_order(&market_order(Decimal::ONE), Decimal::from(100))
        .unwrap();
}

fn market_order(qty: Decimal) -> OrderRequest {
    OrderRequest {
        symbol: "US:NASDAQ:AAPL:EQUITY".to_string(),
        side: OrderSide::Buy,
        order_type: OrderType::Market,
        qty,
        price: None,
        account_id: "paper".to_string(),
    }
}
