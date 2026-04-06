use domain::Side;

pub struct SlippageContext {
    pub volume_24h: f64,
    pub bid_ask_spread: f64,
    pub volatility: f64,
    pub order_book_depth: f64,
}

pub trait SlippageModel {
    fn apply(&self, price: f64, qty: f64, side: Side, context: &SlippageContext) -> f64;
}

/// Fixed slippage in basis points.
pub struct FixedSlippage {
    pub bps: f64,
}

impl SlippageModel for FixedSlippage {
    fn apply(&self, price: f64, qty: f64, side: Side, _context: &SlippageContext) -> f64 {
        let _ = qty;
        let factor = self.bps / 10_000.0;
        match side {
            Side::Buy => price * (1.0 + factor),
            Side::Sell => price * (1.0 - factor),
        }
    }
}

/// Volume-proportional slippage: impact = sqrt(qty/volume_24h) * impact_factor.
pub struct VolumeSlippage {
    pub impact_factor: f64,
}

impl SlippageModel for VolumeSlippage {
    fn apply(&self, price: f64, qty: f64, side: Side, context: &SlippageContext) -> f64 {
        let impact = if context.volume_24h > 0.0 {
            (qty / context.volume_24h).sqrt() * self.impact_factor
        } else {
            0.0
        };
        match side {
            Side::Buy => price * (1.0 + impact),
            Side::Sell => price * (1.0 - impact),
        }
    }
}

/// Depth-based slippage: walks the order book.
pub struct DepthSlippage {
    pub depth_factor: f64,
}

impl SlippageModel for DepthSlippage {
    fn apply(&self, price: f64, qty: f64, side: Side, context: &SlippageContext) -> f64 {
        let impact = if context.order_book_depth > 0.0 {
            (qty / context.order_book_depth) * self.depth_factor
        } else {
            0.0
        };
        match side {
            Side::Buy => price * (1.0 + impact),
            Side::Sell => price * (1.0 - impact),
        }
    }
}

/// Adaptive slippage that scales with volatility.
pub struct AdaptiveSlippage {
    pub base_bps: f64,
    pub vol_scale: f64,
}

impl SlippageModel for AdaptiveSlippage {
    fn apply(&self, price: f64, qty: f64, side: Side, context: &SlippageContext) -> f64 {
        let _ = qty;
        let bps = self.base_bps * (1.0 + self.vol_scale * context.volatility);
        let factor = bps / 10_000.0;
        match side {
            Side::Buy => price * (1.0 + factor),
            Side::Sell => price * (1.0 - factor),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> SlippageContext {
        SlippageContext {
            volume_24h: 10_000.0,
            bid_ask_spread: 0.01,
            volatility: 0.2,
            order_book_depth: 1_000.0,
        }
    }

    #[test]
    fn fixed_slippage_buy() {
        let model = FixedSlippage { bps: 10.0 };
        let price = model.apply(1000.0, 1.0, Side::Buy, &ctx());
        assert!((price - 1001.0).abs() < 0.001);
    }

    #[test]
    fn fixed_slippage_sell() {
        let model = FixedSlippage { bps: 10.0 };
        let price = model.apply(1000.0, 1.0, Side::Sell, &ctx());
        assert!((price - 999.0).abs() < 0.001);
    }

    #[test]
    fn volume_slippage_buy() {
        let model = VolumeSlippage { impact_factor: 1.0 };
        // qty=100, volume=10000 → sqrt(0.01)=0.1 → price*1.1
        let price = model.apply(1000.0, 100.0, Side::Buy, &ctx());
        assert!((price - 1100.0).abs() < 0.001);
    }

    #[test]
    fn adaptive_slippage_scales_with_vol() {
        let model = AdaptiveSlippage { base_bps: 10.0, vol_scale: 1.0 };
        // bps = 10 * (1 + 1.0 * 0.2) = 12 bps
        let price = model.apply(1000.0, 1.0, Side::Buy, &ctx());
        assert!((price - 1001.2).abs() < 0.001);
    }
}
