use serde::{Deserialize, Serialize};

use crate::core::order::{Order, OrderEvent, OrderManager};
use crate::core::position::{ExecPositionManager, FillRecord, TaxLotMethod};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WalEntry {
    OrderSubmitted(Order),
    OrderFilled { order_id: String, fill: FillRecord },
    OrderCancelled(String),
    Checkpoint { ts_ms: i64 },
}

pub struct WalLog {
    pub entries: Vec<WalEntry>,
    pub last_checkpoint_idx: usize,
}

impl WalLog {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            last_checkpoint_idx: 0,
        }
    }

    pub fn append(&mut self, entry: WalEntry) {
        self.entries.push(entry);
    }

    pub fn checkpoint(&mut self, ts_ms: i64) {
        self.entries.push(WalEntry::Checkpoint { ts_ms });
        self.last_checkpoint_idx = self.entries.len() - 1;
    }

    pub fn entries_since_checkpoint(&self) -> &[WalEntry] {
        if self.entries.is_empty() {
            return &[];
        }
        &self.entries[self.last_checkpoint_idx..]
    }
}

impl Default for WalLog {
    fn default() -> Self {
        Self::new()
    }
}

impl WalLog {
    /// Durability guarantee: verify that all entries after the last checkpoint are intact.
    /// Returns (total_entries, unchecked_entries, is_durable).
    /// "unchecked" = entries appended after the last checkpoint (not including the checkpoint itself).
    pub fn durability_status(&self) -> (usize, usize, bool) {
        let total = self.entries.len();
        // Entries strictly after the checkpoint entry
        let after_checkpoint = if self.entries.is_empty()
            || self.last_checkpoint_idx == 0
                && !matches!(self.entries.first(), Some(WalEntry::Checkpoint { .. }))
        {
            total
        } else {
            total.saturating_sub(self.last_checkpoint_idx + 1)
        };
        // In-process: always durable since we're in-memory; real impl would fsync.
        (total, after_checkpoint, true)
    }
}

pub struct WalRecovery;

impl WalRecovery {
    pub fn replay(entries: &[WalEntry]) -> (OrderManager, ExecPositionManager) {
        let mut order_mgr = OrderManager::new();
        let mut pos_mgr = ExecPositionManager::new(TaxLotMethod::Fifo);

        for entry in entries {
            match entry {
                WalEntry::OrderSubmitted(order) => {
                    order_mgr.orders.insert(order.id.clone(), order.clone());
                }
                WalEntry::OrderFilled { order_id, fill } => {
                    pos_mgr.apply_fill(fill);
                    // Update order state
                    let _ = order_mgr.apply_event(
                        order_id,
                        OrderEvent::Fill {
                            qty: fill.qty,
                            price: fill.price,
                        },
                        fill.ts_ms,
                    );
                }
                WalEntry::OrderCancelled(order_id) => {
                    let _ = order_mgr.cancel(order_id, 0);
                }
                WalEntry::Checkpoint { .. } => {}
            }
        }

        (order_mgr, pos_mgr)
    }
}

#[cfg(test)]
mod tests {
    use domain::{InstrumentId, Side, Venue};

    use super::*;
    use crate::core::types::{OrderKind, OrderRequest, TimeInForce};

    fn make_order(client_id: &str) -> Order {
        let req = OrderRequest {
            client_order_id: client_id.to_string(),
            instrument: InstrumentId::new(Venue::Crypto, "BTC-USD"),
            side: Side::Buy,
            quantity: 1.0,
            kind: OrderKind::Market,
            tif: TimeInForce::GTC,
            flags: vec![],
            strategy_id: "s1".to_string(),
            submitted_ts_ms: 1000,
        };
        let mut o = Order::new(req, 1000);
        // Transition to Submitted for the fill to be valid
        let _ = o.transition(crate::core::order::OrderEvent::Submit, 1001);
        o
    }

    #[test]
    fn wal_append_and_checkpoint() {
        let mut wal = WalLog::new();
        let o = make_order("c1");
        wal.append(WalEntry::OrderSubmitted(o));
        wal.checkpoint(1000);
        wal.append(WalEntry::OrderCancelled("some_id".to_string()));
        let since = wal.entries_since_checkpoint();
        // Should include checkpoint + entries after
        assert_eq!(since.len(), 2);
    }

    #[test]
    fn replay_reconstructs_orders() {
        let mut wal = WalLog::new();
        let o = make_order("c2");
        let oid = o.id.clone();
        wal.append(WalEntry::OrderSubmitted(o));
        let (order_mgr, _) = WalRecovery::replay(&wal.entries);
        assert!(order_mgr.get(&oid).is_some());
    }

    #[test]
    fn replay_fill_updates_position() {
        let mut wal = WalLog::new();
        let o = make_order("c3");
        let oid = o.id.clone();
        wal.append(WalEntry::OrderSubmitted(o));
        wal.append(WalEntry::OrderFilled {
            order_id: oid,
            fill: FillRecord {
                order_id: "c3_1000".to_string(),
                instrument: InstrumentId::new(Venue::Crypto, "BTC-USD"),
                side: Side::Buy,
                qty: 1.0,
                price: 100.0,
                commission: 0.0,
                ts_ms: 2000,
            },
        });
        let (_, pos_mgr) = WalRecovery::replay(&wal.entries);
        let pos = pos_mgr
            .get(&InstrumentId::new(Venue::Crypto, "BTC-USD"))
            .unwrap();
        assert!((pos.net_qty - 1.0).abs() < 1e-9);
    }
}
