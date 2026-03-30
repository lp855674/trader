//! HTTP and WebSocket API for quantd.

mod error;
mod handlers;
mod ws;

pub use error::ApiError;
pub use handlers::{OrdersQuery, TickBody, TickResponse};

use std::sync::Arc;

use axum::routing::{get, post};
use axum::Router;
use tokio::sync::broadcast;

#[derive(Clone)]
pub struct AppState {
    pub database: db::Db,
    pub events: broadcast::Sender<StreamEvent>,
    pub execution_router: exec::ExecutionRouter,
    pub ingest_registry: ingest::IngestRegistry,
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
    Error {
        error_code: String,
        message: String,
    },
}

pub fn router(state: AppState) -> Router {
    let shared = Arc::new(state);
    Router::new()
        .route("/health", get(handlers::health))
        .route("/v1/instruments", get(handlers::list_instruments))
        .route("/v1/orders", get(handlers::list_orders))
        .route("/v1/tick", post(handlers::post_tick))
        .route("/v1/stream", get(ws::ws_handler))
        .with_state(shared)
}
