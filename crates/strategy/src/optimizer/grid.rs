use crate::backtest::performance::PerformanceReport;
use std::collections::HashMap;

/// Defines a range of parameter values.
#[derive(Debug, Clone)]
pub enum ParameterRange {
    /// Linearly spaced values between min and max.
    Continuous { min: f64, max: f64, steps: usize },
    /// Explicit list of values.
    Discrete(Vec<f64>),
    /// Boolean: [0.0, 1.0]
    Boolean,
}

impl ParameterRange {
    pub fn values(&self) -> Vec<f64> {
        match self {
            ParameterRange::Continuous { min, max, steps } => {
                if *steps <= 1 {
                    return vec![*min];
                }
                (0..*steps)
                    .map(|i| min + (max - min) * i as f64 / (*steps - 1) as f64)
                    .collect()
            }
            ParameterRange::Discrete(vals) => vals.clone(),
            ParameterRange::Boolean => vec![0.0, 1.0],
        }
    }

    /// Returns the range width (used for density calculations).
    pub fn range_width(&self) -> f64 {
        match self {
            ParameterRange::Continuous { min, max, .. } => max - min,
            ParameterRange::Discrete(vals) => {
                if vals.len() < 2 {
                    return 1.0;
                }
                let min = vals.iter().cloned().fold(f64::INFINITY, f64::min);
                let max = vals.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
                max - min
            }
            ParameterRange::Boolean => 1.0,
        }
    }
}

/// Defines the search space over multiple parameters.
#[derive(Debug, Clone)]
pub struct ParameterSpace {
    pub params: HashMap<String, ParameterRange>,
}

impl ParameterSpace {
    pub fn new() -> Self {
        Self {
            params: HashMap::new(),
        }
    }

    pub fn add(mut self, name: &str, range: ParameterRange) -> Self {
        self.params.insert(name.to_string(), range);
        self
    }

    /// Cartesian product of all parameter ranges.
    pub fn combinations(&self) -> Vec<HashMap<String, f64>> {
        // Collect in deterministic order
        let mut keys: Vec<String> = self.params.keys().cloned().collect();
        keys.sort();

        let value_lists: Vec<Vec<f64>> = keys.iter().map(|k| self.params[k].values()).collect();

        let mut result = vec![HashMap::new()];
        for (key, values) in keys.iter().zip(value_lists.iter()) {
            let mut new_result = Vec::new();
            for existing in &result {
                for &val in values {
                    let mut combo = existing.clone();
                    combo.insert(key.clone(), val);
                    new_result.push(combo);
                }
            }
            result = new_result;
        }
        result
    }
}

impl Default for ParameterSpace {
    fn default() -> Self {
        Self::new()
    }
}

/// A single result from grid search.
#[derive(Debug, Clone)]
pub struct GridSearchResult {
    pub params: HashMap<String, f64>,
    pub report: PerformanceReport,
    pub rank: usize,
}

/// Exhaustive grid search optimizer.
pub struct GridSearch {
    pub space: ParameterSpace,
}

impl GridSearch {
    pub fn new(space: ParameterSpace) -> Self {
        Self { space }
    }

    /// Evaluates all parameter combinations, sorts by sharpe_ratio descending, sets rank.
    pub fn run<F>(&self, mut evaluator: F) -> Vec<GridSearchResult>
    where
        F: FnMut(&HashMap<String, f64>) -> PerformanceReport,
    {
        let combos = self.space.combinations();
        let mut results: Vec<GridSearchResult> = combos
            .into_iter()
            .map(|params| {
                let report = evaluator(&params);
                GridSearchResult {
                    params,
                    report,
                    rank: 0,
                }
            })
            .collect();

        results.sort_by(|a, b| {
            b.report
                .sharpe_ratio
                .partial_cmp(&a.report.sharpe_ratio)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        for (i, result) in results.iter_mut().enumerate() {
            result.rank = i + 1;
        }

        results
    }

    pub fn best<'a>(&self, results: &'a [GridSearchResult]) -> Option<&'a GridSearchResult> {
        results.first()
    }

    pub fn top_n<'a>(&self, results: &'a [GridSearchResult], n: usize) -> &'a [GridSearchResult] {
        let end = n.min(results.len());
        &results[..end]
    }
}

/// Cache for grid search results keyed by JSON-serialized parameters.
pub struct ResultCache {
    pub entries: HashMap<String, PerformanceReport>,
}

impl ResultCache {
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }

    fn key(params: &HashMap<String, f64>) -> String {
        let mut keys: Vec<&String> = params.keys().collect();
        keys.sort();
        let pairs: Vec<String> = keys
            .iter()
            .map(|k| format!("{}:{}", k, params[*k]))
            .collect();
        format!("{{{}}}", pairs.join(","))
    }

    pub fn get(&self, params: &HashMap<String, f64>) -> Option<&PerformanceReport> {
        let key = Self::key(params);
        self.entries.get(&key)
    }

    pub fn insert(&mut self, params: &HashMap<String, f64>, report: PerformanceReport) {
        let key = Self::key(params);
        self.entries.insert(key, report);
    }
}

impl Default for ResultCache {
    fn default() -> Self {
        Self::new()
    }
}

/// Grid search that caches results to avoid duplicate evaluations.
pub struct CachedGridSearch {
    pub inner: GridSearch,
    pub cache: ResultCache,
}

impl CachedGridSearch {
    pub fn new(space: ParameterSpace) -> Self {
        Self {
            inner: GridSearch::new(space),
            cache: ResultCache::new(),
        }
    }

    pub fn run<F>(&mut self, mut evaluator: F) -> Vec<GridSearchResult>
    where
        F: FnMut(&HashMap<String, f64>) -> PerformanceReport,
    {
        let combos = self.inner.space.combinations();
        let mut results: Vec<GridSearchResult> = combos
            .into_iter()
            .map(|params| {
                let report = if let Some(cached) = self.cache.get(&params) {
                    cached.clone()
                } else {
                    let r = evaluator(&params);
                    self.cache.insert(&params, r.clone());
                    r
                };
                GridSearchResult {
                    params,
                    report,
                    rank: 0,
                }
            })
            .collect();

        results.sort_by(|a, b| {
            b.report
                .sharpe_ratio
                .partial_cmp(&a.report.sharpe_ratio)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        for (i, result) in results.iter_mut().enumerate() {
            result.rank = i + 1;
        }

        results
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_report(sharpe: f64) -> PerformanceReport {
        PerformanceReport {
            total_return: 0.1,
            annualised_return: 0.1,
            sharpe_ratio: sharpe,
            sortino_ratio: 0.0,
            calmar_ratio: 0.0,
            max_drawdown: 0.1,
            trade_count: 10,
            win_rate: 0.5,
            profit_factor: 1.2,
            avg_trade_pnl: 10.0,
        }
    }

    #[test]
    fn parameter_range_continuous() {
        let range = ParameterRange::Continuous {
            min: 0.0,
            max: 1.0,
            steps: 3,
        };
        let vals = range.values();
        assert_eq!(vals.len(), 3);
        assert!((vals[0] - 0.0).abs() < 1e-10);
        assert!((vals[1] - 0.5).abs() < 1e-10);
        assert!((vals[2] - 1.0).abs() < 1e-10);
    }

    #[test]
    fn parameter_range_discrete() {
        let range = ParameterRange::Discrete(vec![1.0, 2.0, 5.0]);
        let vals = range.values();
        assert_eq!(vals, vec![1.0, 2.0, 5.0]);
    }

    #[test]
    fn parameter_range_boolean() {
        let range = ParameterRange::Boolean;
        let vals = range.values();
        assert_eq!(vals, vec![0.0, 1.0]);
    }

    #[test]
    fn parameter_space_combinations_2x2() {
        let space = ParameterSpace::new()
            .add("a", ParameterRange::Discrete(vec![1.0, 2.0]))
            .add("b", ParameterRange::Discrete(vec![10.0, 20.0]));
        let combos = space.combinations();
        assert_eq!(combos.len(), 4);
        // Verify all 4 combinations exist
        let has = |a: f64, b: f64| combos.iter().any(|c| c["a"] == a && c["b"] == b);
        assert!(has(1.0, 10.0));
        assert!(has(1.0, 20.0));
        assert!(has(2.0, 10.0));
        assert!(has(2.0, 20.0));
    }

    #[test]
    fn parameter_space_single_param() {
        let space = ParameterSpace::new().add(
            "x",
            ParameterRange::Continuous {
                min: 0.0,
                max: 2.0,
                steps: 3,
            },
        );
        let combos = space.combinations();
        assert_eq!(combos.len(), 3);
    }

    #[test]
    fn grid_search_sorts_by_sharpe() {
        let space = ParameterSpace::new().add("x", ParameterRange::Discrete(vec![1.0, 2.0, 3.0]));
        let gs = GridSearch::new(space);
        let results = gs.run(|params| {
            let sharpe = params["x"]; // x=3 → highest sharpe
            make_report(sharpe)
        });
        assert_eq!(results.len(), 3);
        assert_eq!(results[0].rank, 1);
        assert!((results[0].params["x"] - 3.0).abs() < 1e-10);
        assert!((results[1].params["x"] - 2.0).abs() < 1e-10);
        assert!((results[2].params["x"] - 1.0).abs() < 1e-10);
    }

    #[test]
    fn grid_search_best_and_top_n() {
        let space =
            ParameterSpace::new().add("x", ParameterRange::Discrete(vec![1.0, 2.0, 3.0, 4.0, 5.0]));
        let gs = GridSearch::new(space);
        let results = gs.run(|p| make_report(p["x"]));
        let best = gs.best(&results);
        assert!(best.is_some());
        assert!((best.unwrap().params["x"] - 5.0).abs() < 1e-10);
        let top3 = gs.top_n(&results, 3);
        assert_eq!(top3.len(), 3);
    }

    #[test]
    fn result_cache_avoids_duplicate_calls() {
        let space = ParameterSpace::new().add("x", ParameterRange::Discrete(vec![1.0, 2.0]));
        let mut cgs = CachedGridSearch::new(space);
        let mut call_count = 0usize;
        let results = cgs.run(|p| {
            call_count += 1;
            make_report(p["x"])
        });
        assert_eq!(results.len(), 2);
        assert_eq!(call_count, 2);

        // Running again should use cache — call_count stays same
        let results2 = cgs.run(|_p| {
            call_count += 1;
            make_report(0.0)
        });
        assert_eq!(results2.len(), 2);
        assert_eq!(call_count, 2); // no new calls
    }
}
