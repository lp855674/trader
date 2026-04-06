/// Configuration for Monte Carlo simulation.
#[derive(Debug, Clone)]
pub struct MonteCarloConfig {
    /// Number of simulation paths.
    pub n_simulations: usize,
    /// Number of time steps per path.
    pub n_steps: usize,
    /// Starting portfolio value.
    pub initial_value: f64,
    /// Annualized mean return.
    pub mean_return: f64,
    /// Annualized volatility.
    pub volatility: f64,
    /// Time step in years (e.g. 1/252 for daily).
    pub dt: f64,
    /// Random seed for deterministic simulation.
    pub seed: u64,
}

/// Simple LCG random number generator.
struct Lcg {
    state: u64,
}

impl Lcg {
    fn new(seed: u64) -> Self {
        Self { state: if seed == 0 { 1 } else { seed } }
    }

    fn next_u64(&mut self) -> u64 {
        self.state = self.state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.state
    }

    fn next_normal(&mut self) -> f64 {
        // Box-Muller transform
        let u1 = (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64;
        let u2 = (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64;
        // Guard against log(0)
        let u1 = u1.max(1e-15);
        (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos()
    }

    fn next_usize(&mut self, n: usize) -> usize {
        if n == 0 { return 0; }
        (self.next_u64() % n as u64) as usize
    }
}

/// A single simulation path.
#[derive(Debug, Clone)]
pub struct SimulationPath {
    pub values: Vec<f64>,
    pub final_value: f64,
    pub max_drawdown: f64,
    pub peak: f64,
}

impl SimulationPath {
    fn from_values(values: Vec<f64>) -> Self {
        let final_value = values.last().copied().unwrap_or(0.0);
        let peak = values.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        let max_drawdown = compute_max_drawdown(&values);
        Self { values, final_value, max_drawdown, peak }
    }
}

fn compute_max_drawdown(values: &[f64]) -> f64 {
    let mut max_dd = 0.0_f64;
    let mut running_peak = f64::NEG_INFINITY;
    for &v in values {
        if v > running_peak { running_peak = v; }
        if running_peak > 0.0 {
            let dd = (running_peak - v) / running_peak;
            if dd > max_dd { max_dd = dd; }
        }
    }
    max_dd
}

/// Aggregated simulation results.
#[derive(Debug, Clone)]
pub struct SimulationResult {
    /// Individual paths (may be empty if memory conservation was applied).
    pub paths: Vec<SimulationPath>,
    pub percentile_5: f64,
    pub percentile_25: f64,
    pub median: f64,
    pub percentile_75: f64,
    pub percentile_95: f64,
    pub mean_final: f64,
    pub std_final: f64,
    /// Fraction of paths ending above initial_value.
    pub prob_profit: f64,
    pub mean_max_drawdown: f64,
}

fn compute_percentile(sorted: &[f64], pct: f64) -> f64 {
    if sorted.is_empty() { return 0.0; }
    let idx_f = pct / 100.0 * (sorted.len() - 1) as f64;
    let lo = idx_f.floor() as usize;
    let hi = (lo + 1).min(sorted.len() - 1);
    let frac = idx_f - lo as f64;
    sorted[lo] + frac * (sorted[hi] - sorted[lo])
}

fn aggregate(paths: &[SimulationPath], initial_value: f64) -> SimulationResult {
    let mut finals: Vec<f64> = paths.iter().map(|p| p.final_value).collect();
    finals.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    let mean_final = finals.iter().sum::<f64>() / finals.len() as f64;
    let variance = finals.iter().map(|v| (v - mean_final).powi(2)).sum::<f64>() / finals.len() as f64;
    let std_final = variance.sqrt();

    let prob_profit = finals.iter().filter(|&&v| v > initial_value).count() as f64
        / finals.len() as f64;

    let mean_max_drawdown = if paths.is_empty() {
        0.0
    } else {
        paths.iter().map(|p| p.max_drawdown).sum::<f64>() / paths.len() as f64
    };

    SimulationResult {
        paths: Vec::new(), // don't store all paths by default
        percentile_5: compute_percentile(&finals, 5.0),
        percentile_25: compute_percentile(&finals, 25.0),
        median: compute_percentile(&finals, 50.0),
        percentile_75: compute_percentile(&finals, 75.0),
        percentile_95: compute_percentile(&finals, 95.0),
        mean_final,
        std_final,
        prob_profit,
        mean_max_drawdown,
    }
}

/// Monte Carlo simulator.
pub struct MonteCarloSimulator {
    pub config: MonteCarloConfig,
}

impl MonteCarloSimulator {
    pub fn new(config: MonteCarloConfig) -> Self {
        Self { config }
    }

    /// Run GBM simulation: S(t+1) = S(t) * exp((μ - σ²/2)*dt + σ*sqrt(dt)*Z)
    pub fn run(&self) -> SimulationResult {
        let cfg = &self.config;
        let drift = (cfg.mean_return - 0.5 * cfg.volatility * cfg.volatility) * cfg.dt;
        let diffusion = cfg.volatility * cfg.dt.sqrt();
        let mut rng = Lcg::new(cfg.seed);

        let paths: Vec<SimulationPath> = (0..cfg.n_simulations)
            .map(|_| {
                let mut values = Vec::with_capacity(cfg.n_steps + 1);
                values.push(cfg.initial_value);
                let mut s = cfg.initial_value;
                for _ in 0..cfg.n_steps {
                    let z = rng.next_normal();
                    s *= (drift + diffusion * z).exp();
                    values.push(s);
                }
                SimulationPath::from_values(values)
            })
            .collect();

        let mut result = aggregate(&paths, cfg.initial_value);
        // Store paths in result
        result.paths = paths;
        result
    }

    /// Bootstrap resampling: resample returns with replacement to generate paths.
    pub fn bootstrap_returns(&self, returns: &[f64], n_bootstrap: usize) -> SimulationResult {
        let cfg = &self.config;
        let n = returns.len();
        if n == 0 {
            return SimulationResult {
                paths: Vec::new(),
                percentile_5: cfg.initial_value,
                percentile_25: cfg.initial_value,
                median: cfg.initial_value,
                percentile_75: cfg.initial_value,
                percentile_95: cfg.initial_value,
                mean_final: cfg.initial_value,
                std_final: 0.0,
                prob_profit: 0.5,
                mean_max_drawdown: 0.0,
            };
        }
        let mut rng = Lcg::new(cfg.seed);

        let paths: Vec<SimulationPath> = (0..n_bootstrap)
            .map(|_| {
                let mut values = Vec::with_capacity(cfg.n_steps + 1);
                values.push(cfg.initial_value);
                let mut s = cfg.initial_value;
                for _ in 0..cfg.n_steps {
                    let idx = rng.next_usize(n);
                    s *= 1.0 + returns[idx];
                    values.push(s);
                }
                SimulationPath::from_values(values)
            })
            .collect();

        let mut result = aggregate(&paths, cfg.initial_value);
        result.paths = paths;
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_config(n: usize) -> MonteCarloConfig {
        MonteCarloConfig {
            n_simulations: n,
            n_steps: 252,
            initial_value: 1000.0,
            mean_return: 0.08,
            volatility: 0.20,
            dt: 1.0 / 252.0,
            seed: 42,
        }
    }

    #[test]
    fn simulation_produces_n_paths() {
        let sim = MonteCarloSimulator::new(make_config(100));
        let result = sim.run();
        assert_eq!(result.paths.len(), 100);
    }

    #[test]
    fn percentiles_are_sorted() {
        let sim = MonteCarloSimulator::new(make_config(500));
        let result = sim.run();
        assert!(result.percentile_5 <= result.percentile_25);
        assert!(result.percentile_25 <= result.median);
        assert!(result.median <= result.percentile_75);
        assert!(result.percentile_75 <= result.percentile_95);
    }

    #[test]
    fn prob_profit_in_range() {
        let sim = MonteCarloSimulator::new(make_config(200));
        let result = sim.run();
        assert!(result.prob_profit >= 0.0 && result.prob_profit <= 1.0);
    }

    #[test]
    fn bootstrap_flat_returns_median_near_initial() {
        // All returns = 0 → all paths stay at initial_value
        let sim = MonteCarloSimulator::new(make_config(200));
        let returns = vec![0.0; 100];
        let result = sim.bootstrap_returns(&returns, 200);
        assert!((result.median - 1000.0).abs() < 1e-6);
        assert!((result.mean_final - 1000.0).abs() < 1e-6);
    }

    #[test]
    fn bootstrap_produces_correct_count() {
        let sim = MonteCarloSimulator::new(make_config(50));
        let returns: Vec<f64> = (0..252).map(|i| if i % 2 == 0 { 0.01 } else { -0.01 }).collect();
        let result = sim.bootstrap_returns(&returns, 50);
        assert_eq!(result.paths.len(), 50);
    }

    #[test]
    fn simulation_deterministic_with_same_seed() {
        let cfg = make_config(50);
        let sim1 = MonteCarloSimulator::new(cfg.clone());
        let sim2 = MonteCarloSimulator::new(cfg);
        let r1 = sim1.run();
        let r2 = sim2.run();
        assert!((r1.median - r2.median).abs() < 1e-10);
        assert!((r1.mean_final - r2.mean_final).abs() < 1e-10);
    }
}
