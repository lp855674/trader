use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq)]
pub enum BalanceStrategy {
    RoundRobin,
    LeastConnections,
    ConsistentHash,
    Weighted,
}

#[derive(Debug, Clone)]
pub struct BackendNode {
    pub id: String,
    pub address: String,
    pub weight: u32,
    pub connections: u32,
    pub healthy: bool,
}

pub struct LoadBalancer {
    nodes: Vec<BackendNode>,
    strategy: BalanceStrategy,
    rr_index: usize,
}

impl LoadBalancer {
    pub fn new(strategy: BalanceStrategy) -> Self {
        Self { nodes: Vec::new(), strategy, rr_index: 0 }
    }

    pub fn add_node(&mut self, id: &str, address: &str, weight: u32) {
        self.nodes.push(BackendNode {
            id: id.to_string(),
            address: address.to_string(),
            weight,
            connections: 0,
            healthy: true,
        });
    }

    pub fn set_healthy(&mut self, id: &str, healthy: bool) {
        if let Some(n) = self.nodes.iter_mut().find(|n| n.id == id) {
            n.healthy = healthy;
        }
    }

    pub fn connect(&mut self, id: &str) {
        if let Some(n) = self.nodes.iter_mut().find(|n| n.id == id) {
            n.connections += 1;
        }
    }

    pub fn disconnect(&mut self, id: &str) {
        if let Some(n) = self.nodes.iter_mut().find(|n| n.id == id) {
            n.connections = n.connections.saturating_sub(1);
        }
    }

    /// Select a backend node according to the configured strategy.
    pub fn select(&mut self, key: Option<&str>) -> Option<&BackendNode> {
        let healthy: Vec<usize> = self.nodes.iter().enumerate()
            .filter(|(_, n)| n.healthy)
            .map(|(i, _)| i)
            .collect();
        if healthy.is_empty() {
            return None;
        }
        let chosen = match self.strategy {
            BalanceStrategy::RoundRobin => {
                let idx = self.rr_index % healthy.len();
                self.rr_index = self.rr_index.wrapping_add(1);
                healthy[idx]
            }
            BalanceStrategy::LeastConnections => {
                *healthy.iter().min_by_key(|&&i| self.nodes[i].connections).unwrap()
            }
            BalanceStrategy::ConsistentHash => {
                let hash = key.map(|k| {
                    k.bytes().fold(0u64, |acc, b| acc.wrapping_mul(31).wrapping_add(b as u64))
                }).unwrap_or(0);
                healthy[(hash as usize) % healthy.len()]
            }
            BalanceStrategy::Weighted => {
                let total_weight: u32 = healthy.iter().map(|&i| self.nodes[i].weight).sum();
                if total_weight == 0 { return None; }
                let mut pick = (self.rr_index as u32) % total_weight;
                self.rr_index = self.rr_index.wrapping_add(1);
                let mut chosen = healthy[0];
                for &i in &healthy {
                    let w = self.nodes[i].weight;
                    if pick < w { chosen = i; break; }
                    pick -= w;
                }
                chosen
            }
        };
        Some(&self.nodes[chosen])
    }

    pub fn healthy_count(&self) -> usize {
        self.nodes.iter().filter(|n| n.healthy).count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_robin_cycles() {
        let mut lb = LoadBalancer::new(BalanceStrategy::RoundRobin);
        lb.add_node("n1", "10.0.0.1:9090", 1);
        lb.add_node("n2", "10.0.0.2:9090", 1);
        let a = lb.select(None).unwrap().id.clone();
        let b = lb.select(None).unwrap().id.clone();
        assert_ne!(a, b);
    }

    #[test]
    fn least_connections_picks_lowest() {
        let mut lb = LoadBalancer::new(BalanceStrategy::LeastConnections);
        lb.add_node("n1", "10.0.0.1:9090", 1);
        lb.add_node("n2", "10.0.0.2:9090", 1);
        lb.connect("n1");
        lb.connect("n1");
        let chosen = lb.select(None).unwrap().id.clone();
        assert_eq!(chosen, "n2");
    }

    #[test]
    fn unhealthy_node_excluded() {
        let mut lb = LoadBalancer::new(BalanceStrategy::RoundRobin);
        lb.add_node("n1", "10.0.0.1:9090", 1);
        lb.add_node("n2", "10.0.0.2:9090", 1);
        lb.set_healthy("n1", false);
        for _ in 0..5 {
            let chosen = lb.select(None).unwrap().id.clone();
            assert_eq!(chosen, "n2");
        }
        assert_eq!(lb.healthy_count(), 1);
    }

    #[test]
    fn all_unhealthy_returns_none() {
        let mut lb = LoadBalancer::new(BalanceStrategy::RoundRobin);
        lb.add_node("n1", "10.0.0.1:9090", 1);
        lb.set_healthy("n1", false);
        assert!(lb.select(None).is_none());
    }
}
