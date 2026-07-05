use storage::{Db, NewEventRecord, NewFeeRule, NewFeeRuleTier, NewFeeRuleWithTiers, NewInstrument};

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
    assert!(tables.contains(&"fee_rule_tiers".to_string()));
    assert!(tables.contains(&"lot_size_rules".to_string()));
    assert!(tables.contains(&"price_limit_rules".to_string()));
}

#[tokio::test]
async fn fee_rule_tier_round_trip_lists_tiers_in_volume_order() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();

    db.insert_fee_rule(NewFeeRule {
        id: "fee-us-equity".to_string(),
        market: "US".to_string(),
        exchange: "NASDAQ".to_string(),
        asset_class: "EQUITY".to_string(),
        symbol: None,
        maker_bps: "2".to_string(),
        taker_bps: "4".to_string(),
        minimum_fee: Some("0.01".to_string()),
        tax_bps: Some("1".to_string()),
        exchange_fee_bps: Some("0.5".to_string()),
        effective_from_ms: 0,
        effective_to_ms: None,
    })
    .await
    .unwrap();
    db.insert_fee_rule_tier(NewFeeRuleTier {
        id: "tier-2".to_string(),
        fee_rule_id: "fee-us-equity".to_string(),
        volume_from: "1000".to_string(),
        volume_to: None,
        maker_bps: "1".to_string(),
        taker_bps: "2".to_string(),
    })
    .await
    .unwrap();
    db.insert_fee_rule_tier(NewFeeRuleTier {
        id: "tier-1".to_string(),
        fee_rule_id: "fee-us-equity".to_string(),
        volume_from: "0".to_string(),
        volume_to: Some("1000".to_string()),
        maker_bps: "2".to_string(),
        taker_bps: "4".to_string(),
    })
    .await
    .unwrap();

    let tiers = db.list_fee_rule_tiers("fee-us-equity").await.unwrap();

    assert_eq!(tiers.len(), 2);
    assert_eq!(tiers[0].id, "tier-1");
    assert_eq!(tiers[0].volume_to.as_deref(), Some("1000"));
    assert_eq!(tiers[1].id, "tier-2");
    assert_eq!(tiers[1].volume_to, None);
}

#[tokio::test]
async fn fee_rule_with_tiers_create_and_find_round_trips_symbol_rule() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();

    let created = db
        .create_fee_rule_with_tiers(NewFeeRuleWithTiers {
            rule: NewFeeRule {
                id: "fee-aapl".to_string(),
                market: "US".to_string(),
                exchange: "NASDAQ".to_string(),
                asset_class: "EQUITY".to_string(),
                symbol: Some("US:NASDAQ:AAPL:EQUITY".to_string()),
                maker_bps: "2".to_string(),
                taker_bps: "4".to_string(),
                minimum_fee: Some("0.01".to_string()),
                tax_bps: Some("1".to_string()),
                exchange_fee_bps: Some("0.5".to_string()),
                effective_from_ms: 10,
                effective_to_ms: None,
            },
            tiers: vec![
                NewFeeRuleTier {
                    id: "tier-2".to_string(),
                    fee_rule_id: "fee-aapl".to_string(),
                    volume_from: "1000".to_string(),
                    volume_to: None,
                    maker_bps: "1".to_string(),
                    taker_bps: "2".to_string(),
                },
                NewFeeRuleTier {
                    id: "tier-1".to_string(),
                    fee_rule_id: "fee-aapl".to_string(),
                    volume_from: "0".to_string(),
                    volume_to: Some("1000".to_string()),
                    maker_bps: "2".to_string(),
                    taker_bps: "4".to_string(),
                },
            ],
        })
        .await
        .unwrap();

    assert_eq!(created.rule.id, "fee-aapl");
    assert_eq!(
        created.rule.symbol.as_deref(),
        Some("US:NASDAQ:AAPL:EQUITY")
    );
    assert_eq!(created.tiers[0].id, "tier-1");
    assert_eq!(created.tiers[1].id, "tier-2");

    let found = db
        .find_fee_rule_with_tiers("US", "NASDAQ", "EQUITY", Some("US:NASDAQ:AAPL:EQUITY"), 10)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(found.rule.id, "fee-aapl");
    assert_eq!(found.rule.minimum_fee.as_deref(), Some("0.01"));
    assert_eq!(found.rule.tax_bps.as_deref(), Some("1"));
    assert_eq!(found.rule.exchange_fee_bps.as_deref(), Some("0.5"));
    assert_eq!(
        found
            .tiers
            .iter()
            .map(|tier| tier.volume_from.as_str())
            .collect::<Vec<_>>(),
        vec!["0", "1000"]
    );
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
async fn migration_creates_reference_snapshot_and_ops_tables() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    let tables = sqlx::query_scalar::<_, String>(
        "SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name",
    )
    .fetch_all(db.pool())
    .await
    .unwrap();

    assert!(tables.contains(&"crypto_market_meta".to_string()));
    assert!(tables.contains(&"corporate_actions_meta".to_string()));
    assert!(tables.contains(&"cash_snapshots".to_string()));
    assert!(tables.contains(&"position_snapshots".to_string()));
    assert!(tables.contains(&"configs".to_string()));
    assert!(tables.contains(&"system_logs".to_string()));
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
