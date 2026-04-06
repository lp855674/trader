use domain::Side;

// ─── Slippage Models ─────────────────────────────────────────────────────────

pub trait SlippageModel: Send + Sync {
    fn apply(&self, price: f64, quantity: f64, side: Side, volume: f64) -> f64;
}

pub struct NoSlippage;

impl SlippageModel for NoSlippage {
    fn apply(&self, price: f64, _quantity: f64, _side: Side, _volume: f64) -> f64 {
        price
    }
}

pub struct FixedSlippage {
    pub ticks: f64,
    pub tick_size: f64,
}

impl SlippageModel for FixedSlippage {
    fn apply(&self, price: f64, _quantity: f64, side: Side, _volume: f64) -> f64 {
        let slip = self.ticks * self.tick_size;
        match side {
            Side::Buy => price + slip,
            Side::Sell => price - slip,
        }
    }
}

pub struct VolumeSlippage {
    pub impact_factor: f64,
}

impl SlippageModel for VolumeSlippage {
    fn apply(&self, price: f64, quantity: f64, side: Side, volume: f64) -> f64 {
        let impact = if volume > 0.0 {
            (quantity / volume) * self.impact_factor
        } else {
            0.0
        };
        match side {
            Side::Buy => price * (1.0 + impact),
            Side::Sell => price * (1.0 - impact),
        }
    }
}

// ─── Commission Models ────────────────────────────────────────────────────────

pub trait CommissionModel: Send + Sync {
    fn calculate(&self, price: f64, quantity: f64) -> f64;
}

pub struct FlatCommission {
    pub per_trade: f64,
}

impl CommissionModel for FlatCommission {
    fn calculate(&self, _price: f64, _quantity: f64) -> f64 {
        self.per_trade
    }
}

pub struct PercentCommission {
    pub rate: f64,
}

impl CommissionModel for PercentCommission {
    fn calculate(&self, price: f64, quantity: f64) -> f64 {
        price * quantity * self.rate
    }
}

/// Tiered commission: Vec<(volume_threshold, rate)> sorted ascending by threshold.
/// Uses the lowest rate where price*qty >= threshold.
pub struct TieredCommission {
    pub tiers: Vec<(f64, f64)>,
}

impl CommissionModel for TieredCommission {
    fn calculate(&self, price: f64, quantity: f64) -> f64 {
        let notional = price * quantity;
        // Use the last tier whose threshold is <= notional (i.e. best rate for largest trades)
        let rate = self
            .tiers
            .iter()
            .filter(|(threshold, _)| notional >= *threshold)
            .last()
            .map(|(_, rate)| *rate)
            .unwrap_or_else(|| {
                // If no tier qualifies, use the first tier's rate
                self.tiers.first().map(|(_, r)| *r).unwrap_or(0.0)
            });
        notional * rate
    }
}

// ─── Combined Cost Model ──────────────────────────────────────────────────────

pub struct CostModel {
    pub slippage: Box<dyn SlippageModel + Send + Sync>,
    pub commission: Box<dyn CommissionModel + Send + Sync>,
}

impl CostModel {
    pub fn new(
        slippage: Box<dyn SlippageModel + Send + Sync>,
        commission: Box<dyn CommissionModel + Send + Sync>,
    ) -> Self {
        Self { slippage, commission }
    }

    /// Returns (adjusted_price, commission_amount)
    pub fn apply_costs(
        &self,
        price: f64,
        qty: f64,
        side: Side,
        bar_volume: f64,
    ) -> (f64, f64) {
        let adjusted = self.slippage.apply(price, qty, side, bar_volume);
        let commission = self.commission.calculate(adjusted, qty);
        (adjusted, commission)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::Side;

    #[test]
    fn no_slippage_unchanged() {
        let m = NoSlippage;
        assert_eq!(m.apply(100.0, 1.0, Side::Buy, 1000.0), 100.0);
        assert_eq!(m.apply(100.0, 1.0, Side::Sell, 1000.0), 100.0);
    }

    #[test]
    fn fixed_slippage_adds_ticks() {
        let m = FixedSlippage { ticks: 2.0, tick_size: 0.5 };
        assert!((m.apply(100.0, 1.0, Side::Buy, 1000.0) - 101.0).abs() < 1e-9);
        assert!((m.apply(100.0, 1.0, Side::Sell, 1000.0) - 99.0).abs() < 1e-9);
    }

    #[test]
    fn volume_slippage_scales_with_quantity() {
        let m = VolumeSlippage { impact_factor: 1.0 };
        // quantity=100, volume=1000 => impact=0.1
        let buy_price = m.apply(100.0, 100.0, Side::Buy, 1000.0);
        assert!((buy_price - 110.0).abs() < 1e-9);
        let sell_price = m.apply(100.0, 100.0, Side::Sell, 1000.0);
        assert!((sell_price - 90.0).abs() < 1e-9);
    }

    #[test]
    fn flat_commission() {
        let m = FlatCommission { per_trade: 5.0 };
        assert_eq!(m.calculate(100.0, 10.0), 5.0);
    }

    #[test]
    fn percent_commission() {
        let m = PercentCommission { rate: 0.001 };
        assert!((m.calculate(100.0, 10.0) - 1.0).abs() < 1e-9);
    }

    #[test]
    fn tiered_commission_uses_correct_tier() {
        let m = TieredCommission {
            tiers: vec![(0.0, 0.002), (1000.0, 0.001), (10000.0, 0.0005)],
        };
        // Small trade: 100 * 5 = 500 < 1000, uses 0.002
        let small = m.calculate(100.0, 5.0);
        assert!((small - 1.0).abs() < 1e-9);
        // Medium trade: 200 * 10 = 2000 >= 1000, uses 0.001
        let med = m.calculate(200.0, 10.0);
        assert!((med - 2.0).abs() < 1e-9);
        // Large trade: 1000 * 15 = 15000 >= 10000, uses 0.0005
        let large = m.calculate(1000.0, 15.0);
        assert!((large - 7.5).abs() < 1e-9);
    }

    #[test]
    fn cost_model_combines_both() {
        let model = CostModel::new(
            Box::new(FixedSlippage { ticks: 1.0, tick_size: 1.0 }),
            Box::new(PercentCommission { rate: 0.001 }),
        );
        let (adj_price, commission) = model.apply_costs(100.0, 1.0, Side::Buy, 1000.0);
        assert!((adj_price - 101.0).abs() < 1e-9);
        assert!((commission - 0.101).abs() < 1e-9);
    }
}
