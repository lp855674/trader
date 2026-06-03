use broker::{
    Broker, BrokerKind, FakeBrokerAdapter, MockBroker, SimulatedBrokerSettings,
    simulate_market_fill,
};
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

#[tokio::test]
async fn fake_v1_connector_adapters_report_status_and_accept_orders() {
    for kind in [
        BrokerKind::Futu,
        BrokerKind::Binance,
        BrokerKind::Okx,
        BrokerKind::InteractiveBrokers,
    ] {
        let broker = FakeBrokerAdapter::new(kind);
        let status = broker.status().await.unwrap();

        assert_eq!(status.kind, kind);
        assert!(status.connected);
        assert!(status.capabilities.paper_trading);
        assert!(!status.capabilities.live_trading);

        let ack = broker.place_order(order()).await.unwrap();
        assert!(ack.accepted);
        assert!(ack.broker_order_id.starts_with("fake-"));
    }
}

fn order() -> OrderRequest {
    OrderRequest {
        symbol: "US:NASDAQ:AAPL:EQUITY".to_string(),
        side: OrderSide::Buy,
        order_type: OrderType::Market,
        qty: dec!(1),
        price: None,
        account_id: "paper".to_string(),
    }
}
