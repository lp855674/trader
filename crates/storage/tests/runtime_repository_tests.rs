use rust_decimal::Decimal;
use storage::{
    BrokerPositionSnapshotCommand, ConfigState, Db, NewAccountBalance, NewCashSnapshot,
    NewConfigRecord, NewConfigVersion, NewCorporateActionMeta, NewCryptoMarketMeta,
    NewCryptoPosition, NewEventRecord, NewFill, NewFundingRate, NewLotSizeRule, NewOrder,
    NewOrderEvent, NewPortfolioSnapshot, NewPosition, NewPositionSnapshot, NewPriceLimitRule,
    NewRiskEvent, NewStrategyRun, NewSystemLog, RuntimeEventCommand, StrategyRunStartCommand,
};

#[tokio::test]
async fn config_versioning_creates_versions_and_selects_published() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();

    let v1 = db
        .create_config_version(NewConfigVersion {
            name: "paper-risk".to_string(),
            content_json: r#"{"risk":{"max_order_notional":"1000"},"enabled":true}"#.to_string(),
            created_by: "ops".to_string(),
            parent_version: None,
            target_env: None,
            rollout: None,
            ts_ms: 100,
        })
        .await
        .unwrap();
    let v2 = db
        .create_config_version(NewConfigVersion {
            name: "paper-risk".to_string(),
            content_json: r#"{"risk":{"max_order_notional":"2000"},"enabled":true}"#.to_string(),
            created_by: "ops".to_string(),
            parent_version: Some(v1),
            target_env: None,
            rollout: None,
            ts_ms: 200,
        })
        .await
        .unwrap();

    assert_eq!(v1, 1);
    assert_eq!(v2, 2);

    let versions = db.list_config_versions("paper-risk").await.unwrap();
    assert_eq!(
        versions
            .iter()
            .map(|version| version.version)
            .collect::<Vec<_>>(),
        vec![1, 2]
    );
    assert_eq!(versions[0].state, ConfigState::Draft);
    assert_eq!(versions[1].parent_version, Some(1));
    assert_eq!(
        db.get_latest_config("paper-risk")
            .await
            .unwrap()
            .unwrap()
            .version,
        2
    );
    assert!(
        db.get_published_config("paper-risk")
            .await
            .unwrap()
            .is_none()
    );

    db.update_config_state(
        "paper-risk",
        1,
        ConfigState::PendingReview,
        "reviewer",
        Some("ready for review"),
        300,
    )
    .await
    .unwrap();
    db.update_config_state(
        "paper-risk",
        1,
        ConfigState::Approved,
        "approver",
        Some("risk accepted"),
        400,
    )
    .await
    .unwrap();
    db.update_config_state(
        "paper-risk",
        1,
        ConfigState::Published,
        "release",
        Some("publish v1"),
        500,
    )
    .await
    .unwrap();

    let published = db
        .get_published_config("paper-risk")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(published.version, 1);
    assert_eq!(published.state, ConfigState::Published);
    assert_eq!(published.state_changed_by, "release");
    assert_eq!(published.state_change_reason.as_deref(), Some("publish v1"));
}

#[tokio::test]
async fn config_state_transition_rejects_invalid_edges_and_terminal_archive() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();

    db.create_config_version(NewConfigVersion {
        name: "live-risk".to_string(),
        content_json: r#"{"risk":{"max_drawdown":"0.2"}}"#.to_string(),
        created_by: "ops".to_string(),
        parent_version: None,
        target_env: None,
        rollout: None,
        ts_ms: 100,
    })
    .await
    .unwrap();

    let invalid_publish = db
        .update_config_state(
            "live-risk",
            1,
            ConfigState::Published,
            "release",
            Some("skip review"),
            200,
        )
        .await;
    assert!(invalid_publish.is_err());
    assert_eq!(
        db.get_config("live-risk", 1).await.unwrap().unwrap().state,
        ConfigState::Draft
    );

    db.update_config_state(
        "live-risk",
        1,
        ConfigState::Archived,
        "ops",
        Some("discard draft"),
        300,
    )
    .await
    .unwrap();
    let archived_to_review = db
        .update_config_state(
            "live-risk",
            1,
            ConfigState::PendingReview,
            "reviewer",
            None,
            400,
        )
        .await;
    assert!(archived_to_review.is_err());
}

#[tokio::test]
async fn config_diff_and_rollback_create_new_draft_version() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();

    db.create_config_version(NewConfigVersion {
        name: "ops-config".to_string(),
        content_json: r#"{"risk":{"max_order_notional":"1000"},"enabled":true}"#.to_string(),
        created_by: "ops".to_string(),
        parent_version: None,
        target_env: None,
        rollout: None,
        ts_ms: 100,
    })
    .await
    .unwrap();
    db.create_config_version(NewConfigVersion {
        name: "ops-config".to_string(),
        content_json: r#"{"risk":{"max_order_notional":"2000"},"symbols":["AAPL"]}"#.to_string(),
        created_by: "ops".to_string(),
        parent_version: Some(1),
        target_env: None,
        rollout: None,
        ts_ms: 200,
    })
    .await
    .unwrap();

    let diff = db.diff_configs("ops-config", 1, 2).await.unwrap();
    assert_eq!(diff.name, "ops-config");
    assert_eq!(diff.version_a, 1);
    assert_eq!(diff.version_b, 2);
    assert_eq!(diff.added, vec!["symbols".to_string()]);
    assert_eq!(diff.removed, vec!["enabled".to_string()]);
    assert_eq!(diff.changed.len(), 1);
    assert_eq!(diff.changed[0].path, "risk.max_order_notional");
    assert_eq!(diff.changed[0].before, serde_json::json!("1000"));
    assert_eq!(diff.changed[0].after, serde_json::json!("2000"));

    let rollback_version = db
        .rollback_config_version("ops-config", 1, "ops", Some("restore v1"), 300)
        .await
        .unwrap();
    assert_eq!(rollback_version, 3);

    let rollback = db.get_config("ops-config", 3).await.unwrap().unwrap();
    assert_eq!(rollback.state, ConfigState::Draft);
    assert_eq!(rollback.parent_version, Some(1));
    assert_eq!(
        rollback.content_json,
        r#"{"risk":{"max_order_notional":"1000"},"enabled":true}"#
    );

    let audits = db.list_config_audits(&rollback.id).await.unwrap();
    assert_eq!(audits.len(), 1);
    assert_eq!(audits[0].action, "rollback");
    assert_eq!(audits[0].reason.as_deref(), Some("restore v1"));
}

#[tokio::test]
async fn config_state_change_writes_audit_and_event_store() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();

    db.create_config_version(NewConfigVersion {
        name: "audit-config".to_string(),
        content_json: r#"{"enabled":true}"#.to_string(),
        created_by: "ops".to_string(),
        parent_version: None,
        target_env: None,
        rollout: None,
        ts_ms: 100,
    })
    .await
    .unwrap();
    db.update_config_state(
        "audit-config",
        1,
        ConfigState::PendingReview,
        "reviewer",
        Some("review requested"),
        200,
    )
    .await
    .unwrap();

    let config = db.get_config("audit-config", 1).await.unwrap().unwrap();
    let audits = db.list_config_audits(&config.id).await.unwrap();
    assert_eq!(audits.len(), 1);
    assert_eq!(audits[0].action, "state_changed");
    assert_eq!(audits[0].actor.as_deref(), Some("reviewer"));

    let events = db.list_events_by_source(&config.id).await.unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].category, "config.state.changed");
    let payload: serde_json::Value = serde_json::from_str(&events[0].payload_json).unwrap();
    assert_eq!(payload["name"], "audit-config");
    assert_eq!(payload["version"], 1);
    assert_eq!(payload["old_state"], "draft");
    assert_eq!(payload["new_state"], "pending_review");
    assert_eq!(payload["changed_by"], "reviewer");
}

#[tokio::test]
async fn production_config_publish_requires_independent_approval() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();

    db.create_config_version(NewConfigVersion {
        name: "prod-risk".to_string(),
        content_json: r#"{"risk":{"max_order_notional":"1000"}}"#.to_string(),
        created_by: "release".to_string(),
        parent_version: None,
        target_env: Some("production".to_string()),
        rollout: Some("canary".to_string()),
        ts_ms: 100,
    })
    .await
    .unwrap();
    db.update_config_state(
        "prod-risk",
        1,
        ConfigState::PendingReview,
        "release",
        Some("request production approval"),
        200,
    )
    .await
    .unwrap();

    db.update_config_state(
        "prod-risk",
        1,
        ConfigState::Approved,
        "release",
        Some("self approve"),
        300,
    )
    .await
    .unwrap();
    let self_approved_publish = db
        .update_config_state(
            "prod-risk",
            1,
            ConfigState::Published,
            "release",
            Some("publish production"),
            400,
        )
        .await;
    assert!(self_approved_publish.is_err());

    db.update_config_state(
        "prod-risk",
        1,
        ConfigState::Approved,
        "risk-owner",
        Some("independent approval"),
        500,
    )
    .await
    .unwrap();
    db.update_config_state(
        "prod-risk",
        1,
        ConfigState::Published,
        "release",
        Some("publish production"),
        600,
    )
    .await
    .unwrap();

    let config = db.get_config("prod-risk", 1).await.unwrap().unwrap();
    assert_eq!(config.state, ConfigState::Published);
    assert_eq!(config.target_env.as_deref(), Some("production"));
    assert_eq!(config.rollout.as_deref(), Some("canary"));
    assert_eq!(config.approved_by.as_deref(), Some("risk-owner"));
    assert_eq!(config.published_by.as_deref(), Some("release"));

    let events = db.list_events_by_source(&config.id).await.unwrap();
    let publish_event = events
        .iter()
        .find(|event| event.payload_json.contains(r#""new_state":"published""#))
        .unwrap();
    let payload: serde_json::Value = serde_json::from_str(&publish_event.payload_json).unwrap();
    assert_eq!(payload["target_env"], "production");
    assert_eq!(payload["rollout"], "canary");
    assert_eq!(payload["approved_by"], "risk-owner");
    assert_eq!(payload["published_by"], "release");
}

#[tokio::test]
async fn production_config_policy_requires_roles_and_lists_pending_queue() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();

    db.create_config_version(NewConfigVersion {
        name: "prod-queue".to_string(),
        content_json: r#"{"risk":{"max_order_notional":"1000"}}"#.to_string(),
        created_by: "release".to_string(),
        parent_version: None,
        target_env: Some("production".to_string()),
        rollout: Some("canary".to_string()),
        ts_ms: 100,
    })
    .await
    .unwrap();

    let unauthorized_submit = db
        .update_config_state_with_policy(
            "prod-queue",
            1,
            ConfigState::PendingReview,
            "trader",
            "viewer",
            Some("request approval"),
            200,
        )
        .await;
    assert!(unauthorized_submit.is_err());

    db.update_config_state_with_policy(
        "prod-queue",
        1,
        ConfigState::PendingReview,
        "release",
        "release_manager",
        Some("request approval"),
        300,
    )
    .await
    .unwrap();

    let pending = db
        .list_pending_config_approvals(Some("production"))
        .await
        .unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].name, "prod-queue");
    assert_eq!(pending[0].state, ConfigState::PendingReview);
    assert_eq!(pending[0].target_env.as_deref(), Some("production"));

    let unauthorized_approve = db
        .update_config_state_with_policy(
            "prod-queue",
            1,
            ConfigState::Approved,
            "release",
            "release_manager",
            Some("approve"),
            400,
        )
        .await;
    assert!(unauthorized_approve.is_err());

    db.update_config_state_with_policy(
        "prod-queue",
        1,
        ConfigState::Approved,
        "risk-owner",
        "approver",
        Some("risk approval"),
        500,
    )
    .await
    .unwrap();

    let unauthorized_publish = db
        .update_config_state_with_policy(
            "prod-queue",
            1,
            ConfigState::Published,
            "trader",
            "approver",
            Some("publish"),
            600,
        )
        .await;
    assert!(unauthorized_publish.is_err());

    db.update_config_state_with_policy(
        "prod-queue",
        1,
        ConfigState::Published,
        "release",
        "release_manager",
        Some("publish"),
        700,
    )
    .await
    .unwrap();

    let published = db.get_config("prod-queue", 1).await.unwrap().unwrap();
    assert_eq!(published.state, ConfigState::Published);
    assert_eq!(published.approved_by.as_deref(), Some("risk-owner"));
    assert_eq!(published.published_by.as_deref(), Some("release"));
}

#[tokio::test]
async fn staging_config_policy_requires_roles_and_lists_pending_queue() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();

    db.create_config_version(NewConfigVersion {
        name: "staging-queue".to_string(),
        content_json: r#"{"risk":{"max_order_notional":"1000"}}"#.to_string(),
        created_by: "release".to_string(),
        parent_version: None,
        target_env: Some("staging".to_string()),
        rollout: Some("canary".to_string()),
        ts_ms: 100,
    })
    .await
    .unwrap();

    let unauthorized_submit = db
        .update_config_state_with_policy(
            "staging-queue",
            1,
            ConfigState::PendingReview,
            "trader",
            "viewer",
            Some("request approval"),
            200,
        )
        .await;
    assert!(unauthorized_submit.is_err());

    db.update_config_state_with_policy(
        "staging-queue",
        1,
        ConfigState::PendingReview,
        "release",
        "release_manager",
        Some("request approval"),
        300,
    )
    .await
    .unwrap();

    let pending = db
        .list_pending_config_approvals(Some("staging"))
        .await
        .unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].name, "staging-queue");
    assert_eq!(pending[0].state, ConfigState::PendingReview);
    assert_eq!(pending[0].target_env.as_deref(), Some("staging"));

    let unauthorized_approve = db
        .update_config_state_with_policy(
            "staging-queue",
            1,
            ConfigState::Approved,
            "release",
            "release_manager",
            Some("approve"),
            400,
        )
        .await;
    assert!(unauthorized_approve.is_err());

    db.update_config_state_with_policy(
        "staging-queue",
        1,
        ConfigState::Approved,
        "qa-owner",
        "approver",
        Some("qa approval"),
        500,
    )
    .await
    .unwrap();

    let unauthorized_publish = db
        .update_config_state_with_policy(
            "staging-queue",
            1,
            ConfigState::Published,
            "qa-owner",
            "approver",
            Some("publish"),
            600,
        )
        .await;
    assert!(unauthorized_publish.is_err());

    db.update_config_state_with_policy(
        "staging-queue",
        1,
        ConfigState::Published,
        "release",
        "release_manager",
        Some("publish"),
        700,
    )
    .await
    .unwrap();

    let published = db.get_config("staging-queue", 1).await.unwrap().unwrap();
    assert_eq!(published.state, ConfigState::Published);
    assert_eq!(published.approved_by.as_deref(), Some("qa-owner"));
    assert_eq!(published.published_by.as_deref(), Some("release"));
}

#[tokio::test]
async fn runtime_records_round_trip() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();

    db.insert_strategy_run(NewStrategyRun {
        id: "run-1".to_string(),
        name: "moving_average_cross".to_string(),
        mode: "backtest".to_string(),
        status: "completed".to_string(),
        started_at_ms: 1,
        ended_at_ms: Some(2),
        error: None,
        config_json: "{}".to_string(),
    })
    .await
    .unwrap();

    let run = db.get_strategy_run("run-1").await.unwrap().unwrap();
    assert_eq!(run.id, "run-1");
    assert_eq!(run.status, "completed");
    assert_eq!(db.list_strategy_runs().await.unwrap().len(), 1);

    db.update_strategy_run_status("run-1", "failed", Some(9), Some("boom"))
        .await
        .unwrap();
    let failed = db.get_strategy_run("run-1").await.unwrap().unwrap();
    assert_eq!(failed.status, "failed");
    assert_eq!(failed.ended_at_ms, Some(9));
    assert_eq!(failed.error, Some("boom".to_string()));

    db.insert_order(NewOrder {
        id: "order-1".to_string(),
        run_id: "run-1".to_string(),
        client_order_id: "client-1".to_string(),
        broker_order_id: Some("broker-1".to_string()),
        account_id: "paper".to_string(),
        symbol: "US:NASDAQ:AAPL:EQUITY".to_string(),
        side: "BUY".to_string(),
        order_type: "MARKET".to_string(),
        price: None,
        qty: "1".to_string(),
        filled_qty: "1".to_string(),
        status: "FILLED".to_string(),
        created_at_ms: 1,
        updated_at_ms: 2,
    })
    .await
    .unwrap();

    db.insert_fill(NewFill {
        id: "fill-1".to_string(),
        order_id: "order-1".to_string(),
        run_id: "run-1".to_string(),
        symbol: "US:NASDAQ:AAPL:EQUITY".to_string(),
        side: "BUY".to_string(),
        price: "108".to_string(),
        qty: "1".to_string(),
        fee: "0".to_string(),
        ts_ms: 3,
    })
    .await
    .unwrap();

    db.upsert_position(NewPosition {
        run_id: "run-1".to_string(),
        account_id: "paper".to_string(),
        symbol: "US:NASDAQ:AAPL:EQUITY".to_string(),
        qty: "1".to_string(),
        avg_price: "108".to_string(),
        updated_at_ms: 3,
    })
    .await
    .unwrap();

    db.upsert_account_balance(NewAccountBalance {
        run_id: "run-1".to_string(),
        account_id: "paper".to_string(),
        asset: "USD".to_string(),
        total: "9990".to_string(),
        available: "9990".to_string(),
        frozen: "0".to_string(),
        updated_at_ms: 3,
    })
    .await
    .unwrap();

    db.insert_portfolio_snapshot(NewPortfolioSnapshot {
        id: "snapshot-1".to_string(),
        run_id: "run-1".to_string(),
        account_id: "paper".to_string(),
        ts_ms: 3,
        cash: "9990".to_string(),
        market_value: "108".to_string(),
        equity: "10098".to_string(),
        realized_pnl: "0".to_string(),
        unrealized_pnl: "0".to_string(),
    })
    .await
    .unwrap();

    assert_eq!(db.list_orders("run-1").await.unwrap().len(), 1);
    let order_by_client_id = db
        .get_order_by_client_order_id("client-1")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(order_by_client_id.id, "order-1");
    assert_eq!(
        order_by_client_id.broker_order_id.as_deref(),
        Some("broker-1")
    );
    assert_eq!(db.list_fills("run-1").await.unwrap().len(), 1);
    assert_eq!(db.list_positions("run-1").await.unwrap().len(), 1);
    assert_eq!(db.list_account_balances("run-1").await.unwrap().len(), 1);
    assert_eq!(db.list_portfolio_snapshots("run-1").await.unwrap().len(), 1);

    let recovered = db
        .recover_order_state("run-1", "order-1")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(recovered.order_qty, "1");
    assert_eq!(recovered.filled_qty, "1");
    assert_eq!(recovered.status, "FILLED");

    db.update_order_status_by_broker_id("run-1", "broker-1", "CANCELLED", 9)
        .await
        .unwrap();
    let updated = db.list_orders("run-1").await.unwrap();
    assert_eq!(updated[0].status, "CANCELLED");
    assert_eq!(updated[0].updated_at_ms, 9);

    db.update_order_execution_by_broker_id("run-1", "broker-1", "FILLED", "1", 10)
        .await
        .unwrap();
    let executed = db.list_orders("run-1").await.unwrap();
    assert_eq!(executed[0].status, "FILLED");
    assert_eq!(executed[0].filled_qty, "1");
    assert_eq!(executed[0].updated_at_ms, 10);

    db.insert_order(NewOrder {
        id: "order-2".to_string(),
        run_id: "run-1".to_string(),
        client_order_id: "client-2".to_string(),
        broker_order_id: None,
        account_id: "paper".to_string(),
        symbol: "US:NASDAQ:AAPL:EQUITY".to_string(),
        side: "BUY".to_string(),
        order_type: "MARKET".to_string(),
        price: None,
        qty: "2".to_string(),
        filled_qty: "0".to_string(),
        status: "SUBMITTED".to_string(),
        created_at_ms: 11,
        updated_at_ms: 11,
    })
    .await
    .unwrap();

    let recoverable = db.list_recoverable_orders("run-1").await.unwrap();
    assert_eq!(recoverable.len(), 1);
    assert_eq!(recoverable[0].client_order_id, "client-2");

    db.update_order_execution_by_client_order_id("client-2", "broker-2", "FILLED", "2", 12)
        .await
        .unwrap();
    let recovered = db
        .get_order_by_client_order_id("client-2")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(recovered.broker_order_id.as_deref(), Some("broker-2"));
    assert_eq!(recovered.status, "FILLED");
    assert_eq!(recovered.filled_qty, "2");

    let updated = db
        .update_order_status_by_client_order_id("run-1", "client-2", "broker-2", "CANCELLED", 13)
        .await
        .unwrap();
    assert_eq!(updated, 1);
    let cancelled = db
        .get_order_by_client_order_id("client-2")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(cancelled.broker_order_id.as_deref(), Some("broker-2"));
    assert_eq!(cancelled.status, "CANCELLED");
    assert_eq!(cancelled.updated_at_ms, 13);

    db.insert_order(NewOrder {
        id: "order-3".to_string(),
        run_id: "run-1".to_string(),
        client_order_id: "client-3".to_string(),
        broker_order_id: Some("broker-3".to_string()),
        account_id: "paper".to_string(),
        symbol: "US:NASDAQ:AAPL:EQUITY".to_string(),
        side: "BUY".to_string(),
        order_type: "MARKET".to_string(),
        price: None,
        qty: "3".to_string(),
        filled_qty: "0".to_string(),
        status: "NEW".to_string(),
        created_at_ms: 14,
        updated_at_ms: 14,
    })
    .await
    .unwrap();
    let recoverable = db.list_recoverable_orders("run-1").await.unwrap();
    assert_eq!(recoverable.len(), 1);
    assert_eq!(recoverable[0].client_order_id, "client-3");
}

#[tokio::test]
async fn audit_projection_records_round_trip_in_time_order() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();

    db.insert_strategy_run(NewStrategyRun {
        id: "run-a".to_string(),
        name: "moving_average_cross".to_string(),
        mode: "paper".to_string(),
        status: "completed".to_string(),
        started_at_ms: 1,
        ended_at_ms: Some(3),
        error: None,
        config_json: "{}".to_string(),
    })
    .await
    .unwrap();

    db.insert_event(NewEventRecord {
        event_id: "event-order".to_string(),
        ts_ms: 2,
        source: "run-a".to_string(),
        category: "audit.raw.order".to_string(),
        payload_json: r#"{"run_id":"run-a","status":"SUBMITTED"}"#.to_string(),
    })
    .await
    .unwrap();
    db.insert_event(NewEventRecord {
        event_id: "event-risk".to_string(),
        ts_ms: 1,
        source: "run-a".to_string(),
        category: "audit.raw.risk".to_string(),
        payload_json: r#"{"run_id":"run-a","decision":"approved"}"#.to_string(),
    })
    .await
    .unwrap();

    db.insert_order_event(NewOrderEvent {
        id: "order-event".to_string(),
        event_id: "event-order".to_string(),
        run_id: "run-a".to_string(),
        order_id: Some("order-a".to_string()),
        client_order_id: Some("client-a".to_string()),
        broker_order_id: None,
        account_id: Some("paper".to_string()),
        symbol: Some("US:NASDAQ:AAPL:EQUITY".to_string()),
        status: "SUBMITTED".to_string(),
        event_type: "broker.order.submitted".to_string(),
        message: None,
        ts_ms: 2,
        payload_json: r#"{"run_id":"run-a","status":"SUBMITTED"}"#.to_string(),
    })
    .await
    .unwrap();
    db.insert_risk_event(NewRiskEvent {
        id: "risk-event".to_string(),
        event_id: "event-risk".to_string(),
        run_id: "run-a".to_string(),
        account_id: Some("paper".to_string()),
        symbol: Some("US:NASDAQ:AAPL:EQUITY".to_string()),
        risk_type: "max_exposure".to_string(),
        decision: "approved".to_string(),
        reason: None,
        threshold: Some("10000".to_string()),
        observed_value: Some("100".to_string()),
        ts_ms: 1,
        payload_json: r#"{"run_id":"run-a","decision":"approved"}"#.to_string(),
    })
    .await
    .unwrap();

    let order_events = db.list_order_events("run-a").await.unwrap();
    assert_eq!(order_events.len(), 1);
    assert_eq!(order_events[0].event_type, "broker.order.submitted");
    assert_eq!(order_events[0].client_order_id.as_deref(), Some("client-a"));

    let risk_events = db.list_risk_events("run-a").await.unwrap();
    assert_eq!(risk_events.len(), 1);
    assert_eq!(risk_events[0].risk_type, "max_exposure");
    assert_eq!(risk_events[0].threshold.as_deref(), Some("10000"));
}

#[tokio::test]
async fn risk_events_can_be_filtered_for_reconciliation_audit() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    db.start_strategy_run(StrategyRunStartCommand {
        run_id: "run-a".to_string(),
        name: "recon-a".to_string(),
        mode: "live".to_string(),
        started_at_ms: 1,
        config: serde_json::json!({}),
    })
    .await
    .unwrap();
    db.start_strategy_run(StrategyRunStartCommand {
        run_id: "run-b".to_string(),
        name: "recon-b".to_string(),
        mode: "live".to_string(),
        started_at_ms: 2,
        config: serde_json::json!({}),
    })
    .await
    .unwrap();

    db.record_runtime_event(RuntimeEventCommand {
        source: "runtime".to_string(),
        ts_ms: 200,
        category: "algorithm.risk.rejected".to_string(),
        payload: serde_json::json!({
            "run_id": "run-a",
            "account_id": "paper",
            "symbol": "CRYPTO:BINANCE:BTCUSDT_PERP:CRYPTO_PERP",
            "risk_type": "reconciliation_drift",
            "decision": "warn",
            "reason": "qty mismatch",
            "threshold": "1",
            "observed_value": "2"
        }),
    })
    .await
    .unwrap();
    db.record_runtime_event(RuntimeEventCommand {
        source: "runtime".to_string(),
        ts_ms: 100,
        category: "algorithm.risk.rejected".to_string(),
        payload: serde_json::json!({
            "run_id": "run-b",
            "account_id": "paper",
            "symbol": "CRYPTO:BINANCE:ETHUSDT_PERP:CRYPTO_PERP",
            "risk_type": "reconciliation_drift",
            "decision": "warn",
            "reason": "cash drift",
            "threshold": "5",
            "observed_value": "7"
        }),
    })
    .await
    .unwrap();
    db.record_runtime_event(RuntimeEventCommand {
        source: "runtime".to_string(),
        ts_ms: 300,
        category: "algorithm.risk.rejected".to_string(),
        payload: serde_json::json!({
            "run_id": "run-a",
            "account_id": "paper",
            "symbol": "CRYPTO:BINANCE:BTCUSDT_PERP:CRYPTO_PERP",
            "risk_type": "max_exposure",
            "decision": "reject",
            "reason": "too large",
            "threshold": "100",
            "observed_value": "120"
        }),
    })
    .await
    .unwrap();

    let filtered = db
        .list_risk_events_filtered(storage::RiskEventFilter {
            risk_type: Some("reconciliation_drift".to_string()),
            account_id: Some("paper".to_string()),
            from_ms: Some(90),
            to_ms: Some(250),
            limit: Some(1),
            ..storage::RiskEventFilter::default()
        })
        .await
        .unwrap();

    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].run_id, "run-a");
    assert_eq!(filtered[0].risk_type, "reconciliation_drift");
    assert_eq!(filtered[0].reason.as_deref(), Some("qty mismatch"));
    assert_eq!(filtered[0].ts_ms, 200);
}

#[tokio::test]
async fn market_rule_reference_records_prefer_symbol_specific_rules() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();

    db.insert_lot_size_rule(NewLotSizeRule {
        id: "lot-generic".to_string(),
        market: "US".to_string(),
        exchange: "NASDAQ".to_string(),
        asset_class: "EQUITY".to_string(),
        symbol: None,
        lot_size: "1".to_string(),
        min_qty: "1".to_string(),
        min_notional: "0".to_string(),
        effective_from_ms: 1,
        effective_to_ms: None,
    })
    .await
    .unwrap();
    db.insert_lot_size_rule(NewLotSizeRule {
        id: "lot-aapl".to_string(),
        market: "US".to_string(),
        exchange: "NASDAQ".to_string(),
        asset_class: "EQUITY".to_string(),
        symbol: Some("US:NASDAQ:AAPL:EQUITY".to_string()),
        lot_size: "0.0001".to_string(),
        min_qty: "0.0001".to_string(),
        min_notional: "5.25".to_string(),
        effective_from_ms: 2,
        effective_to_ms: None,
    })
    .await
    .unwrap();
    db.insert_price_limit_rule(NewPriceLimitRule {
        id: "price-aapl".to_string(),
        market: "US".to_string(),
        exchange: "NASDAQ".to_string(),
        asset_class: "EQUITY".to_string(),
        symbol: Some("US:NASDAQ:AAPL:EQUITY".to_string()),
        tick_size: "0.0001".to_string(),
        limit_up_bps: Some("1000".to_string()),
        limit_down_bps: Some("1000".to_string()),
        effective_from_ms: 2,
        effective_to_ms: None,
    })
    .await
    .unwrap();

    let lot_rule = db
        .find_lot_size_rule("US", "NASDAQ", "EQUITY", "US:NASDAQ:AAPL:EQUITY", 3)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(lot_rule.id, "lot-aapl");
    assert_eq!(lot_rule.lot_size, "0.0001");
    assert_eq!(lot_rule.min_notional, "5.25");

    let price_rule = db
        .find_price_limit_rule("US", "NASDAQ", "EQUITY", "US:NASDAQ:AAPL:EQUITY", 3)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(price_rule.id, "price-aapl");
    assert_eq!(price_rule.tick_size, "0.0001");
    assert_eq!(price_rule.limit_up_bps.as_deref(), Some("1000"));
}

#[tokio::test]
async fn contract_accounting_records_round_trip_decimal_strings() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();

    db.upsert_crypto_position(NewCryptoPosition {
        run_id: "run-contract".to_string(),
        account_id: "paper".to_string(),
        exchange: "BINANCE".to_string(),
        symbol: "BTCUSDT_PERP".to_string(),
        asset_class: "CRYPTO_PERP".to_string(),
        margin_mode: "cross".to_string(),
        position_side: "short".to_string(),
        leverage: "3.5".to_string(),
        qty: "-0.125".to_string(),
        avg_price: "65000.1234".to_string(),
        margin_used: "812.5015425".to_string(),
        funding_fee: "-1.25".to_string(),
        realized_pnl: "0".to_string(),
        unrealized_pnl: "12.3456".to_string(),
        updated_at_ms: 10,
    })
    .await
    .unwrap();
    db.upsert_crypto_position(NewCryptoPosition {
        run_id: "run-contract".to_string(),
        account_id: "paper".to_string(),
        exchange: "BINANCE".to_string(),
        symbol: "BTCUSDT_PERP".to_string(),
        asset_class: "CRYPTO_PERP".to_string(),
        margin_mode: "cross".to_string(),
        position_side: "short".to_string(),
        leverage: "3.5".to_string(),
        qty: "-0.250".to_string(),
        avg_price: "65001.0000".to_string(),
        margin_used: "1625.025".to_string(),
        funding_fee: "-1.50".to_string(),
        realized_pnl: "2.00".to_string(),
        unrealized_pnl: "20.0001".to_string(),
        updated_at_ms: 11,
    })
    .await
    .unwrap();

    let positions = db.list_crypto_positions("run-contract").await.unwrap();
    assert_eq!(positions.len(), 1);
    assert_eq!(positions[0].qty, "-0.250");
    assert_eq!(positions[0].avg_price, "65001.0000");
    assert_eq!(positions[0].position_side, "short");

    db.upsert_funding_rate(NewFundingRate {
        id: "funding-1".to_string(),
        exchange: "BINANCE".to_string(),
        symbol: "BTCUSDT_PERP".to_string(),
        funding_time_ms: 1000,
        funding_rate: "0.0001".to_string(),
        mark_price: Some("65000.1234".to_string()),
        source: "testnet".to_string(),
    })
    .await
    .unwrap();
    db.upsert_funding_rate(NewFundingRate {
        id: "funding-1-replacement".to_string(),
        exchange: "BINANCE".to_string(),
        symbol: "BTCUSDT_PERP".to_string(),
        funding_time_ms: 1000,
        funding_rate: "0.0002".to_string(),
        mark_price: Some("65001.0000".to_string()),
        source: "testnet".to_string(),
    })
    .await
    .unwrap();

    let rates = db
        .list_funding_rates("BINANCE", Some("BTCUSDT_PERP"), Some(0), Some(2000))
        .await
        .unwrap();
    assert_eq!(rates.len(), 1);
    assert_eq!(rates[0].funding_rate, "0.0002");
    assert_eq!(rates[0].mark_price.as_deref(), Some("65001.0000"));
}

#[tokio::test]
async fn upsert_crypto_market_meta_idempotent() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();

    db.upsert_crypto_market_meta(NewCryptoMarketMeta {
        exchange: "BINANCE".to_string(),
        symbol: "BTCUSDT".to_string(),
        base_asset: "BTC".to_string(),
        quote_asset: "USDT".to_string(),
        instrument_type: "SPOT".to_string(),
        contract_type: None,
        contract_size: None,
        settlement_asset: None,
        min_notional: Some("5".to_string()),
        min_qty: Some("0.00001".to_string()),
        max_qty: Some("9000".to_string()),
        price_precision: Some(2),
        qty_precision: Some(5),
        price_tick: Some("0.01".to_string()),
        qty_step: Some("0.00001".to_string()),
        maker_fee_rate: None,
        taker_fee_rate: None,
        funding_interval_hours: None,
        max_leverage: None,
        margin_modes: None,
        is_inverse: false,
        is_active: true,
        created_at_ms: 10,
        updated_at_ms: 10,
    })
    .await
    .unwrap();
    db.upsert_crypto_market_meta(NewCryptoMarketMeta {
        exchange: "BINANCE".to_string(),
        symbol: "BTCUSDT".to_string(),
        base_asset: "BTC".to_string(),
        quote_asset: "USDT".to_string(),
        instrument_type: "SPOT".to_string(),
        contract_type: None,
        contract_size: None,
        settlement_asset: None,
        min_notional: Some("10".to_string()),
        min_qty: Some("0.001".to_string()),
        max_qty: Some("1000".to_string()),
        price_precision: Some(1),
        qty_precision: Some(3),
        price_tick: Some("0.10".to_string()),
        qty_step: Some("0.001".to_string()),
        maker_fee_rate: None,
        taker_fee_rate: None,
        funding_interval_hours: None,
        max_leverage: None,
        margin_modes: None,
        is_inverse: false,
        is_active: false,
        created_at_ms: 10,
        updated_at_ms: 20,
    })
    .await
    .unwrap();

    let rows = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM crypto_market_meta WHERE exchange = 'BINANCE' AND symbol = 'BTCUSDT'",
    )
    .fetch_one(db.pool())
    .await
    .unwrap();
    let meta = db
        .find_crypto_market_meta("BINANCE", "BTCUSDT")
        .await
        .unwrap()
        .unwrap();

    assert_eq!(rows, 1);
    assert_eq!(meta.min_notional.as_deref(), Some("10"));
    assert_eq!(meta.qty_step.as_deref(), Some("0.001"));
    assert!(!meta.is_active);
    assert_eq!(meta.created_at_ms, 10);
    assert_eq!(meta.updated_at_ms, 20);
}

#[tokio::test]
async fn upsert_corporate_action_idempotent() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();

    db.upsert_corporate_action_meta(NewCorporateActionMeta {
        market: "US".to_string(),
        exchange: "NASDAQ".to_string(),
        symbol: "US:NASDAQ:AAPL:EQUITY".to_string(),
        action_type: "DIVIDEND".to_string(),
        ex_date_ms: 100,
        record_date_ms: None,
        payable_date_ms: None,
        ratio: None,
        cash_amount: Some("0.24".to_string()),
        currency: Some("USD".to_string()),
        source: Some("fixture".to_string()),
        created_at_ms: 10,
        updated_at_ms: 10,
    })
    .await
    .unwrap();
    db.upsert_corporate_action_meta(NewCorporateActionMeta {
        market: "US".to_string(),
        exchange: "NASDAQ".to_string(),
        symbol: "US:NASDAQ:AAPL:EQUITY".to_string(),
        action_type: "DIVIDEND".to_string(),
        ex_date_ms: 100,
        record_date_ms: Some(90),
        payable_date_ms: Some(120),
        ratio: None,
        cash_amount: Some("0.25".to_string()),
        currency: Some("USD".to_string()),
        source: Some("fixture-update".to_string()),
        created_at_ms: 10,
        updated_at_ms: 20,
    })
    .await
    .unwrap();

    let rows = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM corporate_actions_meta WHERE market = 'US' AND symbol = 'US:NASDAQ:AAPL:EQUITY'",
    )
    .fetch_one(db.pool())
    .await
    .unwrap();
    let actions = db
        .list_corporate_actions("US", "US:NASDAQ:AAPL:EQUITY", 0, 200)
        .await
        .unwrap();

    assert_eq!(rows, 1);
    assert_eq!(actions.len(), 1);
    assert_eq!(actions[0].cash_amount.as_deref(), Some("0.25"));
    assert_eq!(actions[0].payable_date_ms, Some(120));
    assert_eq!(actions[0].source.as_deref(), Some("fixture-update"));
    assert_eq!(actions[0].created_at_ms, 10);
    assert_eq!(actions[0].updated_at_ms, 20);
}

#[tokio::test]
async fn crypto_position_lifecycle_gets_open_update_and_close_state() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();

    db.upsert_crypto_position(NewCryptoPosition {
        run_id: "run-lifecycle".to_string(),
        account_id: "paper".to_string(),
        exchange: "BINANCE".to_string(),
        symbol: "ETHUSDT_PERP".to_string(),
        asset_class: "CRYPTO_PERP".to_string(),
        margin_mode: "isolated".to_string(),
        position_side: "long".to_string(),
        leverage: "5".to_string(),
        qty: "1.25".to_string(),
        avg_price: "3500.125".to_string(),
        margin_used: "875.03125".to_string(),
        funding_fee: "0".to_string(),
        realized_pnl: "0".to_string(),
        unrealized_pnl: "12.50".to_string(),
        updated_at_ms: 100,
    })
    .await
    .unwrap();

    let opened = db
        .get_crypto_position("run-lifecycle", "paper", "BINANCE", "ETHUSDT_PERP", "long")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(opened.qty, "1.25");
    assert_eq!(opened.avg_price, "3500.125");

    db.upsert_crypto_position(NewCryptoPosition {
        run_id: "run-lifecycle".to_string(),
        account_id: "paper".to_string(),
        exchange: "BINANCE".to_string(),
        symbol: "ETHUSDT_PERP".to_string(),
        asset_class: "CRYPTO_PERP".to_string(),
        margin_mode: "isolated".to_string(),
        position_side: "long".to_string(),
        leverage: "5".to_string(),
        qty: "2.00".to_string(),
        avg_price: "3520.000".to_string(),
        margin_used: "1408.000".to_string(),
        funding_fee: "-0.42".to_string(),
        realized_pnl: "0".to_string(),
        unrealized_pnl: "25.00".to_string(),
        updated_at_ms: 200,
    })
    .await
    .unwrap();

    db.upsert_crypto_position(NewCryptoPosition {
        run_id: "run-lifecycle".to_string(),
        account_id: "paper".to_string(),
        exchange: "BINANCE".to_string(),
        symbol: "ETHUSDT_PERP".to_string(),
        asset_class: "CRYPTO_PERP".to_string(),
        margin_mode: "isolated".to_string(),
        position_side: "long".to_string(),
        leverage: "5".to_string(),
        qty: "0".to_string(),
        avg_price: "0".to_string(),
        margin_used: "0".to_string(),
        funding_fee: "-0.42".to_string(),
        realized_pnl: "38.75".to_string(),
        unrealized_pnl: "0".to_string(),
        updated_at_ms: 300,
    })
    .await
    .unwrap();

    let closed = db
        .get_crypto_position("run-lifecycle", "paper", "BINANCE", "ETHUSDT_PERP", "long")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(closed.qty, "0");
    assert_eq!(closed.margin_used, "0");
    assert_eq!(closed.realized_pnl, "38.75");

    let all_positions = db.list_crypto_positions("run-lifecycle").await.unwrap();
    assert_eq!(all_positions.len(), 1);
}

#[tokio::test]
async fn funding_rate_queries_support_optional_filters_and_latest_lookup() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();

    for (id, symbol, funding_time_ms, funding_rate) in [
        ("funding-btc-1", "BTCUSDT_PERP", 1_000, "0.0001"),
        ("funding-eth-1", "ETHUSDT_PERP", 2_000, "0.0003"),
        ("funding-btc-2", "BTCUSDT_PERP", 3_000, "0.0002"),
    ] {
        db.upsert_funding_rate(NewFundingRate {
            id: id.to_string(),
            exchange: "BINANCE".to_string(),
            symbol: symbol.to_string(),
            funding_time_ms,
            funding_rate: funding_rate.to_string(),
            mark_price: Some("65000.00".to_string()),
            source: "testnet".to_string(),
        })
        .await
        .unwrap();
    }

    let all_binance = db
        .list_funding_rates("BINANCE", None, None, None)
        .await
        .unwrap();
    assert_eq!(all_binance.len(), 3);

    let btc_window = db
        .list_funding_rates("BINANCE", Some("BTCUSDT_PERP"), Some(1_500), Some(4_000))
        .await
        .unwrap();
    assert_eq!(btc_window.len(), 1);
    assert_eq!(btc_window[0].id, "funding-btc-2");

    let latest = db
        .get_latest_funding_rate("BINANCE", "BTCUSDT_PERP")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(latest.funding_time_ms, 3_000);
    assert_eq!(latest.funding_rate, "0.0002");
}

#[tokio::test]
async fn funding_settlement_persists_position_funding_fee_and_realized_pnl() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();

    db.upsert_crypto_position(NewCryptoPosition {
        run_id: "run-funding-settlement".to_string(),
        account_id: "paper".to_string(),
        exchange: "BINANCE".to_string(),
        symbol: "BTCUSDT_PERP".to_string(),
        asset_class: "CRYPTO_PERP".to_string(),
        margin_mode: "cross".to_string(),
        position_side: "long".to_string(),
        leverage: "3".to_string(),
        qty: "0.5".to_string(),
        avg_price: "65000".to_string(),
        margin_used: "10833.333333333333333333333333".to_string(),
        funding_fee: "0".to_string(),
        realized_pnl: "0".to_string(),
        unrealized_pnl: "0".to_string(),
        updated_at_ms: 1_000,
    })
    .await
    .unwrap();

    db.upsert_crypto_position(NewCryptoPosition {
        run_id: "run-funding-settlement".to_string(),
        account_id: "paper".to_string(),
        exchange: "BINANCE".to_string(),
        symbol: "BTCUSDT_PERP".to_string(),
        asset_class: "CRYPTO_PERP".to_string(),
        margin_mode: "cross".to_string(),
        position_side: "long".to_string(),
        leverage: "3".to_string(),
        qty: "0.5".to_string(),
        avg_price: "65000".to_string(),
        margin_used: "10833.333333333333333333333333".to_string(),
        funding_fee: "-3.25".to_string(),
        realized_pnl: "-3.25".to_string(),
        unrealized_pnl: "0".to_string(),
        updated_at_ms: 2_000,
    })
    .await
    .unwrap();

    let settled = db
        .get_crypto_position(
            "run-funding-settlement",
            "paper",
            "BINANCE",
            "BTCUSDT_PERP",
            "long",
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(settled.funding_fee, "-3.25");
    assert_eq!(settled.realized_pnl, "-3.25");
    assert_eq!(settled.updated_at_ms, 2_000);
}

#[tokio::test]
async fn reference_snapshot_and_ops_records_round_trip() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();

    db.insert_strategy_run(NewStrategyRun {
        id: "run-reference".to_string(),
        name: "snapshot_boundary".to_string(),
        mode: "paper".to_string(),
        status: "running".to_string(),
        started_at_ms: 1,
        ended_at_ms: None,
        error: None,
        config_json: "{}".to_string(),
    })
    .await
    .unwrap();

    db.upsert_crypto_market_meta(NewCryptoMarketMeta {
        exchange: "BINANCE".to_string(),
        symbol: "BTCUSDT".to_string(),
        base_asset: "BTC".to_string(),
        quote_asset: "USDT".to_string(),
        instrument_type: "SPOT".to_string(),
        contract_type: None,
        contract_size: None,
        settlement_asset: None,
        min_notional: Some("5".to_string()),
        min_qty: Some("0.00001".to_string()),
        max_qty: None,
        price_precision: Some(2),
        qty_precision: Some(5),
        price_tick: Some("0.01".to_string()),
        qty_step: Some("0.00001".to_string()),
        maker_fee_rate: Some("0.001".to_string()),
        taker_fee_rate: Some("0.001".to_string()),
        funding_interval_hours: None,
        max_leverage: None,
        margin_modes: None,
        is_inverse: false,
        is_active: true,
        created_at_ms: 10,
        updated_at_ms: 10,
    })
    .await
    .unwrap();
    db.upsert_crypto_market_meta(NewCryptoMarketMeta {
        exchange: "BINANCE".to_string(),
        symbol: "BTCUSDT".to_string(),
        base_asset: "BTC".to_string(),
        quote_asset: "USDT".to_string(),
        instrument_type: "PERP".to_string(),
        contract_type: Some("LINEAR".to_string()),
        contract_size: Some("1".to_string()),
        settlement_asset: Some("USDT".to_string()),
        min_notional: Some("10".to_string()),
        min_qty: Some("0.001".to_string()),
        max_qty: None,
        price_precision: Some(2),
        qty_precision: Some(3),
        price_tick: Some("0.10".to_string()),
        qty_step: Some("0.001".to_string()),
        maker_fee_rate: Some("0.0002".to_string()),
        taker_fee_rate: Some("0.0004".to_string()),
        funding_interval_hours: Some(8),
        max_leverage: Some("50".to_string()),
        margin_modes: Some(r#"["CROSS","ISOLATED"]"#.to_string()),
        is_inverse: false,
        is_active: true,
        created_at_ms: 10,
        updated_at_ms: 11,
    })
    .await
    .unwrap();

    let market_meta = db
        .find_crypto_market_meta("BINANCE", "BTCUSDT")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(market_meta.instrument_type, "PERP");
    assert_eq!(market_meta.max_leverage.as_deref(), Some("50"));
    assert_eq!(market_meta.qty_step.as_deref(), Some("0.001"));

    db.insert_corporate_action_meta(NewCorporateActionMeta {
        market: "US".to_string(),
        exchange: "NASDAQ".to_string(),
        symbol: "US:NASDAQ:AAPL:EQUITY".to_string(),
        action_type: "SPLIT".to_string(),
        ex_date_ms: 100,
        record_date_ms: Some(101),
        payable_date_ms: Some(102),
        ratio: Some("4:1".to_string()),
        cash_amount: None,
        currency: None,
        source: Some("fixture".to_string()),
        created_at_ms: 12,
        updated_at_ms: 12,
    })
    .await
    .unwrap();

    let actions = db
        .list_corporate_actions("US", "US:NASDAQ:AAPL:EQUITY", 0, 200)
        .await
        .unwrap();
    assert_eq!(actions.len(), 1);
    assert_eq!(actions[0].action_type, "SPLIT");
    assert_eq!(actions[0].ratio.as_deref(), Some("4:1"));

    db.insert_cash_snapshot(NewCashSnapshot {
        run_id: "run-reference".to_string(),
        ts_ms: 20,
        currency: "USD".to_string(),
        cash: "1000.1234".to_string(),
        available_cash: "900.1234".to_string(),
        frozen_cash: "100.0000".to_string(),
        created_at_ms: 20,
    })
    .await
    .unwrap();
    let cash_snapshots = db.list_cash_snapshots("run-reference").await.unwrap();
    assert_eq!(cash_snapshots.len(), 1);
    assert_eq!(cash_snapshots[0].cash, "1000.1234");

    db.insert_cash_snapshot(NewCashSnapshot {
        run_id: "run-reference".to_string(),
        ts_ms: 30,
        currency: "USDT".to_string(),
        cash: "2000.0000".to_string(),
        available_cash: "2000.0000".to_string(),
        frozen_cash: "0".to_string(),
        created_at_ms: 30,
    })
    .await
    .unwrap();
    db.insert_cash_snapshot(NewCashSnapshot {
        run_id: "run-reference".to_string(),
        ts_ms: 40,
        currency: "USD".to_string(),
        cash: "1100.0000".to_string(),
        available_cash: "1000.0000".to_string(),
        frozen_cash: "100.0000".to_string(),
        created_at_ms: 40,
    })
    .await
    .unwrap();
    let filtered_cash = db
        .list_cash_snapshots_filtered("run-reference", Some("USD"), Some(25), Some(45))
        .await
        .unwrap();
    assert_eq!(filtered_cash.len(), 1);
    assert_eq!(filtered_cash[0].currency, "USD");
    assert_eq!(filtered_cash[0].cash, "1100.0000");
    let latest_cash = db
        .get_latest_cash_snapshot("run-reference", Some("USD"))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(latest_cash.ts_ms, 40);
    assert_eq!(latest_cash.cash, "1100.0000");

    db.insert_position_snapshot(NewPositionSnapshot {
        run_id: "run-reference".to_string(),
        ts_ms: 21,
        market: "CRYPTO".to_string(),
        exchange: "BINANCE".to_string(),
        symbol: "BTCUSDT".to_string(),
        asset_class: "CRYPTO_PERP".to_string(),
        position_side: Some("long".to_string()),
        qty: "0.25".to_string(),
        available_qty: "0.25".to_string(),
        avg_price: Some("65000.1234".to_string()),
        entry_price: Some("65000.1234".to_string()),
        market_price: Some("65100.0000".to_string()),
        mark_price: Some("65101.0000".to_string()),
        market_value: Some("16275.0000".to_string()),
        unrealized_pnl: Some("24.96915".to_string()),
        realized_pnl: Some("0".to_string()),
        currency: "USDT".to_string(),
        created_at_ms: 21,
    })
    .await
    .unwrap();
    let position_snapshots = db.list_position_snapshots("run-reference").await.unwrap();
    assert_eq!(position_snapshots.len(), 1);
    assert_eq!(
        position_snapshots[0].mark_price.as_deref(),
        Some("65101.0000")
    );

    db.insert_position_snapshot(NewPositionSnapshot {
        run_id: "run-reference".to_string(),
        ts_ms: 22,
        market: "CRYPTO".to_string(),
        exchange: "BINANCE".to_string(),
        symbol: "ETHUSDT".to_string(),
        asset_class: "CRYPTO_PERP".to_string(),
        position_side: Some("short".to_string()),
        qty: "-1.5".to_string(),
        available_qty: "1.5".to_string(),
        avg_price: Some("3500".to_string()),
        entry_price: Some("3500".to_string()),
        market_price: Some("3490".to_string()),
        mark_price: Some("3491".to_string()),
        market_value: Some("-5235".to_string()),
        unrealized_pnl: Some("15".to_string()),
        realized_pnl: Some("0".to_string()),
        currency: "USDT".to_string(),
        created_at_ms: 22,
    })
    .await
    .unwrap();
    db.insert_position_snapshot(NewPositionSnapshot {
        run_id: "run-reference".to_string(),
        ts_ms: 31,
        market: "CRYPTO".to_string(),
        exchange: "BINANCE".to_string(),
        symbol: "BTCUSDT".to_string(),
        asset_class: "CRYPTO_PERP".to_string(),
        position_side: Some("long".to_string()),
        qty: "0.50".to_string(),
        available_qty: "0.50".to_string(),
        avg_price: Some("65050".to_string()),
        entry_price: Some("65000.1234".to_string()),
        market_price: Some("65200".to_string()),
        mark_price: Some("65201".to_string()),
        market_value: Some("32600".to_string()),
        unrealized_pnl: Some("75".to_string()),
        realized_pnl: Some("0".to_string()),
        currency: "USDT".to_string(),
        created_at_ms: 31,
    })
    .await
    .unwrap();
    let filtered_positions = db
        .list_position_snapshots_filtered(
            "run-reference",
            Some("BTCUSDT"),
            Some("long"),
            Some(25),
            Some(40),
        )
        .await
        .unwrap();
    assert_eq!(filtered_positions.len(), 1);
    assert_eq!(filtered_positions[0].symbol, "BTCUSDT");
    assert_eq!(filtered_positions[0].qty, "0.50");
    let latest_position = db
        .get_latest_position_snapshot("run-reference", "BTCUSDT", Some("long"))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(latest_position.ts_ms, 31);
    assert_eq!(latest_position.qty, "0.50");

    db.upsert_config(NewConfigRecord {
        id: "config-paper".to_string(),
        name: "paper-binance".to_string(),
        config_type: "BROKER".to_string(),
        content: "order_submit_enabled = false".to_string(),
        format: "TOML".to_string(),
        checksum: Some("sha256:old".to_string()),
        created_at_ms: 30,
        updated_at_ms: 30,
    })
    .await
    .unwrap();
    db.upsert_config(NewConfigRecord {
        id: "config-paper".to_string(),
        name: "paper-binance".to_string(),
        config_type: "BROKER".to_string(),
        content: "order_submit_enabled = true".to_string(),
        format: "TOML".to_string(),
        checksum: Some("sha256:new".to_string()),
        created_at_ms: 30,
        updated_at_ms: 31,
    })
    .await
    .unwrap();
    let config = db
        .get_config_by_name("paper-binance")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(config.content, "order_submit_enabled = true");
    assert_eq!(config.checksum.as_deref(), Some("sha256:new"));

    db.record_run_config_snapshot(storage::RunConfigSnapshotCommand {
        run_id: "run-reference".to_string(),
        content: "[runtime]\nrun_id = \"run-reference\"\n".to_string(),
        format: "TOML".to_string(),
        checksum: Some("sha256:run".to_string()),
        ts_ms: 32,
    })
    .await
    .unwrap();
    let configs = db.list_configs().await.unwrap();
    assert_eq!(
        configs
            .iter()
            .map(|config| config.name.as_str())
            .collect::<Vec<_>>(),
        vec!["run-reference", "paper-binance"]
    );
    assert_eq!(configs[0].config_type, "RUN");
    assert_eq!(configs[1].config_type, "BROKER");

    db.record_config_release(storage::ConfigReleaseCommand {
        config_id: "config-paper".to_string(),
        version: "v1".to_string(),
        status: "released".to_string(),
        released_by: Some("ops".to_string()),
        notes: Some("paper broker rollout".to_string()),
        ts_ms: 33,
    })
    .await
    .unwrap();
    db.bind_run_config_version(storage::RunConfigVersionBindingCommand {
        run_id: "run-reference".to_string(),
        config_id: "config-paper".to_string(),
        version: "v1".to_string(),
        ts_ms: 34,
    })
    .await
    .unwrap();
    db.record_config_audit(storage::ConfigAuditCommand {
        config_id: "config-paper".to_string(),
        version: Some("v1".to_string()),
        action: "rollback".to_string(),
        actor: Some("ops".to_string()),
        reason: Some("restore previous release".to_string()),
        ts_ms: 35,
    })
    .await
    .unwrap();

    let release = db
        .get_config_release("config-paper", "v1")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(release.status, "released");
    assert_eq!(release.released_by.as_deref(), Some("ops"));
    assert_eq!(release.notes.as_deref(), Some("paper broker rollout"));
    let binding = db
        .get_run_config_version_binding("run-reference")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(binding.config_id, "config-paper");
    assert_eq!(binding.version, "v1");
    let audits = db.list_config_audits("config-paper").await.unwrap();
    assert_eq!(audits.len(), 1);
    assert_eq!(audits[0].action, "rollback");
    assert_eq!(
        audits[0].reason.as_deref(),
        Some("restore previous release")
    );

    db.insert_system_log(NewSystemLog {
        id: "log-1".to_string(),
        run_id: Some("run-reference".to_string()),
        ts_ms: 40,
        level: "INFO".to_string(),
        target: "paper".to_string(),
        message: "started".to_string(),
        fields_json: Some(r#"{"orders":0}"#.to_string()),
        created_at_ms: 40,
    })
    .await
    .unwrap();
    db.insert_system_log(NewSystemLog {
        id: "log-2".to_string(),
        run_id: Some("run-other".to_string()),
        ts_ms: 41,
        level: "WARN".to_string(),
        target: "paper".to_string(),
        message: "other".to_string(),
        fields_json: None,
        created_at_ms: 41,
    })
    .await
    .unwrap();
    let logs = db.list_system_logs(Some("run-reference")).await.unwrap();
    assert_eq!(logs.len(), 1);
    assert_eq!(logs[0].message, "started");
    assert_eq!(logs[0].fields_json.as_deref(), Some(r#"{"orders":0}"#));

    db.record_system_log(storage::SystemLogCommand {
        run_id: Some("run-reference".to_string()),
        ts_ms: 50,
        level: "ERROR".to_string(),
        target: "runtime.execution".to_string(),
        message: "execution failed".to_string(),
        fields: Some(serde_json::json!({
            "category": "runtime",
            "component": "execution"
        })),
    })
    .await
    .unwrap();
    db.record_system_log(storage::SystemLogCommand {
        run_id: None,
        ts_ms: 60,
        level: "INFO".to_string(),
        target: "system.scheduler".to_string(),
        message: "scheduler tick".to_string(),
        fields: Some(serde_json::json!({
            "category": "system",
            "component": "scheduler"
        })),
    })
    .await
    .unwrap();

    let filtered_logs = db
        .list_system_logs_filtered(storage::SystemLogFilter {
            run_id: Some("run-reference".to_string()),
            level: Some("ERROR".to_string()),
            target: Some("runtime.execution".to_string()),
            from_ms: Some(45),
            to_ms: Some(55),
            limit: Some(10),
        })
        .await
        .unwrap();
    assert_eq!(filtered_logs.len(), 1);
    assert_eq!(filtered_logs[0].message, "execution failed");

    let purged = db
        .purge_system_logs(storage::SystemLogRetentionCommand {
            before_ms: 45,
            target: Some("paper".to_string()),
            run_id: Some("run-reference".to_string()),
        })
        .await
        .unwrap();
    assert_eq!(purged, 1);
    assert!(
        db.list_system_logs_filtered(storage::SystemLogFilter {
            run_id: Some("run-reference".to_string()),
            level: None,
            target: Some("paper".to_string()),
            from_ms: None,
            to_ms: None,
            limit: None,
        })
        .await
        .unwrap()
        .is_empty()
    );
}

#[tokio::test]
async fn broker_position_snapshot_command_preserves_side_and_pnl_fields() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    db.insert_strategy_run(NewStrategyRun {
        id: "run-broker-position".to_string(),
        name: "live-reconciliation".to_string(),
        mode: "live".to_string(),
        status: "running".to_string(),
        started_at_ms: 1,
        ended_at_ms: None,
        error: None,
        config_json: "{}".to_string(),
    })
    .await
    .unwrap();

    db.record_broker_position_snapshot(BrokerPositionSnapshotCommand {
        run_id: "run-broker-position".to_string(),
        account_id: "paper".to_string(),
        ts_ms: 1_700_000_000_000,
        exchange: "BINANCE".to_string(),
        symbol: "CRYPTO:BINANCE:BTCUSDT_PERP:CRYPTO_PERP".to_string(),
        position_side: "long".to_string(),
        qty: dec("0.5"),
        avg_price: dec("65000"),
        mark_price: Some(dec("65025")),
        margin_used: dec("3250"),
        unrealized_pnl: dec("12.5"),
        realized_pnl: Decimal::ZERO,
        currency: "USDT".to_string(),
    })
    .await
    .unwrap();

    let snapshot = db
        .get_latest_position_snapshot(
            "run-broker-position",
            "CRYPTO:BINANCE:BTCUSDT_PERP:CRYPTO_PERP",
            Some("long"),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(snapshot.exchange, "BINANCE");
    assert_eq!(snapshot.asset_class, "CRYPTO_PERP");
    assert_eq!(snapshot.position_side.as_deref(), Some("long"));
    assert_eq!(snapshot.qty, "0.5");
    assert_eq!(snapshot.avg_price.as_deref(), Some("65000"));
    assert_eq!(snapshot.entry_price.as_deref(), Some("65000"));
    assert_eq!(snapshot.mark_price.as_deref(), Some("65025"));
    assert_eq!(snapshot.market_value.as_deref(), Some("32512.5"));
    assert_eq!(snapshot.unrealized_pnl.as_deref(), Some("12.5"));
    assert_eq!(snapshot.realized_pnl.as_deref(), Some("0"));
}

#[tokio::test]
async fn runtime_position_snapshot_command_preserves_side_for_reconciliation() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    db.insert_strategy_run(NewStrategyRun {
        id: "run-runtime-position".to_string(),
        name: "live-reconciliation".to_string(),
        mode: "live".to_string(),
        status: "running".to_string(),
        started_at_ms: 1,
        ended_at_ms: None,
        error: None,
        config_json: "{}".to_string(),
    })
    .await
    .unwrap();

    db.record_runtime_position_snapshot(storage::RuntimePositionSnapshotCommand {
        run_id: "run-runtime-position".to_string(),
        ts_ms: 1_700_000_000_000,
        symbol: "CRYPTO:BINANCE:BTCUSDT_PERP:CRYPTO_PERP".to_string(),
        position_side: "long".to_string(),
        qty: dec("0.25"),
        available_qty: dec("0.25"),
        avg_price: dec("65000"),
        mark_price: Some(dec("65010")),
        currency: "USDT".to_string(),
    })
    .await
    .unwrap();

    let snapshot = db
        .get_latest_position_snapshot(
            "run-runtime-position",
            "CRYPTO:BINANCE:BTCUSDT_PERP:CRYPTO_PERP",
            Some("long"),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(snapshot.position_side.as_deref(), Some("long"));
    assert_eq!(snapshot.qty, "0.25");
    assert_eq!(snapshot.available_qty, "0.25");
    assert_eq!(snapshot.avg_price.as_deref(), Some("65000"));
    assert_eq!(snapshot.mark_price.as_deref(), Some("65010"));
}

#[tokio::test]
async fn backtest_repository_records_completed_run_execution_position_and_events() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();

    db.insert_runtime_events(
        "backtest-run",
        &[storage::StoredRuntimeEvent {
            ts_ms: 1,
            category: "algorithm.alpha.generated".to_string(),
            payload_json: "{}".to_string(),
        }],
    )
    .await
    .unwrap();

    db.insert_filled_backtest_execution(storage::BacktestExecutionRecord {
        run_id: "backtest-run".to_string(),
        order_id: "order-1".to_string(),
        fill_id: "fill-1".to_string(),
        broker_order_id: "broker-1".to_string(),
        account_id: "backtest".to_string(),
        symbol: "US:NASDAQ:AAPL:EQUITY".to_string(),
        side: "BUY".to_string(),
        order_type: "MARKET".to_string(),
        price: None,
        qty: "1".to_string(),
        fill_price: "20".to_string(),
        fee: "0".to_string(),
        ts_ms: 3,
    })
    .await
    .unwrap();

    db.upsert_backtest_position(storage::BacktestPositionRecord {
        run_id: "backtest-run".to_string(),
        account_id: "backtest".to_string(),
        symbol: "US:NASDAQ:AAPL:EQUITY".to_string(),
        qty: "1".to_string(),
        avg_price: "20".to_string(),
        updated_at_ms: 3,
    })
    .await
    .unwrap();

    db.complete_backtest_run(storage::BacktestCompletedRun {
        run_id: "backtest-run".to_string(),
        strategy_name: "moving_average_cross".to_string(),
        started_at_ms: 1,
        ended_at_ms: 3,
        config_json: "{}".to_string(),
    })
    .await
    .unwrap();

    let run = db.get_strategy_run("backtest-run").await.unwrap().unwrap();
    assert_eq!(run.status, "completed");
    assert_eq!(
        db.list_events_by_source("backtest-run")
            .await
            .unwrap()
            .len(),
        1
    );
    assert_eq!(db.list_orders("backtest-run").await.unwrap().len(), 1);
    assert_eq!(db.list_fills("backtest-run").await.unwrap().len(), 1);
    assert_eq!(db.list_positions("backtest-run").await.unwrap().len(), 1);
}

#[tokio::test]
async fn migrate_adds_error_column_to_existing_strategy_runs_table() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    sqlx::query(
        r#"
        CREATE TABLE strategy_runs (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            mode TEXT NOT NULL,
            status TEXT NOT NULL,
            started_at_ms INTEGER NOT NULL,
            ended_at_ms INTEGER,
            config_json TEXT NOT NULL
        )
        "#,
    )
    .execute(db.pool())
    .await
    .unwrap();

    db.migrate().await.unwrap();
    db.insert_strategy_run(NewStrategyRun {
        id: "run-old-schema".to_string(),
        name: "moving_average_cross".to_string(),
        mode: "paper".to_string(),
        status: "running".to_string(),
        started_at_ms: 1,
        ended_at_ms: None,
        error: None,
        config_json: "{}".to_string(),
    })
    .await
    .unwrap();

    db.update_strategy_run_status("run-old-schema", "failed", Some(2), Some("boom"))
        .await
        .unwrap();
    let run = db
        .get_strategy_run("run-old-schema")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(run.error, Some("boom".to_string()));
}

fn dec(value: &str) -> Decimal {
    value.parse().unwrap()
}
