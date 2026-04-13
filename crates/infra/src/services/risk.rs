use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq)]
pub enum RiskDecision {
    Approved,
    Rejected(String),
    Warning(String),
}

#[derive(Debug, Clone)]
pub struct RiskCheck {
    pub instrument: String,
    pub quantity: f64,
    pub notional: f64,
    pub account_id: String,
}

pub struct RiskServiceStub {
    max_notional: f64,
    max_position: f64,
    positions: HashMap<String, f64>,
    alert_count: u64,
}

impl RiskServiceStub {
    pub fn new(max_notional: f64, max_position: f64) -> Self {
        Self {
            max_notional,
            max_position,
            positions: HashMap::new(),
            alert_count: 0,
        }
    }

    pub fn check(&mut self, req: &RiskCheck) -> RiskDecision {
        if req.notional > self.max_notional {
            self.alert_count += 1;
            return RiskDecision::Rejected(format!(
                "notional {} exceeds limit {}",
                req.notional, self.max_notional
            ));
        }
        let current = self.positions.get(&req.instrument).copied().unwrap_or(0.0);
        let new_pos = current + req.quantity;
        if new_pos.abs() > self.max_position {
            self.alert_count += 1;
            return RiskDecision::Rejected(format!(
                "position {} would exceed limit {}",
                new_pos, self.max_position
            ));
        }
        if req.notional > self.max_notional * 0.8 {
            self.alert_count += 1;
            return RiskDecision::Warning(format!(
                "notional {} approaching limit {}",
                req.notional, self.max_notional
            ));
        }
        self.positions.insert(req.instrument.clone(), new_pos);
        RiskDecision::Approved
    }

    pub fn position(&self, instrument: &str) -> f64 {
        self.positions.get(instrument).copied().unwrap_or(0.0)
    }

    pub fn alert_count(&self) -> u64 {
        self.alert_count
    }

    pub fn risk_report(&self) -> serde_json::Value {
        let positions: serde_json::Map<String, serde_json::Value> = self
            .positions
            .iter()
            .map(|(k, v)| (k.clone(), serde_json::Value::from(*v)))
            .collect();
        serde_json::json!({
            "max_notional": self.max_notional,
            "max_position": self.max_position,
            "alert_count": self.alert_count,
            "positions": positions,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn approved_within_limits() {
        let mut svc = RiskServiceStub::new(100_000.0, 1000.0);
        let req = RiskCheck {
            instrument: "AAPL".to_string(),
            quantity: 100.0,
            notional: 15_000.0,
            account_id: "acc1".to_string(),
        };
        assert_eq!(svc.check(&req), RiskDecision::Approved);
        assert_eq!(svc.position("AAPL"), 100.0);
    }

    #[test]
    fn rejected_on_notional_breach() {
        let mut svc = RiskServiceStub::new(10_000.0, 1000.0);
        let req = RiskCheck {
            instrument: "AAPL".to_string(),
            quantity: 100.0,
            notional: 15_000.0,
            account_id: "acc1".to_string(),
        };
        assert!(matches!(svc.check(&req), RiskDecision::Rejected(_)));
        assert_eq!(svc.alert_count(), 1);
    }

    #[test]
    fn risk_report_includes_positions() {
        let mut svc = RiskServiceStub::new(100_000.0, 1000.0);
        let req = RiskCheck {
            instrument: "TSLA".to_string(),
            quantity: 50.0,
            notional: 10_000.0,
            account_id: "acc1".to_string(),
        };
        svc.check(&req);
        let report = svc.risk_report();
        assert!(report["positions"]["TSLA"].as_f64().is_some());
    }
}
