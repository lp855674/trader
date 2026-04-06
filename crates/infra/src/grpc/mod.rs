pub mod health;

use std::collections::HashMap;

/// In-process gRPC server stub (no real tonic/protobuf).
pub struct GrpcServer {
    pub address: String,
    pub max_connections: usize,
    handlers: HashMap<String, String>, // method → description
    running: bool,
}

impl GrpcServer {
    pub fn new(address: &str, max_connections: usize) -> Self {
        Self {
            address: address.to_string(),
            max_connections,
            handlers: HashMap::new(),
            running: false,
        }
    }

    pub fn register_handler(&mut self, method: &str, description: &str) {
        self.handlers.insert(method.to_string(), description.to_string());
    }

    pub fn start(&mut self) -> Result<(), String> {
        if self.running {
            return Err("server already running".to_string());
        }
        self.running = true;
        Ok(())
    }

    pub fn stop(&mut self) {
        self.running = false;
    }

    pub fn is_running(&self) -> bool {
        self.running
    }

    pub fn handler_count(&self) -> usize {
        self.handlers.len()
    }
}

/// Proto-like message: a key-value map (simplified, no codegen).
#[derive(Debug, Clone, Default)]
pub struct ProtoMessage {
    pub fields: HashMap<String, String>,
}

impl ProtoMessage {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set(&mut self, key: &str, value: &str) -> &mut Self {
        self.fields.insert(key.to_string(), value.to_string());
        self
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.fields.get(key).map(|s| s.as_str())
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(&self.fields).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn server_start_stop() {
        let mut srv = GrpcServer::new("0.0.0.0:9090", 100);
        srv.register_handler("StrategyService/Evaluate", "evaluate strategy");
        assert!(!srv.is_running());
        srv.start().unwrap();
        assert!(srv.is_running());
        srv.stop();
        assert!(!srv.is_running());
    }

    #[test]
    fn double_start_fails() {
        let mut srv = GrpcServer::new("0.0.0.0:9090", 100);
        srv.start().unwrap();
        assert!(srv.start().is_err());
    }

    #[test]
    fn proto_message_roundtrip() {
        let mut msg = ProtoMessage::new();
        msg.set("order_id", "ORD-001").set("side", "Buy");
        assert_eq!(msg.get("order_id"), Some("ORD-001"));
        let json = msg.to_json();
        assert!(json.contains("ORD-001"));
    }
}
