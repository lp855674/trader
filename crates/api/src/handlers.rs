use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use axum::extract::Query;
use axum::extract::State;
use axum::{Json, response::IntoResponse};
use serde::{Deserialize, Serialize};

use crate::error::ApiError;
use crate::{AppState, StreamEvent};

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

pub async fn health() -> impl IntoResponse {
    Json(HealthBody { status: "ok" })
}

pub async fn list_instruments(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<db::InstrumentRow>>, ApiError> {
    let rows = db::list_instruments(state.database.pool())
        .await
        .map_err(ApiError::internal)?;
    Ok(Json(rows))
}

pub async fn list_orders(
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

pub async fn post_tick(
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

    let tick = pipeline::VenueTickParams {
        account_id,
        venue,
        symbol: body.symbol,
        ts_ms: now_ms(),
    };

    let ack = pipeline::run_one_tick_for_venue(
        &state.database,
        adapter.as_ref(),
        &state.execution_router,
        state.strategy.as_ref(),
        state.risk_limits,
        &tick,
    )
    .await
    .map_err(ApiError::pipeline)?;

    if let Some(ack) = ack {
        let _ = state.events.send(StreamEvent::OrderCreated {
            order_id: ack.order_id,
            venue,
            symbol: tick.symbol.clone(),
        });
    }

    Ok(TickResponse {
        ok: true,
        venue: venue.as_str().to_string(),
        symbol: tick.symbol.clone(),
    })
}
