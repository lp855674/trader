use api::{AppState, router_with_state};
use events::{RuntimeEvent, TraderEvent, envelope};
use futures::{SinkExt, StreamExt};
use storage::{Db, NewEventRecord};
use tokio_tungstenite::{connect_async, tungstenite::Message};

#[tokio::test]
async fn websocket_subscribe_replays_persisted_run_events() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    db.insert_event(NewEventRecord {
        event_id: "event-1".to_string(),
        ts_ms: 1,
        source: "run-a".to_string(),
        category: "paper.completed".to_string(),
        payload_json: r#"{"orders":1}"#.to_string(),
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
            "algorithm",
            TraderEvent::Runtime(RuntimeEvent {
                category: "algorithm.alpha.generated".to_string(),
                payload_json: serde_json::json!({
                    "run_id": "run-live",
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
async fn websocket_replay_control_message_updates_state() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    let url = spawn_server(db).await;
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

async fn spawn_server(db: Db) -> String {
    spawn_server_with_state(AppState::new(db, "configs/backtest/ma_cross.toml".into())).await
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
