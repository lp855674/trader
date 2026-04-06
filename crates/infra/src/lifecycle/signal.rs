use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Debug, Clone, PartialEq)]
pub enum SignalKind {
    Interrupt,
    Terminate,
    Custom(u32),
}

pub struct SignalHandler {
    pub registered: Vec<SignalKind>,
    fired: Arc<AtomicBool>,
}

impl SignalHandler {
    pub fn new() -> Self {
        Self {
            registered: Vec::new(),
            fired: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn register(&mut self, kind: SignalKind) {
        if !self.registered.contains(&kind) {
            self.registered.push(kind);
        }
    }

    /// Simulate a signal firing (for testing/in-process use).
    pub fn simulate_signal(&self, _kind: SignalKind) {
        self.fired.store(true, Ordering::SeqCst);
    }

    pub fn is_fired(&self) -> bool {
        self.fired.load(Ordering::SeqCst)
    }

    pub fn reset(&self) {
        self.fired.store(false, Ordering::SeqCst);
    }
}

impl Default for SignalHandler {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simulate_sets_fired() {
        let mut h = SignalHandler::new();
        h.register(SignalKind::Interrupt);
        assert!(!h.is_fired());
        h.simulate_signal(SignalKind::Interrupt);
        assert!(h.is_fired());
    }

    #[test]
    fn reset_clears_flag() {
        let h = SignalHandler::new();
        h.simulate_signal(SignalKind::Terminate);
        assert!(h.is_fired());
        h.reset();
        assert!(!h.is_fired());
    }
}
