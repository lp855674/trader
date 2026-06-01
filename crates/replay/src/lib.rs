#![forbid(unsafe_code)]

use data::Bar;
use std::time::Duration;

pub struct ReplayRuntime {
    speed: u32,
}

impl ReplayRuntime {
    pub fn new(speed: u32) -> Self {
        Self {
            speed: speed.max(1),
        }
    }

    pub async fn replay_bars(&self, bars: Vec<Bar>) -> usize {
        let delay = Duration::from_millis(1000 / u64::from(self.speed));
        let mut count = 0;
        for _bar in bars {
            tokio::time::sleep(delay).await;
            count += 1;
        }
        count
    }
}
