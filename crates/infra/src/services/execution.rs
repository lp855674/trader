use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq)]
pub enum OrderStatus {
    Pending,
    Submitted,
    Filled,
    Rejected(String),
}

#[derive(Default)]
pub struct ExecutionServiceStub {
    orders: HashMap<String, OrderStatus>,
    fill_count: u64,
}

impl ExecutionServiceStub {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn submit_order(&mut self, id: &str) -> bool {
        if self.orders.contains_key(id) {
            return false;
        }
        self.orders.insert(id.to_string(), OrderStatus::Submitted);
        true
    }

    pub fn fill_order(&mut self, id: &str) -> bool {
        if let Some(s) = self.orders.get_mut(id) {
            if *s == OrderStatus::Submitted {
                *s = OrderStatus::Filled;
                self.fill_count += 1;
                return true;
            }
        }
        false
    }

    pub fn reject_order(&mut self, id: &str, reason: &str) -> bool {
        if let Some(s) = self.orders.get_mut(id) {
            if *s == OrderStatus::Submitted {
                *s = OrderStatus::Rejected(reason.to_string());
                return true;
            }
        }
        false
    }

    pub fn order_status(&self, id: &str) -> Option<&OrderStatus> {
        self.orders.get(id)
    }

    pub fn fill_count(&self) -> u64 {
        self.fill_count
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn submit_fill_reject_lifecycle() {
        let mut svc = ExecutionServiceStub::new();
        assert!(svc.submit_order("ord1"));
        assert!(!svc.submit_order("ord1")); // duplicate
        assert!(svc.fill_order("ord1"));
        assert_eq!(svc.fill_count(), 1);

        assert!(svc.submit_order("ord2"));
        assert!(svc.reject_order("ord2", "insufficient funds"));
        assert_eq!(svc.order_status("ord2"), Some(&OrderStatus::Rejected("insufficient funds".to_string())));
    }
}
