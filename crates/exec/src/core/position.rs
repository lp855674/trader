use std::collections::HashMap;

use domain::{InstrumentId, Side};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum TaxLotMethod {
    Fifo,
    Lifo,
    Hifo,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TaxLot {
    pub qty: f64,
    pub cost_basis: f64,
    pub opened_ts_ms: i64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FillRecord {
    pub order_id: String,
    pub instrument: InstrumentId,
    pub side: Side,
    pub qty: f64,
    pub price: f64,
    pub commission: f64,
    pub ts_ms: i64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ExecPosition {
    pub instrument: InstrumentId,
    pub net_qty: f64,
    pub avg_cost: f64,
    pub lots: Vec<TaxLot>,
    pub realised_pnl: f64,
    pub unrealised_pnl: f64,
}

impl ExecPosition {
    pub fn new(instrument: InstrumentId) -> Self {
        Self {
            instrument,
            net_qty: 0.0,
            avg_cost: 0.0,
            lots: Vec::new(),
            realised_pnl: 0.0,
            unrealised_pnl: 0.0,
        }
    }

    /// Apply a fill and return the realised PnL from this fill.
    pub fn apply_fill(&mut self, fill: &FillRecord, method: TaxLotMethod) -> f64 {
        let mut realised = 0.0;

        match fill.side {
            Side::Buy => {
                // Adding to long position (or reducing short)
                let new_total_qty = self.net_qty + fill.qty;
                if self.net_qty >= 0.0 {
                    // Increasing long position or opening new long
                    if new_total_qty != 0.0 {
                        self.avg_cost =
                            (self.avg_cost * self.net_qty + fill.price * fill.qty) / new_total_qty;
                    }
                    self.net_qty = new_total_qty;
                    self.lots.push(TaxLot {
                        qty: fill.qty,
                        cost_basis: fill.price,
                        opened_ts_ms: fill.ts_ms,
                    });
                } else {
                    // Covering short position
                    let mut remaining = fill.qty;
                    realised = self.close_lots(remaining, fill.price, &method);
                    self.net_qty = new_total_qty;
                    if self.net_qty > 0.0 {
                        // Went past zero — opened a long
                        self.avg_cost = fill.price;
                        self.lots.push(TaxLot {
                            qty: self.net_qty,
                            cost_basis: fill.price,
                            opened_ts_ms: fill.ts_ms,
                        });
                    } else if self.net_qty == 0.0 {
                        self.avg_cost = 0.0;
                        self.lots.clear();
                    }
                }
            }
            Side::Sell => {
                // Adding to short position (or reducing long)
                let new_total_qty = self.net_qty - fill.qty;
                if self.net_qty <= 0.0 {
                    // Increasing short position or opening new short
                    let short_qty = fill.qty;
                    if new_total_qty != 0.0 {
                        let abs_old = self.net_qty.abs();
                        let abs_new = new_total_qty.abs();
                        self.avg_cost =
                            (self.avg_cost * abs_old + fill.price * short_qty) / abs_new;
                    }
                    self.net_qty = new_total_qty;
                    self.lots.push(TaxLot {
                        qty: fill.qty,
                        cost_basis: fill.price,
                        opened_ts_ms: fill.ts_ms,
                    });
                } else {
                    // Reducing long position
                    realised = self.close_lots(fill.qty, fill.price, &method);
                    self.net_qty = new_total_qty;
                    if self.net_qty < 0.0 {
                        // Went past zero — opened a short
                        self.avg_cost = fill.price;
                        self.lots.push(TaxLot {
                            qty: self.net_qty.abs(),
                            cost_basis: fill.price,
                            opened_ts_ms: fill.ts_ms,
                        });
                    } else if self.net_qty == 0.0 {
                        self.avg_cost = 0.0;
                        self.lots.clear();
                    }
                }
            }
        }

        realised -= fill.commission;
        self.realised_pnl += realised;
        realised
    }

    /// Close lots according to method; returns raw PnL (before commission).
    fn close_lots(&mut self, mut qty_to_close: f64, exit_price: f64, method: &TaxLotMethod) -> f64 {
        // Sort lots based on method
        match method {
            TaxLotMethod::Fifo => {
                self.lots
                    .sort_by(|a, b| a.opened_ts_ms.cmp(&b.opened_ts_ms));
            }
            TaxLotMethod::Lifo => {
                self.lots
                    .sort_by(|a, b| b.opened_ts_ms.cmp(&a.opened_ts_ms));
            }
            TaxLotMethod::Hifo => {
                // Highest cost first
                self.lots.sort_by(|a, b| {
                    b.cost_basis
                        .partial_cmp(&a.cost_basis)
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
            }
        }

        let mut realised = 0.0;
        let mut new_lots = Vec::new();

        for lot in self.lots.drain(..) {
            if qty_to_close <= 0.0 {
                new_lots.push(lot);
                continue;
            }
            let closed = lot.qty.min(qty_to_close);
            realised += closed * (exit_price - lot.cost_basis);
            qty_to_close -= closed;
            let remaining = lot.qty - closed;
            if remaining > 1e-12 {
                new_lots.push(TaxLot {
                    qty: remaining,
                    ..lot
                });
            }
        }

        self.lots = new_lots;

        // Recalculate avg_cost from remaining lots
        let total_qty: f64 = self.lots.iter().map(|l| l.qty).sum();
        if total_qty > 0.0 {
            self.avg_cost = self.lots.iter().map(|l| l.qty * l.cost_basis).sum::<f64>() / total_qty;
        } else {
            self.avg_cost = 0.0;
        }

        realised
    }

    pub fn update_unrealised(&mut self, current_price: f64) {
        self.unrealised_pnl = self.net_qty * (current_price - self.avg_cost);
    }

    pub fn notional(&self, price: f64) -> f64 {
        self.net_qty.abs() * price
    }
}

#[derive(Debug, Error)]
pub enum PositionError {
    #[error("instrument not found: {0}")]
    InstrumentNotFound(InstrumentId),
    #[error("invalid fill: {0}")]
    InvalidFill(String),
}

pub struct ExecPositionManager {
    pub positions: HashMap<InstrumentId, ExecPosition>,
    pub tax_lot_method: TaxLotMethod,
}

impl ExecPositionManager {
    pub fn new(method: TaxLotMethod) -> Self {
        Self {
            positions: HashMap::new(),
            tax_lot_method: method,
        }
    }

    pub fn apply_fill(&mut self, fill: &FillRecord) {
        let method = self.tax_lot_method.clone();
        let pos = self
            .positions
            .entry(fill.instrument.clone())
            .or_insert_with(|| ExecPosition::new(fill.instrument.clone()));
        pos.apply_fill(fill, method);
    }

    pub fn update_prices(&mut self, prices: &HashMap<InstrumentId, f64>) {
        for (instrument, price) in prices {
            if let Some(pos) = self.positions.get_mut(instrument) {
                pos.update_unrealised(*price);
            }
        }
    }

    pub fn total_unrealised_pnl(&self) -> f64 {
        self.positions.values().map(|p| p.unrealised_pnl).sum()
    }

    pub fn get(&self, instrument: &InstrumentId) -> Option<&ExecPosition> {
        self.positions.get(instrument)
    }
}

#[cfg(test)]
mod tests {
    use domain::{InstrumentId, Side, Venue};

    use super::*;

    fn btc() -> InstrumentId {
        InstrumentId::new(Venue::Crypto, "BTC-USD")
    }

    fn fill(side: Side, qty: f64, price: f64) -> FillRecord {
        FillRecord {
            order_id: "o1".to_string(),
            instrument: btc(),
            side,
            qty,
            price,
            commission: 0.0,
            ts_ms: 1000,
        }
    }

    #[test]
    fn fifo_lot_matching() {
        let mut pos = ExecPosition::new(btc());
        // Buy 2 lots
        pos.apply_fill(
            &FillRecord {
                ts_ms: 1000,
                qty: 1.0,
                price: 100.0,
                side: Side::Buy,
                ..fill(Side::Buy, 1.0, 100.0)
            },
            TaxLotMethod::Fifo,
        );
        pos.apply_fill(
            &FillRecord {
                ts_ms: 2000,
                qty: 1.0,
                price: 120.0,
                side: Side::Buy,
                ..fill(Side::Buy, 1.0, 120.0)
            },
            TaxLotMethod::Fifo,
        );
        // Sell 1 — should close oldest (100.0) first with FIFO
        let pnl = pos.apply_fill(&fill(Side::Sell, 1.0, 130.0), TaxLotMethod::Fifo);
        // PnL = 1 * (130 - 100) = 30
        assert!((pnl - 30.0).abs() < 1e-6, "pnl={}", pnl);
        assert!((pos.net_qty - 1.0).abs() < 1e-9);
    }

    #[test]
    fn vwap_avg_cost_update() {
        let mut pos = ExecPosition::new(btc());
        pos.apply_fill(&fill(Side::Buy, 2.0, 100.0), TaxLotMethod::Fifo);
        pos.apply_fill(&fill(Side::Buy, 3.0, 110.0), TaxLotMethod::Fifo);
        // avg = (200 + 330) / 5 = 106
        assert!((pos.avg_cost - 106.0).abs() < 1e-6);
        assert!((pos.net_qty - 5.0).abs() < 1e-9);
    }

    #[test]
    fn unrealised_pnl_correct() {
        let mut pos = ExecPosition::new(btc());
        pos.apply_fill(&fill(Side::Buy, 10.0, 100.0), TaxLotMethod::Fifo);
        pos.update_unrealised(110.0);
        // 10 * (110 - 100) = 100
        assert!((pos.unrealised_pnl - 100.0).abs() < 1e-6);
    }

    #[test]
    fn lifo_vs_fifo() {
        let mut pos_fifo = ExecPosition::new(btc());
        let mut pos_lifo = ExecPosition::new(btc());
        for p in [&mut pos_fifo, &mut pos_lifo] {
            p.apply_fill(
                &FillRecord {
                    ts_ms: 1000,
                    qty: 1.0,
                    price: 90.0,
                    side: Side::Buy,
                    ..fill(Side::Buy, 1.0, 90.0)
                },
                TaxLotMethod::Fifo,
            );
            p.apply_fill(
                &FillRecord {
                    ts_ms: 2000,
                    qty: 1.0,
                    price: 110.0,
                    side: Side::Buy,
                    ..fill(Side::Buy, 1.0, 110.0)
                },
                TaxLotMethod::Fifo,
            );
        }
        let pnl_fifo = pos_fifo.apply_fill(&fill(Side::Sell, 1.0, 120.0), TaxLotMethod::Fifo);
        let pnl_lifo = pos_lifo.apply_fill(&fill(Side::Sell, 1.0, 120.0), TaxLotMethod::Lifo);
        // FIFO: close lot at 90, pnl = 30; LIFO: close lot at 110, pnl = 10
        assert!((pnl_fifo - 30.0).abs() < 1e-6, "fifo pnl={}", pnl_fifo);
        assert!((pnl_lifo - 10.0).abs() < 1e-6, "lifo pnl={}", pnl_lifo);
    }
}
