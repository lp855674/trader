#[derive(Debug, Clone, PartialEq)]
pub enum AlgoState {
    Running,
    Paused,
    Completed,
    Cancelled,
}

#[derive(Debug, Clone)]
pub struct TwapOrder {
    pub total_qty: f64,
    pub duration_ms: u64,
    pub n_slices: u32,
    pub slice_qty: f64,
    pub filled_slices: u32,
    pub state: AlgoState,
    pub start_ts_ms: i64,
    pub last_slice_ts_ms: i64,
}

impl TwapOrder {
    pub fn new(total_qty: f64, duration_ms: u64, n_slices: u32, start_ts_ms: i64) -> Self {
        let slice_qty = if n_slices > 0 {
            total_qty / n_slices as f64
        } else {
            total_qty
        };
        Self {
            total_qty,
            duration_ms,
            n_slices,
            slice_qty,
            filled_slices: 0,
            state: AlgoState::Running,
            start_ts_ms,
            last_slice_ts_ms: start_ts_ms,
        }
    }

    fn interval_ms(&self) -> i64 {
        if self.n_slices == 0 {
            return i64::MAX;
        }
        (self.duration_ms / self.n_slices as u64) as i64
    }

    /// Returns the timestamp when the next slice should be sent.
    pub fn next_slice_ts(&self) -> i64 {
        self.start_ts_ms + (self.filled_slices as i64 + 1) * self.interval_ms()
    }

    /// Returns Some(slice_qty) if it's time for the next slice.
    pub fn tick(&mut self, ts_ms: i64) -> Option<f64> {
        if self.state != AlgoState::Running {
            return None;
        }
        if self.filled_slices >= self.n_slices {
            self.state = AlgoState::Completed;
            return None;
        }
        if ts_ms >= self.next_slice_ts() {
            self.filled_slices += 1;
            self.last_slice_ts_ms = ts_ms;
            if self.filled_slices >= self.n_slices {
                self.state = AlgoState::Completed;
            }
            Some(self.slice_qty)
        } else {
            None
        }
    }

    pub fn pause(&mut self) {
        if self.state == AlgoState::Running {
            self.state = AlgoState::Paused;
        }
    }

    pub fn resume(&mut self) {
        if self.state == AlgoState::Paused {
            self.state = AlgoState::Running;
        }
    }

    pub fn cancel(&mut self) {
        self.state = AlgoState::Cancelled;
    }
}

#[derive(Debug, Clone)]
pub struct VwapOrder {
    pub total_qty: f64,
    pub duration_ms: u64,
    pub participation_rate: f64,
    pub filled_qty: f64,
    pub state: AlgoState,
    pub start_ts_ms: i64,
    pub last_ts_ms: i64,
}

impl VwapOrder {
    pub fn new(
        total_qty: f64,
        duration_ms: u64,
        participation_rate: f64,
        start_ts_ms: i64,
    ) -> Self {
        Self {
            total_qty,
            duration_ms,
            participation_rate,
            filled_qty: 0.0,
            state: AlgoState::Running,
            start_ts_ms,
            last_ts_ms: start_ts_ms,
        }
    }

    /// Returns qty proportional to volume in the bar.
    pub fn tick(&mut self, ts_ms: i64, bar_volume: f64) -> Option<f64> {
        if self.state != AlgoState::Running {
            return None;
        }
        let remaining = self.total_qty - self.filled_qty;
        if remaining <= 0.0 {
            self.state = AlgoState::Completed;
            return None;
        }
        let qty = (bar_volume * self.participation_rate).min(remaining);
        if qty > 0.0 {
            self.filled_qty += qty;
            self.last_ts_ms = ts_ms;
            if self.filled_qty >= self.total_qty {
                self.state = AlgoState::Completed;
            }
            Some(qty)
        } else {
            None
        }
    }

    pub fn pause(&mut self) {
        if self.state == AlgoState::Running {
            self.state = AlgoState::Paused;
        }
    }

    pub fn resume(&mut self) {
        if self.state == AlgoState::Paused {
            self.state = AlgoState::Running;
        }
    }

    pub fn cancel(&mut self) {
        self.state = AlgoState::Cancelled;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn twap_schedules_slices_at_correct_intervals() {
        // 3 slices over 3000ms → interval = 1000ms
        let mut twap = TwapOrder::new(300.0, 3000, 3, 0);
        assert_eq!(twap.slice_qty, 100.0);
        // Before interval
        assert!(twap.tick(500).is_none());
        // At 1000ms
        let s = twap.tick(1000).unwrap();
        assert!((s - 100.0).abs() < 1e-9);
        assert_eq!(twap.filled_slices, 1);
        // At 2000ms
        let s = twap.tick(2000).unwrap();
        assert!((s - 100.0).abs() < 1e-9);
        // At 3000ms — last slice
        let s = twap.tick(3000).unwrap();
        assert!((s - 100.0).abs() < 1e-9);
        assert_eq!(twap.state, AlgoState::Completed);
    }

    #[test]
    fn twap_pause_resume() {
        let mut twap = TwapOrder::new(100.0, 1000, 1, 0);
        twap.pause();
        assert!(twap.tick(1001).is_none()); // paused, no fill
        twap.resume();
        assert!(twap.tick(1001).is_some());
    }

    #[test]
    fn vwap_participates_proportionally() {
        let mut vwap = VwapOrder::new(100.0, 5000, 0.1, 0);
        // Bar volume 500 → 0.1 * 500 = 50
        let qty = vwap.tick(1000, 500.0).unwrap();
        assert!((qty - 50.0).abs() < 1e-9);
        // Second bar: remaining 50, bar 800 → min(0.1*800=80, 50) = 50
        let qty2 = vwap.tick(2000, 800.0).unwrap();
        assert!((qty2 - 50.0).abs() < 1e-9);
        assert_eq!(vwap.state, AlgoState::Completed);
    }
}
