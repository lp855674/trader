// Alert manager: deduplication, escalation, multi-channel dispatch

use crate::risk::metrics::AlertSeverity;
use std::collections::HashMap;

// ── AlertChannel ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum AlertChannel {
    Log,
    Webhook { url: String },
    InMemory,
}

// ── AlertMessage ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct AlertMessage {
    pub id: String,
    pub severity: AlertSeverity,
    pub title: String,
    pub body: String,
    pub ts_ms: i64,
    pub tags: Vec<String>,
}

// ── AlertDeduplication ────────────────────────────────────────────────────────

pub struct AlertDeduplication {
    /// Time window in ms during which same-title alerts are suppressed
    pub window_ms: u64,
    /// title → last_sent_ts_ms
    pub recent: HashMap<String, i64>,
}

impl AlertDeduplication {
    pub fn new(window_ms: u64) -> Self {
        Self {
            window_ms,
            recent: HashMap::new(),
        }
    }

    /// Returns true if this message should be sent (not a duplicate in window).
    pub fn should_send(&mut self, msg: &AlertMessage, ts_ms: i64) -> bool {
        if let Some(&last_sent) = self.recent.get(&msg.title) {
            let elapsed = (ts_ms - last_sent).unsigned_abs();
            if elapsed < self.window_ms {
                return false;
            }
        }
        self.recent.insert(msg.title.clone(), ts_ms);
        true
    }
}

// ── AlertEscalation ───────────────────────────────────────────────────────────

pub struct AlertEscalation {
    pub severity_threshold: AlertSeverity,
    pub escalate_after_ms: u64,
    /// (alert, sent_ts)
    pub unack_alerts: Vec<(AlertMessage, i64)>,
}

impl AlertEscalation {
    pub fn new(severity_threshold: AlertSeverity, escalate_after_ms: u64) -> Self {
        Self {
            severity_threshold,
            escalate_after_ms,
            unack_alerts: Vec::new(),
        }
    }

    /// Returns alerts that have been unacknowledged for > escalate_after_ms.
    /// Upgrades their severity to Critical.
    pub fn check_escalation(&mut self, ts_ms: i64) -> Vec<AlertMessage> {
        let mut escalated = Vec::new();
        for (alert, sent_ts) in &mut self.unack_alerts {
            let elapsed = (ts_ms - *sent_ts).unsigned_abs();
            if elapsed >= self.escalate_after_ms {
                let mut escalated_alert = alert.clone();
                escalated_alert.severity = AlertSeverity::Critical;
                escalated.push(escalated_alert);
            }
        }
        escalated
    }

    pub fn add(&mut self, msg: AlertMessage, ts_ms: i64) {
        self.unack_alerts.push((msg, ts_ms));
    }

    pub fn acknowledge(&mut self, alert_id: &str) {
        self.unack_alerts.retain(|(a, _)| a.id != alert_id);
    }
}

// ── AlertManager ─────────────────────────────────────────────────────────────

pub struct AlertManager {
    pub channels: Vec<AlertChannel>,
    pub dedup: AlertDeduplication,
    pub escalation: AlertEscalation,
    /// Sent messages (InMemory channel + audit log)
    pub sent_log: Vec<AlertMessage>,
    /// alert_id → ack_ts_ms
    pub acknowledged: HashMap<String, i64>,
}

impl AlertManager {
    pub fn new(channels: Vec<AlertChannel>, dedup_window_ms: u64, escalate_after_ms: u64) -> Self {
        Self {
            channels,
            dedup: AlertDeduplication::new(dedup_window_ms),
            escalation: AlertEscalation::new(AlertSeverity::Warning, escalate_after_ms),
            sent_log: Vec::new(),
            acknowledged: HashMap::new(),
        }
    }

    pub fn send(&mut self, msg: AlertMessage, ts_ms: i64) {
        if !self.dedup.should_send(&msg, ts_ms) {
            return;
        }

        // Collect channel types first to avoid borrow conflicts
        let channel_types: Vec<AlertChannel> = self.channels.clone();
        for channel in &channel_types {
            match channel {
                AlertChannel::Log => {
                    tracing::warn!(
                        id = %msg.id,
                        title = %msg.title,
                        body = %msg.body,
                        "Risk alert"
                    );
                }
                AlertChannel::Webhook { url } => {
                    tracing::info!(url = %url, title = %msg.title, "Would POST alert to webhook");
                }
                AlertChannel::InMemory => {
                    self.sent_log.push(msg.clone());
                }
            }
        }

        self.escalation.add(msg, ts_ms);
    }

    pub fn acknowledge(&mut self, alert_id: &str, ts_ms: i64) -> bool {
        if self.sent_log.iter().any(|m| m.id == alert_id)
            || self
                .escalation
                .unack_alerts
                .iter()
                .any(|(m, _)| m.id == alert_id)
        {
            self.acknowledged.insert(alert_id.to_string(), ts_ms);
            self.escalation.acknowledge(alert_id);
            return true;
        }
        false
    }

    /// Check for escalated alerts, dispatch them, return the escalated list.
    pub fn tick(&mut self, ts_ms: i64) -> Vec<AlertMessage> {
        let escalated = self.escalation.check_escalation(ts_ms);
        let channel_types: Vec<AlertChannel> = self.channels.clone();
        for msg in &escalated {
            for channel in &channel_types {
                match channel {
                    AlertChannel::Log => {
                        tracing::warn!(
                            id = %msg.id,
                            title = %msg.title,
                            "ESCALATED alert (Critical)"
                        );
                    }
                    AlertChannel::Webhook { url } => {
                        tracing::warn!(url = %url, title = %msg.title, "Would POST escalated alert");
                    }
                    AlertChannel::InMemory => {
                        self.sent_log.push(msg.clone());
                    }
                }
            }
        }
        // Remove escalated from unack list to prevent infinite escalation
        if !escalated.is_empty() {
            let ids: Vec<String> = escalated.iter().map(|m| m.id.clone()).collect();
            self.escalation
                .unack_alerts
                .retain(|(a, _)| !ids.contains(&a.id));
        }
        escalated
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_msg(id: &str, title: &str) -> AlertMessage {
        AlertMessage {
            id: id.to_string(),
            severity: AlertSeverity::Warning,
            title: title.to_string(),
            body: "test body".to_string(),
            ts_ms: 0,
            tags: vec![],
        }
    }

    #[test]
    fn dedup_prevents_double_send_within_window() {
        let mut mgr = AlertManager::new(
            vec![AlertChannel::InMemory],
            5_000, // 5s window
            60_000,
        );
        let msg1 = make_msg("a1", "VaR breach");
        let msg2 = make_msg("a2", "VaR breach"); // same title

        mgr.send(msg1, 1_000);
        mgr.send(msg2, 2_000); // within 5s window — should be suppressed

        assert_eq!(
            mgr.sent_log.len(),
            1,
            "Second alert with same title within window should be suppressed"
        );
    }

    #[test]
    fn second_alert_sent_after_window_expires() {
        let mut mgr = AlertManager::new(vec![AlertChannel::InMemory], 5_000, 60_000);
        let msg1 = make_msg("a1", "VaR breach");
        let msg2 = make_msg("a2", "VaR breach");

        mgr.send(msg1, 1_000);
        mgr.send(msg2, 10_000); // 9s later — window expired

        assert_eq!(mgr.sent_log.len(), 2, "Alert after window should be sent");
    }

    #[test]
    fn escalation_triggers_after_timeout() {
        let mut mgr = AlertManager::new(
            vec![AlertChannel::InMemory],
            0,     // no dedup
            1_000, // escalate after 1s
        );
        let msg = make_msg("e1", "High VaR");
        mgr.send(msg, 0);
        // sent_log has 1 item now; escalation has the unack alert

        let escalated = mgr.tick(5_000); // 5s later — should escalate
        assert!(
            !escalated.is_empty(),
            "Alert should be escalated after timeout"
        );
        assert_eq!(
            escalated[0].severity,
            AlertSeverity::Critical,
            "Escalated alert should have Critical severity"
        );
    }

    #[test]
    fn acknowledge_stops_escalation() {
        let mut mgr = AlertManager::new(vec![AlertChannel::InMemory], 0, 1_000);
        let msg = make_msg("e2", "Risk alert");
        mgr.send(msg, 0);

        let acked = mgr.acknowledge("e2", 500);
        assert!(acked, "Acknowledge should return true for known alert");

        let escalated = mgr.tick(5_000);
        assert!(
            escalated.is_empty(),
            "Acknowledged alert should not escalate"
        );
    }
}
