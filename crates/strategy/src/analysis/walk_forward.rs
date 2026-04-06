use std::collections::HashMap;
use thiserror::Error;

/// Configuration for walk-forward analysis.
#[derive(Debug, Clone)]
pub struct WalkForwardConfig {
    /// Number of bars in training window.
    pub in_sample_size: usize,
    /// Number of bars in test window.
    pub out_of_sample_size: usize,
    /// How many bars to advance each iteration.
    pub step_size: usize,
    /// Minimum required windows.
    pub min_windows: usize,
}

/// A single walk-forward window with results.
#[derive(Debug, Clone)]
pub struct WalkForwardWindow {
    pub window_id: usize,
    pub in_sample_start: usize,
    pub in_sample_end: usize,
    pub out_of_sample_start: usize,
    pub out_of_sample_end: usize,
    pub best_params: HashMap<String, f64>,
    pub in_sample_score: f64,
    pub out_of_sample_score: f64,
    /// out_of_sample_score / in_sample_score, clamped to [-2.0, 2.0].
    pub efficiency: f64,
}

/// Aggregated walk-forward analysis result.
#[derive(Debug, Clone)]
pub struct WalkForwardResult {
    pub windows: Vec<WalkForwardWindow>,
    /// Mean of window efficiencies.
    pub avg_efficiency: f64,
    /// Fraction of windows where out_of_sample_score > 0.
    pub consistency: f64,
    pub total_out_of_sample_score: f64,
    /// True if last 3 windows avg efficiency < first 3 windows avg * 0.5.
    pub drift_detected: bool,
}

/// Errors from walk-forward analysis.
#[derive(Debug, Error)]
pub enum WalkForwardError {
    #[error("Insufficient data: required {required}, available {available}")]
    InsufficientData { required: usize, available: usize },
    #[error("Too few windows: minimum {min}, found {found}")]
    TooFewWindows { min: usize, found: usize },
}

/// Performs walk-forward analysis over a time series.
pub struct WalkForwardAnalyzer {
    pub config: WalkForwardConfig,
}

impl WalkForwardAnalyzer {
    pub fn new(config: WalkForwardConfig) -> Self {
        Self { config }
    }

    /// Returns list of (in_start, in_end, oos_start, oos_end) index tuples.
    pub fn windows(&self, total_bars: usize) -> Result<Vec<(usize, usize, usize, usize)>, WalkForwardError> {
        let cfg = &self.config;
        let min_required = cfg.in_sample_size + cfg.out_of_sample_size;
        if total_bars < min_required {
            return Err(WalkForwardError::InsufficientData {
                required: min_required,
                available: total_bars,
            });
        }

        let step = cfg.step_size.max(1);
        let mut windows = Vec::new();
        let mut in_start = 0;

        loop {
            let in_end = in_start + cfg.in_sample_size;
            let oos_start = in_end;
            let oos_end = oos_start + cfg.out_of_sample_size;
            if oos_end > total_bars { break; }
            windows.push((in_start, in_end, oos_start, oos_end));
            in_start += step;
        }

        if windows.len() < cfg.min_windows {
            return Err(WalkForwardError::TooFewWindows {
                min: cfg.min_windows,
                found: windows.len(),
            });
        }

        Ok(windows)
    }

    /// Run walk-forward analysis.
    pub fn run<F, G>(
        &self,
        total_bars: usize,
        mut optimizer: F,
        mut evaluator: G,
    ) -> Result<WalkForwardResult, WalkForwardError>
    where
        F: FnMut(usize, usize) -> (HashMap<String, f64>, f64),
        G: FnMut(usize, usize, &HashMap<String, f64>) -> f64,
    {
        let window_indices = self.windows(total_bars)?;
        let mut windows: Vec<WalkForwardWindow> = window_indices
            .into_iter()
            .enumerate()
            .map(|(id, (in_start, in_end, oos_start, oos_end))| {
                let (best_params, in_sample_score) = optimizer(in_start, in_end);
                let out_of_sample_score = evaluator(oos_start, oos_end, &best_params);
                let efficiency = compute_efficiency(in_sample_score, out_of_sample_score);
                WalkForwardWindow {
                    window_id: id,
                    in_sample_start: in_start,
                    in_sample_end: in_end,
                    out_of_sample_start: oos_start,
                    out_of_sample_end: oos_end,
                    best_params,
                    in_sample_score,
                    out_of_sample_score,
                    efficiency,
                }
            })
            .collect();

        // Sort by window_id to ensure order
        windows.sort_by_key(|w| w.window_id);

        let n = windows.len();
        let avg_efficiency = if n == 0 { 0.0 } else {
            windows.iter().map(|w| w.efficiency).sum::<f64>() / n as f64
        };

        let consistency = if n == 0 { 0.0 } else {
            windows.iter().filter(|w| w.out_of_sample_score > 0.0).count() as f64 / n as f64
        };

        let total_out_of_sample_score = windows.iter().map(|w| w.out_of_sample_score).sum();

        let drift_detected = detect_drift(&windows);

        Ok(WalkForwardResult {
            windows,
            avg_efficiency,
            consistency,
            total_out_of_sample_score,
            drift_detected,
        })
    }
}

fn compute_efficiency(in_sample_score: f64, out_of_sample_score: f64) -> f64 {
    if in_sample_score.abs() < 1e-10 {
        return 0.0;
    }
    let ratio = out_of_sample_score / in_sample_score;
    ratio.clamp(-2.0, 2.0)
}

fn detect_drift(windows: &[WalkForwardWindow]) -> bool {
    if windows.len() < 6 { return false; }
    let first3_avg = windows[..3].iter().map(|w| w.efficiency).sum::<f64>() / 3.0;
    let last3_avg = windows[windows.len() - 3..].iter().map(|w| w.efficiency).sum::<f64>() / 3.0;
    last3_avg < first3_avg * 0.5
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_config(in_sample: usize, oos: usize, step: usize) -> WalkForwardConfig {
        WalkForwardConfig {
            in_sample_size: in_sample,
            out_of_sample_size: oos,
            step_size: step,
            min_windows: 1,
        }
    }

    #[test]
    fn windows_generates_correct_ranges() {
        let config = make_config(100, 20, 20);
        let analyzer = WalkForwardAnalyzer::new(config);
        let windows = analyzer.windows(200).unwrap();
        assert!(!windows.is_empty());
        // First window: in=[0,100), oos=[100,120)
        assert_eq!(windows[0], (0, 100, 100, 120));
        // Second: in=[20,120), oos=[120,140)
        assert_eq!(windows[1], (20, 120, 120, 140));
    }

    #[test]
    fn windows_insufficient_data_error() {
        let config = make_config(100, 20, 20);
        let analyzer = WalkForwardAnalyzer::new(config);
        let result = analyzer.windows(50);
        assert!(matches!(result, Err(WalkForwardError::InsufficientData { .. })));
    }

    #[test]
    fn windows_too_few_windows_error() {
        let config = WalkForwardConfig {
            in_sample_size: 100,
            out_of_sample_size: 20,
            step_size: 20,
            min_windows: 10,
        };
        let analyzer = WalkForwardAnalyzer::new(config);
        // Only 2 windows fit in 200 bars
        let result = analyzer.windows(140);
        assert!(matches!(result, Err(WalkForwardError::TooFewWindows { .. })));
    }

    #[test]
    fn efficiency_calculated_correctly() {
        assert!((compute_efficiency(2.0, 1.0) - 0.5).abs() < 1e-10);
        assert!((compute_efficiency(1.0, 2.0) - 2.0).abs() < 1e-10); // clamped at 2.0
        assert!((compute_efficiency(1.0, -5.0) - (-2.0)).abs() < 1e-10); // clamped at -2.0
        assert!((compute_efficiency(0.0, 1.0)).abs() < 1e-10); // zero in_sample
    }

    #[test]
    fn run_produces_correct_results() {
        let config = make_config(50, 10, 10);
        let analyzer = WalkForwardAnalyzer::new(config);
        let result = analyzer.run(
            100,
            |_start, _end| {
                let mut params = HashMap::new();
                params.insert("x".to_string(), 1.0);
                (params, 1.0)
            },
            |_start, _end, _params| 0.5,
        ).unwrap();
        assert!(!result.windows.is_empty());
        // Each efficiency = 0.5/1.0 = 0.5
        for w in &result.windows {
            assert!((w.efficiency - 0.5).abs() < 1e-10);
        }
        assert!((result.avg_efficiency - 0.5).abs() < 1e-10);
    }

    #[test]
    fn drift_detection_on_declining_efficiency() {
        // Create windows with declining efficiency
        let make_window = |id: usize, oos: f64| WalkForwardWindow {
            window_id: id,
            in_sample_start: 0,
            in_sample_end: 100,
            out_of_sample_start: 100,
            out_of_sample_end: 110,
            best_params: HashMap::new(),
            in_sample_score: 1.0,
            out_of_sample_score: oos,
            efficiency: oos, // simplified: efficiency = oos_score
        };
        // First 3: efficiency ~1.0, last 3: efficiency ~0.1
        let windows = vec![
            make_window(0, 1.0),
            make_window(1, 1.0),
            make_window(2, 1.0),
            make_window(3, 0.5),
            make_window(4, 0.1),
            make_window(5, 0.1),
        ];
        assert!(detect_drift(&windows)); // last3 avg (0.233) < first3 avg (1.0) * 0.5
    }

    #[test]
    fn no_drift_with_stable_efficiency() {
        let make_window = |id: usize| WalkForwardWindow {
            window_id: id,
            in_sample_start: 0,
            in_sample_end: 100,
            out_of_sample_start: 100,
            out_of_sample_end: 110,
            best_params: HashMap::new(),
            in_sample_score: 1.0,
            out_of_sample_score: 0.8,
            efficiency: 0.8,
        };
        let windows: Vec<_> = (0..6).map(make_window).collect();
        assert!(!detect_drift(&windows));
    }

    #[test]
    fn consistency_fraction_correct() {
        let config = make_config(50, 10, 10);
        let analyzer = WalkForwardAnalyzer::new(config);
        let mut call_n = 0usize;
        let result = analyzer.run(
            100,
            |_, _| (HashMap::new(), 1.0),
            |_, _, _| {
                call_n += 1;
                if call_n % 2 == 0 { 1.0 } else { -1.0 }
            },
        ).unwrap();
        // Half positive, half negative
        assert!(result.consistency > 0.0 && result.consistency < 1.0);
    }
}
