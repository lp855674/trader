pub struct ShutdownSignal {
    pub triggered: bool,
}

impl ShutdownSignal {
    pub fn new() -> Self {
        Self { triggered: false }
    }

    pub fn trigger(&mut self) {
        self.triggered = true;
    }

    pub fn is_triggered(&self) -> bool {
        self.triggered
    }

    /// Returns current state (non-blocking).
    pub fn wait_for_signal(&self) -> bool {
        self.triggered
    }
}

impl Default for ShutdownSignal {
    fn default() -> Self {
        Self::new()
    }
}

pub struct GracefulShutdown {
    pub signal: ShutdownSignal,
    pub registered_components: Vec<String>,
    pub shutdown_timeout_ms: u64,
}

impl GracefulShutdown {
    pub fn new(timeout_ms: u64) -> Self {
        Self {
            signal: ShutdownSignal::new(),
            registered_components: Vec::new(),
            shutdown_timeout_ms: timeout_ms,
        }
    }

    pub fn register(&mut self, name: &str) {
        self.registered_components.push(name.to_string());
    }

    pub fn initiate(&mut self) {
        self.signal.trigger();
    }

    /// Returns true if shutdown completed within timeout (stub: always true).
    pub fn await_completion(&self, _ts_ms: i64) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shutdown_signal_state_machine() {
        let mut signal = ShutdownSignal::new();
        assert!(!signal.is_triggered());
        signal.trigger();
        assert!(signal.is_triggered());
        assert!(signal.wait_for_signal());
    }

    #[test]
    fn graceful_shutdown_register_and_initiate() {
        let mut gs = GracefulShutdown::new(5000);
        gs.register("cache");
        gs.register("source");
        assert_eq!(gs.registered_components.len(), 2);
        assert!(!gs.signal.is_triggered());
        gs.initiate();
        assert!(gs.signal.is_triggered());
        assert!(gs.await_completion(0));
    }
}
