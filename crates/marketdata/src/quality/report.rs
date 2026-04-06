use serde::{Deserialize, Serialize};
use super::checker::QualityViolation;

// ── QualitySummary ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QualitySummary {
    pub total_items: u64,
    pub violations: u64,
    pub quality_score: f64,
}

impl QualitySummary {
    pub fn new(total_items: u64, violations: u64) -> Self {
        let quality_score = if total_items == 0 {
            1.0
        } else {
            1.0 - (violations as f64 / total_items as f64)
        };
        Self {
            total_items,
            violations,
            quality_score,
        }
    }
}

// ── QualityReport ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QualityReport {
    pub generated_ts_ms: i64,
    pub violations: Vec<QualityViolation>,
    pub summary: QualitySummary,
}

impl QualityReport {
    pub fn new(generated_ts_ms: i64, violations: Vec<QualityViolation>, total_items: u64) -> Self {
        let v_count = violations.len() as u64;
        Self {
            generated_ts_ms,
            summary: QualitySummary::new(total_items, v_count),
            violations,
        }
    }

    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    pub fn is_acceptable(&self, min_score: f64) -> bool {
        self.summary.quality_score >= min_score
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quality_score_calculation() {
        let summary = QualitySummary::new(100, 5);
        assert!((summary.quality_score - 0.95).abs() < 1e-9);
    }

    #[test]
    fn report_to_json() {
        let report = QualityReport::new(0, vec![], 10);
        let json = report.to_json().unwrap();
        assert!(json.contains("quality_score"));
    }

    #[test]
    fn is_acceptable() {
        let report = QualityReport::new(0, vec![], 100);
        assert!(report.is_acceptable(0.95));
        let v = QualityViolation {
            rule: "test".to_string(),
            ts_ms: 0,
            detail: "bad".to_string(),
        };
        let bad_report = QualityReport::new(0, vec![v; 20], 100);
        assert!(!bad_report.is_acceptable(0.95));
    }
}
