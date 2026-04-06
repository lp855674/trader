use std::collections::HashMap;
use serde::{Deserialize, Serialize};
use crate::optimizer::grid::{GridSearch, ParameterSpace, ParameterRange};
use crate::optimizer::bayesian::{BayesianOptimizer, AcquisitionFunction};

/// Method to use for optimization.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum OptimizationMethod {
    Grid,
    Bayesian,
    Random,
}

/// Objective metric to optimize for.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ObjectiveMetric {
    SharpeRatio,
    TotalReturn,
    Calmar,
    SortinoRatio,
}

/// Status of an optimization job.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OptimizationStatus {
    Pending,
    Running { progress: f64 },
    Completed,
    Failed(String),
}

/// Request to run an optimization job.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptimizationRequest {
    pub id: String,
    pub strategy_type: String,
    /// JSON representation of ParameterSpace (used for configuration).
    pub param_space: serde_json::Value,
    pub method: OptimizationMethod,
    pub max_iter: usize,
    pub objective: ObjectiveMetric,
}

/// Response/state of an optimization job.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptimizationResponse {
    pub id: String,
    pub status: OptimizationStatus,
    pub best_params: Option<HashMap<String, f64>>,
    pub best_score: Option<f64>,
    pub iterations_completed: usize,
    pub total_iterations: usize,
    /// Serialized results.
    pub results: Vec<serde_json::Value>,
}

/// In-process optimization service.
pub struct OptimizationService {
    pub jobs: HashMap<String, OptimizationResponse>,
    requests: HashMap<String, OptimizationRequest>,
}

impl OptimizationService {
    pub fn new() -> Self {
        Self { jobs: HashMap::new(), requests: HashMap::new() }
    }

    /// Submit a new job, returns the job id.
    pub fn submit(&mut self, request: OptimizationRequest) -> String {
        let id = request.id.clone();
        let total_iterations = request.max_iter;
        let response = OptimizationResponse {
            id: id.clone(),
            status: OptimizationStatus::Pending,
            best_params: None,
            best_score: None,
            iterations_completed: 0,
            total_iterations,
            results: Vec::new(),
        };
        self.jobs.insert(id.clone(), response);
        self.requests.insert(id.clone(), request);
        id
    }

    pub fn get_status(&self, id: &str) -> Option<&OptimizationResponse> {
        self.jobs.get(id)
    }

    /// Run the job synchronously with the given evaluator. Updates response on completion.
    pub fn run_job<F>(&mut self, id: &str, mut evaluator: F) -> Result<(), String>
    where
        F: FnMut(&HashMap<String, f64>) -> f64,
    {
        let job = self.requests.get(id).ok_or_else(|| format!("Job '{}' not found", id))?.clone();

        // Parse parameter space from JSON
        let space = parse_param_space(&job.param_space);
        let max_iter = job.max_iter;

        // Update status to Running
        if let Some(resp) = self.jobs.get_mut(id) {
            resp.status = OptimizationStatus::Running { progress: 0.0 };
        }

        let (best_params, best_score, iterations_completed, results) = match &job.method {
            OptimizationMethod::Grid => {
                let gs = GridSearch::new(space);
                let grid_results = gs.run(|p| {
                    let score = evaluator(p);
                    // Wrap score in a PerformanceReport-like object
                    crate::backtest::performance::PerformanceReport {
                        total_return: score,
                        annualised_return: score,
                        sharpe_ratio: score,
                        sortino_ratio: score,
                        calmar_ratio: score,
                        max_drawdown: 0.0,
                        trade_count: 1,
                        win_rate: 0.5,
                        profit_factor: 1.0,
                        avg_trade_pnl: 0.0,
                    }
                });
                let best = grid_results.first().map(|r| (r.params.clone(), r.report.sharpe_ratio));
                let n = grid_results.len();
                let results_json: Vec<serde_json::Value> = grid_results
                    .into_iter()
                    .map(|r| serde_json::json!({
                        "params": r.params,
                        "score": r.report.sharpe_ratio,
                        "rank": r.rank,
                    }))
                    .collect();
                match best {
                    Some((p, s)) => (Some(p), Some(s), n, results_json),
                    None => (None, None, 0, vec![]),
                }
            }
            OptimizationMethod::Bayesian => {
                let mut opt = BayesianOptimizer::new(
                    space,
                    AcquisitionFunction::ExpectedImprovement,
                    (max_iter / 4).max(3),
                    max_iter,
                );
                let bayesian_result = opt.run(&mut evaluator);
                let n = bayesian_result.iterations;
                let results_json: Vec<serde_json::Value> = bayesian_result.observations
                    .iter()
                    .map(|o| serde_json::json!({
                        "params": o.params,
                        "score": o.score,
                    }))
                    .collect();
                (
                    Some(bayesian_result.best_params),
                    Some(bayesian_result.best_score),
                    n,
                    results_json,
                )
            }
            OptimizationMethod::Random => {
                let combos = space.combinations();
                let n = max_iter.min(combos.len());
                let mut best_score = f64::NEG_INFINITY;
                let mut best_params = HashMap::new();
                let mut results_json = Vec::new();
                for combo in combos.iter().take(n) {
                    let score = evaluator(combo);
                    if score > best_score {
                        best_score = score;
                        best_params = combo.clone();
                    }
                    results_json.push(serde_json::json!({
                        "params": combo,
                        "score": score,
                    }));
                }
                (Some(best_params), Some(best_score), n, results_json)
            }
        };

        if let Some(resp) = self.jobs.get_mut(id) {
            resp.status = OptimizationStatus::Completed;
            resp.best_params = best_params;
            resp.best_score = best_score;
            resp.iterations_completed = iterations_completed;
            resp.results = results;
        }

        Ok(())
    }

    pub fn list_jobs(&self) -> Vec<String> {
        let mut ids: Vec<String> = self.jobs.keys().cloned().collect();
        ids.sort();
        ids
    }

    /// Cancel a job. Returns false if job not found.
    pub fn cancel_job(&mut self, id: &str) -> bool {
        if let Some(resp) = self.jobs.get_mut(id) {
            resp.status = OptimizationStatus::Failed("cancelled".to_string());
            true
        } else {
            false
        }
    }
}

impl Default for OptimizationService {
    fn default() -> Self {
        Self::new()
    }
}

/// Parse a simple ParameterSpace from JSON.
/// Expected format: { "param_name": { "type": "continuous", "min": 0.0, "max": 1.0, "steps": 10 } }
/// or { "param_name": { "type": "discrete", "values": [1.0, 2.0, 3.0] } }
/// or { "param_name": { "type": "boolean" } }
fn parse_param_space(json: &serde_json::Value) -> ParameterSpace {
    let mut space = ParameterSpace::new();
    if let Some(obj) = json.as_object() {
        for (name, spec) in obj {
            let range = if let Some(t) = spec.get("type").and_then(|v| v.as_str()) {
                match t {
                    "continuous" => {
                        let min = spec.get("min").and_then(|v| v.as_f64()).unwrap_or(0.0);
                        let max = spec.get("max").and_then(|v| v.as_f64()).unwrap_or(1.0);
                        let steps = spec.get("steps").and_then(|v| v.as_u64()).unwrap_or(10) as usize;
                        ParameterRange::Continuous { min, max, steps }
                    }
                    "discrete" => {
                        let vals: Vec<f64> = spec.get("values")
                            .and_then(|v| v.as_array())
                            .map(|arr| arr.iter().filter_map(|v| v.as_f64()).collect())
                            .unwrap_or_default();
                        ParameterRange::Discrete(vals)
                    }
                    "boolean" => ParameterRange::Boolean,
                    _ => ParameterRange::Discrete(vec![0.0, 1.0]),
                }
            } else {
                // Try to infer from values
                if let Some(arr) = spec.as_array() {
                    let vals: Vec<f64> = arr.iter().filter_map(|v| v.as_f64()).collect();
                    ParameterRange::Discrete(vals)
                } else {
                    ParameterRange::Discrete(vec![0.0, 1.0])
                }
            };
            space = space.add(name, range);
        }
    }
    // If no params found, add a default one so optimization runs
    if space.params.is_empty() {
        space = space.add("default", ParameterRange::Discrete(vec![1.0]));
    }
    space
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_request(id: &str, method: OptimizationMethod) -> OptimizationRequest {
        OptimizationRequest {
            id: id.to_string(),
            strategy_type: "test".to_string(),
            param_space: serde_json::json!({
                "x": { "type": "discrete", "values": [1.0, 2.0, 3.0] }
            }),
            method,
            max_iter: 5,
            objective: ObjectiveMetric::SharpeRatio,
        }
    }

    #[test]
    fn submit_job_stores_pending() {
        let mut svc = OptimizationService::new();
        let id = svc.submit(make_request("job1", OptimizationMethod::Grid));
        assert_eq!(id, "job1");
        let status = svc.get_status("job1").unwrap();
        assert!(matches!(status.status, OptimizationStatus::Pending));
    }

    #[test]
    fn run_job_updates_to_completed() {
        let mut svc = OptimizationService::new();
        svc.submit(make_request("job2", OptimizationMethod::Grid));
        svc.run_job("job2", |p| p.get("x").copied().unwrap_or(0.0)).unwrap();
        let resp = svc.get_status("job2").unwrap();
        assert!(matches!(resp.status, OptimizationStatus::Completed));
        assert!(resp.best_params.is_some());
        assert!(resp.best_score.is_some());
        // Best should be x=3 (highest score)
        let best = resp.best_score.unwrap();
        assert!((best - 3.0).abs() < 1e-10);
    }

    #[test]
    fn cancel_job_marks_failed() {
        let mut svc = OptimizationService::new();
        svc.submit(make_request("job3", OptimizationMethod::Grid));
        let cancelled = svc.cancel_job("job3");
        assert!(cancelled);
        let resp = svc.get_status("job3").unwrap();
        assert!(matches!(&resp.status, OptimizationStatus::Failed(msg) if msg == "cancelled"));
    }

    #[test]
    fn cancel_nonexistent_returns_false() {
        let mut svc = OptimizationService::new();
        assert!(!svc.cancel_job("nonexistent"));
    }

    #[test]
    fn list_jobs_returns_all() {
        let mut svc = OptimizationService::new();
        svc.submit(make_request("j1", OptimizationMethod::Grid));
        svc.submit(make_request("j2", OptimizationMethod::Bayesian));
        let jobs = svc.list_jobs();
        assert_eq!(jobs.len(), 2);
        assert!(jobs.contains(&"j1".to_string()));
        assert!(jobs.contains(&"j2".to_string()));
    }

    #[test]
    fn bayesian_job_completes() {
        let mut svc = OptimizationService::new();
        svc.submit(make_request("bay1", OptimizationMethod::Bayesian));
        svc.run_job("bay1", |p| p.get("x").copied().unwrap_or(0.0)).unwrap();
        let resp = svc.get_status("bay1").unwrap();
        assert!(matches!(resp.status, OptimizationStatus::Completed));
        assert!(resp.best_params.is_some());
    }

    #[test]
    fn random_job_completes() {
        let mut svc = OptimizationService::new();
        svc.submit(make_request("rand1", OptimizationMethod::Random));
        svc.run_job("rand1", |p| p.get("x").copied().unwrap_or(0.0)).unwrap();
        let resp = svc.get_status("rand1").unwrap();
        assert!(matches!(resp.status, OptimizationStatus::Completed));
    }

    #[test]
    fn run_job_not_found_returns_error() {
        let mut svc = OptimizationService::new();
        let result = svc.run_job("nonexistent", |_| 0.0);
        assert!(result.is_err());
    }
}
