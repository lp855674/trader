use storage::{Db, NewEventRecord, NewInstrument};

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
