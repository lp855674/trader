// HTTP REST API — pure handler logic.  No axum/actix.  These functions define
// the service interface and can be wired to a real HTTP framework later.

use serde::{Deserialize, Serialize};

use crate::backtest::storage::ResultStore;
use crate::config::schema::{BacktestConfigSchema, ConfigLoader};
use crate::core::registry::StrategyRegistry;

// ─── HealthResponse ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthResponse {
    pub status: String,
    pub version: String,
    pub uptime_ms: u64,
}

// ─── ApiError ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiError {
    pub code: u16,
    pub message: String,
}

impl ApiError {
    pub fn not_found(msg: &str) -> Self {
        Self {
            code: 404,
            message: msg.to_owned(),
        }
    }

    pub fn bad_request(msg: &str) -> Self {
        Self {
            code: 400,
            message: msg.to_owned(),
        }
    }

    pub fn internal(msg: &str) -> Self {
        Self {
            code: 500,
            message: msg.to_owned(),
        }
    }
}

// ─── Handler functions ────────────────────────────────────────────────────────

pub fn handle_health(started_at_ms: i64, current_ts_ms: i64) -> HealthResponse {
    let uptime_ms = (current_ts_ms - started_at_ms).max(0) as u64;
    HealthResponse {
        status: "ok".into(),
        version: env!("CARGO_PKG_VERSION").to_owned(),
        uptime_ms,
    }
}

pub fn handle_list_strategies(registry: &StrategyRegistry) -> Vec<String> {
    let mut ids = registry.list();
    ids.sort();
    ids
}

pub fn handle_get_strategy_config(
    registry: &StrategyRegistry,
    id: &str,
) -> Result<serde_json::Value, ApiError> {
    registry
        .get_config(id)
        .ok_or_else(|| ApiError::not_found(&format!("strategy '{id}' not found")))
}

pub fn handle_run_backtest(config_json: serde_json::Value) -> Result<serde_json::Value, ApiError> {
    // Parse and validate the backtest config
    let schema: BacktestConfigSchema = serde_json::from_value(config_json.clone())
        .map_err(|e| ApiError::bad_request(&e.to_string()))?;

    if schema.initial_capital <= 0.0 {
        return Err(ApiError::bad_request("initial_capital must be > 0"));
    }
    if schema.instruments.is_empty() {
        return Err(ApiError::bad_request("instruments list must not be empty"));
    }

    Ok(serde_json::json!({
        "status": "accepted",
        "config": config_json,
    }))
}

pub fn handle_get_backtest_result(
    store: &ResultStore,
    id: &str,
) -> Result<serde_json::Value, ApiError> {
    let result = store
        .get(id)
        .ok_or_else(|| ApiError::not_found(&format!("backtest result '{id}' not found")))?;
    serde_json::to_value(result).map_err(|e| ApiError::internal(&e.to_string()))
}

pub fn handle_list_backtest_results(store: &ResultStore) -> Vec<String> {
    store.list_ids()
}

// ─── HttpRouter ───────────────────────────────────────────────────────────────

pub struct HttpRouter {
    pub routes: Vec<(String, String, Box<dyn Fn(&str) -> String + Send + Sync>)>,
}

impl HttpRouter {
    pub fn new() -> Self {
        Self { routes: Vec::new() }
    }

    pub fn get(
        mut self,
        path: &str,
        handler: impl Fn(&str) -> String + Send + Sync + 'static,
    ) -> Self {
        self.routes
            .push(("GET".into(), path.to_owned(), Box::new(handler)));
        self
    }

    pub fn post(
        mut self,
        path: &str,
        handler: impl Fn(&str) -> String + Send + Sync + 'static,
    ) -> Self {
        self.routes
            .push(("POST".into(), path.to_owned(), Box::new(handler)));
        self
    }

    pub fn dispatch(&self, method: &str, path: &str, body: &str) -> Option<String> {
        let method_up = method.to_uppercase();
        for (m, p, handler) in &self.routes {
            if m == &method_up && p == path {
                return Some(handler(body));
            }
        }
        None
    }
}

impl Default for HttpRouter {
    fn default() -> Self {
        Self::new()
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    use crate::backtest::performance::PerformanceReport;
    use crate::backtest::storage::{BacktestResult, ResultStore};
    use crate::core::registry::StrategyRegistry;

    fn empty_registry() -> StrategyRegistry {
        StrategyRegistry::new()
    }

    fn empty_store() -> ResultStore {
        ResultStore::new()
    }

    fn dummy_result(id: &str) -> BacktestResult {
        BacktestResult {
            id: id.to_owned(),
            config: serde_json::json!({}),
            report: PerformanceReport {
                total_return: 0.0,
                annualised_return: 0.0,
                sharpe_ratio: 0.0,
                sortino_ratio: 0.0,
                calmar_ratio: 0.0,
                max_drawdown: 0.0,
                trade_count: 0,
                win_rate: 0.0,
                profit_factor: 0.0,
                avg_trade_pnl: 0.0,
            },
            equity_curve: vec![],
            created_at_ms: 0,
        }
    }

    #[test]
    fn health_handler_ok() {
        let h = handle_health(1000, 5000);
        assert_eq!(h.status, "ok");
        assert_eq!(h.uptime_ms, 4000);
    }

    #[test]
    fn health_uptime_never_negative() {
        let h = handle_health(5000, 1000); // current before started
        assert_eq!(h.uptime_ms, 0);
    }

    #[test]
    fn list_strategies_empty() {
        let reg = empty_registry();
        let ids = handle_list_strategies(&reg);
        assert!(ids.is_empty());
    }

    #[test]
    fn get_strategy_config_not_found() {
        let reg = empty_registry();
        let err = handle_get_strategy_config(&reg, "ghost").unwrap_err();
        assert_eq!(err.code, 404);
    }

    #[test]
    fn run_backtest_accepted() {
        let config = serde_json::json!({
            "start_date": "2024-01-01",
            "end_date": "2024-12-31",
            "initial_capital": 10000.0,
            "commission_rate": 0.001,
            "instruments": ["BTC/USDT"]
        });
        let resp = handle_run_backtest(config).unwrap();
        assert_eq!(resp["status"], "accepted");
    }

    #[test]
    fn run_backtest_bad_capital() {
        let config = serde_json::json!({
            "start_date": "2024-01-01",
            "end_date": "2024-12-31",
            "initial_capital": -1.0,
            "commission_rate": 0.001,
            "instruments": ["BTC"]
        });
        let err = handle_run_backtest(config).unwrap_err();
        assert_eq!(err.code, 400);
    }

    #[test]
    fn get_backtest_result_not_found() {
        let store = empty_store();
        let err = handle_get_backtest_result(&store, "nonexistent").unwrap_err();
        assert_eq!(err.code, 404);
    }

    #[test]
    fn list_backtest_results() {
        let mut store = empty_store();
        store.save(dummy_result("r1"));
        store.save(dummy_result("r2"));
        let ids = handle_list_backtest_results(&store);
        assert_eq!(ids, vec!["r1", "r2"]);
    }

    #[test]
    fn router_dispatch_get() {
        let router = HttpRouter::new().get("/health", |_| r#"{"status":"ok"}"#.into());
        let resp = router.dispatch("GET", "/health", "");
        assert_eq!(resp.unwrap(), r#"{"status":"ok"}"#);
    }

    #[test]
    fn router_dispatch_post() {
        let router = HttpRouter::new().post("/echo", |body| body.to_owned());
        let resp = router.dispatch("POST", "/echo", "hello");
        assert_eq!(resp.unwrap(), "hello");
    }

    #[test]
    fn router_dispatch_not_found() {
        let router = HttpRouter::new();
        let resp = router.dispatch("GET", "/missing", "");
        assert!(resp.is_none());
    }

    #[test]
    fn api_error_codes() {
        assert_eq!(ApiError::not_found("x").code, 404);
        assert_eq!(ApiError::bad_request("x").code, 400);
        assert_eq!(ApiError::internal("x").code, 500);
    }
}
