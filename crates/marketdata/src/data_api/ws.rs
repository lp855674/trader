use crate::core::data::DataItem;
use domain::NormalizedBar;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::mpsc::{self, Receiver, Sender};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataWsEvent {
    pub event_type: String,
    pub instrument: String,
    pub data: Value,
    pub ts_ms: i64,
}

pub struct DataWsEventBus {
    senders: Vec<Sender<DataWsEvent>>,
}

impl DataWsEventBus {
    pub fn new() -> Self {
        Self {
            senders: Vec::new(),
        }
    }

    pub fn subscribe(&mut self) -> Receiver<DataWsEvent> {
        let (tx, rx) = mpsc::channel();
        self.senders.push(tx);
        rx
    }

    pub fn publish_bar(&self, instrument: &str, bar: &NormalizedBar) {
        let event = DataWsEvent {
            event_type: "bar".to_string(),
            instrument: instrument.to_string(),
            data: serde_json::to_value(bar).unwrap_or(Value::Null),
            ts_ms: bar.ts_ms,
        };
        self.broadcast(event);
    }

    pub fn publish_tick(&self, instrument: &str, item: &DataItem) {
        let event = DataWsEvent {
            event_type: "tick".to_string(),
            instrument: instrument.to_string(),
            data: serde_json::to_value(item).unwrap_or(Value::Null),
            ts_ms: item.ts_ms(),
        };
        self.broadcast(event);
    }

    fn broadcast(&self, event: DataWsEvent) {
        // Remove dead senders silently
        for sender in &self.senders {
            let _ = sender.send(event.clone());
        }
    }
}

impl Default for DataWsEventBus {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subscriber_receives_bar() {
        let mut bus = DataWsEventBus::new();
        let rx = bus.subscribe();
        let bar = NormalizedBar {
            ts_ms: 1000,
            open: 100.0,
            high: 101.0,
            low: 99.0,
            close: 100.5,
            volume: 500.0,
        };
        bus.publish_bar("BTC", &bar);
        let event = rx.recv().unwrap();
        assert_eq!(event.event_type, "bar");
        assert_eq!(event.instrument, "BTC");
    }
}
