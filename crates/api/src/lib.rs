//! HTTP and WebSocket API for quantd.

use std::sync::Arc;

use axum::extract::State;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::response::IntoResponse;
use axum::routing::get;
use axum::{Json, Router};
use serde::Serialize;
use tokio::sync::broadcast;

#[derive(Clone)]
pub struct AppState {
    pub database: db::Db,
    pub events: broadcast::Sender<StreamEvent>,
}

#[derive(Clone, Debug)]
pub enum StreamEvent {
    OrderCycleDone { venue: domain::Venue },
}

#[derive(Serialize)]
struct HealthBody {
    status: &'static str,
}

async fn health() -> Json<HealthBody> {
    Json(HealthBody { status: "ok" })
}

async fn list_instruments(State(state): State<Arc<AppState>>) -> Result<Json<Vec<db::InstrumentRow>>, ApiError> {
    let rows = db::list_instruments(state.database.pool())
        .await
        .map_err(ApiError::internal)?;
    Ok(Json(rows))
}

async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    let mut rx = state.events.subscribe();
    ws.on_upgrade(move |mut socket: WebSocket| async move {
        let hello = serde_json::json!({
            "kind": "hello",
            "schema_version": 1u32,
        });
        if let Ok(text) = serde_json::to_string(&hello) {
            let _ = socket.send(Message::Text(text)).await;
        }

        loop {
            tokio::select! {
                message = rx.recv() => {
                    match message {
                        Ok(event) => {
                            let envelope = stream_envelope(&event);
                            if let Ok(text) = serde_json::to_string(&envelope) {
                                if socket.send(Message::Text(text)).await.is_err() {
                                    break;
                                }
                            }
                        }
                        Err(broadcast::error::RecvError::Lagged(_skipped)) => {
                            continue;
                        }
                        Err(broadcast::error::RecvError::Closed) => break,
                    }
                }
                incoming = socket.recv() => {
                    match incoming {
                        Some(Ok(Message::Close(_))) | None => break,
                        Some(Ok(_other)) => {}
                        Some(Err(_)) => break,
                    }
                }
            }
        }
    })
}

fn stream_envelope(event: &StreamEvent) -> serde_json::Value {
    let event_id = uuid::Uuid::new_v4().to_string();
    match event {
        StreamEvent::OrderCycleDone { venue } => serde_json::json!({
            "event_id": event_id,
            "kind": "order_cycle_done",
            "payload": { "venue": venue.as_str() },
        }),
    }
}

#[derive(Debug)]
pub struct ApiError {
    code: &'static str,
    message: String,
}

impl ApiError {
    fn internal(err: db::DbError) -> Self {
        Self {
            code: "internal",
            message: err.to_string(),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        let body = Json(serde_json::json!({
            "error_code": self.code,
            "message": self.message,
        }));
        (axum::http::StatusCode::INTERNAL_SERVER_ERROR, body).into_response()
    }
}

pub fn router(state: AppState) -> Router {
    let shared = Arc::new(state);
    Router::new()
        .route("/health", get(health))
        .route("/v1/instruments", get(list_instruments))
        .route("/v1/stream", get(ws_handler))
        .with_state(shared)
}
