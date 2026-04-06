use crate::core::{DataItem, DataQuery, DataSource, Granularity};

// ── ReplayConfig ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct ReplayConfig {
    pub start_ts_ms: i64,
    pub end_ts_ms: i64,
    pub speed_multiplier: f64,
    pub instruments: Vec<String>,
}

impl ReplayConfig {
    pub fn new(start_ts_ms: i64, end_ts_ms: i64) -> Self {
        Self {
            start_ts_ms,
            end_ts_ms,
            speed_multiplier: 0.0, // max speed by default
            instruments: Vec::new(),
        }
    }
}

// ── ReplayState ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum ReplayState {
    Idle,
    Running {
        current_ts_ms: i64,
        items_processed: u64,
    },
    Paused {
        at_ts_ms: i64,
    },
    Completed,
}

// ── ReplayController ──────────────────────────────────────────────────────────

pub struct ReplayController {
    pub source: Box<dyn DataSource>,
    pub config: ReplayConfig,
    pub state: ReplayState,
    step_ms: i64,
}

impl ReplayController {
    pub fn new(source: Box<dyn DataSource>, config: ReplayConfig) -> Self {
        let step_ms = Granularity::Minutes(1).to_ms().unwrap_or(60_000) as i64;
        Self {
            source,
            config,
            state: ReplayState::Idle,
            step_ms,
        }
    }

    pub fn with_step_ms(mut self, step_ms: i64) -> Self {
        self.step_ms = step_ms;
        self
    }

    fn current_ts(&self) -> i64 {
        match &self.state {
            ReplayState::Idle => self.config.start_ts_ms,
            ReplayState::Running { current_ts_ms, .. } => *current_ts_ms,
            ReplayState::Paused { at_ts_ms } => *at_ts_ms,
            ReplayState::Completed => self.config.end_ts_ms,
        }
    }

    pub fn step(&mut self) -> Option<Vec<DataItem>> {
        let current = self.current_ts();
        if current > self.config.end_ts_ms {
            self.state = ReplayState::Completed;
            return None;
        }

        let window_end = (current + self.step_ms - 1).min(self.config.end_ts_ms);

        // Build query for each instrument (or all if empty)
        let mut all_items = Vec::new();
        let instruments = if self.config.instruments.is_empty() {
            vec!["".to_string()]
        } else {
            self.config.instruments.clone()
        };

        for inst in &instruments {
            let q = DataQuery::new(inst, current, window_end);
            if let Ok(items) = self.source.query(&q) {
                all_items.extend(items);
            }
        }

        let items_count = all_items.len() as u64;
        let next_ts = current + self.step_ms;

        let items_processed = match &self.state {
            ReplayState::Running { items_processed, .. } => *items_processed + items_count,
            _ => items_count,
        };

        if next_ts > self.config.end_ts_ms {
            self.state = ReplayState::Completed;
        } else {
            self.state = ReplayState::Running {
                current_ts_ms: next_ts,
                items_processed,
            };
        }

        Some(all_items)
    }

    pub fn run_to_completion<F>(&mut self, mut on_bar: F) -> u64
    where
        F: FnMut(DataItem),
    {
        self.state = ReplayState::Running {
            current_ts_ms: self.config.start_ts_ms,
            items_processed: 0,
        };
        let mut total = 0u64;
        loop {
            match self.step() {
                Some(items) => {
                    total += items.len() as u64;
                    for item in items {
                        on_bar(item);
                    }
                }
                None => break,
            }
            if matches!(self.state, ReplayState::Completed) {
                break;
            }
        }
        total
    }

    pub fn pause(&mut self) {
        let ts = self.current_ts();
        self.state = ReplayState::Paused { at_ts_ms: ts };
    }

    pub fn resume(&mut self) {
        if let ReplayState::Paused { at_ts_ms } = self.state {
            self.state = ReplayState::Running {
                current_ts_ms: at_ts_ms,
                items_processed: 0,
            };
        }
    }

    pub fn seek(&mut self, ts_ms: i64) {
        self.state = ReplayState::Running {
            current_ts_ms: ts_ms,
            items_processed: 0,
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::InMemoryDataSource;
    use domain::NormalizedBar;

    fn make_source(count: usize) -> Box<dyn DataSource> {
        let items: Vec<DataItem> = (0..count)
            .map(|i| {
                DataItem::Bar(NormalizedBar {
                    ts_ms: (i as i64) * 60_000,
                    open: 1.0,
                    high: 1.0,
                    low: 1.0,
                    close: 1.0,
                    volume: 1.0,
                })
            })
            .collect();
        Box::new(InMemoryDataSource::new("test", items))
    }

    #[test]
    fn replay_runs_to_completion() {
        let config = ReplayConfig::new(0, 5 * 60_000 - 1);
        let mut ctrl = ReplayController::new(make_source(5), config);
        let mut seen = 0u64;
        let total = ctrl.run_to_completion(|_| {
            seen += 1;
        });
        assert_eq!(total, seen);
        assert!(total > 0);
    }

    #[test]
    fn pause_and_resume() {
        let config = ReplayConfig::new(0, 10 * 60_000);
        let mut ctrl = ReplayController::new(make_source(10), config);
        ctrl.seek(0);
        ctrl.step();
        ctrl.pause();
        assert!(matches!(ctrl.state, ReplayState::Paused { .. }));
        ctrl.resume();
        assert!(matches!(ctrl.state, ReplayState::Running { .. }));
    }
}
