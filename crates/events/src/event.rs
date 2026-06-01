use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum EventCategory {
    Market,
    Signal,
    Portfolio,
    Risk,
    Execution,
    Order,
    Trade,
    Position,
    Account,
    System,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EventEnvelope<T> {
    pub event_id: Uuid,
    pub ts: DateTime<Utc>,
    pub source: String,
    pub category: EventCategory,
    pub payload: T,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SignalSide {
    Buy,
    Sell,
    CloseLong,
    CloseShort,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SignalEvent {
    pub strategy_id: String,
    pub symbol: String,
    pub side: SignalSide,
    pub confidence: f64,
    pub ts: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TraderEvent {
    Signal(SignalEvent),
}

impl TraderEvent {
    pub fn category(&self) -> EventCategory {
        match self {
            Self::Signal(_) => EventCategory::Signal,
        }
    }
}

pub type AnyEventEnvelope = EventEnvelope<TraderEvent>;

pub fn envelope(source: impl Into<String>, payload: TraderEvent) -> AnyEventEnvelope {
    EventEnvelope {
        event_id: Uuid::new_v4(),
        ts: Utc::now(),
        source: source.into(),
        category: payload.category(),
        payload,
    }
}
