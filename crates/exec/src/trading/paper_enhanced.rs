use std::collections::HashMap;

use domain::InstrumentId;

use crate::core::{
    ExecPosition, ExecPositionManager, FillRecord, OrderError, OrderEvent, OrderKind, OrderManager,
    OrderRequest, OrderState, TaxLotMethod,
};

pub struct EnhancedPaperAdapter {
    pub order_manager: OrderManager,
    pub position_manager: ExecPositionManager,
    pub fill_delay_ms: u64,
    pub slippage_bps: f64,
    pub commission_rate: f64,
    pub market_prices: HashMap<InstrumentId, f64>,
    pub fill_log: Vec<FillRecord>,
}

impl EnhancedPaperAdapter {
    pub fn new(fill_delay_ms: u64, slippage_bps: f64, commission_rate: f64) -> Self {
        Self {
            order_manager: OrderManager::new(),
            position_manager: ExecPositionManager::new(TaxLotMethod::Fifo),
            fill_delay_ms,
            slippage_bps,
            commission_rate,
            market_prices: HashMap::new(),
            fill_log: Vec::new(),
        }
    }

    pub fn set_price(&mut self, instrument: InstrumentId, price: f64) {
        self.market_prices.insert(instrument, price);
    }

    pub fn submit(&mut self, request: OrderRequest, ts_ms: i64) -> Result<String, OrderError> {
        let id = self.order_manager.submit(request, ts_ms)?;
        // Transition to Submitted
        let _ = self.order_manager.apply_event(&id, OrderEvent::Submit, ts_ms);
        Ok(id)
    }

    /// Attempt to fill pending/submitted orders. Returns fills generated this tick.
    pub fn tick(&mut self, ts_ms: i64) -> Vec<FillRecord> {
        let mut new_fills = Vec::new();
        let order_ids: Vec<String> = self.order_manager.orders.keys().cloned().collect();

        for order_id in order_ids {
            let order = match self.order_manager.orders.get(&order_id) {
                Some(o) => o,
                None => continue,
            };
            if order.state.is_terminal() {
                continue;
            }
            // Only fill Submitted orders
            if !matches!(order.state, OrderState::Submitted | OrderState::PartiallyFilled { .. }) {
                continue;
            }
            // Check fill delay
            let age_ms = (ts_ms - order.created_ts_ms) as u64;
            if age_ms < self.fill_delay_ms {
                continue;
            }

            let instrument = order.request.instrument.clone();
            let market_price = match self.market_prices.get(&instrument) {
                Some(&p) => p,
                None => continue,
            };

            let side = order.request.side;
            let qty = order.remaining_qty();
            let kind = order.request.kind.clone();
            let submitted_ts = order.request.submitted_ts_ms;

            // Determine if order should fill
            let should_fill = match &kind {
                OrderKind::Market => true,
                OrderKind::Limit { price } => {
                    // Buy: fill if market <= limit; Sell: fill if market >= limit
                    match side {
                        domain::Side::Buy => market_price <= *price,
                        domain::Side::Sell => market_price >= *price,
                    }
                }
                OrderKind::Stop { stop } => match side {
                    domain::Side::Buy => market_price >= *stop,
                    domain::Side::Sell => market_price <= *stop,
                },
                _ => true, // other types fill immediately in paper mode
            };

            if !should_fill {
                continue;
            }

            // Apply slippage
            let slippage_factor = self.slippage_bps / 10_000.0;
            let fill_price = match side {
                domain::Side::Buy => market_price * (1.0 + slippage_factor),
                domain::Side::Sell => market_price * (1.0 - slippage_factor),
            };
            let notional = qty * fill_price;
            let commission = notional * self.commission_rate;

            let fill = FillRecord {
                order_id: order_id.clone(),
                instrument: instrument.clone(),
                side,
                qty,
                price: fill_price,
                commission,
                ts_ms,
            };

            // Apply fill event to order
            let _ = self.order_manager.apply_event(
                &order_id,
                OrderEvent::Fill { qty, price: fill_price },
                ts_ms,
            );

            self.position_manager.apply_fill(&fill);
            self.fill_log.push(fill.clone());
            new_fills.push(fill);
        }

        new_fills
    }

    pub fn get_position(&self, instrument: &InstrumentId) -> Option<&ExecPosition> {
        self.position_manager.get(instrument)
    }
}

#[cfg(test)]
mod tests {
    use domain::{InstrumentId, Side, Venue};

    use super::*;
    use crate::core::types::{OrderKind, TimeInForce};

    fn btc() -> InstrumentId {
        InstrumentId::new(Venue::Crypto, "BTC-USD")
    }

    fn market_req(client_id: &str) -> OrderRequest {
        OrderRequest {
            client_order_id: client_id.to_string(),
            instrument: btc(),
            side: Side::Buy,
            quantity: 1.0,
            kind: OrderKind::Market,
            tif: TimeInForce::GTC,
            flags: vec![],
            strategy_id: "s1".to_string(),
            submitted_ts_ms: 1000,
        }
    }

    #[test]
    fn market_order_fills_with_slippage() {
        let mut adapter = EnhancedPaperAdapter::new(0, 10.0, 0.001);
        adapter.set_price(btc(), 1000.0);
        let id = adapter.submit(market_req("m1"), 1000).unwrap();
        let fills = adapter.tick(1001);
        assert_eq!(fills.len(), 1);
        // 10 bps slippage on buy: 1000 * 1.001 = 1001
        assert!((fills[0].price - 1001.0).abs() < 0.01);
        let pos = adapter.get_position(&btc()).unwrap();
        assert!((pos.net_qty - 1.0).abs() < 1e-9);
    }

    #[test]
    fn limit_order_fills_when_price_crosses() {
        let mut adapter = EnhancedPaperAdapter::new(0, 0.0, 0.0);
        // Limit buy at 1000, market price is 1010 — should NOT fill
        let req = OrderRequest {
            kind: OrderKind::Limit { price: 1000.0 },
            ..market_req("l1")
        };
        adapter.set_price(btc(), 1010.0);
        let id = adapter.submit(req, 1000).unwrap();
        let fills = adapter.tick(1001);
        assert!(fills.is_empty());

        // Lower price to 990 — should now fill
        adapter.set_price(btc(), 990.0);
        let fills = adapter.tick(1002);
        assert_eq!(fills.len(), 1);
    }

    #[test]
    fn fill_delay_respected() {
        let mut adapter = EnhancedPaperAdapter::new(500, 0.0, 0.0);
        adapter.set_price(btc(), 1000.0);
        let id = adapter.submit(market_req("d1"), 1000).unwrap();
        // Tick at ms 1200 — age = 200ms < 500ms delay
        let fills = adapter.tick(1200);
        assert!(fills.is_empty());
        // Tick at ms 1600 — age = 600ms >= 500ms
        let fills = adapter.tick(1600);
        assert_eq!(fills.len(), 1);
    }
}
