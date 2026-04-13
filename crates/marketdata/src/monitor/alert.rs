use crate::monitor::metrics::DataMetricsSnapshot;
use serde::Serialize;
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize)]
pub enum DataAlertType {
    HighLatency,
    LowCacheHitRate,
    DataQualityDegraded,
    IngestionStalled,
    StorageFull,
}

#[derive(Debug, Clone, Serialize)]
pub struct DataAlert {
    pub id: u64,
    pub alert_type: DataAlertType,
    pub message: String,
    pub value: f64,
    pub ts_ms: i64,
}

pub struct DataAlertManager {
    thresholds: HashMap<String, f64>,
    alert_counter: u64,
    active_alerts: Vec<DataAlert>,
}

impl DataAlertManager {
    pub fn new(thresholds: HashMap<String, f64>) -> Self {
        Self {
            thresholds,
            alert_counter: 0,
            active_alerts: Vec::new(),
        }
    }

    pub fn check(&mut self, snapshot: &DataMetricsSnapshot, ts_ms: i64) -> Vec<DataAlert> {
        let mut new_alerts = Vec::new();

        if let Some(&max_latency) = self.thresholds.get("max_latency_us") {
            if snapshot.avg_query_latency_us > max_latency {
                self.alert_counter += 1;
                new_alerts.push(DataAlert {
                    id: self.alert_counter,
                    alert_type: DataAlertType::HighLatency,
                    message: format!(
                        "Avg latency {:.0}us exceeds threshold {:.0}us",
                        snapshot.avg_query_latency_us, max_latency
                    ),
                    value: snapshot.avg_query_latency_us,
                    ts_ms,
                });
            }
        }

        if let Some(&min_hit_rate) = self.thresholds.get("min_cache_hit_rate") {
            if snapshot.cache_hit_rate < min_hit_rate {
                self.alert_counter += 1;
                new_alerts.push(DataAlert {
                    id: self.alert_counter,
                    alert_type: DataAlertType::LowCacheHitRate,
                    message: format!(
                        "Cache hit rate {:.2} below threshold {:.2}",
                        snapshot.cache_hit_rate, min_hit_rate
                    ),
                    value: snapshot.cache_hit_rate,
                    ts_ms,
                });
            }
        }

        if let Some(&min_quality) = self.thresholds.get("min_quality_score") {
            if snapshot.quality_score < min_quality {
                self.alert_counter += 1;
                new_alerts.push(DataAlert {
                    id: self.alert_counter,
                    alert_type: DataAlertType::DataQualityDegraded,
                    message: format!(
                        "Quality score {:.2} below threshold {:.2}",
                        snapshot.quality_score, min_quality
                    ),
                    value: snapshot.quality_score,
                    ts_ms,
                });
            }
        }

        self.active_alerts.extend(new_alerts.clone());
        new_alerts
    }

    pub fn active_count(&self) -> usize {
        self.active_alerts.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::monitor::metrics::DataMetricsCollector;

    #[test]
    fn alert_triggered_on_low_cache_hit_rate() {
        let mut thresholds = HashMap::new();
        thresholds.insert("min_cache_hit_rate".to_string(), 0.8);
        let mut manager = DataAlertManager::new(thresholds);

        let mut collector = DataMetricsCollector::new();
        collector.record_query(100, false); // miss
        let snap = collector.snapshot(1000);

        let alerts = manager.check(&snap, 1000);
        assert!(!alerts.is_empty());
        assert!(matches!(
            alerts[0].alert_type,
            DataAlertType::LowCacheHitRate
        ));
    }
}
