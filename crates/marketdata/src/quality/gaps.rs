use domain::NormalizedBar;
use crate::align::GapSpec;

// ── GapReport ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct GapReport {
    pub instrument: String,
    pub total_bars: u64,
    pub gaps: Vec<GapSpec>,
    pub coverage_pct: f64,
}

// ── DataGapDetector ───────────────────────────────────────────────────────────

pub struct DataGapDetector;

impl DataGapDetector {
    pub fn detect(
        instrument: &str,
        bars: &[NormalizedBar],
        expected_interval_ms: u64,
    ) -> GapReport {
        let mut gaps = Vec::new();
        for i in 1..bars.len() {
            let actual = (bars[i].ts_ms - bars[i - 1].ts_ms) as u64;
            if actual > expected_interval_ms {
                gaps.push(GapSpec {
                    instrument: instrument.to_string(),
                    start_ts_ms: bars[i - 1].ts_ms,
                    end_ts_ms: bars[i].ts_ms,
                    gap_ms: actual,
                });
            }
        }

        let (start, end) = if bars.is_empty() {
            (0, 0)
        } else {
            (bars[0].ts_ms, bars[bars.len() - 1].ts_ms)
        };

        let coverage = Self::coverage(bars, start, end, expected_interval_ms);

        GapReport {
            instrument: instrument.to_string(),
            total_bars: bars.len() as u64,
            gaps,
            coverage_pct: coverage,
        }
    }

    pub fn coverage(
        bars: &[NormalizedBar],
        start_ms: i64,
        end_ms: i64,
        interval_ms: u64,
    ) -> f64 {
        if interval_ms == 0 || start_ms >= end_ms {
            return 1.0;
        }
        let expected = ((end_ms - start_ms) as u64 / interval_ms + 1) as f64;
        if expected <= 0.0 {
            return 1.0;
        }
        (bars.len() as f64 / expected).min(1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bar(ts_ms: i64) -> NormalizedBar {
        NormalizedBar {
            ts_ms,
            open: 1.0,
            high: 1.0,
            low: 1.0,
            close: 1.0,
            volume: 1.0,
        }
    }

    #[test]
    fn detects_known_gap() {
        let bars = vec![bar(0), bar(1000), bar(5000), bar(6000)];
        let report = DataGapDetector::detect("BTC", &bars, 1000);
        assert_eq!(report.gaps.len(), 1);
        assert_eq!(report.gaps[0].start_ts_ms, 1000);
        assert_eq!(report.gaps[0].end_ts_ms, 5000);
    }

    #[test]
    fn coverage_full() {
        let bars: Vec<NormalizedBar> = (0..6).map(|i| bar(i * 1000)).collect();
        let cov = DataGapDetector::coverage(&bars, 0, 5000, 1000);
        assert!((cov - 1.0).abs() < 1e-9);
    }

    #[test]
    fn coverage_partial() {
        // 3 bars out of 6 expected
        let bars: Vec<NormalizedBar> = vec![bar(0), bar(2000), bar(4000)];
        let cov = DataGapDetector::coverage(&bars, 0, 5000, 1000);
        assert!(cov < 1.0);
    }
}
