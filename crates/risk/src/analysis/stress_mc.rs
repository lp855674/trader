// Monte Carlo risk simulator for portfolio stress testing

// ── LCG ──────────────────────────────────────────────────────────────────────

struct Lcg {
    state: u64,
}

impl Lcg {
    fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    fn next_u64(&mut self) -> u64 {
        self.state = self.state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.state
    }

    fn next_normal(&mut self) -> f64 {
        let u1 = ((self.next_u64() >> 11) as f64 / (1u64 << 53) as f64).max(1e-15);
        let u2 = (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64;
        (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos()
    }
}

// ── StressScenario ────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct StressScenario {
    pub name: String,
    pub description: String,
    /// Scale market vol (e.g. 3.0 = crisis)
    pub volatility_multiplier: f64,
    /// Add to all correlations (e.g. 0.3 = contagion)
    pub correlation_shift: f64,
    /// Add to mean return (e.g. -0.02 = bear market)
    pub mean_return_shift: f64,
}

// ── RiskMonteCarloConfig ──────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct RiskMonteCarloConfig {
    pub n_simulations: usize,
    pub n_steps: usize,
    /// Time step in years
    pub dt: f64,
    pub seed: u64,
    pub scenarios: Vec<StressScenario>,
}

impl Default for RiskMonteCarloConfig {
    fn default() -> Self {
        Self {
            n_simulations: 1000,
            n_steps: 252,
            dt: 1.0 / 252.0,
            seed: 42,
            scenarios: Vec::new(),
        }
    }
}

// ── PathResult ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct PathResult {
    pub scenario_name: String,
    pub final_pnl: f64,
    pub max_drawdown: f64,
    pub var_95: f64,
    pub var_99: f64,
    /// Fraction of paths with pnl < -0.5 * initial
    pub prob_ruin: f64,
}

// ── RiskMonteCarloSimulator ───────────────────────────────────────────────────

pub struct RiskMonteCarloSimulator {
    config: RiskMonteCarloConfig,
}

impl RiskMonteCarloSimulator {
    pub fn new(config: RiskMonteCarloConfig) -> Self {
        Self { config }
    }

    pub fn run_scenario(
        &self,
        scenario: &StressScenario,
        base_vol: f64,
        base_mean: f64,
    ) -> PathResult {
        let vol = base_vol * scenario.volatility_multiplier;
        let mean = base_mean + scenario.mean_return_shift;
        let dt = self.config.dt;
        let drift = (mean - vol * vol / 2.0) * dt;
        let diffusion = vol * dt.sqrt();

        let mut rng = Lcg::new(self.config.seed);
        let mut final_pnls: Vec<f64> = Vec::with_capacity(self.config.n_simulations);
        let mut sum_max_drawdown = 0.0;
        let mut ruin_count = 0usize;

        for _ in 0..self.config.n_simulations {
            let mut s = 1.0_f64;
            let mut peak = 1.0_f64;
            let mut path_max_drawdown = 0.0_f64;

            for _ in 0..self.config.n_steps {
                let z = rng.next_normal();
                s *= (drift + diffusion * z).exp();
                if s > peak {
                    peak = s;
                }
                let dd = (peak - s) / peak;
                if dd > path_max_drawdown {
                    path_max_drawdown = dd;
                }
            }

            let final_pnl = s - 1.0;
            final_pnls.push(final_pnl);
            sum_max_drawdown += path_max_drawdown;
            if final_pnl < -0.5 {
                ruin_count += 1;
            }
        }

        let n = final_pnls.len();
        let max_drawdown = sum_max_drawdown / n as f64;
        let prob_ruin = ruin_count as f64 / n as f64;

        // Compute VaR: sort and pick percentile of losses (left tail)
        let mut sorted = final_pnls.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

        let idx_95 = ((1.0 - 0.95) * n as f64).floor() as usize;
        let idx_95 = idx_95.min(n - 1);
        let var_95 = -sorted[idx_95].min(0.0);

        let idx_99 = ((1.0 - 0.99) * n as f64).floor() as usize;
        let idx_99 = idx_99.min(n - 1);
        let var_99 = -sorted[idx_99].min(0.0);

        // final_pnl = mean of all paths
        let mean_final = final_pnls.iter().sum::<f64>() / n as f64;

        PathResult {
            scenario_name: scenario.name.clone(),
            final_pnl: mean_final,
            max_drawdown,
            var_95,
            var_99,
            prob_ruin,
        }
    }

    pub fn run_all(&self, base_vol: f64, base_mean: f64) -> Vec<PathResult> {
        self.config
            .scenarios
            .iter()
            .map(|s| self.run_scenario(s, base_vol, base_mean))
            .collect()
    }

    pub fn worst_case<'a>(&self, results: &'a [PathResult]) -> Option<&'a PathResult> {
        results.iter().min_by(|a, b| {
            a.final_pnl
                .partial_cmp(&b.final_pnl)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn normal_scenario() -> StressScenario {
        StressScenario {
            name: "Normal".into(),
            description: "Normal market".into(),
            volatility_multiplier: 1.0,
            correlation_shift: 0.0,
            mean_return_shift: 0.0,
        }
    }

    fn crisis_scenario() -> StressScenario {
        StressScenario {
            name: "Crisis".into(),
            description: "3x vol crisis".into(),
            volatility_multiplier: 3.0,
            correlation_shift: 0.3,
            mean_return_shift: -0.02,
        }
    }

    #[test]
    fn crisis_has_higher_drawdown_than_normal() {
        let config = RiskMonteCarloConfig {
            n_simulations: 500,
            n_steps: 50,
            dt: 1.0 / 252.0,
            seed: 42,
            scenarios: vec![normal_scenario(), crisis_scenario()],
        };
        let sim = RiskMonteCarloSimulator::new(config);
        let normal_result = sim.run_scenario(&normal_scenario(), 0.02, 0.0001);
        let crisis_result = sim.run_scenario(&crisis_scenario(), 0.02, 0.0001);
        assert!(
            crisis_result.max_drawdown > normal_result.max_drawdown,
            "Crisis drawdown {:.4} should exceed normal {:.4}",
            crisis_result.max_drawdown,
            normal_result.max_drawdown
        );
    }

    #[test]
    fn worst_case_picks_min_final_pnl() {
        let config = RiskMonteCarloConfig {
            n_simulations: 200,
            n_steps: 20,
            dt: 1.0 / 252.0,
            seed: 99,
            scenarios: vec![normal_scenario(), crisis_scenario()],
        };
        let sim = RiskMonteCarloSimulator::new(config.clone());
        let results = sim.run_all(0.02, 0.0001);
        assert_eq!(results.len(), 2);
        let worst = sim.worst_case(&results).unwrap();
        let min_pnl = results.iter().map(|r| r.final_pnl).fold(f64::INFINITY, f64::min);
        assert!((worst.final_pnl - min_pnl).abs() < 1e-9);
    }

    #[test]
    fn run_all_returns_one_result_per_scenario() {
        let scenarios = vec![normal_scenario(), crisis_scenario()];
        let n = scenarios.len();
        let config = RiskMonteCarloConfig {
            n_simulations: 100,
            n_steps: 10,
            dt: 1.0 / 252.0,
            seed: 1,
            scenarios,
        };
        let sim = RiskMonteCarloSimulator::new(config);
        let results = sim.run_all(0.02, 0.0);
        assert_eq!(results.len(), n);
    }
}
