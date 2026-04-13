use domain::Side;

use super::commission::CommissionModel;
use super::slippage::{SlippageContext, SlippageModel};

#[derive(Debug, Clone)]
pub struct CostBreakdown {
    pub commission: f64,
    pub slippage: f64,
    pub market_impact: f64,
    pub timing_cost: f64,
    pub total: f64,
}

pub struct ExecutionCostCalculator;

impl ExecutionCostCalculator {
    pub fn calculate(
        price: f64,
        qty: f64,
        side: Side,
        slippage: &dyn SlippageModel,
        commission: &dyn CommissionModel,
        context: &SlippageContext,
        is_maker: bool,
    ) -> CostBreakdown {
        let fill_price = slippage.apply(price, qty, side, context);
        let slippage_cost = match side {
            Side::Buy => (fill_price - price) * qty,
            Side::Sell => (price - fill_price) * qty,
        };
        let commission_cost = commission.calculate(fill_price, qty, is_maker);
        let market_impact = 0.0; // placeholder — would use MarketImpactModel
        let timing_cost = 0.0;
        let total = slippage_cost + commission_cost + market_impact + timing_cost;
        CostBreakdown {
            commission: commission_cost,
            slippage: slippage_cost,
            market_impact,
            timing_cost,
            total,
        }
    }

    pub fn total_cost_bps(breakdown: &CostBreakdown, notional: f64) -> f64 {
        if notional == 0.0 {
            return 0.0;
        }
        (breakdown.total / notional) * 10_000.0
    }

    /// Benchmark vs VWAP. Positive = beat VWAP.
    /// `fills` is `(qty, price)` pairs.
    pub fn benchmark_vs_vwap(fills: &[(f64, f64)], vwap: f64, side: Side) -> f64 {
        if fills.is_empty() || vwap == 0.0 {
            return 0.0;
        }
        let total_qty: f64 = fills.iter().map(|(q, _)| q).sum();
        if total_qty == 0.0 {
            return 0.0;
        }
        let avg_fill = fills.iter().map(|(q, p)| q * p).sum::<f64>() / total_qty;
        // Beat VWAP if: buy at lower than VWAP, or sell at higher than VWAP
        match side {
            Side::Buy => (vwap - avg_fill) / vwap,
            Side::Sell => (avg_fill - vwap) / vwap,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::quality::{FixedSlippage, FlatCommission};

    fn ctx() -> SlippageContext {
        SlippageContext {
            volume_24h: 100_000.0,
            bid_ask_spread: 0.01,
            volatility: 0.1,
            order_book_depth: 5_000.0,
        }
    }

    #[test]
    fn cost_decomposition() {
        let slip = FixedSlippage { bps: 10.0 };
        let comm = FlatCommission { per_trade: 5.0 };
        let breakdown =
            ExecutionCostCalculator::calculate(1000.0, 1.0, Side::Buy, &slip, &comm, &ctx(), false);
        // slippage: 1000 * 0.001 = 1.0
        assert!(
            (breakdown.slippage - 1.0).abs() < 0.01,
            "slip={}",
            breakdown.slippage
        );
        // commission: 5.0
        assert!((breakdown.commission - 5.0).abs() < 0.01);
        // total: 6.0
        assert!((breakdown.total - 6.0).abs() < 0.01);
    }

    #[test]
    fn total_cost_bps() {
        let breakdown = CostBreakdown {
            commission: 5.0,
            slippage: 1.0,
            market_impact: 0.0,
            timing_cost: 0.0,
            total: 6.0,
        };
        let bps = ExecutionCostCalculator::total_cost_bps(&breakdown, 1000.0);
        assert!((bps - 60.0).abs() < 0.01);
    }

    #[test]
    fn benchmark_vs_vwap_beat() {
        let fills = vec![(1.0, 990.0)];
        let result = ExecutionCostCalculator::benchmark_vs_vwap(&fills, 1000.0, Side::Buy);
        // Beat VWAP: bought at 990 vs 1000 → positive
        assert!(result > 0.0);
    }
}
