#![forbid(unsafe_code)]

use data::Bar;
use events::{EventBus, runtime_envelope};
use serde::Serialize;
use std::{sync::Arc, time::Duration};
use tokio::sync::Mutex;

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

    pub fn advance(&mut self) {
        self.state.offset += 1;
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
    run_id: Option<String>,
    speed: u32,
    event_bus: Option<EventBus>,
    controller: Option<Arc<Mutex<ReplayController>>>,
}

impl ReplayRuntime {
    pub fn new(speed: u32) -> Self {
        Self {
            run_id: None,
            speed: speed.max(1),
            event_bus: None,
            controller: None,
        }
    }

    pub fn new_for_run(run_id: impl Into<String>, speed: u32) -> Self {
        Self {
            run_id: Some(run_id.into()),
            speed: speed.max(1),
            event_bus: None,
            controller: None,
        }
    }

    pub fn with_event_bus(mut self, event_bus: EventBus) -> Self {
        self.event_bus = Some(event_bus);
        self
    }

    pub fn with_controller(mut self, controller: Arc<Mutex<ReplayController>>) -> Self {
        self.controller = Some(controller);
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
        let mut count = 0;
        let mut events = Vec::new();
        let mut offset = 0;
        while offset < bars.len() {
            let state = self.wait_until_running().await;
            if self.controller.is_some() && state.offset < bars.len() {
                offset = state.offset;
            }
            self.sleep_while_running(Duration::from_millis(1000 / u64::from(state.speed)))
                .await;
            let state = self.wait_until_running().await;
            if self.controller.is_some() && state.offset < bars.len() {
                offset = state.offset;
            }
            let bar = bars[offset].clone();
            let source = self.event_source();
            let event = ReplayEvent {
                ts_ms: bar.ts_ms,
                category: "market.bar".to_string(),
                payload_json: serde_json::json!({
                    "run_id": source,
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
            offset += 1;
            self.advance_controller().await;
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
        let Ok(envelope) =
            serde_json::from_str::<serde_json::Value>(&event.payload_json).and_then(|payload| {
                runtime_envelope(self.event_source(), event.category.clone(), payload)
            })
        else {
            return;
        };
        // best-effort: replay observers may lag or disconnect.
        let _ = event_bus.publish(envelope);
    }

    fn event_source(&self) -> &str {
        self.run_id.as_deref().unwrap_or("replay")
    }

    async fn wait_until_running(&self) -> ReplayState {
        loop {
            let Some(controller) = &self.controller else {
                return ReplayState {
                    run_id: self.event_source().to_string(),
                    status: ReplayStatus::Running,
                    speed: self.speed,
                    offset: 0,
                };
            };
            let state = controller.lock().await.state().clone();
            if state.status == ReplayStatus::Running {
                return state;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    async fn advance_controller(&self) {
        let Some(controller) = &self.controller else {
            return;
        };
        controller.lock().await.advance();
    }

    async fn sleep_while_running(&self, delay: Duration) {
        let mut elapsed = Duration::ZERO;
        while elapsed < delay {
            let step = (delay - elapsed).min(Duration::from_millis(10));
            tokio::time::sleep(step).await;
            elapsed += step;
            let Some(controller) = &self.controller else {
                continue;
            };
            if controller.lock().await.state().status != ReplayStatus::Running {
                return;
            }
        }
    }
}
