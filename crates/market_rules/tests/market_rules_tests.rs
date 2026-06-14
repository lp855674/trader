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
        initial_margin_rate: Decimal::ZERO,
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

#[test]
fn selects_cn_equity_rules_from_symbol() {
    let rules = MarketRuleSet::for_symbol("CN:SSE:600000:EQUITY").unwrap();

    assert_eq!(rules.lot_size, Decimal::from(100));
    assert_eq!(rules.tick_size, Decimal::new(1, 2));
}

#[test]
fn selects_hk_equity_rules_from_symbol() {
    let rules = MarketRuleSet::for_symbol("HK:HKEX:00700:EQUITY").unwrap();

    assert_eq!(rules.lot_size, Decimal::from(100));
    assert_eq!(rules.tick_size, Decimal::new(1, 3));
}

#[test]
fn selects_us_equity_rules_from_symbol() {
    let rules = MarketRuleSet::for_symbol("US:NASDAQ:AAPL:EQUITY").unwrap();

    assert_eq!(rules.lot_size, Decimal::ONE);
    assert_eq!(rules.tick_size, Decimal::new(1, 2));
}

#[test]
fn selects_crypto_spot_rules_from_symbol() {
    let rules = MarketRuleSet::for_symbol("CRYPTO:BINANCE:BTCUSDT:CRYPTO_SPOT").unwrap();

    assert_eq!(rules.lot_size, Decimal::new(1, 6));
    assert_eq!(rules.tick_size, Decimal::new(1, 2));
    assert_eq!(rules.min_notional, Decimal::from(10));
}

#[test]
fn selects_crypto_perp_rules_from_symbol() {
    let rules = MarketRuleSet::for_symbol("CRYPTO:BINANCE:BTCUSDT_PERP:CRYPTO_PERP").unwrap();

    assert_eq!(rules.lot_size, Decimal::new(1, 3));
    assert_eq!(rules.tick_size, Decimal::new(1, 2));
    assert_eq!(rules.min_notional, Decimal::from(5));
    assert_eq!(rules.initial_margin_rate, Decimal::new(1, 1));
}

#[test]
fn selects_crypto_future_rules_from_symbol() {
    let rules = MarketRuleSet::for_symbol("CRYPTO:BINANCE:BTCUSDT_240628:CRYPTO_FUTURE").unwrap();

    assert_eq!(rules.lot_size, Decimal::new(1, 3));
    assert_eq!(rules.tick_size, Decimal::new(1, 2));
    assert_eq!(rules.min_notional, Decimal::from(5));
    assert_eq!(rules.initial_margin_rate, Decimal::new(1, 1));
}

#[test]
fn rejects_unknown_symbol_rule_set() {
    let error = MarketRuleSet::for_symbol("US:NASDAQ:AAPL:OPTION").unwrap_err();

    assert_eq!(
        error,
        MarketRuleError::UnsupportedSymbol("US:NASDAQ:AAPL:OPTION".to_string())
    );
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
