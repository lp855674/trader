//! HTTP and WebSocket API for quantd.

mod error;
mod handlers;
mod middleware;
mod ws;

pub use error::ApiError;
pub use handlers::{
    AmendOrderBody, CancelOrderBody, CreateOrderBody, OrderActionResponse, OrdersQuery,
    RuntimeExecutionStateBody, RuntimeExecutionStateQuery, RuntimeModeBody, RuntimeModeUpdate,
    RuntimeReconciliationLatestBody, RuntimeReconciliationQuery, StrategyConfigBody,
    StrategyConfigUpdate, SymbolAllowlistBody, SymbolAllowlistEntry, SymbolAllowlistUpdate,
    TickBody, TickResponse, UniverseCycleTrigger, get_strategy_config, put_strategy_config,
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
        side: Option<String>,
        qty: Option<f64>,
        status: Option<String>,
        order_type: Option<String>,
        limit_price: Option<f64>,
        exchange_ref: Option<String>,
        created_at_ms: Option<i64>,
        updated_at_ms: Option<i64>,
    },
    OrderUpdated {
        order_id: String,
        status: String,
        qty: f64,
        limit_price: Option<f64>,
    },
    OrderCancelled { order_id: String },
    OrderReplaced {
        order_id: String,
        qty: f64,
        limit_price: Option<f64>,
    },
    QuoteUpdated {
        symbol: String,
        venue: String,
        last_price: Option<f64>,
        day_high: Option<f64>,
        day_low: Option<f64>,
        bars: Vec<db::BarRow>,
    },
    /// Business or transport-level notice on the stream (`kind: "error"`, includes `error_code`).
    Error { error_code: String, message: String },
}

pub fn router(state: AppState) -> Router {
    let shared = Arc::new(state);
    let v1 = Router::new()
        .route("/instruments", get(handlers::list_instruments))
        .route("/orders", get(handlers::list_orders).post(handlers::post_order))
        .route(
            "/orders/:order_id/cancel",
            post(handlers::post_cancel_order),
        )
        .route("/orders/:order_id/amend", post(handlers::post_amend_order))
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
        .route(
            "/runtime/reconciliation/latest",
            get(handlers::get_runtime_reconciliation_latest),
        )
        .route("/terminal/overview", get(handlers::get_terminal_overview))
        .route("/quotes/:symbol", get(handlers::get_quote))
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
