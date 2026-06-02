use broker::{Broker, MockBroker, SimulatedBrokerSettings, simulate_market_fill};
use rust_decimal_macros::dec;
use trader_core::{OrderRequest, OrderSide, OrderType};

#[tokio::test]
async fn mock_broker_accepts_order() {
    let broker = MockBroker;
    let ack = broker
        .place_order(OrderRequest {
            symbol: "US:NASDAQ:AAPL:EQUITY".to_string(),
            side: OrderSide::Buy,
            order_type: OrderType::Market,
            qty: dec!(1),
            price: None,
            account_id: "paper".to_string(),
        })
        .await
        .unwrap();

    assert!(ack.accepted);
}

#[test]
fn simulated_buy_market_fill_applies_slippage_and_fee() {
    let fill = simulate_market_fill(
        OrderRequest {
            symbol: "US:NASDAQ:AAPL:EQUITY".to_string(),
            side: OrderSide::Buy,
            order_type: OrderType::Market,
            qty: dec!(1),
            price: None,
            account_id: "paper".to_string(),
        },
        dec!(100),
        SimulatedBrokerSettings {
            slippage_bps: dec!(100),
            fee_bps: dec!(10),
        },
    )
    .unwrap();

    assert_eq!(fill.price, dec!(101));
    assert_eq!(fill.qty, dec!(1));
    assert_eq!(fill.fee, dec!(0.101));
}
