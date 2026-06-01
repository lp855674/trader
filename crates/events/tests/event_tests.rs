use events::{EventBus, EventCategory, SignalEvent, SignalSide};

#[tokio::test]
async fn event_bus_delivers_published_events() {
    let bus = EventBus::new(16);
    let mut receiver = bus.subscribe();

    bus.publish_signal(SignalEvent {
        strategy_id: "ma_cross".to_string(),
        symbol: "US:NASDAQ:AAPL:EQUITY".to_string(),
        side: SignalSide::Buy,
        confidence: 0.8,
        ts: chrono::Utc::now(),
    })
    .unwrap();

    let event = receiver.recv().await.unwrap();
    assert_eq!(event.category, EventCategory::Signal);
}

#[test]
fn public_event_enums_use_stable_wire_format() {
    assert_eq!(
        serde_json::to_value(SignalSide::CloseShort).unwrap(),
        serde_json::json!("CLOSE_SHORT")
    );
    assert_eq!(
        serde_json::to_value(EventCategory::Execution).unwrap(),
        serde_json::json!("EXECUTION")
    );
}
