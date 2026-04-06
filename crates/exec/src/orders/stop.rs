use domain::{InstrumentId, Side};

#[derive(Debug, Clone, PartialEq)]
pub enum StopOrderState {
    Dormant,
    Triggered,
    Submitted(String),
    Filled,
    Cancelled,
}

#[derive(Debug, Clone)]
pub struct StopOrder {
    pub instrument: InstrumentId,
    pub side: Side,
    pub stop_price: f64,
    /// None = stop market; Some = stop limit
    pub limit_price: Option<f64>,
    pub quantity: f64,
    pub state: StopOrderState,
}

impl StopOrder {
    pub fn new(
        instrument: InstrumentId,
        side: Side,
        stop_price: f64,
        limit_price: Option<f64>,
        quantity: f64,
    ) -> Self {
        Self {
            instrument,
            side,
            stop_price,
            limit_price,
            quantity,
            state: StopOrderState::Dormant,
        }
    }

    /// Check if the stop should trigger given the current market price.
    /// Returns true if triggered (state changes to Triggered).
    pub fn check_trigger(&mut self, market_price: f64) -> bool {
        if self.state != StopOrderState::Dormant {
            return false;
        }
        let triggered = match self.side {
            Side::Buy => market_price >= self.stop_price,
            Side::Sell => market_price <= self.stop_price,
        };
        if triggered {
            self.state = StopOrderState::Triggered;
        }
        triggered
    }
}

#[derive(Debug, Clone)]
pub struct TrailingStop {
    pub instrument: InstrumentId,
    pub side: Side,
    pub quantity: f64,
    pub trail_amount: f64,
    pub peak_price: f64,
    pub current_stop: f64,
    pub activated: bool,
}

impl TrailingStop {
    pub fn new(instrument: InstrumentId, side: Side, quantity: f64, trail_amount: f64) -> Self {
        Self {
            instrument,
            side,
            quantity,
            trail_amount,
            peak_price: 0.0,
            current_stop: 0.0,
            activated: false,
        }
    }

    /// Activate the trailing stop from the entry price.
    pub fn activate(&mut self, entry_price: f64) {
        self.peak_price = entry_price;
        self.current_stop = match self.side {
            Side::Buy => entry_price + self.trail_amount,
            Side::Sell => entry_price - self.trail_amount,
        };
        self.activated = true;
    }

    /// Update with new market price. Returns true if the stop is triggered.
    pub fn update_price(&mut self, market_price: f64) -> bool {
        if !self.activated {
            return false;
        }
        match self.side {
            Side::Sell => {
                // Long position: trail upward, trigger if price drops to stop
                if market_price > self.peak_price {
                    self.peak_price = market_price;
                    self.current_stop = market_price - self.trail_amount;
                }
                market_price <= self.current_stop
            }
            Side::Buy => {
                // Short position: trail downward, trigger if price rises to stop
                if market_price < self.peak_price {
                    self.peak_price = market_price;
                    self.current_stop = market_price + self.trail_amount;
                }
                market_price >= self.current_stop
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use domain::Venue;

    use super::*;

    fn btc() -> InstrumentId {
        InstrumentId::new(Venue::Crypto, "BTC-USD")
    }

    #[test]
    fn stop_order_triggers_buy() {
        let mut order = StopOrder::new(btc(), Side::Buy, 1050.0, None, 1.0);
        assert!(!order.check_trigger(1000.0));
        assert_eq!(order.state, StopOrderState::Dormant);
        assert!(order.check_trigger(1055.0));
        assert_eq!(order.state, StopOrderState::Triggered);
    }

    #[test]
    fn stop_order_triggers_sell() {
        let mut order = StopOrder::new(btc(), Side::Sell, 950.0, None, 1.0);
        assert!(!order.check_trigger(1000.0));
        assert!(order.check_trigger(945.0));
        assert_eq!(order.state, StopOrderState::Triggered);
    }

    #[test]
    fn trailing_stop_follows_price_up_triggers_on_reversal() {
        let mut ts = TrailingStop::new(btc(), Side::Sell, 1.0, 50.0);
        ts.activate(1000.0);
        // Price rises — stop follows
        assert!(!ts.update_price(1100.0));
        assert!((ts.current_stop - 1050.0).abs() < 1e-9);
        assert!(!ts.update_price(1200.0));
        assert!((ts.current_stop - 1150.0).abs() < 1e-9);
        // Price reverses down to stop
        assert!(ts.update_price(1150.0));
    }
}
