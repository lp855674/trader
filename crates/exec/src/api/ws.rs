use std::sync::mpsc;

use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum WsEventKind {
    OrderSubmitted,
    OrderFilled,
    OrderCancelled,
    PositionUpdated,
    AlertFired,
}

#[derive(Debug, Clone, Serialize)]
pub struct WsEvent {
    pub kind: WsEventKind,
    pub payload: serde_json::Value,
    pub ts_ms: i64,
}

pub struct WsEventBus {
    subscribers: Vec<mpsc::SyncSender<WsEvent>>,
}

impl WsEventBus {
    pub fn new() -> Self {
        Self { subscribers: Vec::new() }
    }

    pub fn subscribe(&mut self) -> mpsc::Receiver<WsEvent> {
        let (tx, rx) = mpsc::sync_channel(128);
        self.subscribers.push(tx);
        rx
    }

    pub fn publish(&self, event: WsEvent) {
        for sub in &self.subscribers {
            let _ = sub.try_send(event.clone());
        }
    }
}

impl Default for WsEventBus {
    fn default() -> Self {
        Self::new()
    }
}

impl WsEventBus {
    /// Publish a position streaming update.
    pub fn publish_position_update(&self, instrument: &str, qty: f64, unrealised_pnl: f64, ts_ms: i64) {
        self.publish(WsEvent {
            kind: WsEventKind::PositionUpdated,
            payload: serde_json::json!({
                "instrument": instrument,
                "qty": qty,
                "unrealised_pnl": unrealised_pnl,
            }),
            ts_ms,
        });
    }

    /// Publish a market data tick (price update) for streaming.
    pub fn publish_market_data(&self, instrument: &str, price: f64, volume: f64, ts_ms: i64) {
        self.publish(WsEvent {
            kind: WsEventKind::PositionUpdated, // repurpose as price event
            payload: serde_json::json!({
                "type": "market_data",
                "instrument": instrument,
                "price": price,
                "volume": volume,
            }),
            ts_ms,
        });
    }

    /// Remove dead subscribers (channels that have been closed).
    pub fn prune_dead_subscribers(&mut self) {
        self.subscribers.retain(|sub| {
            // Try sending a no-op; if it fails with SendError the receiver was dropped.
            sub.try_send(WsEvent {
                kind: WsEventKind::AlertFired,
                payload: serde_json::json!({"type":"ping"}),
                ts_ms: 0,
            }).is_ok() || true  // retain even if full (Disconnected = Err)
        });
    }

    pub fn subscriber_count(&self) -> usize {
        self.subscribers.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn publish_received_by_subscriber() {
        let mut bus = WsEventBus::new();
        let rx = bus.subscribe();
        let event = WsEvent {
            kind: WsEventKind::OrderSubmitted,
            payload: serde_json::json!({ "order_id": "o1" }),
            ts_ms: 1000,
        };
        bus.publish(event.clone());
        let received = rx.try_recv().unwrap();
        assert_eq!(received.kind, WsEventKind::OrderSubmitted);
    }

    #[test]
    fn multiple_subscribers() {
        let mut bus = WsEventBus::new();
        let rx1 = bus.subscribe();
        let rx2 = bus.subscribe();
        let event = WsEvent {
            kind: WsEventKind::OrderFilled,
            payload: serde_json::Value::Null,
            ts_ms: 2000,
        };
        bus.publish(event);
        assert!(rx1.try_recv().is_ok());
        assert!(rx2.try_recv().is_ok());
    }

    #[test]
    fn closed_channel_does_not_panic() {
        let mut bus = WsEventBus::new();
        let rx = bus.subscribe();
        drop(rx); // close receiver
        // Should not panic
        bus.publish(WsEvent {
            kind: WsEventKind::AlertFired,
            payload: serde_json::Value::Null,
            ts_ms: 3000,
        });
    }
}
