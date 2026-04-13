// Risk metrics collection module
use domain::InstrumentId;
use std::collections::{HashMap, VecDeque};

// ── AlertType ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum AlertType {
    DrawdownExceeded,
    VarBreached,
    CircuitBreakerTriggered,
    DailyLossLimit,
    ConcentrationLimit,
}

// ── AlertSeverity ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum AlertSeverity {
    Info,
    Warning,
    Critical,
}

// ── RiskAlert ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct RiskAlert {
    pub id: String,
    pub instrument: Option<InstrumentId>,
    pub alert_type: AlertType,
    pub severity: AlertSeverity,
    pub message: String,
    pub ts_ms: i64,
    pub acknowledged: bool,
}

// ── AlertThreshold ────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct AlertThreshold {
    pub alert_type: AlertType,
    /// Threshold value; semantics depend on alert_type
    pub threshold: f64,
    pub severity: AlertSeverity,
}

// ── RiskTimeSeries ────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct RiskTimeSeries {
    pub entries: VecDeque<(i64, f64)>,
    pub max_len: usize,
}

impl RiskTimeSeries {
    pub fn new(max_len: usize) -> Self {
        Self {
            entries: VecDeque::new(),
            max_len,
        }
    }

    pub fn push(&mut self, ts_ms: i64, value: f64) {
        self.entries.push_back((ts_ms, value));
        if self.entries.len() > self.max_len {
            self.entries.pop_front();
        }
    }

    pub fn latest(&self) -> Option<f64> {
        self.entries.back().map(|&(_, v)| v)
    }

    pub fn values_since(&self, since_ms: i64) -> Vec<f64> {
        self.entries
            .iter()
            .filter(|&&(ts, _)| ts >= since_ms)
            .map(|&(_, v)| v)
            .collect()
    }
}

// ── RiskMetricsCollector ──────────────────────────────────────────────────

pub struct RiskMetricsCollector {
    pub var_series: HashMap<String, RiskTimeSeries>,
    pub pnl_series: RiskTimeSeries,
    pub exposure_series: RiskTimeSeries,
    pub alerts: Vec<RiskAlert>,
    pub thresholds: Vec<AlertThreshold>,
    max_series_len: usize,
    alert_counter: u64,
}

impl RiskMetricsCollector {
    pub fn new(thresholds: Vec<AlertThreshold>, max_series_len: usize) -> Self {
        Self {
            var_series: HashMap::new(),
            pnl_series: RiskTimeSeries::new(max_series_len),
            exposure_series: RiskTimeSeries::new(max_series_len),
            alerts: Vec::new(),
            thresholds,
            max_series_len,
            alert_counter: 0,
        }
    }

    pub fn record_var(&mut self, instrument: &str, ts_ms: i64, var: f64) {
        let series = self
            .var_series
            .entry(instrument.to_string())
            .or_insert_with(|| RiskTimeSeries::new(self.max_series_len));
        series.push(ts_ms, var);
    }

    pub fn record_pnl(&mut self, ts_ms: i64, pnl: f64) {
        self.pnl_series.push(ts_ms, pnl);
    }

    pub fn record_exposure(&mut self, ts_ms: i64, exposure: f64) {
        self.exposure_series.push(ts_ms, exposure);
    }

    /// Check all thresholds against latest values; generate and store new alerts
    pub fn check_thresholds(&mut self, ts_ms: i64) -> Vec<RiskAlert> {
        let mut new_alerts = Vec::new();

        for threshold in &self.thresholds {
            let value_opt: Option<f64> = match threshold.alert_type {
                AlertType::DailyLossLimit => self.pnl_series.latest(),
                AlertType::VarBreached => {
                    // Use the maximum VaR across all instruments
                    self.var_series
                        .values()
                        .filter_map(|s| s.latest())
                        .reduce(f64::max)
                }
                AlertType::DrawdownExceeded => self.pnl_series.latest(),
                AlertType::ConcentrationLimit => self.exposure_series.latest(),
                AlertType::CircuitBreakerTriggered => self.exposure_series.latest(),
            };

            if let Some(value) = value_opt {
                let triggered = match threshold.alert_type {
                    AlertType::DailyLossLimit => value < threshold.threshold,
                    AlertType::VarBreached => value > threshold.threshold,
                    AlertType::DrawdownExceeded => value < threshold.threshold,
                    AlertType::ConcentrationLimit => value > threshold.threshold,
                    AlertType::CircuitBreakerTriggered => value > threshold.threshold,
                };

                if triggered {
                    self.alert_counter += 1;
                    let alert = RiskAlert {
                        id: format!("alert-{}", self.alert_counter),
                        instrument: None,
                        alert_type: threshold.alert_type.clone(),
                        severity: threshold.severity.clone(),
                        message: format!(
                            "Threshold breached: value={:.4}, threshold={:.4}",
                            value, threshold.threshold
                        ),
                        ts_ms,
                        acknowledged: false,
                    };
                    self.alerts.push(alert.clone());
                    new_alerts.push(alert);
                }
            }
        }

        new_alerts
    }

    /// Acknowledge an alert by id; returns true if found and acknowledged
    pub fn acknowledge(&mut self, alert_id: &str) -> bool {
        for alert in &mut self.alerts {
            if alert.id == alert_id {
                alert.acknowledged = true;
                return true;
            }
        }
        false
    }

    /// Returns all unacknowledged alerts
    pub fn active_alerts(&self) -> Vec<&RiskAlert> {
        self.alerts.iter().filter(|a| !a.acknowledged).collect()
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_collector() -> RiskMetricsCollector {
        let thresholds = vec![
            AlertThreshold {
                alert_type: AlertType::DailyLossLimit,
                threshold: -1_000.0,
                severity: AlertSeverity::Critical,
            },
            AlertThreshold {
                alert_type: AlertType::VarBreached,
                threshold: 0.05,
                severity: AlertSeverity::Warning,
            },
        ];
        RiskMetricsCollector::new(thresholds, 100)
    }

    #[test]
    fn threshold_breach_generates_alert() {
        let mut collector = make_collector();
        collector.record_pnl(1_000, -2_000.0); // below -1000 threshold
        let alerts = collector.check_thresholds(1_000);
        assert!(!alerts.is_empty(), "Should generate alert on breach");
        assert_eq!(alerts[0].alert_type, AlertType::DailyLossLimit);
        assert_eq!(alerts[0].severity, AlertSeverity::Critical);
    }

    #[test]
    fn no_alert_when_within_threshold() {
        let mut collector = make_collector();
        collector.record_pnl(1_000, 500.0); // above -1000 threshold — no breach
        let alerts = collector.check_thresholds(1_000);
        // DailyLossLimit not triggered; VarBreached has no data
        let daily_loss_alerts: Vec<_> = alerts
            .iter()
            .filter(|a| a.alert_type == AlertType::DailyLossLimit)
            .collect();
        assert!(daily_loss_alerts.is_empty());
    }

    #[test]
    fn acknowledge_works() {
        let mut collector = make_collector();
        collector.record_pnl(1_000, -2_000.0);
        let alerts = collector.check_thresholds(1_000);
        assert!(!alerts.is_empty());

        let alert_id = alerts[0].id.clone();
        let found = collector.acknowledge(&alert_id);
        assert!(found);

        let active = collector.active_alerts();
        assert!(
            active.iter().all(|a| a.id != alert_id),
            "Acknowledged alert should not appear in active alerts"
        );
    }

    #[test]
    fn acknowledge_nonexistent_returns_false() {
        let mut collector = make_collector();
        assert!(!collector.acknowledge("nonexistent-id"));
    }

    #[test]
    fn time_series_rolls_over_max_len() {
        let mut series = RiskTimeSeries::new(5);
        for i in 0..10 {
            series.push(i * 1000, i as f64);
        }
        assert_eq!(series.entries.len(), 5, "Should be capped at max_len");
        // Last 5 values: 5, 6, 7, 8, 9
        assert_eq!(series.latest(), Some(9.0));
    }

    #[test]
    fn time_series_values_since() {
        let mut series = RiskTimeSeries::new(100);
        series.push(1_000, 1.0);
        series.push(2_000, 2.0);
        series.push(3_000, 3.0);
        series.push(4_000, 4.0);

        let values = series.values_since(2_500);
        assert_eq!(values, vec![3.0, 4.0]);
    }

    #[test]
    fn var_breach_triggers_alert() {
        let mut collector = make_collector();
        collector.record_var("CRYPTO:BTC-USD", 1_000, 0.08); // above 0.05 threshold
        let alerts = collector.check_thresholds(1_000);
        let var_alerts: Vec<_> = alerts
            .iter()
            .filter(|a| a.alert_type == AlertType::VarBreached)
            .collect();
        assert!(!var_alerts.is_empty(), "VaR breach should generate alert");
    }
}
