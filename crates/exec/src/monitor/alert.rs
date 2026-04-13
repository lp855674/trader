use serde::Serialize;

use super::metrics::MetricsSnapshot;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum ExecAlertType {
    HighRejectionRate,
    SlowFills,
    QueueBacklog,
    AbnormalSlippage,
    PositionLimit,
}

#[derive(Debug, Clone)]
pub struct ExecAlertThreshold {
    pub alert_type: ExecAlertType,
    pub threshold: f64,
    pub window_ms: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExecAlert {
    pub id: String,
    pub alert_type: ExecAlertType,
    pub message: String,
    pub value: f64,
    pub threshold: f64,
    pub ts_ms: i64,
}

pub struct ExecAlertManager {
    pub thresholds: Vec<ExecAlertThreshold>,
    pub fired: Vec<ExecAlert>,
    pub next_id: u64,
}

impl ExecAlertManager {
    pub fn new(thresholds: Vec<ExecAlertThreshold>) -> Self {
        Self {
            thresholds,
            fired: Vec::new(),
            next_id: 1,
        }
    }

    pub fn check(&mut self, snapshot: &MetricsSnapshot, ts_ms: i64) -> Vec<ExecAlert> {
        let mut new_alerts = Vec::new();
        for thresh in &self.thresholds {
            let (value, triggered) = match thresh.alert_type {
                ExecAlertType::HighRejectionRate => {
                    let v = snapshot.rejection_rate;
                    (v, v > thresh.threshold)
                }
                ExecAlertType::QueueBacklog => {
                    let v = snapshot.queue_depth as f64;
                    (v, v > thresh.threshold)
                }
                ExecAlertType::SlowFills => {
                    let v = snapshot.fill_latency.p99_us;
                    (v, v > thresh.threshold)
                }
                ExecAlertType::AbnormalSlippage => {
                    // placeholder — no slippage in snapshot yet
                    (0.0, false)
                }
                ExecAlertType::PositionLimit => {
                    // placeholder — no position data in snapshot
                    (0.0, false)
                }
            };
            if triggered {
                let alert = ExecAlert {
                    id: format!("alert-{}", self.next_id),
                    alert_type: thresh.alert_type.clone(),
                    message: format!(
                        "{:?} triggered: value={:.4} threshold={:.4}",
                        thresh.alert_type, value, thresh.threshold
                    ),
                    value,
                    threshold: thresh.threshold,
                    ts_ms,
                };
                self.next_id += 1;
                self.fired.push(alert.clone());
                new_alerts.push(alert);
            }
        }
        new_alerts
    }

    pub fn clear(&mut self) {
        self.fired.clear();
    }

    /// Anomaly detection: z-score based detection on a sliding window of values.
    /// Returns true if the value is anomalous (|z| > threshold).
    pub fn detect_anomaly(history: &[f64], new_value: f64, z_threshold: f64) -> bool {
        let n = history.len();
        if n < 2 {
            return false;
        }
        let mean = history.iter().sum::<f64>() / n as f64;
        let variance = history.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / (n - 1) as f64;
        let std = variance.sqrt();
        if std < 1e-12 {
            return false;
        }
        ((new_value - mean) / std).abs() > z_threshold
    }

    /// Group fired alerts by type — returns map of alert type to list of alerts.
    pub fn grouped_alerts(&self) -> std::collections::HashMap<String, Vec<&ExecAlert>> {
        let mut groups: std::collections::HashMap<String, Vec<&ExecAlert>> =
            std::collections::HashMap::new();
        for alert in &self.fired {
            let key = format!("{:?}", alert.alert_type);
            groups.entry(key).or_default().push(alert);
        }
        groups
    }

    /// Multi-channel delivery: serialize alerts to JSON for downstream consumers.
    pub fn alerts_as_json(&self) -> String {
        serde_json::to_string(&self.fired).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::monitor::metrics::ExecutionMetrics;

    #[test]
    fn high_rejection_rate_fires_alert() {
        let thresholds = vec![ExecAlertThreshold {
            alert_type: ExecAlertType::HighRejectionRate,
            threshold: 0.1,
            window_ms: 1000,
        }];
        let mut mgr = ExecAlertManager::new(thresholds);
        let mut metrics = ExecutionMetrics::new();
        // 5 submitted, 3 rejected → 60% rejection rate
        for _ in 0..5 {
            metrics.record_submit(100);
        }
        for _ in 0..3 {
            metrics.record_rejection();
        }
        let snap = metrics.snapshot(1000);
        let alerts = mgr.check(&snap, 1000);
        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].alert_type, ExecAlertType::HighRejectionRate);
    }

    #[test]
    fn low_fill_rate_no_queue_alert() {
        // queue backlog fires when depth > threshold
        let thresholds = vec![ExecAlertThreshold {
            alert_type: ExecAlertType::QueueBacklog,
            threshold: 10.0,
            window_ms: 1000,
        }];
        let mut mgr = ExecAlertManager::new(thresholds);
        let mut metrics = ExecutionMetrics::new();
        metrics.set_queue_depth(5);
        let snap = metrics.snapshot(1000);
        let alerts = mgr.check(&snap, 1000);
        assert!(alerts.is_empty());

        metrics.set_queue_depth(20);
        let snap2 = metrics.snapshot(2000);
        let alerts2 = mgr.check(&snap2, 2000);
        assert_eq!(alerts2.len(), 1);
        assert_eq!(alerts2[0].alert_type, ExecAlertType::QueueBacklog);
    }

    #[test]
    fn clear_removes_fired_alerts() {
        let thresholds = vec![ExecAlertThreshold {
            alert_type: ExecAlertType::HighRejectionRate,
            threshold: 0.0,
            window_ms: 1000,
        }];
        let mut mgr = ExecAlertManager::new(thresholds);
        let mut metrics = ExecutionMetrics::new();
        metrics.record_submit(100);
        metrics.record_rejection();
        let snap = metrics.snapshot(1000);
        mgr.check(&snap, 1000);
        assert!(!mgr.fired.is_empty());
        mgr.clear();
        assert!(mgr.fired.is_empty());
    }
}
