#[derive(Debug, Clone, PartialEq)]
pub enum CircuitState {
    Closed,
    Open,
    HalfOpen,
}

pub struct CircuitBreaker {
    state: CircuitState,
    failure_count: u32,
    failure_threshold: u32,
    success_count: u32,
    half_open_threshold: u32,
}

impl CircuitBreaker {
    pub fn new(failure_threshold: u32, half_open_threshold: u32) -> Self {
        Self {
            state: CircuitState::Closed,
            failure_count: 0,
            failure_threshold,
            success_count: 0,
            half_open_threshold,
        }
    }

    pub fn call_success(&mut self) {
        match self.state {
            CircuitState::Closed => {
                self.failure_count = 0;
            }
            CircuitState::HalfOpen => {
                self.success_count += 1;
                if self.success_count >= self.half_open_threshold {
                    self.state = CircuitState::Closed;
                    self.failure_count = 0;
                    self.success_count = 0;
                }
            }
            CircuitState::Open => {}
        }
    }

    pub fn call_failure(&mut self) {
        match self.state {
            CircuitState::Closed => {
                self.failure_count += 1;
                if self.failure_count >= self.failure_threshold {
                    self.state = CircuitState::Open;
                }
            }
            CircuitState::HalfOpen => {
                self.state = CircuitState::Open;
                self.success_count = 0;
            }
            CircuitState::Open => {}
        }
    }

    /// Transition Open → HalfOpen (called externally after a reset timeout).
    pub fn attempt_reset(&mut self) {
        if self.state == CircuitState::Open {
            self.state = CircuitState::HalfOpen;
            self.success_count = 0;
        }
    }

    pub fn state(&self) -> &CircuitState {
        &self.state
    }

    pub fn is_open(&self) -> bool {
        self.state == CircuitState::Open
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opens_after_threshold_failures() {
        let mut cb = CircuitBreaker::new(3, 2);
        assert!(!cb.is_open());
        cb.call_failure();
        cb.call_failure();
        assert!(!cb.is_open());
        cb.call_failure();
        assert!(cb.is_open());
    }

    #[test]
    fn half_open_to_closed_on_successes() {
        let mut cb = CircuitBreaker::new(1, 2);
        cb.call_failure();
        assert!(cb.is_open());
        cb.attempt_reset();
        assert_eq!(cb.state(), &CircuitState::HalfOpen);
        cb.call_success();
        cb.call_success();
        assert_eq!(cb.state(), &CircuitState::Closed);
    }

    #[test]
    fn half_open_to_open_on_failure() {
        let mut cb = CircuitBreaker::new(1, 3);
        cb.call_failure();
        cb.attempt_reset();
        cb.call_failure();
        assert!(cb.is_open());
    }
}
