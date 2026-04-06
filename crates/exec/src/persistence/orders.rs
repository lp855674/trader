use std::collections::HashMap;

use domain::InstrumentId;

use crate::core::order::{Order, OrderState};

pub struct OrderRepository {
    orders: HashMap<String, Order>,
}

impl OrderRepository {
    pub fn new() -> Self {
        Self { orders: HashMap::new() }
    }

    pub fn save(&mut self, order: &Order) {
        self.orders.insert(order.id.clone(), order.clone());
    }

    pub fn find(&self, id: &str) -> Option<&Order> {
        self.orders.get(id)
    }

    pub fn find_by_instrument(&self, instrument: &InstrumentId) -> Vec<&Order> {
        self.orders.values().filter(|o| &o.request.instrument == instrument).collect()
    }

    pub fn find_open(&self) -> Vec<&Order> {
        self.orders.values().filter(|o| !o.state.is_terminal()).collect()
    }

    pub fn count(&self) -> usize {
        self.orders.len()
    }

    pub fn delete(&mut self, id: &str) -> bool {
        self.orders.remove(id).is_some()
    }
}

impl Default for OrderRepository {
    fn default() -> Self {
        Self::new()
    }
}

impl OrderRepository {
    /// Partitioning strategy: return orders grouped by instrument (simulate partition key).
    pub fn partition_by_instrument(&self) -> std::collections::HashMap<String, Vec<&Order>> {
        let mut map: std::collections::HashMap<String, Vec<&Order>> = std::collections::HashMap::new();
        for order in self.orders.values() {
            map.entry(order.request.instrument.to_string()).or_default().push(order);
        }
        map
    }

    /// Connection pooling: returns a simulated pool health check (always healthy in-process).
    pub fn pool_health(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use domain::{InstrumentId, Side, Venue};

    use super::*;
    use crate::core::order::{Order, OrderEvent, OrderState};
    use crate::core::types::{OrderKind, OrderRequest, TimeInForce};

    fn make_order(id: &str) -> Order {
        let req = OrderRequest {
            client_order_id: id.to_string(),
            instrument: InstrumentId::new(Venue::Crypto, "BTC-USD"),
            side: Side::Buy,
            quantity: 1.0,
            kind: OrderKind::Market,
            tif: TimeInForce::GTC,
            flags: vec![],
            strategy_id: "s1".to_string(),
            submitted_ts_ms: 1000,
        };
        Order::new(req, 1000)
    }

    #[test]
    fn save_and_find() {
        let mut repo = OrderRepository::new();
        let o = make_order("c1");
        let id = o.id.clone();
        repo.save(&o);
        assert!(repo.find(&id).is_some());
        assert_eq!(repo.count(), 1);
    }

    #[test]
    fn find_open_filters_terminal() {
        let mut repo = OrderRepository::new();
        let mut o = make_order("c2");
        repo.save(&o);
        // Transition to cancelled
        o.transition(OrderEvent::Cancel, 2000).unwrap();
        repo.save(&o);
        let open = repo.find_open();
        assert!(open.is_empty());
    }

    #[test]
    fn delete_works() {
        let mut repo = OrderRepository::new();
        let o = make_order("c3");
        let id = o.id.clone();
        repo.save(&o);
        assert!(repo.delete(&id));
        assert!(!repo.delete(&id));
    }
}
