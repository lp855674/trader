use serde::{Deserialize, Serialize};

use crate::core::order::{Order, OrderManager};
use crate::core::position::{ExecPosition, ExecPositionManager};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snapshot {
    pub ts_ms: i64,
    pub orders: Vec<Order>,
    pub positions: Vec<(String, ExecPosition)>,
}

pub struct SnapshotManager {
    pub snapshots: Vec<Snapshot>,
    pub max_snapshots: usize,
}

impl SnapshotManager {
    pub fn new(max_snapshots: usize) -> Self {
        Self { snapshots: Vec::new(), max_snapshots }
    }

    pub fn take(
        &mut self,
        manager: &OrderManager,
        positions: &ExecPositionManager,
        ts_ms: i64,
    ) {
        if self.snapshots.len() >= self.max_snapshots {
            self.snapshots.remove(0);
        }
        let orders: Vec<Order> = manager.orders.values().cloned().collect();
        let pos_vec: Vec<(String, ExecPosition)> = positions
            .positions
            .iter()
            .map(|(id, p)| (id.to_string(), p.clone()))
            .collect();
        self.snapshots.push(Snapshot { ts_ms, orders, positions: pos_vec });
    }

    pub fn latest(&self) -> Option<&Snapshot> {
        self.snapshots.last()
    }

    pub fn restore_orders(snapshot: &Snapshot) -> OrderManager {
        let mut mgr = OrderManager::new();
        for order in &snapshot.orders {
            mgr.orders.insert(order.id.clone(), order.clone());
        }
        mgr
    }

    pub fn to_json(snapshot: &Snapshot) -> Result<String, serde_json::Error> {
        serde_json::to_string(snapshot)
    }

    pub fn from_json(json: &str) -> Result<Snapshot, serde_json::Error> {
        serde_json::from_str(json)
    }

    /// Incremental backup: returns only the entries that changed since `since_ts_ms`.
    /// In this in-process implementation, returns snapshots newer than the given timestamp.
    pub fn incremental_since(&self, since_ts_ms: i64) -> Vec<&Snapshot> {
        self.snapshots.iter().filter(|s| s.ts_ms > since_ts_ms).collect()
    }

    /// Disaster recovery test: verify every snapshot can be serialized and restored.
    pub fn verify_all(&self) -> Vec<String> {
        self.snapshots.iter().enumerate().filter_map(|(i, snap)| {
            match Self::to_json(snap) {
                Ok(json) => match Self::from_json(&json) {
                    Ok(_) => None,
                    Err(e) => Some(format!("snapshot[{}] restore failed: {}", i, e)),
                },
                Err(e) => Some(format!("snapshot[{}] serialize failed: {}", i, e)),
            }
        }).collect()
    }
}

#[cfg(test)]
mod tests {
    use domain::{InstrumentId, Side, Venue};

    use super::*;
    use crate::core::order::Order;
    use crate::core::position::{ExecPositionManager, TaxLotMethod};
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
    fn snapshot_round_trip_json() {
        let mut order_mgr = OrderManager::new();
        let o = make_order("c1");
        let oid = o.id.clone();
        order_mgr.orders.insert(oid.clone(), o);
        let pos_mgr = ExecPositionManager::new(TaxLotMethod::Fifo);
        let mut snap_mgr = SnapshotManager::new(10);
        snap_mgr.take(&order_mgr, &pos_mgr, 1000);
        let snap = snap_mgr.latest().unwrap();
        let json = SnapshotManager::to_json(snap).unwrap();
        let restored_snap = SnapshotManager::from_json(&json).unwrap();
        let restored_mgr = SnapshotManager::restore_orders(&restored_snap);
        assert!(restored_mgr.get(&oid).is_some());
    }

    #[test]
    fn max_snapshots_evict() {
        let order_mgr = OrderManager::new();
        let pos_mgr = ExecPositionManager::new(TaxLotMethod::Fifo);
        let mut snap_mgr = SnapshotManager::new(2);
        snap_mgr.take(&order_mgr, &pos_mgr, 1000);
        snap_mgr.take(&order_mgr, &pos_mgr, 2000);
        snap_mgr.take(&order_mgr, &pos_mgr, 3000);
        assert_eq!(snap_mgr.snapshots.len(), 2);
        assert_eq!(snap_mgr.latest().unwrap().ts_ms, 3000);
    }
}
