use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq)]
pub enum HealthStatus {
    Serving,
    NotServing,
    Unknown,
}

impl HealthStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            HealthStatus::Serving => "SERVING",
            HealthStatus::NotServing => "NOT_SERVING",
            HealthStatus::Unknown => "UNKNOWN",
        }
    }
}

#[derive(Debug, Clone)]
pub struct ServiceHealthInfo {
    pub name: String,
    pub status: HealthStatus,
    pub version: String,
    pub uptime_secs: u64,
}

pub struct HealthCheckService {
    services: HashMap<String, ServiceHealthInfo>,
    start_time_ms: u64,
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

impl HealthCheckService {
    pub fn new() -> Self {
        Self {
            services: HashMap::new(),
            start_time_ms: now_ms(),
        }
    }

    pub fn register(&mut self, name: &str, version: &str) {
        self.services.insert(
            name.to_string(),
            ServiceHealthInfo {
                name: name.to_string(),
                status: HealthStatus::Unknown,
                version: version.to_string(),
                uptime_secs: 0,
            },
        );
    }

    pub fn set_status(&mut self, name: &str, status: HealthStatus) {
        if let Some(info) = self.services.get_mut(name) {
            info.status = status;
            info.uptime_secs = (now_ms() - self.start_time_ms) / 1000;
        }
    }

    pub fn check(&self, name: &str) -> HealthStatus {
        self.services
            .get(name)
            .map(|i| i.status.clone())
            .unwrap_or(HealthStatus::Unknown)
    }

    pub fn all_serving(&self) -> bool {
        !self.services.is_empty()
            && self.services.values().all(|i| i.status == HealthStatus::Serving)
    }

    pub fn summary(&self) -> Vec<(&str, &str)> {
        self.services
            .values()
            .map(|i| (i.name.as_str(), i.status.as_str()))
            .collect()
    }
}

impl Default for HealthCheckService {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn register_and_check() {
        let mut hc = HealthCheckService::new();
        hc.register("strategy_svc", "1.0");
        hc.register("risk_svc", "1.0");
        hc.set_status("strategy_svc", HealthStatus::Serving);
        hc.set_status("risk_svc", HealthStatus::Serving);
        assert!(hc.all_serving());
        assert_eq!(hc.check("strategy_svc"), HealthStatus::Serving);
    }

    #[test]
    fn not_all_serving_when_one_down() {
        let mut hc = HealthCheckService::new();
        hc.register("svc_a", "1.0");
        hc.register("svc_b", "1.0");
        hc.set_status("svc_a", HealthStatus::Serving);
        hc.set_status("svc_b", HealthStatus::NotServing);
        assert!(!hc.all_serving());
    }

    #[test]
    fn unknown_service_returns_unknown() {
        let hc = HealthCheckService::new();
        assert_eq!(hc.check("ghost_svc"), HealthStatus::Unknown);
    }
}
