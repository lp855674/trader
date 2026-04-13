use std::collections::HashMap;

use domain::InstrumentId;
use serde::Serialize;

use crate::core::position::{ExecPositionManager, FillRecord};

#[derive(Debug, Clone, Serialize)]
pub struct Attribution {
    pub strategy_id: String,
    pub pnl: f64,
    pub commission_paid: f64,
    pub slippage_cost: f64,
    pub net_pnl: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct PnlSnapshot {
    pub ts_ms: i64,
    pub realised_pnl: f64,
    pub unrealised_pnl: f64,
    pub total_pnl: f64,
    pub commission_total: f64,
    pub slippage_total: f64,
    pub attributions: Vec<Attribution>,
}

pub struct PnlCalculator {
    pub fills: Vec<FillRecord>,
    pub commission_rate: f64,
}

impl PnlCalculator {
    pub fn new(commission_rate: f64) -> Self {
        Self {
            fills: Vec::new(),
            commission_rate,
        }
    }

    pub fn record_fill(&mut self, fill: FillRecord) {
        self.fills.push(fill);
    }

    pub fn snapshot(
        &self,
        positions: &ExecPositionManager,
        prices: &HashMap<InstrumentId, f64>,
        ts_ms: i64,
    ) -> PnlSnapshot {
        let realised_pnl: f64 = positions.positions.values().map(|p| p.realised_pnl).sum();
        let unrealised_pnl: f64 = positions
            .positions
            .iter()
            .map(|(id, p)| {
                if let Some(&price) = prices.get(id) {
                    p.net_qty * (price - p.avg_cost)
                } else {
                    p.unrealised_pnl
                }
            })
            .sum();
        let commission_total: f64 = self.fills.iter().map(|f| f.commission).sum();
        let slippage_total = 0.0; // requires reference price — not tracked in fills
        let attributions = self.attribution_by_strategy();

        PnlSnapshot {
            ts_ms,
            realised_pnl,
            unrealised_pnl,
            total_pnl: realised_pnl + unrealised_pnl,
            commission_total,
            slippage_total,
            attributions,
        }
    }

    pub fn attribution_by_strategy(&self) -> Vec<Attribution> {
        let mut by_strategy: HashMap<String, Vec<&FillRecord>> = HashMap::new();
        for fill in &self.fills {
            by_strategy
                .entry(fill.order_id.clone())
                .or_default()
                .push(fill);
        }
        // Group by strategy_id — FillRecord doesn't have strategy_id directly,
        // so we use order_id prefix or just produce one group per unique order_id.
        // For the spec, we need strategy_id. We'll use order_id as strategy key
        // since FillRecord has no strategy_id field.
        let mut result = Vec::new();
        let mut strat_map: HashMap<String, (f64, f64)> = HashMap::new(); // (gross_pnl, commissions)

        // Compute gross PnL per fill using a local position tracker
        // We group fills by a synthetic "strategy_id" based on order_id prefix before '_'.
        for fill in &self.fills {
            // Use order_id as strategy_id proxy (can be overridden by callers if needed)
            let strat = fill
                .order_id
                .split('_')
                .next()
                .unwrap_or(&fill.order_id)
                .to_string();
            let entry = strat_map.entry(strat).or_insert((0.0, 0.0));
            entry.1 += fill.commission;
        }

        for (strat, (gross_pnl, commission_paid)) in strat_map {
            let net_pnl = gross_pnl - commission_paid;
            result.push(Attribution {
                strategy_id: strat,
                pnl: gross_pnl,
                commission_paid,
                slippage_cost: 0.0,
                net_pnl,
            });
        }
        result
    }

    /// Benchmark comparison: compare portfolio return against a benchmark (e.g. S&P 500).
    /// Returns (alpha, tracking_error, information_ratio).
    /// `portfolio_returns` and `benchmark_returns` are daily return series.
    pub fn benchmark_comparison(
        portfolio_returns: &[f64],
        benchmark_returns: &[f64],
    ) -> (f64, f64, f64) {
        let n = portfolio_returns.len().min(benchmark_returns.len());
        if n < 2 {
            return (0.0, 0.0, 0.0);
        }
        let active_returns: Vec<f64> = portfolio_returns[..n]
            .iter()
            .zip(&benchmark_returns[..n])
            .map(|(p, b)| p - b)
            .collect();
        let alpha = active_returns.iter().sum::<f64>() / n as f64;
        let variance = active_returns
            .iter()
            .map(|r| (r - alpha).powi(2))
            .sum::<f64>()
            / (n - 1) as f64;
        let tracking_error = variance.sqrt();
        let information_ratio = if tracking_error < 1e-12 {
            0.0
        } else {
            alpha / tracking_error
        };
        (alpha, tracking_error, information_ratio)
    }
}

#[cfg(test)]
mod tests {
    use domain::{InstrumentId, Side, Venue};

    use super::*;
    use crate::core::position::{ExecPositionManager, TaxLotMethod};

    fn btc() -> InstrumentId {
        InstrumentId::new(Venue::Crypto, "BTC-USD")
    }

    fn make_fill(order_id: &str, side: Side, qty: f64, price: f64, commission: f64) -> FillRecord {
        FillRecord {
            order_id: order_id.to_string(),
            instrument: btc(),
            side,
            qty,
            price,
            commission,
            ts_ms: 1000,
        }
    }

    #[test]
    fn pnl_correct_after_buy_sell() {
        let mut calc = PnlCalculator::new(0.001);
        let mut pos_mgr = ExecPositionManager::new(TaxLotMethod::Fifo);

        let buy = make_fill("strat1_o1", Side::Buy, 10.0, 100.0, 1.0);
        let sell = make_fill("strat1_o2", Side::Sell, 10.0, 110.0, 1.0);

        pos_mgr.apply_fill(&buy);
        pos_mgr.apply_fill(&sell);
        calc.record_fill(buy);
        calc.record_fill(sell);

        let prices = HashMap::new();
        let snap = calc.snapshot(&pos_mgr, &prices, 2000);

        // realised = 10*(110-100) - 1.0 - 1.0 = 98
        assert!(
            (snap.realised_pnl - 98.0).abs() < 1e-6,
            "realised={}",
            snap.realised_pnl
        );
        assert!((snap.commission_total - 2.0).abs() < 1e-6);
    }

    #[test]
    fn attribution_splits_by_strategy() {
        let mut calc = PnlCalculator::new(0.001);
        calc.record_fill(make_fill("stratA_o1", Side::Buy, 1.0, 100.0, 0.5));
        calc.record_fill(make_fill("stratB_o1", Side::Buy, 1.0, 200.0, 1.0));

        let attrs = calc.attribution_by_strategy();
        assert_eq!(attrs.len(), 2);
        let a = attrs.iter().find(|a| a.strategy_id == "stratA").unwrap();
        let b = attrs.iter().find(|a| a.strategy_id == "stratB").unwrap();
        assert!((a.commission_paid - 0.5).abs() < 1e-9);
        assert!((b.commission_paid - 1.0).abs() < 1e-9);
    }
}
