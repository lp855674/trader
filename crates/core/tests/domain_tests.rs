use trader_core::{AssetClass, Market, OrderSide, OrderStatus, Symbol};

#[test]
fn symbol_display_is_stable() {
    let symbol = Symbol::new(Market::Us, "NASDAQ", "AAPL", AssetClass::Equity);
    assert_eq!(symbol.to_string(), "US:NASDAQ:AAPL:EQUITY");
}

#[test]
fn order_status_identifies_terminal_states() {
    assert!(OrderStatus::Filled.is_terminal());
    assert!(OrderStatus::Canceled.is_terminal());
    assert!(OrderStatus::Rejected.is_terminal());
    assert!(!OrderStatus::Submitted.is_terminal());
}

#[test]
fn order_side_has_sign() {
    assert_eq!(OrderSide::Buy.sign(), 1);
    assert_eq!(OrderSide::Sell.sign(), -1);
}
