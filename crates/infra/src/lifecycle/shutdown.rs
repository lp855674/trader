#[derive(Debug, Clone, PartialEq)]
pub enum ShutdownPhase {
    Running,
    Initiated,
    DrainConnections,
    SaveState,
    Complete,
}

pub struct GracefulShutdown {
    phase: ShutdownPhase,
    pub timeout_ms: u64,
    steps_completed: Vec<String>,
}

impl GracefulShutdown {
    pub fn new(timeout_ms: u64) -> Self {
        Self {
            phase: ShutdownPhase::Running,
            timeout_ms,
            steps_completed: Vec::new(),
        }
    }

    pub fn initiate(&mut self) {
        if self.phase == ShutdownPhase::Running {
            self.phase = ShutdownPhase::Initiated;
        }
    }

    pub fn complete_step(&mut self, name: &str) {
        self.steps_completed.push(name.to_string());
    }

    pub fn advance_phase(&mut self) {
        self.phase = match self.phase {
            ShutdownPhase::Running => ShutdownPhase::Initiated,
            ShutdownPhase::Initiated => ShutdownPhase::DrainConnections,
            ShutdownPhase::DrainConnections => ShutdownPhase::SaveState,
            ShutdownPhase::SaveState => ShutdownPhase::Complete,
            ShutdownPhase::Complete => ShutdownPhase::Complete,
        };
    }

    pub fn current_phase(&self) -> &ShutdownPhase {
        &self.phase
    }

    pub fn is_complete(&self) -> bool {
        self.phase == ShutdownPhase::Complete
    }

    pub fn steps_completed(&self) -> &[String] {
        &self.steps_completed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn advance_through_all_phases() {
        let mut sd = GracefulShutdown::new(5000);
        assert_eq!(sd.current_phase(), &ShutdownPhase::Running);
        sd.advance_phase();
        assert_eq!(sd.current_phase(), &ShutdownPhase::Initiated);
        sd.advance_phase();
        assert_eq!(sd.current_phase(), &ShutdownPhase::DrainConnections);
        sd.advance_phase();
        assert_eq!(sd.current_phase(), &ShutdownPhase::SaveState);
        sd.advance_phase();
        assert!(sd.is_complete());
    }

    #[test]
    fn initiate_transitions_from_running() {
        let mut sd = GracefulShutdown::new(1000);
        sd.initiate();
        assert_eq!(sd.current_phase(), &ShutdownPhase::Initiated);
    }

    #[test]
    fn complete_step_records_name() {
        let mut sd = GracefulShutdown::new(1000);
        sd.complete_step("flush_orders");
        sd.complete_step("close_db");
        assert_eq!(sd.steps_completed().len(), 2);
    }
}
