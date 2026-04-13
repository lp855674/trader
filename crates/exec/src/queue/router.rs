use crate::core::OrderRequest;

#[derive(Debug, Clone)]
pub struct RoutingRule {
    pub instrument_pattern: String,
    pub venue: String,
    pub max_qty: Option<f64>,
}

#[derive(Debug, Clone)]
pub struct RoutingDecision {
    pub venue: String,
    pub split_orders: Vec<(f64, String)>,
}

pub struct SmartOrderRouter {
    pub rules: Vec<RoutingRule>,
    pub default_venue: String,
}

impl SmartOrderRouter {
    pub fn new(default_venue: &str) -> Self {
        Self {
            rules: Vec::new(),
            default_venue: default_venue.to_string(),
        }
    }

    pub fn add_rule(&mut self, rule: RoutingRule) {
        self.rules.push(rule);
    }

    /// Match instrument symbol prefix to a rule, return single venue routing decision.
    pub fn route(&self, request: &OrderRequest) -> RoutingDecision {
        let symbol = &request.instrument.symbol;
        for rule in &self.rules {
            if symbol.starts_with(&rule.instrument_pattern) {
                return RoutingDecision {
                    venue: rule.venue.clone(),
                    split_orders: vec![(request.quantity, rule.venue.clone())],
                };
            }
        }
        RoutingDecision {
            venue: self.default_venue.clone(),
            split_orders: vec![(request.quantity, self.default_venue.clone())],
        }
    }

    /// Split order proportionally by liquidity weights across venues.
    pub fn split_order(&self, request: &OrderRequest, venues: &[(&str, f64)]) -> RoutingDecision {
        let total_weight: f64 = venues.iter().map(|(_, w)| w).sum();
        if total_weight == 0.0 || venues.is_empty() {
            return self.route(request);
        }
        let splits: Vec<(f64, String)> = venues
            .iter()
            .map(|(venue, weight)| {
                let qty = request.quantity * weight / total_weight;
                (qty, venue.to_string())
            })
            .collect();
        let primary_venue = venues[0].0.to_string();
        RoutingDecision {
            venue: primary_venue,
            split_orders: splits,
        }
    }
}

#[cfg(test)]
mod tests {
    use domain::{InstrumentId, Side, Venue};

    use super::*;
    use crate::core::types::{OrderKind, TimeInForce};

    fn make_req(symbol: &str) -> OrderRequest {
        OrderRequest {
            client_order_id: "c1".to_string(),
            instrument: InstrumentId::new(Venue::Crypto, symbol),
            side: Side::Buy,
            quantity: 100.0,
            kind: OrderKind::Market,
            tif: TimeInForce::GTC,
            flags: vec![],
            strategy_id: "s".to_string(),
            submitted_ts_ms: 0,
        }
    }

    #[test]
    fn routes_by_prefix_match() {
        let mut router = SmartOrderRouter::new("default_venue");
        router.add_rule(RoutingRule {
            instrument_pattern: "BTC".to_string(),
            venue: "crypto_exchange".to_string(),
            max_qty: None,
        });
        let decision = router.route(&make_req("BTC-USD"));
        assert_eq!(decision.venue, "crypto_exchange");
    }

    #[test]
    fn fallback_to_default_venue() {
        let router = SmartOrderRouter::new("fallback");
        let decision = router.route(&make_req("ETH-USD"));
        assert_eq!(decision.venue, "fallback");
    }

    #[test]
    fn split_order_proportional() {
        let router = SmartOrderRouter::new("default");
        let req = make_req("BTC-USD");
        let venues = vec![("venue_a", 3.0), ("venue_b", 1.0)];
        let decision = router.split_order(&req, &venues);
        assert_eq!(decision.split_orders.len(), 2);
        let (qty_a, _) = &decision.split_orders[0];
        let (qty_b, _) = &decision.split_orders[1];
        // venue_a: 100 * 3/4 = 75; venue_b: 100 * 1/4 = 25
        assert!((qty_a - 75.0).abs() < 1e-9);
        assert!((qty_b - 25.0).abs() < 1e-9);
    }
}
