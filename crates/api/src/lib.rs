//! HTTP and WebSocket API for quantd.

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use axum::extract::Query;
use axum::extract::State;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
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
    OrderCycleDone { venue: domain::Venue },
}

#[derive(Serialize)]
struct HealthBody {
    status: &'static str,
}

#[derive(Deserialize)]
pub struct TickBody {
    /// `US_EQUITY`, `HK_EQUITY`, `CRYPTO`, `POLYMARKET`
    pub venue: String,
    pub symbol: String,
    #[serde(default)]
    pub account_id: Option<String>,
}

#[derive(Serialize)]
pub struct TickResponse {
    pub ok: bool,
    pub venue: String,
    pub symbol: String,
}

#[derive(Deserialize)]
pub struct OrdersQuery {
    pub account_id: String,
}

async fn health() -> Json<HealthBody> {
    Json(HealthBody { status: "ok" })
}

async fn list_instruments(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<db::InstrumentRow>>, ApiError> {
    let rows = db::list_instruments(state.database.pool())
        .await
        .map_err(ApiError::internal)?;
    Ok(Json(rows))
}

async fn list_orders(
    State(state): State<Arc<AppState>>,
    Query(query): Query<OrdersQuery>,
) -> Result<Json<Vec<db::OrderListRow>>, ApiError> {
    if query.account_id.is_empty() {
        return Err(ApiError::bad_request("account_id must not be empty"));
    }
    let rows = db::list_orders_for_account(state.database.pool(), &query.account_id)
        .await
        .map_err(ApiError::internal)?;
    Ok(Json(rows))
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0)
}

async fn post_tick(
    State(state): State<Arc<AppState>>,
    Json(body): Json<TickBody>,
) -> Result<Json<TickResponse>, ApiError> {
    run_tick_inner(state, body).await.map(Json)
}

async fn run_tick_inner(state: Arc<AppState>, body: TickBody) -> Result<TickResponse, ApiError> {
    let venue = domain::Venue::parse(&body.venue)
        .ok_or_else(|| ApiError::bad_request("unknown venue; use US_EQUITY, HK_EQUITY, CRYPTO, POLYMARKET"))?;

    let adapter = state
        .ingest_registry
        .adapter_for_venue(venue)
        .ok_or_else(|| ApiError::not_found("no ingest adapter registered for this venue"))?;

    let account_id = body
        .account_id
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "acc_mvp_paper".to_string());

    if body.symbol.is_empty() {
        return Err(ApiError::bad_request("symbol must not be empty"));
    }

    let strategy = strategy::AlwaysLongOne;
    let tick = pipeline::VenueTickParams {
        account_id,
        venue,
        symbol: body.symbol,
        ts_ms: now_ms(),
    };

    pipeline::run_one_tick_for_venue(
        &state.database,
        adapter.as_ref(),
        &state.execution_router,
        &strategy,
        &tick,
    )
    .await
    .map_err(ApiError::pipeline)?;

    let _ = state.events.send(StreamEvent::OrderCycleDone { venue });

    Ok(TickResponse {
        ok: true,
        venue: venue.as_str().to_string(),
        symbol: tick.symbol.clone(),
    })
}

async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    let mut rx = state.events.subscribe();
    ws.on_upgrade(move |mut socket: WebSocket| async move {
        let hello = serde_json::json!({
            "kind": "hello",
            "schema_version": 1u32,
        });
        if let Ok(text) = serde_json::to_string(&hello) {
            let _ = socket.send(Message::Text(text)).await;
        }

        loop {
            tokio::select! {
                message = rx.recv() => {
                    match message {
                        Ok(event) => {
                            let envelope = stream_envelope(&event);
                            if let Ok(text) = serde_json::to_string(&envelope) {
                                if socket.send(Message::Text(text)).await.is_err() {
                                    break;
                                }
                            }
                        }
                        Err(broadcast::error::RecvError::Lagged(_skipped)) => {
                            continue;
                        }
                        Err(broadcast::error::RecvError::Closed) => break,
                    }
                }
                incoming = socket.recv() => {
                    match incoming {
                        Some(Ok(Message::Close(_))) | None => break,
                        Some(Ok(_other)) => {}
                        Some(Err(_)) => break,
                    }
                }
            }
        }
    })
}

fn stream_envelope(event: &StreamEvent) -> serde_json::Value {
    let event_id = uuid::Uuid::new_v4().to_string();
    match event {
        StreamEvent::OrderCycleDone { venue } => serde_json::json!({
            "event_id": event_id,
            "kind": "order_cycle_done",
            "payload": { "venue": venue.as_str() },
        }),
    }
}

#[derive(Debug)]
pub struct ApiError {
    status: StatusCode,
    code: &'static str,
    message: String,
}

impl ApiError {
    fn internal(err: db::DbError) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            code: "internal",
            message: err.to_string(),
        }
    }

    fn bad_request(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            code: "bad_request",
            message: message.into(),
        }
    }

    fn not_found(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            code: "not_found",
            message: message.into(),
        }
    }

    fn pipeline(err: pipeline::PipelineError) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            code: "pipeline_error",
            message: err.to_string(),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        let body = Json(serde_json::json!({
            "error_code": self.code,
            "message": self.message,
        }));
        (self.status, body).into_response()
    }
}

pub fn router(state: AppState) -> Router {
    let shared = Arc::new(state);
    Router::new()
        .route("/health", get(health))
        .route("/v1/instruments", get(list_instruments))
        .route("/v1/orders", get(list_orders))
        .route("/v1/tick", post(post_tick))
        .route("/v1/stream", get(ws_handler))
        .with_state(shared)
}
