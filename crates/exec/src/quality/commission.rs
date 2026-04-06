pub trait CommissionModel {
    fn calculate(&self, price: f64, qty: f64, is_maker: bool) -> f64;
}

/// Tiered commission: sorted by (notional_threshold, rate).
/// Use the rate from the lowest tier whose threshold the notional meets.
pub struct TieredCommission {
    /// Each tuple: (notional_threshold, rate_fraction)
    /// Tiers should be sorted ascending by threshold.
    pub tiers: Vec<(f64, f64)>,
}

impl CommissionModel for TieredCommission {
    fn calculate(&self, price: f64, qty: f64, _is_maker: bool) -> f64 {
        let notional = price * qty;
        // Find the lowest matching rate — use the last tier whose threshold <= notional
        let mut rate = self.tiers.last().map(|(_, r)| *r).unwrap_or(0.0);
        for &(threshold, tier_rate) in &self.tiers {
            if notional >= threshold {
                rate = tier_rate;
            }
        }
        notional * rate
    }
}

/// Maker/taker fee. Negative maker_rate = rebate.
pub struct MakerTakerFee {
    pub maker_rate: f64,
    pub taker_rate: f64,
    pub min_fee: f64,
}

impl CommissionModel for MakerTakerFee {
    fn calculate(&self, price: f64, qty: f64, is_maker: bool) -> f64 {
        let notional = price * qty;
        let rate = if is_maker { self.maker_rate } else { self.taker_rate };
        let fee = notional * rate;
        // Apply min fee only for positive fees; rebates pass through
        if rate >= 0.0 {
            fee.max(self.min_fee)
        } else {
            fee
        }
    }
}

/// Flat fee per trade.
pub struct FlatCommission {
    pub per_trade: f64,
}

impl CommissionModel for FlatCommission {
    fn calculate(&self, _price: f64, _qty: f64, _is_maker: bool) -> f64 {
        self.per_trade
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tiered_commission_selects_correct_tier() {
        // threshold 0 → 0.1%, threshold 10000 → 0.05%
        let model = TieredCommission {
            tiers: vec![(0.0, 0.001), (10_000.0, 0.0005)],
        };
        // notional = 5000 < 10000 → 0.1%
        let fee = model.calculate(500.0, 10.0, false);
        assert!((fee - 5.0).abs() < 1e-9, "fee={}", fee);
        // notional = 20000 >= 10000 → 0.05%
        let fee2 = model.calculate(2000.0, 10.0, false);
        assert!((fee2 - 10.0).abs() < 1e-9, "fee2={}", fee2);
    }

    #[test]
    fn maker_rebate() {
        let model = MakerTakerFee { maker_rate: -0.0002, taker_rate: 0.0004, min_fee: 0.10 };
        // Maker rebate: 1000 * -0.0002 = -0.20
        let rebate = model.calculate(1000.0, 1.0, true);
        assert!((rebate - (-0.20)).abs() < 1e-9);
        // Taker: max(1000 * 0.0004, 0.10) = max(0.40, 0.10) = 0.40
        let taker = model.calculate(1000.0, 1.0, false);
        assert!((taker - 0.40).abs() < 1e-9);
    }

    #[test]
    fn flat_commission() {
        let model = FlatCommission { per_trade: 5.0 };
        assert_eq!(model.calculate(999.0, 100.0, true), 5.0);
    }
}
