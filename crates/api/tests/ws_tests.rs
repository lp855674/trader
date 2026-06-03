use api::{AppState, router_with_state};
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
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let app = router_with_state(AppState::new(db, "configs/backtest/ma_cross.toml".into()));
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("ws://{address}")
}
