use storage::{Db, NewEventRecord, NewInstrument};

#[tokio::test]
async fn migration_creates_audit_projection_tables() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    let tables = sqlx::query_scalar::<_, String>(
        "SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name",
    )
    .fetch_all(db.pool())
    .await
    .unwrap();

    assert!(tables.contains(&"order_events".to_string()));
    assert!(tables.contains(&"risk_events".to_string()));
    assert!(tables.contains(&"insights".to_string()));
    assert!(tables.contains(&"portfolio_targets".to_string()));
}

#[tokio::test]
async fn migration_creates_market_rule_reference_tables() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    let tables = sqlx::query_scalar::<_, String>(
        "SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name",
    )
    .fetch_all(db.pool())
    .await
    .unwrap();

    assert!(tables.contains(&"market_calendars".to_string()));
    assert!(tables.contains(&"trading_sessions".to_string()));
    assert!(tables.contains(&"fee_rules".to_string()));
    assert!(tables.contains(&"lot_size_rules".to_string()));
    assert!(tables.contains(&"price_limit_rules".to_string()));
}

#[tokio::test]
async fn migration_creates_contract_accounting_tables() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    let tables = sqlx::query_scalar::<_, String>(
        "SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name",
    )
    .fetch_all(db.pool())
    .await
    .unwrap();

    assert!(tables.contains(&"crypto_positions".to_string()));
    assert!(tables.contains(&"funding_rates".to_string()));
}

#[tokio::test]
async fn instrument_round_trip() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();

    db.insert_instrument(NewInstrument {
        symbol: "US:NASDAQ:AAPL:EQUITY".to_string(),
        market: "US".to_string(),
        exchange: "NASDAQ".to_string(),
        asset_class: "EQUITY".to_string(),
        currency: "USD".to_string(),
        lot_size: "1".to_string(),
        tick_size: "0.01".to_string(),
        tradable: true,
    })
    .await
    .unwrap();

    let instrument = db
        .get_instrument("US:NASDAQ:AAPL:EQUITY")
        .await
        .unwrap()
        .unwrap();

    assert_eq!(instrument.symbol, "US:NASDAQ:AAPL:EQUITY");
    assert!(instrument.tradable);
}

#[tokio::test]
async fn event_round_trip_lists_all_events_in_time_order() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();

    db.insert_event(NewEventRecord {
        event_id: "event-2".to_string(),
        ts_ms: 2,
        source: "run-a".to_string(),
        category: "paper.completed".to_string(),
        payload_json: r#"{"orders":1}"#.to_string(),
    })
    .await
    .unwrap();
    db.insert_event(NewEventRecord {
        event_id: "event-1".to_string(),
        ts_ms: 1,
        source: "run-a".to_string(),
        category: "paper.started".to_string(),
        payload_json: r#"{"run_id":"run-a"}"#.to_string(),
    })
    .await
    .unwrap();

    let events = db.list_events().await.unwrap();

    assert_eq!(events.len(), 2);
    assert_eq!(events[0].event_id, "event-1");
    assert_eq!(events[1].category, "paper.completed");
}

#[tokio::test]
async fn list_events_by_source_filters_to_one_run() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();

    db.insert_event(NewEventRecord {
        event_id: "event-a".to_string(),
        ts_ms: 1,
        source: "run-a".to_string(),
        category: "paper.started".to_string(),
        payload_json: "{}".to_string(),
    })
    .await
    .unwrap();
    db.insert_event(NewEventRecord {
        event_id: "event-b".to_string(),
        ts_ms: 2,
        source: "run-b".to_string(),
        category: "replay.completed".to_string(),
        payload_json: "{}".to_string(),
    })
    .await
    .unwrap();

    let events = db.list_events_by_source("run-b").await.unwrap();

    assert_eq!(events.len(), 1);
    assert_eq!(events[0].source, "run-b");
    assert_eq!(events[0].category, "replay.completed");
}

#[tokio::test]
async fn persisted_events_can_replay_to_event_bus() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    db.insert_event(NewEventRecord {
        event_id: "01890f0e-d8b1-7cc6-94f4-8f9f0f7f0a11".to_string(),
        ts_ms: 1,
        source: "run-a".to_string(),
        category: "paper.started".to_string(),
        payload_json: "{}".to_string(),
    })
    .await
    .unwrap();
    db.insert_event(NewEventRecord {
        event_id: "01890f0e-d8b1-7cc6-94f4-8f9f0f7f0a12".to_string(),
        ts_ms: 2,
        source: "run-a".to_string(),
        category: "paper.completed".to_string(),
        payload_json: r#"{"orders":1}"#.to_string(),
    })
    .await
    .unwrap();
    let bus = events::EventBus::new(16);
    let mut receiver = bus.subscribe();

    let replayed = db.replay_events_to_bus("run-a", &bus).await.unwrap();

    assert_eq!(replayed, 2);
    let first = receiver.recv().await.unwrap();
    let second = receiver.recv().await.unwrap();
    assert_eq!(first.source, "run-a");
    assert_eq!(
        first.event_id.to_string(),
        "01890f0e-d8b1-7cc6-94f4-8f9f0f7f0a11"
    );
    match second.payload {
        events::TraderEvent::Runtime(event) => {
            assert_eq!(event.category, "paper.completed");
            assert_eq!(event.payload_json, r#"{"orders":1}"#);
        }
        other => panic!("expected runtime event, got {other:?}"),
    }
}
