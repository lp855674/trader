// Paper trading adapter — simulates exchange execution with configurable
// slippage, commission and fill latency.

use std::collections::HashMap;

use domain::{InstrumentId, Side};

// ─── PaperConfig ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct PaperConfig {
    pub initial_capital: f64,
    pub commission_rate: f64,
    pub slippage_bps: f64,
    pub max_positions: usize,
    pub fill_delay_ms: u64,
}

impl Default for PaperConfig {
    fn default() -> Self {
        Self {
            initial_capital: 100_000.0,
            commission_rate: 0.001,
            slippage_bps: 5.0,
            max_positions: 10,
            fill_delay_ms: 50,
        }
    }
}

// ─── MarketDataSnapshot ───────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct MarketDataSnapshot {
    pub instrument: InstrumentId,
    pub bid: f64,
    pub ask: f64,
    pub last: f64,
    pub ts_ms: i64,
    pub volume_24h: f64,
}

impl MarketDataSnapshot {
    pub fn mid_price(&self) -> f64 {
        (self.bid + self.ask) / 2.0
    }
}

// ─── PendingOrder ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct PendingOrder {
    pub id: u64,
    pub instrument: InstrumentId,
    pub side: Side,
    pub quantity: f64,
    pub limit_price: Option<f64>,
    pub submitted_ts_ms: i64,
    pub fill_after_ts_ms: i64,
}

// ─── Fill ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct Fill {
    pub order_id: u64,
    pub instrument: InstrumentId,
    pub side: Side,
    pub fill_price: f64,
    pub fill_qty: f64,
    pub commission: f64,
    pub ts_ms: i64,
}

// ─── PaperState ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct PaperState {
    /// Available cash
    pub capital: f64,
    /// instrument → (qty, avg_price)
    pub positions: HashMap<InstrumentId, (f64, f64)>,
    pub fills: Vec<Fill>,
    pub pending_orders: Vec<PendingOrder>,
    pub ts_ms: i64,
}

impl PaperState {
    pub fn new(initial_capital: f64) -> Self {
        Self {
            capital: initial_capital,
            positions: HashMap::new(),
            fills: Vec::new(),
            pending_orders: Vec::new(),
            ts_ms: 0,
        }
    }

    /// Total equity = cash + sum of open position mark-to-market values.
    pub fn total_equity(&self, market_data: &HashMap<InstrumentId, MarketDataSnapshot>) -> f64 {
        let mtm: f64 = self
            .positions
            .iter()
            .map(|(inst, (qty, _avg))| {
                let price = market_data
                    .get(inst)
                    .map(|s| s.last)
                    .unwrap_or(0.0);
                qty * price
            })
            .sum();
        self.capital + mtm
    }

    /// Unrealised PnL across all open positions.
    pub fn unrealised_pnl(&self, market_data: &HashMap<InstrumentId, MarketDataSnapshot>) -> f64 {
        self.positions
            .iter()
            .map(|(inst, (qty, avg_price))| {
                let price = market_data
                    .get(inst)
                    .map(|s| s.last)
                    .unwrap_or(*avg_price);
                (price - avg_price) * qty
            })
            .sum()
    }
}

// ─── PaperAdapter ────────────────────────────────────────────────────────────

pub struct PaperAdapter {
    pub config: PaperConfig,
    pub state: PaperState,
    pub next_order_id: u64,
}

impl PaperAdapter {
    pub fn new(config: PaperConfig) -> Self {
        let capital = config.initial_capital;
        Self {
            state: PaperState::new(capital),
            config,
            next_order_id: 1,
        }
    }

    /// Submit an order.  Returns the assigned order ID.
    pub fn submit_order(
        &mut self,
        instrument: InstrumentId,
        side: Side,
        qty: f64,
        limit_price: Option<f64>,
        ts_ms: i64,
    ) -> u64 {
        let id = self.next_order_id;
        self.next_order_id += 1;
        let fill_after_ts_ms = ts_ms + self.config.fill_delay_ms as i64;
        self.state.pending_orders.push(PendingOrder {
            id,
            instrument,
            side,
            quantity: qty,
            limit_price,
            submitted_ts_ms: ts_ms,
            fill_after_ts_ms,
        });
        id
    }

    /// Process pending orders that are due to fill.  Returns new fills.
    pub fn process_market_data(
        &mut self,
        snapshots: &HashMap<InstrumentId, MarketDataSnapshot>,
        ts_ms: i64,
    ) -> Vec<Fill> {
        let mut new_fills = Vec::new();
        let mut remaining = Vec::new();

        for order in self.state.pending_orders.drain(..) {
            if order.fill_after_ts_ms <= ts_ms {
                if let Some(snap) = snapshots.get(&order.instrument) {
                    let raw_price = snap.last;
                    let fill_price = match order.side {
                        Side::Buy => raw_price * (1.0 + self.config.slippage_bps / 10_000.0),
                        Side::Sell => raw_price * (1.0 - self.config.slippage_bps / 10_000.0),
                    };
                    let notional = fill_price * order.quantity;
                    let commission = notional * self.config.commission_rate;

                    // Update capital and positions
                    match order.side {
                        Side::Buy => {
                            self.state.capital -= notional + commission;
                            let entry = self
                                .state
                                .positions
                                .entry(order.instrument.clone())
                                .or_insert((0.0, 0.0));
                            let new_qty = entry.0 + order.quantity;
                            let new_avg = (entry.0 * entry.1 + order.quantity * fill_price) / new_qty;
                            *entry = (new_qty, new_avg);
                        }
                        Side::Sell => {
                            self.state.capital += notional - commission;
                            let entry = self
                                .state
                                .positions
                                .entry(order.instrument.clone())
                                .or_insert((0.0, 0.0));
                            entry.0 -= order.quantity;
                            if entry.0 <= 0.0 {
                                self.state.positions.remove(&order.instrument);
                            }
                        }
                    }

                    let fill = Fill {
                        order_id: order.id,
                        instrument: order.instrument,
                        side: order.side,
                        fill_price,
                        fill_qty: order.quantity,
                        commission,
                        ts_ms,
                    };
                    self.state.fills.push(fill.clone());
                    new_fills.push(fill);
                } else {
                    // No market data — keep pending
                    remaining.push(order);
                }
            } else {
                remaining.push(order);
            }
        }

        self.state.pending_orders = remaining;
        new_fills
    }

    /// Cancel an order by ID.  Returns true if found and cancelled.
    pub fn cancel_order(&mut self, order_id: u64) -> bool {
        let before = self.state.pending_orders.len();
        self.state.pending_orders.retain(|o| o.id != order_id);
        self.state.pending_orders.len() < before
    }

    /// Update the state timestamp; could be extended to mark expired orders.
    pub fn sync_state(&mut self, ts_ms: i64) {
        self.state.ts_ms = ts_ms;
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use domain::{InstrumentId, Venue};

    fn btc() -> InstrumentId {
        InstrumentId::new(Venue::Crypto, "BTC")
    }

    fn snap(price: f64) -> MarketDataSnapshot {
        MarketDataSnapshot {
            instrument: btc(),
            bid: price - 1.0,
            ask: price + 1.0,
            last: price,
            ts_ms: 1000,
            volume_24h: 100.0,
        }
    }

    fn snapshots(price: f64) -> HashMap<InstrumentId, MarketDataSnapshot> {
        let mut m = HashMap::new();
        m.insert(btc(), snap(price));
        m
    }

    #[test]
    fn submit_and_fill_order() {
        let cfg = PaperConfig {
            initial_capital: 10_000.0,
            commission_rate: 0.001,
            slippage_bps: 0.0, // no slippage for simplicity
            fill_delay_ms: 0,
            max_positions: 10,
        };
        let mut adapter = PaperAdapter::new(cfg);

        let id = adapter.submit_order(btc(), Side::Buy, 1.0, None, 1000);
        assert_eq!(id, 1);

        let fills = adapter.process_market_data(&snapshots(100.0), 1000);
        assert_eq!(fills.len(), 1);
        let fill = &fills[0];
        assert_eq!(fill.fill_price, 100.0);
        assert_eq!(fill.fill_qty, 1.0);
        // commission = 100 * 0.001 = 0.1
        assert!((fill.commission - 0.1).abs() < 1e-9);
        // capital should drop by 100.0 + 0.1
        assert!((adapter.state.capital - 9_899.9).abs() < 1e-6);
    }

    #[test]
    fn slippage_applied_on_buy() {
        let cfg = PaperConfig {
            initial_capital: 100_000.0,
            commission_rate: 0.0,
            slippage_bps: 10.0, // 10 bps
            fill_delay_ms: 0,
            max_positions: 10,
        };
        let mut adapter = PaperAdapter::new(cfg);
        adapter.submit_order(btc(), Side::Buy, 1.0, None, 0);
        let fills = adapter.process_market_data(&snapshots(1000.0), 0);
        let fp = fills[0].fill_price;
        let expected = 1000.0 * (1.0 + 10.0 / 10_000.0);
        assert!((fp - expected).abs() < 1e-9);
    }

    #[test]
    fn slippage_applied_on_sell() {
        let cfg = PaperConfig {
            initial_capital: 100_000.0,
            commission_rate: 0.0,
            slippage_bps: 10.0,
            fill_delay_ms: 0,
            max_positions: 10,
        };
        let mut adapter = PaperAdapter::new(cfg);
        // First buy some
        adapter.submit_order(btc(), Side::Buy, 2.0, None, 0);
        adapter.process_market_data(&snapshots(1000.0), 0);
        // Then sell
        let cap_before = adapter.state.capital;
        adapter.submit_order(btc(), Side::Sell, 1.0, None, 0);
        let fills = adapter.process_market_data(&snapshots(1000.0), 0);
        let fp = fills[0].fill_price;
        let expected = 1000.0 * (1.0 - 10.0 / 10_000.0);
        assert!((fp - expected).abs() < 1e-9);
        // capital should increase
        assert!(adapter.state.capital > cap_before);
    }

    #[test]
    fn fill_delay_respected() {
        let cfg = PaperConfig {
            initial_capital: 10_000.0,
            commission_rate: 0.0,
            slippage_bps: 0.0,
            fill_delay_ms: 100,
            max_positions: 10,
        };
        let mut adapter = PaperAdapter::new(cfg);
        adapter.submit_order(btc(), Side::Buy, 1.0, None, 1000);

        // ts_ms = 1050 — not yet due
        let fills = adapter.process_market_data(&snapshots(50.0), 1050);
        assert!(fills.is_empty());

        // ts_ms = 1100 — exactly due
        let fills = adapter.process_market_data(&snapshots(50.0), 1100);
        assert_eq!(fills.len(), 1);
    }

    #[test]
    fn cancel_order() {
        let cfg = PaperConfig::default();
        let mut adapter = PaperAdapter::new(cfg);
        let id = adapter.submit_order(btc(), Side::Buy, 1.0, None, 0);
        assert!(adapter.cancel_order(id));
        assert!(!adapter.cancel_order(id)); // already removed
        assert!(adapter.state.pending_orders.is_empty());
    }

    #[test]
    fn mid_price() {
        let s = snap(100.0);
        assert!((s.mid_price() - 100.0).abs() < 1e-9);
    }

    #[test]
    fn total_equity_and_unrealised_pnl() {
        let cfg = PaperConfig {
            initial_capital: 10_000.0,
            commission_rate: 0.0,
            slippage_bps: 0.0,
            fill_delay_ms: 0,
            max_positions: 10,
        };
        let mut adapter = PaperAdapter::new(cfg);
        adapter.submit_order(btc(), Side::Buy, 1.0, None, 0);
        adapter.process_market_data(&snapshots(100.0), 0);

        // Position: 1 BTC at 100, capital = 9900
        let snaps = snapshots(150.0);
        let equity = adapter.state.total_equity(&snaps);
        // 9900 + 1*150 = 10050
        assert!((equity - 10_050.0).abs() < 1e-6);
        let upnl = adapter.state.unrealised_pnl(&snaps);
        assert!((upnl - 50.0).abs() < 1e-6);
    }
}
