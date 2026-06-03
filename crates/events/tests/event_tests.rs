use events::{
    EventBus, EventCategory, EventEnvelope, RuntimeEvent, SignalEvent, SignalSide, TraderEvent,
};

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

#[tokio::test]
async fn event_bus_replays_envelopes_in_order() {
    let bus = EventBus::new(16);
    let mut receiver = bus.subscribe();
    let first = EventEnvelope {
        event_id: uuid::Uuid::parse_str("01890f0e-d8b1-7cc6-94f4-8f9f0f7f0a11").unwrap(),
        ts: chrono::Utc::now(),
        source: "run-1".to_string(),
        category: EventCategory::System,
        payload: TraderEvent::Runtime(RuntimeEvent {
            category: "paper.started".to_string(),
            payload_json: "{}".to_string(),
        }),
    };
    let second = EventEnvelope {
        event_id: uuid::Uuid::parse_str("01890f0e-d8b1-7cc6-94f4-8f9f0f7f0a12").unwrap(),
        ts: chrono::Utc::now(),
        source: "run-1".to_string(),
        category: EventCategory::System,
        payload: TraderEvent::Runtime(RuntimeEvent {
            category: "paper.completed".to_string(),
            payload_json: "{}".to_string(),
        }),
    };

    bus.replay([first, second]).unwrap();

    let first = receiver.recv().await.unwrap();
    let second = receiver.recv().await.unwrap();
    assert_eq!(
        first.event_id.to_string(),
        "01890f0e-d8b1-7cc6-94f4-8f9f0f7f0a11"
    );
    assert_eq!(
        second.event_id.to_string(),
        "01890f0e-d8b1-7cc6-94f4-8f9f0f7f0a12"
    );
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

#[test]
fn runtime_event_uses_stable_wire_format() {
    let event = TraderEvent::Runtime(RuntimeEvent {
        category: "replay.speed".to_string(),
        payload_json: r#"{"speed":25}"#.to_string(),
    });

    let value = serde_json::to_value(event).unwrap();

    assert_eq!(value["kind"], serde_json::json!("RUNTIME"));
    assert_eq!(value["data"]["category"], serde_json::json!("replay.speed"));
    assert_eq!(
        value["data"]["payload_json"],
        serde_json::json!(r#"{"speed":25}"#)
    );
}
