use crate::core::r#trait::Kline;
use domain::{InstrumentId, Side};

#[derive(Debug, Clone)]
pub enum OrderType {
    Market,
    Limit { price: f64 },
    StopLimit { stop: f64, limit: f64 },
}

#[derive(Debug, Clone)]
pub struct Order {
    pub id: u64,
    pub instrument: InstrumentId,
    pub side: Side,
    pub order_type: OrderType,
    pub quantity: f64,
    pub ts_submitted_ms: i64,
}

#[derive(Debug, Clone)]
pub struct Fill {
    pub order_id: u64,
    pub instrument: InstrumentId,
    pub side: Side,
    pub fill_price: f64,
    pub fill_qty: f64,
    pub ts_fill_ms: i64,
    pub commission: f64,
}

pub struct SimulatedExecutor {
    pub pending_orders: Vec<Order>,
    pub fills: Vec<Fill>,
    pub commission_rate: f64,
    pub next_order_id: u64,
}

impl SimulatedExecutor {
    pub fn new(commission_rate: f64) -> Self {
        Self {
            pending_orders: Vec::new(),
            fills: Vec::new(),
            commission_rate,
            next_order_id: 1,
        }
    }

    pub fn submit(
        &mut self,
        instrument: InstrumentId,
        side: Side,
        order_type: OrderType,
        quantity: f64,
        ts_ms: i64,
    ) -> u64 {
        let id = self.next_order_id;
        self.next_order_id += 1;
        self.pending_orders.push(Order {
            id,
            instrument,
            side,
            order_type,
            quantity,
            ts_submitted_ms: ts_ms,
        });
        id
    }

    pub fn process_bar(&mut self, kline: &Kline) -> Vec<Fill> {
        let mut new_fills = Vec::new();
        let mut remaining = Vec::new();

        for order in self.pending_orders.drain(..) {
            if order.instrument != kline.instrument {
                remaining.push(order);
                continue;
            }

            let fill_price = match &order.order_type {
                OrderType::Market => Some(kline.open),
                OrderType::Limit { price } => {
                    let triggered = match order.side {
                        Side::Buy => kline.low <= *price,
                        Side::Sell => kline.high >= *price,
                    };
                    if triggered { Some(*price) } else { None }
                }
                OrderType::StopLimit { stop, limit } => {
                    let stop_triggered = match order.side {
                        Side::Buy => kline.high >= *stop,
                        Side::Sell => kline.low <= *stop,
                    };
                    if stop_triggered {
                        // Check limit is still fillable
                        let limit_ok = match order.side {
                            Side::Buy => kline.low <= *limit,
                            Side::Sell => kline.high >= *limit,
                        };
                        if limit_ok { Some(*limit) } else { None }
                    } else {
                        None
                    }
                }
            };

            if let Some(fp) = fill_price {
                let commission = fp * order.quantity * self.commission_rate;
                let fill = Fill {
                    order_id: order.id,
                    instrument: order.instrument.clone(),
                    side: order.side.clone(),
                    fill_price: fp,
                    fill_qty: order.quantity,
                    ts_fill_ms: kline.close_ts_ms,
                    commission,
                };
                new_fills.push(fill.clone());
                self.fills.push(fill);
            } else {
                remaining.push(order);
            }
        }

        self.pending_orders = remaining;
        new_fills
    }

    pub fn all_fills(&self) -> &[Fill] {
        &self.fills
    }

    pub fn pending(&self) -> &[Order] {
        &self.pending_orders
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::{InstrumentId, Side, Venue};

    fn instrument() -> InstrumentId {
        InstrumentId::new(Venue::Crypto, "BTC")
    }

    fn kline(open: f64, high: f64, low: f64, close: f64) -> Kline {
        Kline {
            instrument: instrument(),
            open_ts_ms: 0,
            close_ts_ms: 60_000,
            open,
            high,
            low,
            close,
            volume: 1000.0,
        }
    }

    #[test]
    fn market_order_fills_at_open() {
        let mut exec = SimulatedExecutor::new(0.001);
        exec.submit(instrument(), Side::Buy, OrderType::Market, 1.0, 0);
        let fills = exec.process_bar(&kline(100.0, 110.0, 90.0, 105.0));
        assert_eq!(fills.len(), 1);
        assert_eq!(fills[0].fill_price, 100.0);
    }

    #[test]
    fn limit_buy_fills_when_price_crossed() {
        let mut exec = SimulatedExecutor::new(0.001);
        // Limit buy at 95, bar low is 90 — should fill
        exec.submit(
            instrument(),
            Side::Buy,
            OrderType::Limit { price: 95.0 },
            1.0,
            0,
        );
        let fills = exec.process_bar(&kline(100.0, 110.0, 90.0, 105.0));
        assert_eq!(fills.len(), 1);
        assert_eq!(fills[0].fill_price, 95.0);
    }

    #[test]
    fn limit_buy_no_fill_when_not_crossed() {
        let mut exec = SimulatedExecutor::new(0.001);
        // Limit buy at 85, bar low is 90 — should NOT fill
        exec.submit(
            instrument(),
            Side::Buy,
            OrderType::Limit { price: 85.0 },
            1.0,
            0,
        );
        let fills = exec.process_bar(&kline(100.0, 110.0, 90.0, 105.0));
        assert_eq!(fills.len(), 0);
        assert_eq!(exec.pending().len(), 1);
    }

    #[test]
    fn stop_limit_triggered_correctly() {
        let mut exec = SimulatedExecutor::new(0.001);
        // Stop buy at 105 (triggered when price rises above 105), limit at 106
        exec.submit(
            instrument(),
            Side::Buy,
            OrderType::StopLimit {
                stop: 105.0,
                limit: 106.0,
            },
            1.0,
            0,
        );
        // Bar high = 110 triggers stop; bar low = 90, so limit 106 is fillable
        let fills = exec.process_bar(&kline(100.0, 110.0, 90.0, 108.0));
        assert_eq!(fills.len(), 1);
        assert_eq!(fills[0].fill_price, 106.0);
    }
}
