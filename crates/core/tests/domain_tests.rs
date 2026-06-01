use trader_core::{AssetClass, Market, OrderId, OrderSide, OrderStatus, Symbol};
use uuid::Uuid;

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

#[test]
fn public_enums_use_stable_json_strings() {
    assert_eq!(
        serde_json::to_string(&AssetClass::CryptoSpot).unwrap(),
        "\"CRYPTO_SPOT\""
    );
    assert_eq!(
        serde_json::to_string(&OrderStatus::PartiallyFilled).unwrap(),
        "\"PARTIALLY_FILLED\""
    );
    assert_eq!(
        serde_json::to_string(&OrderStatus::Canceled).unwrap(),
        "\"CANCELLED\""
    );
    assert_eq!(serde_json::to_string(&OrderSide::Buy).unwrap(), "\"BUY\"");
}

#[test]
fn order_id_display_delegates_to_uuid() {
    let uuid = Uuid::from_u128(0x67e55044_10b1_426f_9247_bb680e5fe0c8);
    assert_eq!(OrderId(uuid).to_string(), uuid.to_string());
}
