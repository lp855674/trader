// Portfolio risk module
use crate::core::{RiskChecker, RiskDecision, RiskError, RiskInput};
use domain::InstrumentId;
use std::collections::{HashMap, VecDeque};

// ── PortfolioRiskConfig ───────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PortfolioRiskConfig {
    /// Max fraction of total capital (e.g. 0.80 = 80%)
    pub max_total_exposure_pct: f64,
    pub max_sector_exposure_pct: f64,
    /// Max single position as fraction of total capital
    pub max_single_position_pct: f64,
    /// VaR confidence level (e.g. 0.95)
    pub var_confidence: f64,
    pub var_lookback: usize,
    pub correlation_lookback: usize,
    /// VaR budget as fraction of total capital
    pub max_var_pct: f64,
}

impl Default for PortfolioRiskConfig {
    fn default() -> Self {
        Self {
            max_total_exposure_pct: 0.90,
            max_sector_exposure_pct: 0.50,
            max_single_position_pct: 0.20,
            var_confidence: 0.95,
            var_lookback: 252,
            correlation_lookback: 60,
            max_var_pct: 0.05,
        }
    }
}

// ── PortfolioMetrics ──────────────────────────────────────────────────────

#[derive(Debug, Clone, Default)]
pub struct PortfolioMetrics {
    pub total_exposure: f64,
    /// Herfindahl index
    pub concentration: f64,
    pub var_95: f64,
    pub var_99: f64,
    /// Weighted avg vol / portfolio vol
    pub diversification_ratio: f64,
}

// ── CorrelationMatrix ─────────────────────────────────────────────────────

pub struct CorrelationMatrix {
    pub returns: HashMap<String, VecDeque<f64>>,
    pub window: usize,
}

impl CorrelationMatrix {
    pub fn new(window: usize) -> Self {
        Self {
            returns: HashMap::new(),
            window,
        }
    }

    pub fn push(&mut self, instrument: &InstrumentId, return_: f64) {
        let key = instrument.to_string();
        let deque = self.returns.entry(key).or_insert_with(VecDeque::new);
        deque.push_back(return_);
        if deque.len() > self.window {
            deque.pop_front();
        }
    }

    /// Pearson correlation coefficient between two instruments; 0.0 if insufficient data
    pub fn correlation(&self, a: &InstrumentId, b: &InstrumentId) -> f64 {
        let key_a = a.to_string();
        let key_b = b.to_string();

        let ra = match self.returns.get(&key_a) {
            Some(r) => r,
            None => return 0.0,
        };
        let rb = match self.returns.get(&key_b) {
            Some(r) => r,
            None => return 0.0,
        };

        let n = ra.len().min(rb.len());
        if n < 2 {
            return 0.0;
        }

        let ra: Vec<f64> = ra.iter().rev().take(n).cloned().collect();
        let rb: Vec<f64> = rb.iter().rev().take(n).cloned().collect();

        pearson_correlation(&ra, &rb)
    }

    /// NxN covariance matrix for given instruments
    pub fn covariance_matrix(&self, instruments: &[InstrumentId]) -> Vec<Vec<f64>> {
        let n = instruments.len();
        let mut matrix = vec![vec![0.0; n]; n];

        for i in 0..n {
            for j in 0..n {
                let key_i = instruments[i].to_string();
                let key_j = instruments[j].to_string();

                let ri = self.returns.get(&key_i);
                let rj = self.returns.get(&key_j);

                if let (Some(ri), Some(rj)) = (ri, rj) {
                    let k = ri.len().min(rj.len());
                    if k >= 2 {
                        let ri: Vec<f64> = ri.iter().rev().take(k).cloned().collect();
                        let rj: Vec<f64> = rj.iter().rev().take(k).cloned().collect();
                        matrix[i][j] = covariance(&ri, &rj);
                    }
                }
            }
        }

        matrix
    }
}

// ── VarCalculator ─────────────────────────────────────────────────────────

pub struct VarCalculator {
    pub returns: HashMap<String, VecDeque<f64>>,
    pub window: usize,
}

impl VarCalculator {
    pub fn new(window: usize) -> Self {
        Self {
            returns: HashMap::new(),
            window,
        }
    }

    pub fn push(&mut self, instrument: &InstrumentId, return_: f64) {
        let key = instrument.to_string();
        let deque = self.returns.entry(key).or_insert_with(VecDeque::new);
        deque.push_back(return_);
        if deque.len() > self.window {
            deque.pop_front();
        }
    }

    /// Historical VaR at given confidence (e.g. 0.95 → 5th percentile of returns, returned as positive loss)
    pub fn historical_var(&self, instrument: &InstrumentId, confidence: f64) -> f64 {
        let key = instrument.to_string();
        let returns = match self.returns.get(&key) {
            Some(r) if !r.is_empty() => r,
            _ => return 0.0,
        };

        let mut sorted: Vec<f64> = returns.iter().cloned().collect();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

        // VaR at (1-confidence) quantile — use floor to avoid floating-point rounding issues
        // For 95% confidence: floor(0.05 * N) = index of worst loss in bottom 5%
        let n = sorted.len();
        let idx = ((1.0 - confidence) * n as f64).floor() as usize;
        let idx = idx.saturating_sub(1).max(0).min(n - 1);
        // Make sure idx points to worst loss in the left tail
        // If idx=0 and sorted[0]>=0, return 0 (no losses observed)
        -sorted[idx].min(0.0) // VaR is the magnitude of the loss (positive number)
    }

    /// Simplified portfolio VaR: weighted sum of individual VaRs
    pub fn portfolio_var(&self, weights: &HashMap<String, f64>, confidence: f64) -> f64 {
        weights
            .iter()
            .map(|(key, &w)| {
                let dummy_id = parse_instrument_id_from_string(key);
                self.historical_var(&dummy_id, confidence) * w.abs()
            })
            .sum()
    }

    /// Approximate incremental VaR for adding weight to portfolio
    pub fn incremental_var(
        &self,
        instrument: &InstrumentId,
        weight: f64,
        portfolio_var: f64,
    ) -> f64 {
        let individual_var = self.historical_var(instrument, 0.95);
        // Simple approximation: marginal contribution
        (individual_var * weight.abs()).max(0.0) + portfolio_var * 0.1
    }

    /// Monte Carlo VaR using GBM simulation with LCG random numbers (no external rand crate).
    /// `n_sims`: number of simulation paths; `horizon`: days; `confidence`: e.g. 0.95
    pub fn monte_carlo_var(
        &self,
        instrument: &InstrumentId,
        n_sims: usize,
        horizon: usize,
        confidence: f64,
    ) -> f64 {
        let key = instrument.to_string();
        let returns = match self.returns.get(&key) {
            Some(r) if r.len() >= 2 => r,
            _ => return 0.0,
        };

        // Estimate mu and sigma from historical returns
        let rets: Vec<f64> = returns.iter().cloned().collect();
        let n = rets.len() as f64;
        let mu = rets.iter().sum::<f64>() / n;
        let variance = rets.iter().map(|r| (r - mu).powi(2)).sum::<f64>() / (n - 1.0);
        let sigma = variance.sqrt();
        if sigma < 1e-12 {
            return 0.0;
        }

        // LCG RNG — deterministic seed for reproducibility
        let mut state: u64 = 0xDEAD_BEEF_1234_5678;
        let lcg_next = |s: &mut u64| -> f64 {
            *s = s
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            let bits = (*s >> 11) as f64;
            bits / (1u64 << 53) as f64
        };

        // Box-Muller transform for standard normal samples
        let mut normal = || -> f64 {
            let u1 = lcg_next(&mut state).max(1e-12);
            let u2 = lcg_next(&mut state);
            (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos()
        };

        // Simulate n_sims paths of `horizon` steps
        let mut final_returns: Vec<f64> = Vec::with_capacity(n_sims);
        for _ in 0..n_sims {
            let mut cumulative = 0.0f64;
            for _ in 0..horizon {
                cumulative += mu + sigma * normal();
            }
            final_returns.push(cumulative);
        }

        // VaR at (1-confidence) quantile
        final_returns.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let idx = ((1.0 - confidence) * n_sims as f64).floor() as usize;
        let idx = idx.min(final_returns.len() - 1);
        -final_returns[idx].min(0.0)
    }
}

// ── EigenDecomposition ────────────────────────────────────────────────────

/// Power-iteration eigenvalue decomposition for symmetric matrices.
/// Returns (eigenvalues, eigenvectors) sorted descending by eigenvalue.
pub fn eigen_decompose(
    matrix: &[Vec<f64>],
    max_iter: usize,
    tol: f64,
) -> (Vec<f64>, Vec<Vec<f64>>) {
    let n = matrix.len();
    if n == 0 {
        return (vec![], vec![]);
    }

    let mut eigenvalues = Vec::with_capacity(n);
    let mut eigenvectors = Vec::with_capacity(n);

    // Deflated matrix copy
    let mut m: Vec<Vec<f64>> = matrix.to_vec();

    for _ in 0..n {
        // Power iteration to find dominant eigenvector
        let mut v: Vec<f64> = (0..n).map(|i| if i == 0 { 1.0 } else { 0.0 }).collect();
        let mut lambda = 0.0f64;

        for _ in 0..max_iter {
            // Av
            let mut av: Vec<f64> = vec![0.0; n];
            for i in 0..n {
                for j in 0..n {
                    av[i] += m[i][j] * v[j];
                }
            }
            // Rayleigh quotient
            let new_lambda: f64 = v.iter().zip(av.iter()).map(|(vi, avi)| vi * avi).sum();
            // Normalize
            let norm = av.iter().map(|x| x * x).sum::<f64>().sqrt();
            if norm < 1e-14 {
                break;
            }
            let new_v: Vec<f64> = av.iter().map(|x| x / norm).collect();
            if (new_lambda - lambda).abs() < tol {
                v = new_v;
                lambda = new_lambda;
                break;
            }
            v = new_v;
            lambda = new_lambda;
        }

        eigenvalues.push(lambda);
        eigenvectors.push(v.clone());

        // Deflation: M = M - λ vvᵀ
        for i in 0..n {
            for j in 0..n {
                m[i][j] -= lambda * v[i] * v[j];
            }
        }
    }

    (eigenvalues, eigenvectors)
}

// ── PortfolioOptimizer ────────────────────────────────────────────────────

/// Minimum-variance portfolio optimizer (equal-weight fallback, analytical for 2-asset).
pub struct PortfolioOptimizer {
    pub risk_free_rate: f64,
}

impl PortfolioOptimizer {
    pub fn new(risk_free_rate: f64) -> Self {
        Self { risk_free_rate }
    }

    /// Equal-weight portfolio (N assets).
    pub fn equal_weight(&self, n: usize) -> Vec<f64> {
        if n == 0 {
            return vec![];
        }
        vec![1.0 / n as f64; n]
    }

    /// Minimum-variance weights using the covariance matrix via gradient descent.
    /// Returns weights that sum to 1.0 and minimize portfolio variance.
    pub fn min_variance(&self, cov: &[Vec<f64>]) -> Vec<f64> {
        let n = cov.len();
        if n == 0 {
            return vec![];
        }
        if n == 1 {
            return vec![1.0];
        }

        // Projected gradient descent with simplex constraint (w ≥ 0, sum = 1)
        let mut w = self.equal_weight(n);
        let lr = 0.01;

        for _ in 0..500 {
            // Gradient of w'Σw = 2Σw
            let mut grad = vec![0.0; n];
            for i in 0..n {
                for j in 0..n {
                    grad[i] += 2.0 * cov[i][j] * w[j];
                }
            }
            // Gradient step
            let mut new_w: Vec<f64> = w
                .iter()
                .zip(grad.iter())
                .map(|(wi, gi)| wi - lr * gi)
                .collect();
            // Project onto simplex (clip negatives, renormalize)
            new_w.iter_mut().for_each(|x| *x = x.max(0.0));
            let s: f64 = new_w.iter().sum();
            if s > 1e-12 {
                new_w.iter_mut().for_each(|x| *x /= s);
            }
            w = new_w;
        }
        w
    }

    /// Sharpe-maximizing weights (simplified: uses mean returns and covariance).
    pub fn max_sharpe(&self, means: &[f64], cov: &[Vec<f64>]) -> Vec<f64> {
        let n = means.len();
        if n == 0 {
            return vec![];
        }

        // Excess returns
        let excess: Vec<f64> = means.iter().map(|m| m - self.risk_free_rate).collect();

        // Unconstrained analytical solution: w ∝ Σ⁻¹ μ (approx via gradient)
        let mut w = self.equal_weight(n);
        let lr = 0.005;

        for _ in 0..500 {
            let port_ret: f64 = w.iter().zip(excess.iter()).map(|(wi, ei)| wi * ei).sum();
            let mut port_var = 0.0f64;
            for i in 0..n {
                for j in 0..n {
                    port_var += w[i] * cov[i][j] * w[j];
                }
            }
            let port_std = port_var.sqrt().max(1e-12);

            // Gradient of Sharpe = (μ - rf) / σ
            let mut grad = vec![0.0; n];
            for i in 0..n {
                let d_ret = excess[i];
                let mut d_var = 0.0;
                for j in 0..n {
                    d_var += 2.0 * cov[i][j] * w[j];
                }
                grad[i] =
                    (d_ret * port_std - port_ret * d_var / (2.0 * port_std)) / (port_var + 1e-12);
            }

            let mut new_w: Vec<f64> = w
                .iter()
                .zip(grad.iter())
                .map(|(wi, gi)| wi + lr * gi)
                .collect();
            new_w.iter_mut().for_each(|x| *x = x.max(0.0));
            let s: f64 = new_w.iter().sum();
            if s > 1e-12 {
                new_w.iter_mut().for_each(|x| *x /= s);
            }
            w = new_w;
        }
        w
    }
}

// ── PortfolioRiskChecker ──────────────────────────────────────────────────

pub struct PortfolioRiskChecker {
    config: PortfolioRiskConfig,
    var_calculator: VarCalculator,
}

impl PortfolioRiskChecker {
    pub fn new(config: PortfolioRiskConfig) -> Self {
        let var_calculator = VarCalculator::new(config.var_lookback);
        Self {
            config,
            var_calculator,
        }
    }

    pub fn push_return(&mut self, instrument: &InstrumentId, return_: f64) {
        self.var_calculator.push(instrument, return_);
    }
}

impl RiskChecker for PortfolioRiskChecker {
    fn check(&self, input: &RiskInput) -> Result<RiskDecision, RiskError> {
        let order = &input.order;
        let portfolio = &input.portfolio;
        let market = &input.market;
        let cfg = &self.config;

        let price = order.limit_price.unwrap_or(market.mid_price);
        let order_notional = order.quantity * price;

        // 1. Total exposure check
        let new_exposure = portfolio.total_exposure + order_notional;
        let max_exposure = portfolio.total_capital * cfg.max_total_exposure_pct;
        if new_exposure > max_exposure {
            return Ok(RiskDecision::Reject {
                reason: format!(
                    "Total exposure {:.2} would exceed limit {:.2}",
                    new_exposure, max_exposure
                ),
                risk_score: 90.0,
            });
        }

        // 2. Single position concentration check
        let max_single = portfolio.total_capital * cfg.max_single_position_pct;
        if order_notional > max_single {
            return Ok(RiskDecision::Reject {
                reason: format!(
                    "Single position notional {:.2} exceeds limit {:.2}",
                    order_notional, max_single
                ),
                risk_score: 80.0,
            });
        }

        // 3. VaR budget check
        let instrument_var = self
            .var_calculator
            .historical_var(&order.instrument, cfg.var_confidence);
        let position_var = instrument_var * order_notional;
        let max_var = portfolio.total_capital * cfg.max_var_pct;
        if position_var > max_var {
            return Ok(RiskDecision::Reject {
                reason: format!(
                    "Position VaR {:.2} exceeds budget {:.2}",
                    position_var, max_var
                ),
                risk_score: 85.0,
            });
        }

        Ok(RiskDecision::Approve)
    }

    fn name(&self) -> &str {
        "PortfolioRiskChecker"
    }

    fn priority(&self) -> u32 {
        40
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────

fn mean(data: &[f64]) -> f64 {
    if data.is_empty() {
        return 0.0;
    }
    data.iter().sum::<f64>() / data.len() as f64
}

fn covariance(a: &[f64], b: &[f64]) -> f64 {
    let n = a.len().min(b.len());
    if n < 2 {
        return 0.0;
    }
    let ma = mean(&a[..n]);
    let mb = mean(&b[..n]);
    a[..n]
        .iter()
        .zip(b[..n].iter())
        .map(|(x, y)| (x - ma) * (y - mb))
        .sum::<f64>()
        / (n - 1) as f64
}

fn pearson_correlation(a: &[f64], b: &[f64]) -> f64 {
    let cov = covariance(a, b);
    let std_a = covariance(a, a).sqrt();
    let std_b = covariance(b, b).sqrt();
    if std_a < 1e-12 || std_b < 1e-12 {
        return 0.0;
    }
    (cov / (std_a * std_b)).clamp(-1.0, 1.0)
}

/// Parse InstrumentId from "VENUE:SYMBOL" string format
fn parse_instrument_id_from_string(s: &str) -> InstrumentId {
    use domain::Venue;
    if let Some(pos) = s.find(':') {
        let venue_str = &s[..pos];
        let symbol = &s[pos + 1..];
        let venue = Venue::parse(venue_str).unwrap_or(Venue::Crypto);
        InstrumentId::new(venue, symbol)
    } else {
        InstrumentId::new(Venue::Crypto, s)
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{MarketContext, OrderContext, OrderType, PortfolioContext, RiskInput};
    use domain::{Side, Venue};

    fn btc() -> InstrumentId {
        InstrumentId::new(Venue::Crypto, "BTC-USD")
    }

    fn eth() -> InstrumentId {
        InstrumentId::new(Venue::Crypto, "ETH-USD")
    }

    fn make_input(order_notional: f64) -> RiskInput {
        let qty = order_notional / 50_000.0;
        RiskInput {
            order: OrderContext {
                instrument: btc(),
                side: Side::Buy,
                quantity: qty,
                limit_price: Some(50_000.0),
                order_type: OrderType::Limit,
                strategy_id: "test".into(),
                submitted_ts_ms: 0,
            },
            market: MarketContext {
                instrument: btc(),
                mid_price: 50_000.0,
                bid: 49_990.0,
                ask: 50_010.0,
                volume_24h: 1_000_000.0,
                volatility: 0.02,
                ts_ms: 0,
            },
            portfolio: PortfolioContext {
                total_capital: 100_000.0,
                available_capital: 80_000.0,
                total_exposure: 10_000.0,
                open_positions: 1,
                daily_pnl: 500.0,
                daily_pnl_limit: -5_000.0,
            },
        }
    }

    #[test]
    fn concentration_limit_triggers() {
        let config = PortfolioRiskConfig {
            max_single_position_pct: 0.10, // 10% = 10k of 100k
            ..PortfolioRiskConfig::default()
        };
        let checker = PortfolioRiskChecker::new(config);
        // 25k order > 10k limit
        let input = make_input(25_000.0);
        let result = checker.check(&input).unwrap();
        assert!(matches!(result, RiskDecision::Reject { .. }));
    }

    #[test]
    fn concentration_limit_approves_small() {
        let checker = PortfolioRiskChecker::new(PortfolioRiskConfig::default());
        // 5k order, well within 20k limit
        let input = make_input(5_000.0);
        let result = checker.check(&input).unwrap();
        assert!(matches!(result, RiskDecision::Approve));
    }

    #[test]
    fn var_computed_on_known_returns() {
        let mut calc = VarCalculator::new(200);
        let instrument = btc();
        // Feed 100 returns: 95 small gains, 5 large losses
        for _ in 0..95 {
            calc.push(&instrument, 0.01);
        }
        for _ in 0..5 {
            calc.push(&instrument, -0.10);
        }
        let var = calc.historical_var(&instrument, 0.95);
        // 95th confidence VaR: worst 5% of 100 obs = 5 losses of -10%
        // sorted ascending: [-0.10, -0.10, -0.10, -0.10, -0.10, 0.01, ...]
        // floor(0.05 * 100) = 5, idx = 4, sorted[4] = -0.10, VaR = 0.10
        assert!(var > 0.0, "VaR should be positive but got: {}", var);
        assert!(
            var >= 0.09 && var <= 0.11,
            "VaR should be ~0.10, got: {}",
            var
        );
    }

    #[test]
    fn correlation_one_for_identical_series() {
        let mut matrix = CorrelationMatrix::new(50);
        let a = btc();
        let b = InstrumentId::new(Venue::Crypto, "BTC-USD-COPY");
        let returns = vec![
            0.01, -0.02, 0.03, -0.01, 0.02, 0.00, -0.03, 0.01, 0.02, -0.01,
        ];
        for &r in &returns {
            matrix.push(&a, r);
            matrix.push(&b, r);
        }
        let corr = matrix.correlation(&a, &b);
        assert!(
            (corr - 1.0).abs() < 1e-9,
            "Identical series should have corr=1.0, got {}",
            corr
        );
    }

    #[test]
    fn correlation_zero_for_no_data() {
        let matrix = CorrelationMatrix::new(50);
        let corr = matrix.correlation(&btc(), &eth());
        assert_eq!(corr, 0.0);
    }

    #[test]
    fn covariance_matrix_shape() {
        let mut matrix = CorrelationMatrix::new(50);
        let instruments = vec![btc(), eth()];
        for i in 0..20 {
            matrix.push(&btc(), (i as f64) * 0.001);
            matrix.push(&eth(), (i as f64) * 0.0015);
        }
        let cov = matrix.covariance_matrix(&instruments);
        assert_eq!(cov.len(), 2);
        assert_eq!(cov[0].len(), 2);
        // Variance should be positive (diagonal elements)
        assert!(cov[0][0] > 0.0);
        assert!(cov[1][1] > 0.0);
    }

    #[test]
    fn var_budget_breach_rejects() {
        let config = PortfolioRiskConfig {
            max_var_pct: 0.0001, // Very tight VaR budget
            ..PortfolioRiskConfig::default()
        };
        let mut checker = PortfolioRiskChecker::new(config);

        // Feed some returns so VaR is non-zero
        for _ in 0..10 {
            checker.push_return(&btc(), -0.05);
        }
        for _ in 0..10 {
            checker.push_return(&btc(), 0.01);
        }

        let input = make_input(10_000.0);
        let result = checker.check(&input).unwrap();
        // With tight VaR budget and non-zero returns, should reject
        assert!(matches!(result, RiskDecision::Reject { .. }));
    }
}
