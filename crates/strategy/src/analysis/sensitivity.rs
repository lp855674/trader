use crate::optimizer::grid::{GridSearchResult, ParameterSpace};
use std::collections::HashMap;

/// Sensitivity of a single parameter.
#[derive(Debug, Clone)]
pub struct ParameterSensitivity {
    pub param_name: String,
    pub base_score: f64,
    /// (param_value, score) pairs across the param's range.
    pub sensitivities: Vec<(f64, f64)>,
    /// Approximate first derivative: score change per unit param change.
    pub gradient: f64,
    /// True if max score variation < 20% of |base_score|.
    pub is_robust: bool,
}

/// Full robustness report across all parameters.
#[derive(Debug, Clone)]
pub struct RobustnessReport {
    pub base_params: HashMap<String, f64>,
    pub base_score: f64,
    pub sensitivities: HashMap<String, ParameterSensitivity>,
    /// Fraction of parameters that are robust.
    pub overall_robustness: f64,
    /// Name of the parameter with the largest gradient magnitude.
    pub most_sensitive_param: Option<String>,
}

/// Analyzes parameter sensitivity for strategy optimization.
pub struct SensitivityAnalyzer {
    pub space: ParameterSpace,
}

impl SensitivityAnalyzer {
    pub fn new(space: ParameterSpace) -> Self {
        Self { space }
    }

    /// For each param, vary across its full range while fixing others at base.
    pub fn analyze<F>(
        &self,
        base_params: &HashMap<String, f64>,
        mut evaluator: F,
        perturbation_steps: usize,
    ) -> RobustnessReport
    where
        F: FnMut(&HashMap<String, f64>) -> f64,
    {
        let base_score = evaluator(base_params);
        let mut sensitivities = HashMap::new();

        let mut keys: Vec<String> = self.space.params.keys().cloned().collect();
        keys.sort();

        for param_name in &keys {
            let range = &self.space.params[param_name];
            // Generate perturbation_steps evenly spaced values across the range
            let values = if perturbation_steps <= 1 {
                range.values()
            } else {
                let range_vals = range.values();
                if range_vals.len() <= perturbation_steps {
                    range_vals
                } else {
                    // Sample perturbation_steps evenly from range_vals
                    let n = range_vals.len();
                    (0..perturbation_steps)
                        .map(|i| {
                            let idx = i * (n - 1) / (perturbation_steps - 1);
                            range_vals[idx]
                        })
                        .collect()
                }
            };

            let mut pairs: Vec<(f64, f64)> = values
                .iter()
                .map(|&v| {
                    let mut p = base_params.clone();
                    p.insert(param_name.clone(), v);
                    let score = evaluator(&p);
                    (v, score)
                })
                .collect();
            pairs.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));

            let gradient = compute_gradient(&pairs);
            let max_variation = compute_max_variation(&pairs);
            let is_robust = max_variation < 0.20 * base_score.abs().max(1e-10);

            sensitivities.insert(
                param_name.clone(),
                ParameterSensitivity {
                    param_name: param_name.clone(),
                    base_score,
                    sensitivities: pairs,
                    gradient,
                    is_robust,
                },
            );
        }

        let total = sensitivities.len();
        let robust_count = sensitivities.values().filter(|s| s.is_robust).count();
        let overall_robustness = if total == 0 {
            1.0
        } else {
            robust_count as f64 / total as f64
        };

        let most_sensitive_param = sensitivities
            .values()
            .max_by(|a, b| {
                a.gradient
                    .abs()
                    .partial_cmp(&b.gradient.abs())
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|s| s.param_name.clone());

        RobustnessReport {
            base_params: base_params.clone(),
            base_score,
            sensitivities,
            overall_robustness,
            most_sensitive_param,
        }
    }

    /// Pearson correlation between each pair of parameter values across all results,
    /// weighted by sharpe_ratio (weight = max(sharpe, 0)).
    pub fn correlation_matrix(
        &self,
        results: &[GridSearchResult],
    ) -> HashMap<(String, String), f64> {
        let mut param_names: Vec<String> = self.space.params.keys().cloned().collect();
        param_names.sort();

        let mut correlations = HashMap::new();
        for i in 0..param_names.len() {
            for j in 0..param_names.len() {
                let a = &param_names[i];
                let b = &param_names[j];
                let corr = pearson_correlation(results, a, b);
                correlations.insert((a.clone(), b.clone()), corr);
            }
        }
        correlations
    }
}

fn compute_gradient(pairs: &[(f64, f64)]) -> f64 {
    if pairs.len() < 2 {
        return 0.0;
    }
    let first = pairs.first().unwrap();
    let last = pairs.last().unwrap();
    let dx = last.0 - first.0;
    if dx.abs() < 1e-10 {
        0.0
    } else {
        (last.1 - first.1) / dx
    }
}

fn compute_max_variation(pairs: &[(f64, f64)]) -> f64 {
    if pairs.is_empty() {
        return 0.0;
    }
    let scores: Vec<f64> = pairs.iter().map(|p| p.1).collect();
    let max = scores.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let min = scores.iter().cloned().fold(f64::INFINITY, f64::min);
    max - min
}

fn pearson_correlation(results: &[GridSearchResult], a: &str, b: &str) -> f64 {
    let pairs: Vec<(f64, f64)> = results
        .iter()
        .filter_map(|r| {
            let va = r.params.get(a)?;
            let vb = r.params.get(b)?;
            Some((*va, *vb))
        })
        .collect();

    let n = pairs.len();
    if n < 2 {
        return if a == b { 1.0 } else { 0.0 };
    }

    let mean_a = pairs.iter().map(|p| p.0).sum::<f64>() / n as f64;
    let mean_b = pairs.iter().map(|p| p.1).sum::<f64>() / n as f64;

    let cov: f64 = pairs.iter().map(|p| (p.0 - mean_a) * (p.1 - mean_b)).sum();
    let std_a: f64 = pairs
        .iter()
        .map(|p| (p.0 - mean_a).powi(2))
        .sum::<f64>()
        .sqrt();
    let std_b: f64 = pairs
        .iter()
        .map(|p| (p.1 - mean_b).powi(2))
        .sum::<f64>()
        .sqrt();

    if std_a < 1e-10 || std_b < 1e-10 {
        if a == b { 1.0 } else { 0.0 }
    } else {
        cov / (std_a * std_b)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backtest::performance::PerformanceReport;
    use crate::optimizer::grid::{GridSearch, ParameterRange};

    fn make_space() -> ParameterSpace {
        ParameterSpace::new()
            .add(
                "x",
                ParameterRange::Continuous {
                    min: 0.0,
                    max: 10.0,
                    steps: 11,
                },
            )
            .add("y", ParameterRange::Discrete(vec![1.0, 2.0, 3.0]))
    }

    fn make_base_params() -> HashMap<String, f64> {
        let mut p = HashMap::new();
        p.insert("x".to_string(), 5.0);
        p.insert("y".to_string(), 2.0);
        p
    }

    #[test]
    fn sensitivity_detects_high_gradient_param() {
        let space = make_space();
        let analyzer = SensitivityAnalyzer::new(space);
        let base = make_base_params();
        // score = 10*x + y → x has very high gradient
        let report = analyzer.analyze(&base, |p| 10.0 * p["x"] + p["y"], 5);
        assert!(report.sensitivities.contains_key("x"));
        let x_sens = &report.sensitivities["x"];
        assert!(x_sens.gradient.abs() > 5.0);
        assert_eq!(report.most_sensitive_param.as_deref(), Some("x"));
    }

    #[test]
    fn robustness_flag_works() {
        let space = ParameterSpace::new().add(
            "x",
            ParameterRange::Continuous {
                min: 0.0,
                max: 10.0,
                steps: 11,
            },
        );
        let analyzer = SensitivityAnalyzer::new(space);
        let mut base = HashMap::new();
        base.insert("x".to_string(), 5.0);

        // Flat evaluator → robust
        let report_flat = analyzer.analyze(&base, |_| 1.0, 5);
        assert!(report_flat.sensitivities["x"].is_robust);

        // High sensitivity evaluator → not robust
        let report_steep = analyzer.analyze(&base, |p| p["x"] * 100.0, 5);
        assert!(!report_steep.sensitivities["x"].is_robust);
    }

    #[test]
    fn correlation_between_identical_params() {
        let space =
            ParameterSpace::new().add("x", ParameterRange::Discrete(vec![1.0, 2.0, 3.0, 4.0, 5.0]));
        let analyzer = SensitivityAnalyzer::new(space.clone());
        let gs = GridSearch::new(space);
        let results = gs.run(|p| PerformanceReport {
            total_return: p["x"],
            annualised_return: p["x"],
            sharpe_ratio: p["x"],
            sortino_ratio: 0.0,
            calmar_ratio: 0.0,
            max_drawdown: 0.1,
            trade_count: 1,
            win_rate: 0.5,
            profit_factor: 1.0,
            avg_trade_pnl: 1.0,
        });
        let corr = analyzer.correlation_matrix(&results);
        let self_corr = corr[&("x".to_string(), "x".to_string())];
        assert!((self_corr - 1.0).abs() < 1e-9);
    }

    #[test]
    fn robustness_report_overall_fraction() {
        let space = ParameterSpace::new()
            .add(
                "x",
                ParameterRange::Continuous {
                    min: 0.0,
                    max: 10.0,
                    steps: 11,
                },
            )
            .add("y", ParameterRange::Discrete(vec![1.0, 2.0, 3.0]));
        let analyzer = SensitivityAnalyzer::new(space);
        let mut base = HashMap::new();
        base.insert("x".to_string(), 5.0);
        base.insert("y".to_string(), 2.0);
        // score = constant → all params robust
        let report = analyzer.analyze(&base, |_| 1.0, 5);
        assert!((report.overall_robustness - 1.0).abs() < 1e-10);
    }
}
