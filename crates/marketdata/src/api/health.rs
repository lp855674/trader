use crate::monitor::metrics::DataMetricsSnapshot;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub enum DataHealthStatus {
    Ok,
    Degraded(String),
    Down(String),
}

#[derive(Debug, Clone, Serialize)]
pub struct DataHealthReport {
    pub overall: DataHealthStatus,
    pub ts_ms: i64,
    pub cache_ok: bool,
    pub source_ok: bool,
    pub quality_ok: bool,
}

pub struct DataHealthChecker;

impl DataHealthChecker {
    pub fn check(
        metrics: &DataMetricsSnapshot,
        min_quality: f64,
        min_cache_hit: f64,
    ) -> DataHealthReport {
        let quality_ok = metrics.quality_score >= min_quality;
        let cache_ok = metrics.cache_hit_rate >= min_cache_hit;
        let source_ok = true; // stub: no source health check here

        let overall = if !source_ok {
            DataHealthStatus::Down("Source unavailable".to_string())
        } else if !quality_ok || !cache_ok {
            let mut reasons = Vec::new();
            if !quality_ok {
                reasons.push(format!("quality={:.2}", metrics.quality_score));
            }
            if !cache_ok {
                reasons.push(format!("cache_hit={:.2}", metrics.cache_hit_rate));
            }
            DataHealthStatus::Degraded(reasons.join(", "))
        } else {
            DataHealthStatus::Ok
        };

        DataHealthReport {
            overall,
            ts_ms: metrics.ts_ms,
            cache_ok,
            source_ok,
            quality_ok,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::monitor::metrics::DataMetricsCollector;

    #[test]
    fn health_ok_when_all_good() {
        let mut collector = DataMetricsCollector::new();
        collector.record_query(100, true);
        collector.record_quality(0.95);
        let snap = collector.snapshot(0);
        let report = DataHealthChecker::check(&snap, 0.8, 0.7);
        assert!(matches!(report.overall, DataHealthStatus::Ok));
    }

    #[test]
    fn health_degraded_on_low_quality() {
        let mut collector = DataMetricsCollector::new();
        collector.record_query(100, true);
        collector.record_quality(0.3);
        let snap = collector.snapshot(0);
        let report = DataHealthChecker::check(&snap, 0.8, 0.5);
        assert!(!report.quality_ok);
        assert!(matches!(report.overall, DataHealthStatus::Degraded(_)));
    }
}
