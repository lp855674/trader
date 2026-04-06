use std::collections::HashMap;
use crate::optimizer::grid::ParameterSpace;

/// Acquisition function used to select the next candidate.
#[derive(Debug, Clone)]
pub enum AcquisitionFunction {
    ExpectedImprovement,
    UpperConfidenceBound { kappa: f64 },
    ProbabilityOfImprovement,
}

/// A single observation: parameters tried and the resulting score.
#[derive(Debug, Clone)]
pub struct Observation {
    pub params: HashMap<String, f64>,
    pub score: f64,
}

/// Result of Bayesian optimization run.
#[derive(Debug, Clone)]
pub struct BayesianResult {
    pub best_params: HashMap<String, f64>,
    pub best_score: f64,
    pub observations: Vec<Observation>,
    pub iterations: usize,
}

/// Simple LCG random number generator (deterministic, no external crate).
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

    /// Returns a float in [0, 1)
    fn next_f64(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }

    /// Sample a random index in [0, n)
    fn next_usize(&mut self, n: usize) -> usize {
        if n == 0 { return 0; }
        (self.next_u64() % n as u64) as usize
    }
}

/// Bayesian optimizer using a simplified TPE (Tree-structured Parzen Estimator).
pub struct BayesianOptimizer {
    pub space: ParameterSpace,
    pub acquisition: AcquisitionFunction,
    pub n_initial: usize,
    pub max_iter: usize,
    pub observations: Vec<Observation>,
}

impl BayesianOptimizer {
    pub fn new(
        space: ParameterSpace,
        acquisition: AcquisitionFunction,
        n_initial: usize,
        max_iter: usize,
    ) -> Self {
        Self {
            space,
            acquisition,
            n_initial,
            max_iter,
            observations: Vec::new(),
        }
    }

    /// Suggests the next set of parameters to try.
    pub fn suggest(&self) -> HashMap<String, f64> {
        if self.observations.len() < self.n_initial {
            // Random sampling using observation count as seed
            let seed = self.observations.len() as u64 + 1;
            return self.random_params(seed);
        }
        // Simplified TPE: split into good (top 25%) and bad (rest)
        let n_good = ((self.observations.len() as f64 * 0.25).ceil() as usize).max(1);
        let mut sorted = self.observations.clone();
        sorted.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
        let good = &sorted[..n_good];
        let bad = &sorted[n_good..];

        // Sample 50 candidates and pick the one with highest good/bad density ratio
        let seed = self.observations.len() as u64 * 7919 + 1;
        let mut rng = Lcg::new(seed);
        let all_combos = self.space.combinations();
        let n_candidates = 50.min(all_combos.len());

        let mut best_score = f64::NEG_INFINITY;
        let mut best_params = self.random_params(seed);

        for _ in 0..n_candidates {
            let idx = rng.next_usize(all_combos.len());
            let candidate = &all_combos[idx];
            let good_density = self.density(candidate, good);
            let bad_density = self.density(candidate, bad);
            let ratio = good_density / (bad_density + 1e-10);
            if ratio > best_score {
                best_score = ratio;
                best_params = candidate.clone();
            }
        }
        best_params
    }

    /// Density: fraction of group members with each param within 20% of that param's range.
    fn density(&self, candidate: &HashMap<String, f64>, group: &[Observation]) -> f64 {
        if group.is_empty() {
            return 1.0;
        }
        let count = group.iter().filter(|obs| {
            candidate.iter().all(|(k, &v)| {
                let width = self.space.params.get(k)
                    .map(|r| r.range_width())
                    .unwrap_or(1.0);
                let threshold = width * 0.20;
                let obs_val = obs.params.get(k).copied().unwrap_or(v);
                (obs_val - v).abs() <= threshold.max(1e-10)
            })
        }).count();
        count as f64 / group.len() as f64
    }

    /// Generate random parameters using an LCG seeded by `seed`.
    fn random_params(&self, seed: u64) -> HashMap<String, f64> {
        let mut rng = Lcg::new(seed);
        let mut params = HashMap::new();
        let mut keys: Vec<&String> = self.space.params.keys().collect();
        keys.sort();
        for key in keys {
            let values = self.space.params[key].values();
            if !values.is_empty() {
                let idx = rng.next_usize(values.len());
                params.insert(key.clone(), values[idx]);
            }
        }
        params
    }

    /// Record an observation.
    pub fn observe(&mut self, params: HashMap<String, f64>, score: f64) {
        self.observations.push(Observation { params, score });
    }

    /// Run the optimization loop.
    pub fn run<F>(&mut self, mut evaluator: F) -> BayesianResult
    where
        F: FnMut(&HashMap<String, f64>) -> f64,
    {
        for _ in 0..self.max_iter {
            let params = self.suggest();
            let score = evaluator(&params);
            self.observe(params, score);
        }

        let best = self.observations.iter()
            .max_by(|a, b| a.score.partial_cmp(&b.score).unwrap_or(std::cmp::Ordering::Equal));

        let (best_params, best_score) = best
            .map(|o| (o.params.clone(), o.score))
            .unwrap_or_else(|| (HashMap::new(), f64::NEG_INFINITY));

        BayesianResult {
            best_params,
            best_score,
            observations: self.observations.clone(),
            iterations: self.observations.len(),
        }
    }

    /// Returns true if the last `window` observations improved by less than `tolerance`.
    pub fn early_stopping_check(&self, window: usize, tolerance: f64) -> bool {
        if self.observations.len() < window {
            return false;
        }
        let recent = &self.observations[self.observations.len() - window..];
        let max_score = recent.iter().map(|o| o.score).fold(f64::NEG_INFINITY, f64::max);
        let min_score = recent.iter().map(|o| o.score).fold(f64::INFINITY, f64::min);
        (max_score - min_score).abs() < tolerance
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::optimizer::grid::ParameterRange;

    fn make_space() -> ParameterSpace {
        ParameterSpace::new()
            .add("x", ParameterRange::Continuous { min: 0.0, max: 10.0, steps: 11 })
            .add("y", ParameterRange::Discrete(vec![1.0, 2.0, 3.0]))
    }

    #[test]
    fn runs_n_initial_random_trials() {
        let space = make_space();
        let mut opt = BayesianOptimizer::new(space, AcquisitionFunction::ExpectedImprovement, 5, 10);
        let result = opt.run(|p| p["x"] + p["y"]);
        assert_eq!(result.iterations, 10);
        assert_eq!(result.observations.len(), 10);
    }

    #[test]
    fn observe_updates_history() {
        let space = make_space();
        let mut opt = BayesianOptimizer::new(space, AcquisitionFunction::ExpectedImprovement, 3, 5);
        let mut params = HashMap::new();
        params.insert("x".to_string(), 5.0);
        params.insert("y".to_string(), 2.0);
        opt.observe(params, 7.0);
        assert_eq!(opt.observations.len(), 1);
        assert!((opt.observations[0].score - 7.0).abs() < 1e-10);
    }

    #[test]
    fn early_stopping_triggers_when_flat() {
        let space = make_space();
        let mut opt = BayesianOptimizer::new(space, AcquisitionFunction::ExpectedImprovement, 3, 10);
        // Add 5 flat observations
        for _ in 0..5 {
            let mut p = HashMap::new();
            p.insert("x".to_string(), 1.0);
            p.insert("y".to_string(), 1.0);
            opt.observe(p, 1.0);
        }
        assert!(opt.early_stopping_check(5, 0.01));
    }

    #[test]
    fn early_stopping_does_not_trigger_when_improving() {
        let space = make_space();
        let mut opt = BayesianOptimizer::new(space, AcquisitionFunction::ExpectedImprovement, 3, 10);
        for i in 0..5 {
            let mut p = HashMap::new();
            p.insert("x".to_string(), i as f64);
            p.insert("y".to_string(), 1.0);
            opt.observe(p, i as f64);
        }
        assert!(!opt.early_stopping_check(5, 0.01));
    }

    #[test]
    fn early_stopping_returns_false_when_insufficient_data() {
        let space = make_space();
        let opt = BayesianOptimizer::new(space, AcquisitionFunction::ExpectedImprovement, 3, 10);
        assert!(!opt.early_stopping_check(5, 0.01));
    }

    #[test]
    fn suggest_returns_valid_params() {
        let space = make_space();
        let opt = BayesianOptimizer::new(space.clone(), AcquisitionFunction::ExpectedImprovement, 3, 10);
        let params = opt.suggest();
        assert!(params.contains_key("x"));
        assert!(params.contains_key("y"));
    }

    #[test]
    fn bayesian_finds_optimum() {
        // Simple: x in [0, 10], best at x=10
        let space = ParameterSpace::new()
            .add("x", ParameterRange::Continuous { min: 0.0, max: 10.0, steps: 11 });
        let mut opt = BayesianOptimizer::new(
            space,
            AcquisitionFunction::ExpectedImprovement,
            3,
            20,
        );
        let result = opt.run(|p| p["x"]);
        // After 20 iterations with n_initial=3, should find a decent value
        assert!(result.best_score >= 5.0);
    }
}
