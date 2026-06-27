use api::{AppState, router_with_state};
use events::{RuntimeEvent, TraderEvent, envelope};
use futures::{SinkExt, StreamExt};
use replay::ReplayController;
use std::sync::Arc;
use storage::{Db, RuntimeEventCommand};
use tokio::sync::Mutex;
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
    let state = AppState::new(db, "configs/backtest/ma_cross.toml".into());
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
    let state = AppState::new(db, "configs/backtest/ma_cross.toml".into());
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
    let state = AppState::new(db, "configs/backtest/ma_cross.toml".into());
    register_replay_controller(&state, "run-a").await;
    let url = spawn_server_with_state(state).await;
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

async fn spawn_server(db: Db) -> String {
    spawn_server_with_state(AppState::new(db, "configs/backtest/ma_cross.toml".into())).await
}

async fn register_replay_controller(state: &AppState, run_id: &str) {
    state.replay_controllers.lock().await.insert(
        run_id.to_string(),
        Arc::new(Mutex::new(ReplayController::new(run_id.to_string(), 1))),
    );
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
