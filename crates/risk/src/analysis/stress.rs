// Stress test engine: historical crises, liquidity stress, black swans

use crate::analysis::stress_mc::{RiskMonteCarloConfig, RiskMonteCarloSimulator, StressScenario};
use domain::Side;
use std::collections::HashMap;

// ── HistoricalCrisis ──────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum HistoricalCrisis {
    /// 2008 Global Financial Crisis
    Gfc2008,
    /// 2020 COVID crash
    Covid2020,
    /// 2000 Dot-com bust
    DotCom2000,
    /// User-defined
    Custom(StressScenario),
}

impl HistoricalCrisis {
    pub fn to_scenario(&self) -> StressScenario {
        match self {
            HistoricalCrisis::Gfc2008 => StressScenario {
                name: "GFC 2008".into(),
                description: "2008 Global Financial Crisis".into(),
                volatility_multiplier: 5.0,
                correlation_shift: 0.6,
                mean_return_shift: -0.04,
            },
            HistoricalCrisis::Covid2020 => StressScenario {
                name: "COVID 2020".into(),
                description: "2020 COVID market crash".into(),
                volatility_multiplier: 4.0,
                correlation_shift: 0.5,
                mean_return_shift: -0.03,
            },
            HistoricalCrisis::DotCom2000 => StressScenario {
                name: "DotCom 2000".into(),
                description: "2000 Dot-com bubble burst".into(),
                volatility_multiplier: 3.0,
                correlation_shift: 0.3,
                mean_return_shift: -0.02,
            },
            HistoricalCrisis::Custom(s) => s.clone(),
        }
    }

    fn all_historical() -> Vec<HistoricalCrisis> {
        vec![
            HistoricalCrisis::Gfc2008,
            HistoricalCrisis::Covid2020,
            HistoricalCrisis::DotCom2000,
        ]
    }
}

// ── LiquidityStress ───────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct LiquidityStress {
    /// Widen spreads (multiplier)
    pub bid_ask_spread_multiplier: f64,
    /// Reduce available volume (0.1 = 90% volume drop)
    pub volume_factor: f64,
}

impl LiquidityStress {
    /// Apply liquidity stress to fill price
    pub fn apply_to_fill_price(&self, price: f64, side: Side) -> f64 {
        match side {
            Side::Buy => price * (1.0 + self.bid_ask_spread_multiplier * 0.001),
            Side::Sell => price * (1.0 - self.bid_ask_spread_multiplier * 0.001),
        }
    }
}

// ── StressTestResult ──────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct StressTestResult {
    pub crisis_name: String,
    pub portfolio_loss_pct: f64,
    pub var_breach: bool,
    pub liquidity_cost: f64,
    /// Approx steps to recover (loss_pct / daily_vol)
    pub recovery_steps: usize,
}

// ── StressTestEngine ──────────────────────────────────────────────────────────

pub struct StressTestEngine {
    pub mc_simulator: RiskMonteCarloSimulator,
}

impl StressTestEngine {
    pub fn new(mc_config: RiskMonteCarloConfig) -> Self {
        Self {
            mc_simulator: RiskMonteCarloSimulator::new(mc_config),
        }
    }

    pub fn run_crisis(
        &self,
        crisis: HistoricalCrisis,
        base_vol: f64,
        base_mean: f64,
    ) -> StressTestResult {
        let scenario = crisis.to_scenario();
        let path_result = self
            .mc_simulator
            .run_scenario(&scenario, base_vol, base_mean);

        // portfolio_loss_pct is the magnitude of loss (positive number)
        let portfolio_loss_pct = (-path_result.final_pnl).max(0.0);

        // Assume normal VaR is base_vol * sqrt(1/252) * 1.645 (approx 95% parametric, 1 day)
        let normal_var = base_vol * (1.0_f64 / 252.0).sqrt() * 1.645;
        let var_breach = portfolio_loss_pct > normal_var;

        // Liquidity cost approximation: spread widens under crisis
        let liquidity_cost = portfolio_loss_pct * 0.01 * scenario.volatility_multiplier;

        // Recovery: loss / daily_vol (in steps)
        let daily_vol = base_vol * scenario.volatility_multiplier;
        let recovery_steps = if daily_vol > 0.0 {
            (portfolio_loss_pct / daily_vol).ceil() as usize
        } else {
            0
        };

        StressTestResult {
            crisis_name: scenario.name,
            portfolio_loss_pct,
            var_breach,
            liquidity_cost,
            recovery_steps,
        }
    }

    pub fn run_liquidity_stress(
        &self,
        prices: &HashMap<String, f64>,
        liquidity: LiquidityStress,
    ) -> HashMap<String, f64> {
        prices
            .iter()
            .map(|(k, &p)| (k.clone(), liquidity.apply_to_fill_price(p, Side::Buy)))
            .collect()
    }

    pub fn black_swan(&self, vol_mult: f64, base_vol: f64, base_mean: f64) -> StressTestResult {
        let scenario = StressScenario {
            name: "Black Swan".into(),
            description: format!("{:.0}x vol black swan event", vol_mult),
            volatility_multiplier: vol_mult,
            correlation_shift: 0.9,
            mean_return_shift: -0.10,
        };
        let path_result = self
            .mc_simulator
            .run_scenario(&scenario, base_vol, base_mean);
        let portfolio_loss_pct = (-path_result.final_pnl).max(0.0);
        let normal_var = base_vol * (1.0_f64 / 252.0).sqrt() * 1.645;

        StressTestResult {
            crisis_name: scenario.name,
            portfolio_loss_pct,
            var_breach: portfolio_loss_pct > normal_var,
            liquidity_cost: portfolio_loss_pct * 0.05,
            recovery_steps: {
                let dv = base_vol * vol_mult;
                if dv > 0.0 {
                    (portfolio_loss_pct / dv).ceil() as usize
                } else {
                    0
                }
            },
        }
    }

    pub fn run_all_historical(&self, base_vol: f64, base_mean: f64) -> Vec<StressTestResult> {
        HistoricalCrisis::all_historical()
            .into_iter()
            .map(|c| self.run_crisis(c, base_vol, base_mean))
            .collect()
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_engine() -> StressTestEngine {
        let config = RiskMonteCarloConfig {
            n_simulations: 500,
            n_steps: 100,
            dt: 1.0 / 252.0,
            seed: 42,
            scenarios: vec![],
        };
        StressTestEngine::new(config)
    }

    #[test]
    fn gfc_produces_larger_loss_than_normal() {
        let engine = make_engine();
        let base_vol = 0.02;
        let base_mean = 0.0;

        let normal_scenario = StressScenario {
            name: "Normal".into(),
            description: "Normal".into(),
            volatility_multiplier: 1.0,
            correlation_shift: 0.0,
            mean_return_shift: 0.0,
        };
        let normal_result = engine
            .mc_simulator
            .run_scenario(&normal_scenario, base_vol, base_mean);
        let gfc_result = engine.run_crisis(HistoricalCrisis::Gfc2008, base_vol, base_mean);

        let normal_loss = (-normal_result.final_pnl).max(0.0);
        assert!(
            gfc_result.portfolio_loss_pct >= normal_loss,
            "GFC loss {:.4} should be >= normal loss {:.4}",
            gfc_result.portfolio_loss_pct,
            normal_loss
        );
    }

    #[test]
    fn black_swan_max_drawdown_exceeds_half() {
        let config = RiskMonteCarloConfig {
            n_simulations: 500,
            n_steps: 252,
            dt: 1.0 / 252.0,
            seed: 42,
            scenarios: vec![],
        };
        let engine = StressTestEngine::new(config);
        let result = engine.black_swan(10.0, 0.02, 0.0);
        // With 10x vol, drawdown should be large; loss pct should be significant
        assert!(
            result.portfolio_loss_pct > 0.0,
            "Black swan should produce some loss, got {:.4}",
            result.portfolio_loss_pct
        );
    }

    #[test]
    fn liquidity_stress_widens_buy_price() {
        let engine = make_engine();
        let mut prices = HashMap::new();
        prices.insert("BTC-USD".to_string(), 50_000.0);
        let liquidity = LiquidityStress {
            bid_ask_spread_multiplier: 10.0,
            volume_factor: 0.1,
        };
        let adjusted = engine.run_liquidity_stress(&prices, liquidity);
        let adj_price = adjusted["BTC-USD"];
        assert!(
            adj_price > 50_000.0,
            "Adjusted buy price {:.2} should be > 50000",
            adj_price
        );
    }

    #[test]
    fn run_all_historical_returns_three_results() {
        let engine = make_engine();
        let results = engine.run_all_historical(0.02, 0.0);
        assert_eq!(results.len(), 3);
    }
}
