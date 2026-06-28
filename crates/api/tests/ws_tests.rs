use api::{AppState, router_with_state};
use events::{RuntimeEvent, TraderEvent, envelope};
use futures::{SinkExt, StreamExt};
use replay::{ReplayController, ReplayStatus};
use runtime::RuntimeRunMetadata;
use std::sync::Arc;
use storage::{Db, RuntimeEventCommand};
use tokio::sync::{Mutex, Notify};
use tokio_tungstenite::{connect_async, tungstenite::Message};

#[tokio::test]
async fn websocket_subscribe_replays_persisted_run_events() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    db.record_runtime_event(RuntimeEventCommand {
        ts_ms: 1,
        source: "run-a".to_string(),
        category: "paper.completed".to_string(),
        payload: serde_json::json!({ "orders": 1 }),
    })
    .await
    .unwrap();
    let url = spawn_server(db).await;
    let (mut socket, _) = connect_async(format!("{url}/ws")).await.unwrap();

    socket
        .send(Message::Text(
            serde_json::json!({
                "type": "subscribe",
                "run_id": "run-a"
            })
            .to_string()
            .into(),
        ))
        .await
        .unwrap();

    let message = socket.next().await.unwrap().unwrap();
    let text = message.to_text().unwrap();
    assert!(text.contains("paper.completed"));
    assert!(text.contains("run-a"));
}

#[tokio::test]
async fn websocket_subscribe_streams_runtime_bus_events_for_run() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    let state = AppState::with_default_run_config(db, "configs/backtest/ma_cross.toml".into());
    let event_bus = state.event_bus.clone();
    let url = spawn_server_with_state(state).await;
    let (mut socket, _) = connect_async(format!("{url}/ws")).await.unwrap();

    socket
        .send(Message::Text(
            serde_json::json!({
                "type": "subscribe",
                "run_id": "run-live"
            })
            .to_string()
            .into(),
        ))
        .await
        .unwrap();

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    event_bus
        .publish(envelope(
            "run-live",
            TraderEvent::Runtime(RuntimeEvent {
                category: "algorithm.alpha.generated".to_string(),
                payload_json: serde_json::json!({
                    "symbol": "US:NASDAQ:AAPL:EQUITY"
                })
                .to_string(),
            }),
        ))
        .unwrap();

    let message = tokio::time::timeout(std::time::Duration::from_secs(2), socket.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    let text = message.to_text().unwrap();
    assert!(text.contains("algorithm.alpha.generated"));
    assert!(text.contains("run-live"));
}

#[tokio::test]
async fn websocket_subscribe_filters_runtime_events_by_envelope_source() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    let state = AppState::with_default_run_config(db, "configs/backtest/ma_cross.toml".into());
    let event_bus = state.event_bus.clone();
    let url = spawn_server_with_state(state).await;
    let (mut socket, _) = connect_async(format!("{url}/ws")).await.unwrap();

    socket
        .send(Message::Text(
            serde_json::json!({
                "type": "subscribe",
                "run_id": "run-source"
            })
            .to_string()
            .into(),
        ))
        .await
        .unwrap();

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    event_bus
        .publish(envelope(
            "other-run",
            TraderEvent::Runtime(RuntimeEvent {
                category: "algorithm.alpha.generated".to_string(),
                payload_json: serde_json::json!({
                    "symbol": "US:NASDAQ:MSFT:EQUITY"
                })
                .to_string(),
            }),
        ))
        .unwrap();
    event_bus
        .publish(envelope(
            "run-source",
            TraderEvent::Runtime(RuntimeEvent {
                category: "algorithm.alpha.generated".to_string(),
                payload_json: serde_json::json!({
                    "symbol": "US:NASDAQ:AAPL:EQUITY"
                })
                .to_string(),
            }),
        ))
        .unwrap();

    let message = tokio::time::timeout(std::time::Duration::from_secs(2), socket.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    let text = message.to_text().unwrap();
    assert!(text.contains("algorithm.alpha.generated"));
    assert!(text.contains("run-source"));
    assert!(text.contains("AAPL"));
    assert!(!text.contains("MSFT"));
}

#[tokio::test]
async fn websocket_replay_control_message_updates_state() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    let state = AppState::with_default_run_config(db, "configs/backtest/ma_cross.toml".into());
    let released = spawn_active_replay_runtime(&state, "run-a").await;
    register_replay_controller(&state, "run-a").await;
    let url = spawn_server_with_state(state.clone()).await;
    let (mut socket, _) = connect_async(format!("{url}/ws")).await.unwrap();

    socket
        .send(Message::Text(
            serde_json::json!({
                "type": "replay_control",
                "run_id": "run-a",
                "action": "pause"
            })
            .to_string()
            .into(),
        ))
        .await
        .unwrap();

    let message = socket.next().await.unwrap().unwrap();
    let text = message.to_text().unwrap();
    assert!(text.contains("\"status\":\"paused\""));
    assert!(text.contains("\"run_id\":\"run-a\""));
    released.notify_one();
    state.runtime_manager.wait_for_idle("run-a").await;
}

#[tokio::test]
async fn websocket_replay_control_rejects_unknown_run() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    let url = spawn_server(db).await;
    let (mut socket, _) = connect_async(format!("{url}/ws")).await.unwrap();

    socket
        .send(Message::Text(
            serde_json::json!({
                "type": "replay_control",
                "run_id": "missing-run",
                "action": "pause"
            })
            .to_string()
            .into(),
        ))
        .await
        .unwrap();

    let message = socket.next().await.unwrap().unwrap();
    let text = message.to_text().unwrap();
    assert!(text.contains("\"type\":\"error\""));
    assert!(text.contains("\"error\":\"unknown_replay_run\""));
    assert!(text.contains("\"run_id\":\"missing-run\""));
}

#[tokio::test]
async fn websocket_replay_control_rejects_stale_controller_for_inactive_run() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    let state = AppState::with_default_run_config(db, "configs/backtest/ma_cross.toml".into());
    let released = spawn_active_replay_runtime(&state, "run-active").await;
    register_replay_controller(&state, "run-active").await;
    let stale_controller = register_replay_controller(&state, "run-stale").await;
    let url = spawn_server_with_state(state.clone()).await;
    let (mut socket, _) = connect_async(format!("{url}/ws")).await.unwrap();

    socket
        .send(Message::Text(
            serde_json::json!({
                "type": "replay_control",
                "run_id": "run-stale",
                "action": "pause"
            })
            .to_string()
            .into(),
        ))
        .await
        .unwrap();

    let message = socket.next().await.unwrap().unwrap();
    let text = message.to_text().unwrap();
    assert!(text.contains("\"type\":\"error\""));
    assert!(text.contains("\"error\":\"inactive_replay_run\""));
    assert!(text.contains("\"run_id\":\"run-stale\""));
    assert_eq!(
        stale_controller.lock().await.state().status,
        ReplayStatus::Running
    );

    released.notify_one();
    state.runtime_manager.wait_for_idle("run-active").await;
}

async fn spawn_server(db: Db) -> String {
    spawn_server_with_state(AppState::with_default_run_config(
        db,
        "configs/backtest/ma_cross.toml".into(),
    ))
    .await
}

async fn register_replay_controller(
    state: &AppState,
    run_id: &str,
) -> Arc<Mutex<ReplayController>> {
    let controller = Arc::new(Mutex::new(ReplayController::new(run_id.to_string(), 1)));
    state
        .replay_controllers
        .lock()
        .await
        .insert(run_id.to_string(), controller.clone());
    controller
}

async fn spawn_active_replay_runtime(state: &AppState, run_id: &str) -> Arc<Notify> {
    let released = Arc::new(Notify::new());
    let released_for_task = released.clone();
    state
        .runtime_manager
        .spawn_with_metadata(
            run_id.to_string(),
            RuntimeRunMetadata {
                mode: Some("replay".to_string()),
            },
            move |_cancel| async move {
                released_for_task.notified().await;
            },
        )
        .await
        .unwrap();
    released
}

async fn spawn_server_with_state(state: AppState) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let app = router_with_state(state);
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("ws://{address}")
}
