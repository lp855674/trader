use risk::{PortfolioRiskPolicy, PortfolioRiskState, RiskError, RiskPolicy};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
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

#[test]
fn rejects_portfolio_exposure_above_limit() {
    let policy = PortfolioRiskPolicy::new(dec!(1000), dec!(0.2), dec!(2), dec!(500));
    let state = PortfolioRiskState::new(dec!(1000), dec!(1000), dec!(1001), Decimal::ZERO, false);

    assert_eq!(
        policy.check_portfolio(&state).unwrap_err(),
        RiskError::MaxExposure
    );
}

#[test]
fn rejects_portfolio_drawdown_above_limit() {
    let policy = PortfolioRiskPolicy::new(dec!(1000), dec!(0.1), dec!(2), dec!(500));
    let state = PortfolioRiskState::new(dec!(800), dec!(1000), dec!(100), Decimal::ZERO, false);

    assert_eq!(
        policy.check_portfolio(&state).unwrap_err(),
        RiskError::MaxDrawdown
    );
}

#[test]
fn rejects_portfolio_leverage_above_limit() {
    let policy = PortfolioRiskPolicy::new(dec!(5000), dec!(0.2), dec!(2), dec!(500));
    let state = PortfolioRiskState::new(dec!(1000), dec!(1000), dec!(2001), Decimal::ZERO, false);

    assert_eq!(
        policy.check_portfolio(&state).unwrap_err(),
        RiskError::MaxLeverage
    );
}

#[test]
fn rejects_portfolio_margin_above_limit() {
    let policy = PortfolioRiskPolicy::new(dec!(5000), dec!(0.2), dec!(2), dec!(500));
    let state = PortfolioRiskState::new(dec!(1000), dec!(1000), dec!(100), dec!(501), false);

    assert_eq!(
        policy.check_portfolio(&state).unwrap_err(),
        RiskError::MaxMargin
    );
}

#[test]
fn rejects_projected_buy_when_exposure_would_exceed_limit() {
    let policy = PortfolioRiskPolicy::new(dec!(1000), dec!(0.2), dec!(2), dec!(500));
    let state = PortfolioRiskState::new(dec!(1000), dec!(1000), dec!(900), Decimal::ZERO, false);

    assert_eq!(
        policy
            .check_projected_order(&buy_order(dec!(2)), dec!(100), &state)
            .unwrap_err(),
        RiskError::MaxExposure
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
