use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct ServiceInfo {
    pub name: String,
    pub address: String,
    pub healthy: bool,
    pub version: String,
}

#[derive(Default)]
pub struct ServiceRegistry {
    services: HashMap<String, ServiceInfo>,
}

impl ServiceRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, info: ServiceInfo) {
        self.services.insert(info.name.clone(), info);
    }

    pub fn deregister(&mut self, name: &str) {
        self.services.remove(name);
    }

    pub fn heartbeat(&mut self, name: &str, healthy: bool) {
        if let Some(svc) = self.services.get_mut(name) {
            svc.healthy = healthy;
        }
    }

    pub fn healthy_services(&self) -> Vec<&ServiceInfo> {
        self.services.values().filter(|s| s.healthy).collect()
    }

    pub fn find(&self, name: &str) -> Option<&ServiceInfo> {
        self.services.get(name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn register_and_filter_healthy() {
        let mut reg = ServiceRegistry::new();
        reg.register(ServiceInfo {
            name: "strategy".to_string(),
            address: "127.0.0.1:9001".to_string(),
            healthy: true,
            version: "1.0".to_string(),
        });
        reg.register(ServiceInfo {
            name: "risk".to_string(),
            address: "127.0.0.1:9002".to_string(),
            healthy: true,
            version: "1.0".to_string(),
        });
        reg.heartbeat("risk", false);
        assert_eq!(reg.healthy_services().len(), 1);
        assert_eq!(reg.healthy_services()[0].name, "strategy");
    }

    #[test]
    fn deregister_removes_service() {
        let mut reg = ServiceRegistry::new();
        reg.register(ServiceInfo {
            name: "exec".to_string(),
            address: "127.0.0.1:9003".to_string(),
            healthy: true,
            version: "1.0".to_string(),
        });
        reg.deregister("exec");
        assert!(reg.find("exec").is_none());
    }
}
