//! WebSocket `/v1/stream` — Task 11.

use std::sync::Arc;

use axum::extract::State;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::response::IntoResponse;
use tokio::sync::broadcast;

use crate::AppState;
use crate::StreamEvent;

pub async fn ws_handler(
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
                            let text = match serde_json::to_string(&envelope) {
                                Ok(t) => t,
                                Err(_) => serde_json::to_string(&stream_envelope(
                                    &StreamEvent::Error {
                                        error_code: "serialization_failed".to_string(),
                                        message: "failed to serialize outbound event".to_string(),
                                    },
                                ))
                                .unwrap_or_else(|_| r#"{"kind":"error","error_code":"internal","message":"serialize"}"#.to_string()),
                            };
                            if socket.send(Message::Text(text)).await.is_err() {
                                break;
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

pub(crate) fn stream_envelope(event: &StreamEvent) -> serde_json::Value {
    let event_id = uuid::Uuid::new_v4().to_string();
    match event {
        StreamEvent::OrderCreated {
            order_id,
            venue,
            symbol,
        } => serde_json::json!({
            "event_id": event_id,
            "kind": "order_created",
            "payload": {
                "order_id": order_id,
                "venue": venue.as_str(),
                "symbol": symbol,
            },
        }),
        StreamEvent::Error {
            error_code,
            message,
        } => serde_json::json!({
            "event_id": event_id,
            "kind": "error",
            "error_code": error_code,
            "message": message,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::stream_envelope;
    use crate::StreamEvent;
    use domain::Venue;

    #[test]
    fn error_envelope_has_kind_error_and_error_code() {
        let v = stream_envelope(&StreamEvent::Error {
            error_code: "execution_not_configured".to_string(),
            message: "no route".to_string(),
        });
        assert_eq!(v["kind"], "error");
        assert_eq!(v["error_code"], "execution_not_configured");
        assert!(v.get("payload").is_none());
    }

    #[test]
    fn order_created_envelope_has_no_top_level_error_code() {
        let v = stream_envelope(&StreamEvent::OrderCreated {
            order_id: "o1".to_string(),
            venue: Venue::Crypto,
            symbol: "X".to_string(),
        });
        assert_eq!(v["kind"], "order_created");
        assert!(v.get("error_code").is_none());
    }
}
