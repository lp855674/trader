use domain::Side;

#[derive(Debug, Clone)]
pub struct ImpactMetrics {
    pub vwap_deviation: f64,
    pub implementation_shortfall: f64,
    pub market_impact_bps: f64,
    pub timing_cost_bps: f64,
}

pub struct MarketImpactModel;

impl MarketImpactModel {
    pub fn new() -> Self {
        Self
    }

    /// VWAP deviation: weighted average fill price vs market VWAP.
    /// `fills` is `(qty, price)` pairs.
    pub fn calculate_vwap_deviation(fills: &[(f64, f64)], market_vwap: f64) -> f64 {
        let total_qty: f64 = fills.iter().map(|(q, _)| q).sum();
        if total_qty == 0.0 || market_vwap == 0.0 {
            return 0.0;
        }
        let avg_fill = fills.iter().map(|(q, p)| q * p).sum::<f64>() / total_qty;
        (avg_fill - market_vwap) / market_vwap
    }

    /// Implementation shortfall: (avg_fill - decision_price) * sign(side) / decision_price.
    pub fn implementation_shortfall(decision_price: f64, fills: &[(f64, f64)], side: Side) -> f64 {
        if decision_price == 0.0 || fills.is_empty() {
            return 0.0;
        }
        let total_qty: f64 = fills.iter().map(|(q, _)| q).sum();
        if total_qty == 0.0 {
            return 0.0;
        }
        let avg_fill = fills.iter().map(|(q, p)| q * p).sum::<f64>() / total_qty;
        let sign = match side {
            Side::Buy => 1.0,
            Side::Sell => -1.0,
        };
        (avg_fill - decision_price) * sign / decision_price
    }

    /// Almgren-Chriss simplified market impact estimate in bps.
    /// impact = σ * sqrt(qty/adv) * 0.1 * 10000
    pub fn estimate_impact(qty: f64, adv: f64, volatility: f64) -> f64 {
        if adv == 0.0 {
            return 0.0;
        }
        volatility * (qty / adv).sqrt() * 0.1 * 10_000.0
    }
}

impl Default for MarketImpactModel {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vwap_deviation_zero_when_matched() {
        let fills = vec![(1.0, 100.0), (2.0, 100.0)];
        let dev = MarketImpactModel::calculate_vwap_deviation(&fills, 100.0);
        assert!(dev.abs() < 1e-9);
    }

    #[test]
    fn implementation_shortfall_buy_positive() {
        // Decision at 100, filled at 102 → shortfall = (102-100)/100 = 0.02
        let fills = vec![(1.0, 102.0)];
        let is = MarketImpactModel::implementation_shortfall(100.0, &fills, Side::Buy);
        assert!((is - 0.02).abs() < 1e-9);
    }

    #[test]
    fn impact_estimate() {
        // qty=100, adv=10000, vol=0.02 → 0.02 * sqrt(0.01) * 0.1 * 10000 = 0.02 * 0.1 * 0.1 * 10000 = 2
        let impact = MarketImpactModel::estimate_impact(100.0, 10_000.0, 0.02);
        assert!((impact - 2.0).abs() < 1e-6, "impact={}", impact);
    }
}
