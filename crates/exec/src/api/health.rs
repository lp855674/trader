use serde::Serialize;

use crate::core::order::OrderManager;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", content = "message")]
pub enum HealthStatus {
    Ok,
    Degraded(String),
    Down(String),
}

#[derive(Debug, Clone, Serialize)]
pub struct ComponentHealth {
    pub name: String,
    pub status: HealthStatus,
    pub latency_ms: Option<f64>,
    pub details: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct HealthReport {
    pub overall: HealthStatus,
    pub components: Vec<ComponentHealth>,
    pub ts_ms: i64,
    pub version: String,
}

impl HealthReport {
    pub fn is_ready(&self) -> bool {
        self.components
            .iter()
            .all(|c| matches!(c.status, HealthStatus::Ok | HealthStatus::Degraded(_)))
    }

    pub fn is_live(&self) -> bool {
        self.components
            .iter()
            .any(|c| matches!(c.status, HealthStatus::Ok))
    }
}

pub struct HealthChecker {
    pub checks: Vec<Box<dyn Fn() -> ComponentHealth + Send + Sync>>,
}

impl HealthChecker {
    pub fn new() -> Self {
        Self { checks: Vec::new() }
    }

    pub fn add_check(&mut self, check: impl Fn() -> ComponentHealth + Send + Sync + 'static) {
        self.checks.push(Box::new(check));
    }

    pub fn check_all(&self, ts_ms: i64) -> HealthReport {
        let components: Vec<ComponentHealth> = self.checks.iter().map(|f| f()).collect();

        let overall = if components
            .iter()
            .any(|c| matches!(c.status, HealthStatus::Down(_)))
        {
            HealthStatus::Down("one or more components are down".to_string())
        } else if components
            .iter()
            .any(|c| matches!(c.status, HealthStatus::Degraded(_)))
        {
            HealthStatus::Degraded("one or more components are degraded".to_string())
        } else {
            HealthStatus::Ok
        };

        HealthReport {
            overall,
            components,
            ts_ms,
            version: env!("CARGO_PKG_VERSION").to_string(),
        }
    }

    pub fn order_manager_check(manager: &OrderManager) -> ComponentHealth {
        let open = manager.open_orders().len();
        ComponentHealth {
            name: "order_manager".to_string(),
            status: HealthStatus::Ok,
            latency_ms: None,
            details: format!("open_orders={}", open),
        }
    }
}

impl Default for HealthChecker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_ok_overall() {
        let mut checker = HealthChecker::new();
        checker.add_check(|| ComponentHealth {
            name: "a".to_string(),
            status: HealthStatus::Ok,
            latency_ms: None,
            details: String::new(),
        });
        let report = checker.check_all(1000);
        assert_eq!(report.overall, HealthStatus::Ok);
        assert!(report.is_ready());
        assert!(report.is_live());
    }

    #[test]
    fn one_down_overall_down() {
        let mut checker = HealthChecker::new();
        checker.add_check(|| ComponentHealth {
            name: "a".to_string(),
            status: HealthStatus::Ok,
            latency_ms: None,
            details: String::new(),
        });
        checker.add_check(|| ComponentHealth {
            name: "b".to_string(),
            status: HealthStatus::Down("db gone".to_string()),
            latency_ms: None,
            details: String::new(),
        });
        let report = checker.check_all(1000);
        assert!(matches!(report.overall, HealthStatus::Down(_)));
        assert!(!report.is_ready());
    }

    #[test]
    fn readiness_probe_degraded_ok() {
        let mut checker = HealthChecker::new();
        checker.add_check(|| ComponentHealth {
            name: "a".to_string(),
            status: HealthStatus::Degraded("slow".to_string()),
            latency_ms: Some(500.0),
            details: String::new(),
        });
        let report = checker.check_all(1000);
        assert!(report.is_ready());
        assert!(!report.is_live()); // no Ok component
    }

    #[test]
    fn order_manager_check_ok() {
        let mgr = OrderManager::new();
        let ch = HealthChecker::order_manager_check(&mgr);
        assert_eq!(ch.status, HealthStatus::Ok);
    }
}
