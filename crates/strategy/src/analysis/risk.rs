/// Method for computing Value at Risk.
#[derive(Debug, Clone, PartialEq)]
pub enum VarMethod {
    Historical,
    Parametric,
    CornishFisher,
}

/// Comprehensive risk metrics for a return series.
#[derive(Debug, Clone)]
pub struct RiskMetrics {
    /// Value at Risk at 95% confidence (negative value = potential loss).
    pub var_95: f64,
    /// Value at Risk at 99% confidence.
    pub var_99: f64,
    /// Conditional VaR (Expected Shortfall) at 95%.
    pub cvar_95: f64,
    /// Conditional VaR at 99%.
    pub cvar_99: f64,
    pub max_drawdown: f64,
    pub avg_drawdown: f64,
    /// Average absolute position change per period (approximated).
    pub turnover: f64,
    /// sqrt(mean(drawdown²)) — measures pain duration.
    pub ulcer_index: f64,
    /// mean_return / ulcer_index.
    pub pain_ratio: f64,
}

/// Computes risk metrics from a return series.
pub struct RiskCalculator;

impl RiskCalculator {
    pub fn calculate(returns: &[f64], method: VarMethod) -> RiskMetrics {
        if returns.is_empty() {
            return RiskMetrics {
                var_95: 0.0,
                var_99: 0.0,
                cvar_95: 0.0,
                cvar_99: 0.0,
                max_drawdown: 0.0,
                avg_drawdown: 0.0,
                turnover: 0.0,
                ulcer_index: 0.0,
                pain_ratio: 0.0,
            };
        }

        let (mean, std, skew, kurt) = Self::stats(returns);

        let (var_95, var_99) = match method {
            VarMethod::Historical => {
                let mut sorted = returns.to_vec();
                sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
                let v95 = percentile_sorted(&sorted, 5.0);
                let v99 = percentile_sorted(&sorted, 1.0);
                (v95, v99)
            }
            VarMethod::Parametric => {
                let v95 = mean - 1.645 * std;
                let v99 = mean - 2.326 * std;
                (v95, v99)
            }
            VarMethod::CornishFisher => {
                let z95 = 1.645_f64;
                let z99 = 2.326_f64;
                let z_cf95 = cornish_fisher(z95, skew, kurt);
                let z_cf99 = cornish_fisher(z99, skew, kurt);
                let v95 = mean - z_cf95 * std;
                let v99 = mean - z_cf99 * std;
                (v95, v99)
            }
        };

        let cvar_95 = cvar(returns, var_95);
        let cvar_99 = cvar(returns, var_99);

        let (max_drawdown, avg_drawdown, ulcer_index) = drawdown_metrics(returns);
        let turnover = std * 2.0;
        let pain_ratio = if ulcer_index < 1e-10 { 0.0 } else { mean / ulcer_index };

        RiskMetrics {
            var_95,
            var_99,
            cvar_95,
            cvar_99,
            max_drawdown,
            avg_drawdown,
            turnover,
            ulcer_index,
            pain_ratio,
        }
    }

    /// Returns (mean, std, skewness, excess_kurtosis).
    pub fn stats(returns: &[f64]) -> (f64, f64, f64, f64) {
        let n = returns.len() as f64;
        if returns.is_empty() { return (0.0, 0.0, 0.0, 0.0); }

        let mean = returns.iter().sum::<f64>() / n;
        let variance = returns.iter().map(|r| (r - mean).powi(2)).sum::<f64>() / n;
        let std = variance.sqrt();

        if std < 1e-10 {
            return (mean, std, 0.0, 0.0);
        }

        let skewness = returns.iter().map(|r| ((r - mean) / std).powi(3)).sum::<f64>() / n;
        let kurtosis = returns.iter().map(|r| ((r - mean) / std).powi(4)).sum::<f64>() / n - 3.0;

        (mean, std, skewness, kurtosis)
    }
}

fn percentile_sorted(sorted: &[f64], pct: f64) -> f64 {
    if sorted.is_empty() { return 0.0; }
    let idx_f = pct / 100.0 * (sorted.len() - 1) as f64;
    let lo = idx_f.floor() as usize;
    let hi = (lo + 1).min(sorted.len() - 1);
    let frac = idx_f - lo as f64;
    sorted[lo] + frac * (sorted[hi] - sorted[lo])
}

fn cvar(returns: &[f64], var_threshold: f64) -> f64 {
    let tail: Vec<f64> = returns.iter().filter(|&&r| r <= var_threshold).copied().collect();
    if tail.is_empty() { return var_threshold; }
    tail.iter().sum::<f64>() / tail.len() as f64
}

fn cornish_fisher(z: f64, skew: f64, kurt: f64) -> f64 {
    z + (z * z - 1.0) * skew / 6.0
        + (z * z * z - 3.0 * z) * kurt / 24.0
        - (2.0 * z * z * z - 5.0 * z) * skew * skew / 36.0
}

fn drawdown_metrics(returns: &[f64]) -> (f64, f64, f64) {
    // Build equity curve from returns starting at 1.0
    let mut equity = 1.0_f64;
    let mut peak = 1.0_f64;
    let mut max_dd = 0.0_f64;
    let mut drawdowns = Vec::new();

    for &r in returns {
        equity *= 1.0 + r;
        if equity > peak { peak = equity; }
        let dd = if peak > 0.0 { (peak - equity) / peak } else { 0.0 };
        drawdowns.push(dd);
        if dd > max_dd { max_dd = dd; }
    }

    let avg_dd = if drawdowns.is_empty() {
        0.0
    } else {
        drawdowns.iter().sum::<f64>() / drawdowns.len() as f64
    };

    let ulcer = if drawdowns.is_empty() {
        0.0
    } else {
        let mean_sq = drawdowns.iter().map(|d| d * d).sum::<f64>() / drawdowns.len() as f64;
        mean_sq.sqrt()
    };

    (max_dd, avg_dd, ulcer)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn simple_returns() -> Vec<f64> {
        // [-0.05, -0.03, -0.01, 0.00, 0.01, 0.02, 0.03, 0.05, 0.08, 0.10]
        vec![-0.05, -0.03, -0.01, 0.0, 0.01, 0.02, 0.03, 0.05, 0.08, 0.10]
    }

    #[test]
    fn historical_var_on_known_returns() {
        let returns = simple_returns();
        let metrics = RiskCalculator::calculate(&returns, VarMethod::Historical);
        // 5th percentile of 10 values is the smallest or second-smallest
        assert!(metrics.var_95 <= 0.0); // should be negative (loss side)
        assert!(metrics.var_99 <= metrics.var_95); // 99% VaR is worse
    }

    #[test]
    fn cvar_is_worse_than_var() {
        let returns = simple_returns();
        let metrics = RiskCalculator::calculate(&returns, VarMethod::Historical);
        // CVaR should be <= VaR (more negative, i.e. worse)
        assert!(metrics.cvar_95 <= metrics.var_95);
        assert!(metrics.cvar_99 <= metrics.var_99);
    }

    #[test]
    fn parametric_var_uses_normal_assumption() {
        let returns = simple_returns();
        let metrics = RiskCalculator::calculate(&returns, VarMethod::Parametric);
        let (mean, std, _, _) = RiskCalculator::stats(&returns);
        let expected_95 = mean - 1.645 * std;
        assert!((metrics.var_95 - expected_95).abs() < 1e-10);
    }

    #[test]
    fn ulcer_index_positive_for_drawdown() {
        // Returns that create a significant drawdown
        let returns = vec![-0.1, -0.1, -0.1, 0.05, 0.05];
        let metrics = RiskCalculator::calculate(&returns, VarMethod::Historical);
        assert!(metrics.ulcer_index > 0.0);
        assert!(metrics.max_drawdown > 0.0);
    }

    #[test]
    fn flat_returns_zero_drawdown() {
        let returns = vec![0.0, 0.0, 0.0, 0.0, 0.0];
        let metrics = RiskCalculator::calculate(&returns, VarMethod::Parametric);
        assert!((metrics.max_drawdown).abs() < 1e-10);
        assert!((metrics.ulcer_index).abs() < 1e-10);
    }

    #[test]
    fn stats_correct_mean() {
        let returns = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let (mean, _, _, _) = RiskCalculator::stats(&returns);
        assert!((mean - 3.0).abs() < 1e-10);
    }

    #[test]
    fn cornish_fisher_var_computed() {
        let returns = simple_returns();
        let metrics = RiskCalculator::calculate(&returns, VarMethod::CornishFisher);
        // Just verify it runs and produces a finite value
        assert!(metrics.var_95.is_finite());
        assert!(metrics.var_99.is_finite());
    }
}
