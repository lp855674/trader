#![forbid(unsafe_code)]

use data::Bar;
use events::{EventBus, RuntimeEvent, TraderEvent, envelope};
use serde::Serialize;
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReplayStatus {
    Running,
    Paused,
    Completed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReplayState {
    pub run_id: String,
    pub status: ReplayStatus,
    pub speed: u32,
    pub offset: usize,
}

pub struct ReplayController {
    state: ReplayState,
}

impl ReplayController {
    pub fn new(run_id: impl Into<String>, speed: u32) -> Self {
        Self {
            state: ReplayState {
                run_id: run_id.into(),
                status: ReplayStatus::Running,
                speed: speed.max(1),
                offset: 0,
            },
        }
    }

    pub fn state(&self) -> &ReplayState {
        &self.state
    }

    pub fn pause(&mut self) {
        self.state.status = ReplayStatus::Paused;
    }

    pub fn resume(&mut self) {
        self.state.status = ReplayStatus::Running;
    }

    pub fn seek(&mut self, offset: usize) {
        self.state.offset = offset;
    }

    pub fn set_speed(&mut self, speed: u32) {
        self.state.speed = speed.max(1);
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReplaySummary {
    pub bars: usize,
    pub speed: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReplayEvent {
    pub ts_ms: i64,
    pub category: String,
    pub payload_json: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReplayEventSummary {
    pub bars: usize,
    pub speed: u32,
    pub events: Vec<ReplayEvent>,
}

pub struct ReplayRuntime {
    speed: u32,
    event_bus: Option<EventBus>,
}

impl ReplayRuntime {
    pub fn new(speed: u32) -> Self {
        Self {
            speed: speed.max(1),
            event_bus: None,
        }
    }

    pub fn with_event_bus(mut self, event_bus: EventBus) -> Self {
        self.event_bus = Some(event_bus);
        self
    }

    pub async fn replay_bars(&self, bars: Vec<Bar>) -> ReplaySummary {
        let summary = self.replay_bars_with_events(bars).await;
        ReplaySummary {
            bars: summary.bars,
            speed: summary.speed,
        }
    }

    pub async fn replay_bars_with_events(&self, bars: Vec<Bar>) -> ReplayEventSummary {
        let delay = Duration::from_millis(1000 / u64::from(self.speed));
        let mut count = 0;
        let mut events = Vec::new();
        for bar in bars {
            tokio::time::sleep(delay).await;
            let event = ReplayEvent {
                ts_ms: bar.ts_ms,
                category: "market.bar".to_string(),
                payload_json: serde_json::json!({
                    "ts_ms": bar.ts_ms,
                    "open": bar.open.to_string(),
                    "high": bar.high.to_string(),
                    "low": bar.low.to_string(),
                    "close": bar.close.to_string(),
                    "volume": bar.volume.to_string()
                })
                .to_string(),
            };
            self.publish_event(&event);
            events.push(event);
            count += 1;
        }
        ReplayEventSummary {
            bars: count,
            speed: self.speed,
            events,
        }
    }

    fn publish_event(&self, event: &ReplayEvent) {
        let Some(event_bus) = &self.event_bus else {
            return;
        };
        // best-effort: replay observers may lag or disconnect.
        let _ = event_bus.publish(envelope(
            "replay",
            TraderEvent::Runtime(RuntimeEvent {
                category: event.category.clone(),
                payload_json: event.payload_json.clone(),
            }),
        ));
    }
}
