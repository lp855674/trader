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
            side,
            qty,
            status,
            order_type,
            limit_price,
            exchange_ref,
            created_at_ms,
            updated_at_ms,
        } => serde_json::json!({
            "event_id": event_id,
            "kind": "order_created",
            "payload": {
                "order_id": order_id,
                "venue": venue.as_str(),
                "symbol": symbol,
                "side": side,
                "qty": qty,
                "status": status,
                "order_type": order_type,
                "limit_price": limit_price,
                "exchange_ref": exchange_ref,
                "created_at_ms": created_at_ms,
                "updated_at_ms": updated_at_ms,
            },
        }),
        StreamEvent::OrderUpdated {
            order_id,
            status,
            qty,
            limit_price,
        } => serde_json::json!({
            "event_id": event_id,
            "kind": "order_updated",
            "payload": {
                "order_id": order_id,
                "status": status,
                "qty": qty,
                "limit_price": limit_price,
            },
        }),
        StreamEvent::OrderCancelled { order_id } => serde_json::json!({
            "event_id": event_id,
            "kind": "order_cancelled",
            "payload": {
                "order_id": order_id,
            },
        }),
        StreamEvent::OrderReplaced {
            order_id,
            qty,
            limit_price,
        } => serde_json::json!({
            "event_id": event_id,
            "kind": "order_replaced",
            "payload": {
                "order_id": order_id,
                "qty": qty,
                "limit_price": limit_price,
            },
        }),
        StreamEvent::QuoteUpdated {
            symbol,
            venue,
            last_price,
            day_high,
            day_low,
            bars,
        } => serde_json::json!({
            "event_id": event_id,
            "kind": "quote_updated",
            "payload": {
                "symbol": symbol,
                "venue": venue,
                "last_price": last_price,
                "day_high": day_high,
                "day_low": day_low,
                "bars": bars,
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
            side: None,
            qty: None,
            status: None,
            order_type: None,
            limit_price: None,
            exchange_ref: None,
            created_at_ms: None,
            updated_at_ms: None,
        });
        assert_eq!(v["kind"], "order_created");
        assert!(v.get("error_code").is_none());
    }

    #[test]
    fn order_created_envelope_contains_full_payload_when_available() {
        let v = stream_envelope(&StreamEvent::OrderCreated {
            order_id: "o3".to_string(),
            venue: Venue::UsEquity,
            symbol: "AAPL.US".to_string(),
            side: Some("buy".to_string()),
            qty: Some(10.0),
            status: Some("SUBMITTED".to_string()),
            order_type: Some("limit".to_string()),
            limit_price: Some(123.45),
            exchange_ref: Some("lb-1".to_string()),
            created_at_ms: Some(1000),
            updated_at_ms: Some(1001),
        });
        assert_eq!(v["payload"]["order_id"], "o3");
        assert_eq!(v["payload"]["side"], "buy");
        assert_eq!(v["payload"]["qty"], 10.0);
        assert_eq!(v["payload"]["status"], "SUBMITTED");
        assert_eq!(v["payload"]["order_type"], "limit");
        assert_eq!(v["payload"]["limit_price"], 123.45);
        assert_eq!(v["payload"]["exchange_ref"], "lb-1");
        assert_eq!(v["payload"]["created_at_ms"], 1000);
        assert_eq!(v["payload"]["updated_at_ms"], 1001);
    }

    #[test]
    fn order_replaced_envelope_contains_payload_fields() {
        let v = stream_envelope(&StreamEvent::OrderReplaced {
            order_id: "o2".to_string(),
            qty: 12.0,
            limit_price: Some(101.5),
        });
        assert_eq!(v["kind"], "order_replaced");
        assert_eq!(v["payload"]["order_id"], "o2");
        assert_eq!(v["payload"]["qty"], 12.0);
        assert_eq!(v["payload"]["limit_price"], 101.5);
    }

    #[test]
    fn quote_updated_envelope_contains_quote_fields() {
        let v = stream_envelope(&StreamEvent::QuoteUpdated {
            symbol: "AAPL.US".to_string(),
            venue: "US_EQUITY".to_string(),
            last_price: Some(128.0),
            day_high: Some(128.0),
            day_low: Some(119.5),
            bars: vec![db::BarRow {
                ts_ms: 1000,
                open: 120.0,
                high: 128.0,
                low: 119.5,
                close: 128.0,
                volume: 1400.0,
            }],
        });
        assert_eq!(v["kind"], "quote_updated");
        assert_eq!(v["payload"]["symbol"], "AAPL.US");
        assert_eq!(v["payload"]["last_price"], 128.0);
        assert_eq!(v["payload"]["bars"][0]["close"], 128.0);
    }
}
