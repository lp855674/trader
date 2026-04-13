//! HTTP and WebSocket API for quantd.

mod error;
mod handlers;
mod middleware;
mod ws;

pub use error::ApiError;
pub use handlers::{
    OrdersQuery, RuntimeExecutionStateBody, RuntimeExecutionStateQuery, RuntimeModeBody,
    RuntimeModeUpdate, StrategyConfigBody, StrategyConfigUpdate, SymbolAllowlistBody,
    SymbolAllowlistEntry, SymbolAllowlistUpdate, TickBody, TickResponse, UniverseCycleTrigger,
    get_strategy_config, put_strategy_config,
};

use std::sync::Arc;

use axum::Router;
use axum::routing::{get, post, put};
use tokio::sync::broadcast;

#[derive(Clone)]
pub struct AppState {
    pub database: db::Db,
    pub events: broadcast::Sender<StreamEvent>,
    pub execution_router: exec::ExecutionRouter,
    pub ingest_registry: ingest::IngestRegistry,
    pub risk_limits: pipeline::RiskLimits,
    /// `/v1/tick` 使用的策略（由 `quantd` 根据 `QUANTD_STRATEGY` 注入）。
    pub strategy: std::sync::Arc<dyn strategy::Strategy>,
    pub api_key: Option<String>,
}

#[derive(Clone, Debug)]
pub enum StreamEvent {
    /// Emitted after a successful paper/live placement (MVP: matches plan Task 11 `order_created`).
    OrderCreated {
        order_id: String,
        venue: domain::Venue,
        symbol: String,
    },
    /// Business or transport-level notice on the stream (`kind: "error"`, includes `error_code`).
    Error { error_code: String, message: String },
}

pub fn router(state: AppState) -> Router {
    let shared = Arc::new(state);
    let v1 = Router::new()
        .route("/instruments", get(handlers::list_instruments))
        .route("/orders", get(handlers::list_orders))
        .route("/tick", post(handlers::post_tick))
        .route("/stream", get(ws::ws_handler))
        .route("/runtime/mode", get(handlers::get_runtime_mode))
        .route("/runtime/mode", put(handlers::put_runtime_mode))
        .route("/runtime/allowlist", get(handlers::get_symbol_allowlist))
        .route("/runtime/allowlist", put(handlers::put_symbol_allowlist))
        .route("/runtime/cycle", post(handlers::post_runtime_cycle))
        .route(
            "/runtime/cycle/latest",
            get(handlers::get_latest_runtime_cycle),
        )
        .route(
            "/runtime/cycle/history",
            get(handlers::get_runtime_cycle_history),
        )
        .route(
            "/runtime/execution-state",
            get(handlers::get_runtime_execution_state),
        )
        .route("/strategy/config", get(handlers::get_strategy_config))
        .route("/strategy/config", put(handlers::put_strategy_config))
        .route_layer(axum::middleware::from_fn_with_state(
            shared.clone(),
            middleware::require_api_key,
        ));

    Router::new()
        .route("/health", get(handlers::health))
        .nest("/v1", v1)
        .with_state(shared)
}
