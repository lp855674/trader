#![forbid(unsafe_code)]

use data::Bar;
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

pub struct ReplayRuntime {
    speed: u32,
}

impl ReplayRuntime {
    pub fn new(speed: u32) -> Self {
        Self {
            speed: speed.max(1),
        }
    }

    pub async fn replay_bars(&self, bars: Vec<Bar>) -> ReplaySummary {
        let delay = Duration::from_millis(1000 / u64::from(self.speed));
        let mut count = 0;
        for _bar in bars {
            tokio::time::sleep(delay).await;
            count += 1;
        }
        ReplaySummary {
            bars: count,
            speed: self.speed,
        }
    }
}
