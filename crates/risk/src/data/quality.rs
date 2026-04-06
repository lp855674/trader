// Data quality checks: anomaly detection, staleness, gaps

use std::collections::{HashMap, VecDeque};

// ── QualityIssue ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum QualityIssue {
    MissingData { instrument: String, ts_ms: i64 },
    StaleData { instrument: String, last_update_ms: i64, current_ms: i64 },
    AnomalousPrice { instrument: String, price: f64, expected_range: (f64, f64) },
    ZeroVolume { instrument: String, ts_ms: i64 },
    GapDetected { instrument: String, gap_ms: u64, expected_ms: u64 },
}

// ── QualityReport ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct QualityReport {
    pub ts_ms: i64,
    pub issues: Vec<QualityIssue>,
    /// Fraction of expected data points received (0.0 – 1.0)
    pub data_completeness: f64,
}

impl QualityReport {
    pub fn has_critical_issues(&self) -> bool {
        self.issues.iter().any(|i| {
            matches!(
                i,
                QualityIssue::AnomalousPrice { .. } | QualityIssue::MissingData { .. }
            )
        })
    }
}

// ── AnomalyDetector ───────────────────────────────────────────────────────────

pub struct AnomalyDetector {
    pub price_history: HashMap<String, VecDeque<f64>>,
    pub window: usize,
    pub z_score_threshold: f64,
}

impl AnomalyDetector {
    pub fn new(window: usize, z_score_threshold: f64) -> Self {
        Self {
            price_history: HashMap::new(),
            window,
            z_score_threshold,
        }
    }

    /// Push a price; returns a QualityIssue if anomalous.
    pub fn push(&mut self, instrument: &str, price: f64) -> Option<QualityIssue> {
        let deque = self
            .price_history
            .entry(instrument.to_string())
            .or_insert_with(VecDeque::new);

        let z = if deque.len() >= 2 {
            let prices: Vec<f64> = deque.iter().cloned().collect();
            let mean = prices.iter().sum::<f64>() / prices.len() as f64;
            let variance = prices.iter().map(|p| (p - mean).powi(2)).sum::<f64>()
                / prices.len() as f64;
            let std = variance.sqrt();
            if std < 1e-12 {
                0.0
            } else {
                (price - mean) / std
            }
        } else {
            0.0
        };

        deque.push_back(price);
        if deque.len() > self.window {
            deque.pop_front();
        }

        if z.abs() > self.z_score_threshold {
            // Compute expected range from history (±2 std of current window)
            let prices: Vec<f64> = deque.iter().cloned().collect();
            let mean = prices.iter().sum::<f64>() / prices.len() as f64;
            let variance =
                prices.iter().map(|p| (p - mean).powi(2)).sum::<f64>() / prices.len() as f64;
            let std = variance.sqrt();
            let expected_range = (mean - 2.0 * std, mean + 2.0 * std);
            Some(QualityIssue::AnomalousPrice {
                instrument: instrument.to_string(),
                price,
                expected_range,
            })
        } else {
            None
        }
    }

    /// Returns z-score for a price against instrument history; 0 if insufficient.
    pub fn z_score(&self, instrument: &str, price: f64) -> f64 {
        if let Some(deque) = self.price_history.get(instrument) {
            if deque.len() < 2 {
                return 0.0;
            }
            let prices: Vec<f64> = deque.iter().cloned().collect();
            let mean = prices.iter().sum::<f64>() / prices.len() as f64;
            let variance = prices.iter().map(|p| (p - mean).powi(2)).sum::<f64>()
                / prices.len() as f64;
            let std = variance.sqrt();
            if std < 1e-12 {
                0.0
            } else {
                (price - mean) / std
            }
        } else {
            0.0
        }
    }
}

// ── DataQualityChecker ────────────────────────────────────────────────────────

pub struct DataQualityChecker {
    pub anomaly_detector: AnomalyDetector,
    pub max_stale_ms: u64,
    pub expected_interval_ms: u64,
    pub last_seen: HashMap<String, i64>,
}

impl DataQualityChecker {
    pub fn new(
        window: usize,
        z_threshold: f64,
        max_stale_ms: u64,
        expected_interval_ms: u64,
    ) -> Self {
        Self {
            anomaly_detector: AnomalyDetector::new(window, z_threshold),
            max_stale_ms,
            expected_interval_ms,
            last_seen: HashMap::new(),
        }
    }

    pub fn check_price(
        &mut self,
        instrument: &str,
        price: f64,
        ts_ms: i64,
    ) -> Vec<QualityIssue> {
        let mut issues = Vec::new();

        // Check gap from last update
        if let Some(&last) = self.last_seen.get(instrument) {
            let gap_ms = (ts_ms - last).unsigned_abs();
            if gap_ms > self.expected_interval_ms * 2 {
                issues.push(QualityIssue::GapDetected {
                    instrument: instrument.to_string(),
                    gap_ms,
                    expected_ms: self.expected_interval_ms,
                });
            }
        }

        self.last_seen.insert(instrument.to_string(), ts_ms);

        // Anomaly check
        if let Some(issue) = self.anomaly_detector.push(instrument, price) {
            issues.push(issue);
        }

        issues
    }

    pub fn check_volume(
        &mut self,
        instrument: &str,
        volume: f64,
        ts_ms: i64,
    ) -> Vec<QualityIssue> {
        if volume == 0.0 {
            vec![QualityIssue::ZeroVolume {
                instrument: instrument.to_string(),
                ts_ms,
            }]
        } else {
            vec![]
        }
    }

    /// Check all tracked instruments for staleness at current time.
    pub fn check_staleness(&self, ts_ms: i64) -> Vec<QualityIssue> {
        self.last_seen
            .iter()
            .filter_map(|(instrument, &last_update_ms)| {
                let age = (ts_ms - last_update_ms).unsigned_abs();
                if age > self.max_stale_ms {
                    Some(QualityIssue::StaleData {
                        instrument: instrument.clone(),
                        last_update_ms,
                        current_ms: ts_ms,
                    })
                } else {
                    None
                }
            })
            .collect()
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn anomaly_detection_flags_large_z_score() {
        let mut checker = DataQualityChecker::new(50, 3.0, 60_000, 1_000);
        let instrument = "BTC-USD";

        // Feed 40 normal prices around 50_000
        for i in 0..40 {
            let price = 50_000.0 + (i as f64 % 10.0) * 10.0;
            checker.check_price(instrument, price, i * 1000);
        }

        // Now send a 10σ outlier
        let issues = checker.check_price(instrument, 50_000.0 * 2.0, 40_000);
        assert!(
            !issues.is_empty(),
            "Anomalous price should generate issues"
        );
        assert!(issues.iter().any(|i| matches!(i, QualityIssue::AnomalousPrice { .. })));
    }

    #[test]
    fn stale_data_detected() {
        let mut checker = DataQualityChecker::new(10, 3.0, 5_000, 1_000);
        checker.check_price("ETH-USD", 2_000.0, 0);
        let issues = checker.check_staleness(10_000); // 10s later > 5s max_stale
        assert!(!issues.is_empty(), "Should detect stale data");
        assert!(issues.iter().any(|i| matches!(i, QualityIssue::StaleData { .. })));
    }

    #[test]
    fn gap_detection_works() {
        let mut checker = DataQualityChecker::new(10, 3.0, 60_000, 1_000);
        checker.check_price("SOL-USD", 100.0, 0);
        let issues = checker.check_price("SOL-USD", 101.0, 10_000); // 10s gap > 2*1s expected
        assert!(
            issues.iter().any(|i| matches!(i, QualityIssue::GapDetected { .. })),
            "Should detect data gap"
        );
    }

    #[test]
    fn zero_volume_flagged() {
        let mut checker = DataQualityChecker::new(10, 3.0, 60_000, 1_000);
        let issues = checker.check_volume("BTC-USD", 0.0, 1_000);
        assert!(!issues.is_empty());
        assert!(issues.iter().any(|i| matches!(i, QualityIssue::ZeroVolume { .. })));
    }

    #[test]
    fn quality_report_has_critical_issues_for_anomaly() {
        let report = QualityReport {
            ts_ms: 0,
            issues: vec![QualityIssue::AnomalousPrice {
                instrument: "BTC-USD".into(),
                price: 100.0,
                expected_range: (45_000.0, 55_000.0),
            }],
            data_completeness: 0.9,
        };
        assert!(report.has_critical_issues());
    }

    #[test]
    fn z_score_returns_zero_for_insufficient_history() {
        let detector = AnomalyDetector::new(50, 3.0);
        assert_eq!(detector.z_score("BTC-USD", 50_000.0), 0.0);
    }
}
