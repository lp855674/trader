use crate::core::OrderRequest;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum OrderPriority {
    Urgent = 0,
    Normal = 1,
    Delayed = 2,
}

#[derive(Debug, Clone)]
pub struct PrioritizedOrder {
    pub request: OrderRequest,
    pub priority: OrderPriority,
    pub enqueued_ts_ms: i64,
}

pub struct PriorityQueue {
    pub items: Vec<PrioritizedOrder>,
}

impl PriorityQueue {
    pub fn new() -> Self {
        Self { items: Vec::new() }
    }

    pub fn push(&mut self, req: OrderRequest, priority: OrderPriority, ts_ms: i64) {
        let item = PrioritizedOrder { request: req, priority, enqueued_ts_ms: ts_ms };
        // Insert maintaining sort order: Urgent first, FIFO within priority
        let pos = self.items.partition_point(|existing| {
            existing.priority < item.priority
                || (existing.priority == item.priority
                    && existing.enqueued_ts_ms <= item.enqueued_ts_ms)
        });
        self.items.insert(pos, item);
    }

    pub fn pop(&mut self) -> Option<PrioritizedOrder> {
        if self.items.is_empty() {
            None
        } else {
            Some(self.items.remove(0))
        }
    }

    /// Upgrade Delayed orders older than max_age_ms to Normal. Returns count upgraded.
    pub fn preempt_delayed(&mut self, ts_ms: i64, max_age_ms: u64) -> usize {
        let mut count = 0;
        for item in &mut self.items {
            if item.priority == OrderPriority::Delayed {
                let age = (ts_ms - item.enqueued_ts_ms).max(0) as u64;
                if age >= max_age_ms {
                    item.priority = OrderPriority::Normal;
                    count += 1;
                }
            }
        }
        if count > 0 {
            // Re-sort after upgrades
            self.items.sort_by(|a, b| {
                a.priority.cmp(&b.priority).then(a.enqueued_ts_ms.cmp(&b.enqueued_ts_ms))
            });
        }
        count
    }
}

impl Default for PriorityQueue {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use domain::{InstrumentId, Side, Venue};

    use super::*;
    use crate::core::types::{OrderKind, TimeInForce};

    fn make_req(n: u32) -> OrderRequest {
        OrderRequest {
            client_order_id: format!("c{}", n),
            instrument: InstrumentId::new(Venue::Crypto, "BTC-USD"),
            side: Side::Buy,
            quantity: 1.0,
            kind: OrderKind::Market,
            tif: TimeInForce::GTC,
            flags: vec![],
            strategy_id: "s".to_string(),
            submitted_ts_ms: 0,
        }
    }

    #[test]
    fn urgent_before_normal_before_delayed() {
        let mut pq = PriorityQueue::new();
        pq.push(make_req(1), OrderPriority::Delayed, 1000);
        pq.push(make_req(2), OrderPriority::Normal, 2000);
        pq.push(make_req(3), OrderPriority::Urgent, 3000);
        let first = pq.pop().unwrap();
        assert_eq!(first.request.client_order_id, "c3");
        let second = pq.pop().unwrap();
        assert_eq!(second.request.client_order_id, "c2");
        let third = pq.pop().unwrap();
        assert_eq!(third.request.client_order_id, "c1");
    }

    #[test]
    fn fifo_within_priority() {
        let mut pq = PriorityQueue::new();
        pq.push(make_req(1), OrderPriority::Normal, 1000);
        pq.push(make_req(2), OrderPriority::Normal, 2000);
        pq.push(make_req(3), OrderPriority::Normal, 3000);
        let first = pq.pop().unwrap();
        assert_eq!(first.request.client_order_id, "c1");
        let second = pq.pop().unwrap();
        assert_eq!(second.request.client_order_id, "c2");
    }

    #[test]
    fn preempt_delayed_upgrades_old_orders() {
        let mut pq = PriorityQueue::new();
        pq.push(make_req(1), OrderPriority::Delayed, 0);
        pq.push(make_req(2), OrderPriority::Delayed, 2500);
        // At ts=3000: order 1 age=3000>=2000 → upgraded; order 2 age=500<2000 → not upgraded
        let upgraded = pq.preempt_delayed(3000, 2000);
        assert_eq!(upgraded, 1);
        let first = pq.pop().unwrap();
        assert_eq!(first.priority, OrderPriority::Normal);
    }
}
