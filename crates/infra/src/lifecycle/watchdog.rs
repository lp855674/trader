use std::collections::HashMap;

pub struct WatchdogEntry {
    pub name: String,
    pub last_heartbeat_ms: u64,
    pub timeout_ms: u64,
}

#[derive(Default)]
pub struct Watchdog {
    entries: HashMap<String, WatchdogEntry>,
    current_time_ms: u64,
}

impl Watchdog {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, name: &str, timeout_ms: u64) {
        self.entries.insert(
            name.to_string(),
            WatchdogEntry {
                name: name.to_string(),
                last_heartbeat_ms: self.current_time_ms,
                timeout_ms,
            },
        );
    }

    pub fn heartbeat(&mut self, name: &str) {
        if let Some(e) = self.entries.get_mut(name) {
            e.last_heartbeat_ms = self.current_time_ms;
        }
    }

    pub fn tick(&mut self, elapsed_ms: u64) {
        self.current_time_ms += elapsed_ms;
    }

    pub fn unhealthy(&self) -> Vec<String> {
        self.entries
            .values()
            .filter(|e| self.current_time_ms.saturating_sub(e.last_heartbeat_ms) > e.timeout_ms)
            .map(|e| e.name.clone())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_timeout() {
        let mut wd = Watchdog::new();
        wd.register("strategy_runner", 1000);
        wd.tick(1500);
        assert!(wd.unhealthy().contains(&"strategy_runner".to_string()));
    }

    #[test]
    fn heartbeat_keeps_healthy() {
        let mut wd = Watchdog::new();
        wd.register("order_manager", 1000);
        wd.tick(800);
        wd.heartbeat("order_manager");
        wd.tick(800);
        assert!(wd.unhealthy().is_empty());
    }
}
