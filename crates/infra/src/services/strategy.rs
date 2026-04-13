use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq)]
pub enum StrategyStatus {
    Active,
    Paused,
    Error(String),
}

#[derive(Default)]
pub struct StrategyServiceStub {
    strategies: HashMap<String, StrategyStatus>,
}

impl StrategyServiceStub {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, id: &str) {
        self.strategies
            .insert(id.to_string(), StrategyStatus::Active);
    }

    pub fn pause(&mut self, id: &str) -> bool {
        if let Some(s) = self.strategies.get_mut(id) {
            if *s == StrategyStatus::Active {
                *s = StrategyStatus::Paused;
                return true;
            }
        }
        false
    }

    pub fn resume(&mut self, id: &str) -> bool {
        if let Some(s) = self.strategies.get_mut(id) {
            if *s == StrategyStatus::Paused {
                *s = StrategyStatus::Active;
                return true;
            }
        }
        false
    }

    pub fn status(&self, id: &str) -> Option<&StrategyStatus> {
        self.strategies.get(id)
    }

    pub fn active_count(&self) -> usize {
        self.strategies
            .values()
            .filter(|s| **s == StrategyStatus::Active)
            .count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn register_pause_resume() {
        let mut svc = StrategyServiceStub::new();
        svc.register("momentum_v1");
        assert_eq!(svc.active_count(), 1);
        assert!(svc.pause("momentum_v1"));
        assert_eq!(svc.active_count(), 0);
        assert!(svc.resume("momentum_v1"));
        assert_eq!(svc.active_count(), 1);
    }

    #[test]
    fn pause_unknown_returns_false() {
        let mut svc = StrategyServiceStub::new();
        assert!(!svc.pause("ghost"));
    }
}
