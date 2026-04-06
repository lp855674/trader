use crate::core::types::OrderRequest;

#[derive(Debug, Clone)]
pub struct LongbridgeConfig {
    pub api_key: String,
    pub app_key: String,
    pub region: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum LongbridgeOrderStatus {
    Pending,
    Filled,
    PartiallyFilled,
    Cancelled,
    Rejected(String),
}

pub struct LongbridgeAdapter {
    pub config: LongbridgeConfig,
    pub connected: bool,
    pub submitted_count: u64,
}

impl LongbridgeAdapter {
    pub fn new(config: LongbridgeConfig) -> Self {
        Self { config, connected: false, submitted_count: 0 }
    }

    pub fn connect(&mut self) -> Result<(), String> {
        self.connected = true;
        Ok(())
    }

    pub fn submit_order(&mut self, req: &OrderRequest) -> Result<String, String> {
        if !self.connected {
            return Err("not connected".to_string());
        }
        self.submitted_count += 1;
        Ok(format!("lb-order-{}-{}", req.client_order_id, self.submitted_count))
    }

    pub fn cancel_order(&mut self, order_id: &str) -> Result<(), String> {
        if !self.connected {
            return Err("not connected".to_string());
        }
        // Stub — always succeeds
        let _ = order_id;
        Ok(())
    }

    pub fn get_status(&self, order_id: &str) -> LongbridgeOrderStatus {
        let _ = order_id;
        LongbridgeOrderStatus::Pending
    }

    pub fn is_connected(&self) -> bool {
        self.connected
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> LongbridgeConfig {
        LongbridgeConfig {
            api_key: "key".to_string(),
            app_key: "app".to_string(),
            region: "HK".to_string(),
        }
    }

    #[test]
    fn connect_and_submit() {
        use domain::{InstrumentId, Side, Venue};

        use crate::core::types::{OrderKind, TimeInForce};

        let mut adapter = LongbridgeAdapter::new(config());
        assert!(!adapter.is_connected());
        adapter.connect().unwrap();
        assert!(adapter.is_connected());

        let req = OrderRequest {
            client_order_id: "c1".to_string(),
            instrument: InstrumentId::new(Venue::Crypto, "BTC-USD"),
            side: Side::Buy,
            quantity: 1.0,
            kind: OrderKind::Market,
            tif: TimeInForce::GTC,
            flags: vec![],
            strategy_id: "s1".to_string(),
            submitted_ts_ms: 1000,
        };
        let id = adapter.submit_order(&req).unwrap();
        assert!(id.starts_with("lb-order-"));
    }

    #[test]
    fn not_connected_returns_error() {
        use domain::{InstrumentId, Side, Venue};

        use crate::core::types::{OrderKind, TimeInForce};

        let mut adapter = LongbridgeAdapter::new(config());
        let req = OrderRequest {
            client_order_id: "c2".to_string(),
            instrument: InstrumentId::new(Venue::Crypto, "BTC-USD"),
            side: Side::Buy,
            quantity: 1.0,
            kind: OrderKind::Market,
            tif: TimeInForce::GTC,
            flags: vec![],
            strategy_id: "s1".to_string(),
            submitted_ts_ms: 1000,
        };
        assert!(adapter.submit_order(&req).is_err());
    }
}
