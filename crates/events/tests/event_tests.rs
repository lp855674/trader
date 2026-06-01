use events::{EventBus, EventCategory, EventEnvelope, SignalEvent, SignalSide, TraderEvent};

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

#[tokio::test]
async fn publish_without_subscribers_is_ok() {
    let bus = EventBus::new(16);

    let result = bus.publish_signal(SignalEvent {
        strategy_id: "ma_cross".to_string(),
        symbol: "US:NASDAQ:AAPL:EQUITY".to_string(),
        side: SignalSide::Buy,
        confidence: 0.8,
        ts: chrono::Utc::now(),
    });

    assert!(result.is_ok());
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

#[test]
fn event_envelope_uses_stable_json_shape() {
    let signal_ts = chrono::DateTime::parse_from_rfc3339("2026-01-02T03:04:05Z")
        .unwrap()
        .with_timezone(&chrono::Utc);
    let envelope_ts = chrono::DateTime::parse_from_rfc3339("2026-01-02T03:04:06Z")
        .unwrap()
        .with_timezone(&chrono::Utc);
    let event = EventEnvelope {
        event_id: uuid::Uuid::parse_str("01890f0e-d8b1-7cc6-94f4-8f9f0f7f0a11").unwrap(),
        ts: envelope_ts,
        source: "strategy".to_string(),
        category: EventCategory::Signal,
        payload: TraderEvent::Signal(SignalEvent {
            strategy_id: "ma_cross".to_string(),
            symbol: "US:NASDAQ:AAPL:EQUITY".to_string(),
            side: SignalSide::Buy,
            confidence: 0.8,
            ts: signal_ts,
        }),
    };

    let value = serde_json::to_value(event).unwrap();

    assert_eq!(value["category"], serde_json::json!("SIGNAL"));
    assert_eq!(value["payload"]["kind"], serde_json::json!("SIGNAL"));
    assert_eq!(
        value["payload"]["data"]["strategy_id"],
        serde_json::json!("ma_cross")
    );
}
