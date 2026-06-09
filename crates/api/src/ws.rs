use axum::{
    extract::{
        State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    response::IntoResponse,
};
use events::{AnyEventEnvelope, TraderEvent};
use replay::{ReplayController, ReplayState};
use serde::Deserialize;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::AppState;

#[derive(Debug, Deserialize)]
struct WebSocketRequest {
    #[serde(rename = "type")]
    message_type: String,
    run_id: String,
    action: Option<String>,
    offset: Option<usize>,
    speed: Option<u32>,
}

pub async fn ws_handler(State(state): State<AppState>, ws: WebSocketUpgrade) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_socket(socket, state))
}

async fn handle_socket(mut socket: WebSocket, state: AppState) {
    while let Some(message) = socket.recv().await {
        let Ok(message) = message else {
            break;
        };
        let Message::Text(text) = message else {
            continue;
        };
        let Ok(request) = serde_json::from_str::<WebSocketRequest>(&text) else {
            if socket
                .send(Message::Text(
                    serde_json::json!({
                        "type": "error",
                        "error": "invalid_message"
                    })
                    .to_string()
                    .into(),
                ))
                .await
                .is_err()
            {
                break;
            }
            continue;
        };

        let result = match request.message_type.as_str() {
            "subscribe" => send_subscription_events(&mut socket, &state, &request.run_id).await,
            "replay_control" => send_replay_control(&mut socket, &state, request).await,
            _ => {
                socket
                    .send(Message::Text(
                        serde_json::json!({
                            "type": "error",
                            "error": "unsupported_message_type"
                        })
                        .to_string()
                        .into(),
                    ))
                    .await
            }
        };

        if result.is_err() {
            break;
        }
    }
}

async fn send_subscription_events(
    socket: &mut WebSocket,
    state: &AppState,
    run_id: &str,
) -> Result<(), axum::Error> {
    send_persisted_events(socket, state, run_id).await?;
    stream_runtime_events(socket, state, run_id).await
}

async fn send_persisted_events(
    socket: &mut WebSocket,
    state: &AppState,
    run_id: &str,
) -> Result<(), axum::Error> {
    let events = match state.db.list_events_by_source(run_id).await {
        Ok(events) => events,
        Err(error) => {
            return socket
                .send(Message::Text(
                    serde_json::json!({
                        "type": "error",
                        "error": error.to_string()
                    })
                    .to_string()
                    .into(),
                ))
                .await;
        }
    };

    for event in events {
        socket
            .send(Message::Text(
                serde_json::json!({
                    "type": "event",
                    "event": event
                })
                .to_string()
                .into(),
            ))
            .await?;
    }
    Ok(())
}

async fn stream_runtime_events(
    socket: &mut WebSocket,
    state: &AppState,
    run_id: &str,
) -> Result<(), axum::Error> {
    let mut receiver = state.event_bus.subscribe();
    loop {
        let Ok(event) = receiver.recv().await else {
            continue;
        };
        if !runtime_event_matches_run(&event, run_id) {
            continue;
        }
        socket
            .send(Message::Text(
                serde_json::json!({
                    "type": "event",
                    "event": event
                })
                .to_string()
                .into(),
            ))
            .await?;
    }
}

fn runtime_event_matches_run(event: &AnyEventEnvelope, run_id: &str) -> bool {
    matches!(&event.payload, TraderEvent::Runtime(_)) && event.source == run_id
}

async fn send_replay_control(
    socket: &mut WebSocket,
    state: &AppState,
    request: WebSocketRequest,
) -> Result<(), axum::Error> {
    let replay_state = {
        let mut controllers = state.replay_controllers.lock().await;
        let controller = controllers
            .entry(request.run_id.clone())
            .or_insert_with(|| {
                Arc::new(Mutex::new(ReplayController::new(request.run_id.clone(), 1)))
            })
            .clone();
        drop(controllers);
        let mut controller = controller.lock().await;
        match request.action.as_deref() {
            Some("pause") => controller.pause(),
            Some("resume") => controller.resume(),
            Some("seek") => controller.seek(request.offset.unwrap_or(0)),
            Some("speed") => controller.set_speed(request.speed.unwrap_or(1)),
            _ => {
                return socket
                    .send(Message::Text(
                        serde_json::json!({
                            "type": "error",
                            "error": "unsupported_replay_action"
                        })
                        .to_string()
                        .into(),
                    ))
                    .await;
            }
        }
        controller.state().clone()
    };

    persist_replay_control_event(
        &state.db,
        &request.run_id,
        request.action.as_deref().unwrap_or("control"),
        &replay_state,
    )
    .await;
    socket
        .send(Message::Text(
            serde_json::json!({
                "type": "replay_state",
                "state": replay_state
            })
            .to_string()
            .into(),
        ))
        .await
}

async fn persist_replay_control_event(
    db: &storage::Db,
    run_id: &str,
    action: &str,
    replay_state: &ReplayState,
) {
    let category = match action {
        "pause" => "replay.pause",
        "resume" => "replay.resume",
        "seek" => "replay.seek",
        "speed" => "replay.speed",
        _ => "replay.control",
    };
    let Ok(payload_json) = serde_json::to_string(replay_state) else {
        return;
    };
    // best-effort: websocket replay control response should not fail on audit write errors.
    let _ = db
        .insert_event(storage::NewEventRecord {
            event_id: uuid::Uuid::new_v4().to_string(),
            ts_ms: chrono::Utc::now().timestamp_millis(),
            source: run_id.to_string(),
            category: category.to_string(),
            payload_json,
        })
        .await;
}
