use std::collections::HashMap;

use crate::core::order::Order;

pub struct IndexEntry {
    pub key: String,
    pub value: String,
    pub ts_ms: i64,
}

pub struct QueryIndex {
    pub by_instrument: HashMap<String, Vec<String>>,
    pub by_strategy: HashMap<String, Vec<String>>,
}

impl QueryIndex {
    pub fn new() -> Self {
        Self { by_instrument: HashMap::new(), by_strategy: HashMap::new() }
    }

    pub fn index_order(&mut self, order: &Order) {
        let instrument_key = order.request.instrument.to_string();
        self.by_instrument.entry(instrument_key).or_default().push(order.id.clone());

        let strategy_key = order.request.strategy_id.clone();
        self.by_strategy.entry(strategy_key).or_default().push(order.id.clone());
    }

    pub fn lookup_by_instrument(&self, instrument: &str) -> &[String] {
        self.by_instrument.get(instrument).map(|v| v.as_slice()).unwrap_or(&[])
    }

    pub fn lookup_by_strategy(&self, strategy_id: &str) -> &[String] {
        self.by_strategy.get(strategy_id).map(|v| v.as_slice()).unwrap_or(&[])
    }
}

impl Default for QueryIndex {
    fn default() -> Self {
        Self::new()
    }
}

impl QueryIndex {
    /// Index maintenance: rebuild all indexes from scratch (drops stale entries).
    pub fn rebuild(&mut self, orders: &[&crate::core::order::Order]) {
        self.by_instrument.clear();
        self.by_strategy.clear();
        for o in orders {
            self.index_order(o);
        }
    }

    /// Query plan analysis: return a description of which index would be used for a query.
    pub fn explain_query(&self, by: &str, value: &str) -> String {
        match by {
            "instrument" => format!("INDEX SCAN by_instrument[{}] → {} entries",
                value, self.by_instrument.get(value).map(|v| v.len()).unwrap_or(0)),
            "strategy" => format!("INDEX SCAN by_strategy[{}] → {} entries",
                value, self.by_strategy.get(value).map(|v| v.len()).unwrap_or(0)),
            _ => "FULL TABLE SCAN (no index)".to_string(),
        }
    }

    /// Performance tuning: return index statistics.
    pub fn stats(&self) -> (usize, usize) {
        let total_instrument_entries: usize = self.by_instrument.values().map(|v| v.len()).sum();
        let total_strategy_entries: usize = self.by_strategy.values().map(|v| v.len()).sum();
        (total_instrument_entries, total_strategy_entries)
    }
}

#[cfg(test)]
mod tests {
    use domain::{InstrumentId, Side, Venue};

    use super::*;
    use crate::core::order::Order;
    use crate::core::types::{OrderKind, OrderRequest, TimeInForce};

    fn make_order(client_id: &str, strategy: &str) -> Order {
        let req = OrderRequest {
            client_order_id: client_id.to_string(),
            instrument: InstrumentId::new(Venue::Crypto, "BTC-USD"),
            side: Side::Buy,
            quantity: 1.0,
            kind: OrderKind::Market,
            tif: TimeInForce::GTC,
            flags: vec![],
            strategy_id: strategy.to_string(),
            submitted_ts_ms: 1000,
        };
        Order::new(req, 1000)
    }

    #[test]
    fn lookup_by_instrument() {
        let mut idx = QueryIndex::new();
        let o1 = make_order("c1", "s1");
        let o2 = make_order("c2", "s2");
        idx.index_order(&o1);
        idx.index_order(&o2);
        let ids = idx.lookup_by_instrument("CRYPTO:BTC-USD");
        assert_eq!(ids.len(), 2);
    }

    #[test]
    fn lookup_by_strategy() {
        let mut idx = QueryIndex::new();
        let o1 = make_order("c1", "stratA");
        let o2 = make_order("c2", "stratA");
        let o3 = make_order("c3", "stratB");
        idx.index_order(&o1);
        idx.index_order(&o2);
        idx.index_order(&o3);
        assert_eq!(idx.lookup_by_strategy("stratA").len(), 2);
        assert_eq!(idx.lookup_by_strategy("stratB").len(), 1);
        assert_eq!(idx.lookup_by_strategy("unknown").len(), 0);
    }
}
