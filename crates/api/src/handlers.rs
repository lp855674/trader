use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use axum::extract::Query;
use axum::extract::State;
use axum::http::StatusCode;
use axum::{Json, response::IntoResponse};
use serde::{Deserialize, Serialize};

use crate::error::ApiError;
use crate::{AppState, StreamEvent};

#[derive(Serialize)]
struct HealthBody {
    status: &'static str,
}

const RUNTIME_MODE_KEY: &str = "mode";

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

#[derive(Serialize)]
pub struct RuntimeModeBody {
    pub mode: String,
}

#[derive(Deserialize)]
pub struct RuntimeModeUpdate {
    pub mode: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct SymbolAllowlistEntry {
    pub symbol: String,
    #[serde(default = "default_allowlist_enabled")]
    pub enabled: bool,
}

#[derive(Serialize)]
pub struct SymbolAllowlistBody {
    pub symbols: Vec<SymbolAllowlistEntry>,
}

#[derive(Deserialize)]
pub struct SymbolAllowlistUpdate {
    pub symbols: Vec<SymbolAllowlistEntry>,
}

#[derive(Deserialize)]
pub struct UniverseCycleTrigger {
    pub venue: String,
    #[serde(default)]
    pub account_id: Option<String>,
}

#[derive(Deserialize)]
pub struct RuntimeCycleHistoryQuery {
    pub limit: Option<i64>,
}

#[derive(Deserialize)]
pub struct RuntimeExecutionStateQuery {
    pub account_id: String,
}

#[derive(Serialize)]
pub struct RuntimeExecutionCycleSummary {
    pub mode: String,
    pub venue: String,
    pub triggered_at_ms: i64,
    pub accepted: Vec<String>,
    pub placed: Vec<pipeline::PlacedOrder>,
    pub skipped: Vec<pipeline::SymbolDecision>,
}

#[derive(Serialize)]
pub struct RuntimeExecutionStateBody {
    pub account_id: String,
    pub positions: Vec<db::LocalPositionViewRow>,
    pub open_orders: Vec<db::OpenOrderViewRow>,
    pub latest_cycle: Option<RuntimeExecutionCycleSummary>,
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

fn default_allowlist_enabled() -> bool {
    true
}

fn normalize_runtime_mode(mode: &str) -> Result<&'static str, ApiError> {
    match mode.trim() {
        "enabled" => Ok("enabled"),
        "observe_only" => Ok("observe_only"),
        "paper_only" => Ok("paper_only"),
        "degraded" => Ok("degraded"),
        _ => Err(ApiError::bad_request(
            "mode must be one of enabled, observe_only, paper_only, degraded",
        )),
    }
}

fn normalize_allowlist_entry(entry: SymbolAllowlistEntry) -> Result<(String, bool), ApiError> {
    let symbol = entry.symbol.trim();
    if symbol.is_empty() {
        return Err(ApiError::bad_request("symbol must not be empty"));
    }
    Ok((symbol.to_string(), entry.enabled))
}

pub async fn get_runtime_mode(
    State(state): State<Arc<AppState>>,
) -> Result<Json<RuntimeModeBody>, ApiError> {
    let mode = db::get_runtime_control(state.database.pool(), RUNTIME_MODE_KEY)
        .await
        .map_err(ApiError::internal)?
        .unwrap_or_else(|| "observe_only".to_string());
    Ok(Json(RuntimeModeBody { mode }))
}

pub async fn put_runtime_mode(
    State(state): State<Arc<AppState>>,
    Json(body): Json<RuntimeModeUpdate>,
) -> Result<StatusCode, ApiError> {
    let mode = normalize_runtime_mode(&body.mode)?;
    db::set_runtime_control(state.database.pool(), RUNTIME_MODE_KEY, mode)
        .await
        .map_err(ApiError::internal)?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn get_symbol_allowlist(
    State(state): State<Arc<AppState>>,
) -> Result<Json<SymbolAllowlistBody>, ApiError> {
    let rows = db::list_symbol_allowlist(state.database.pool())
        .await
        .map_err(ApiError::internal)?;
    let symbols = rows
        .into_iter()
        .map(|(symbol, enabled)| SymbolAllowlistEntry { symbol, enabled })
        .collect();
    Ok(Json(SymbolAllowlistBody { symbols }))
}

pub async fn put_symbol_allowlist(
    State(state): State<Arc<AppState>>,
    Json(body): Json<SymbolAllowlistUpdate>,
) -> Result<StatusCode, ApiError> {
    let mut entries = Vec::with_capacity(body.symbols.len());
    for entry in body.symbols {
        entries.push(normalize_allowlist_entry(entry)?);
    }
    db::replace_symbol_allowlist(state.database.pool(), &entries)
        .await
        .map_err(ApiError::internal)?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn post_runtime_cycle(
    State(state): State<Arc<AppState>>,
    Json(body): Json<UniverseCycleTrigger>,
) -> Result<Json<pipeline::UniverseCycleReport>, ApiError> {
    let venue = domain::Venue::parse(&body.venue).ok_or_else(|| {
        ApiError::bad_request("unknown venue; use US_EQUITY, HK_EQUITY, CRYPTO, POLYMARKET")
    })?;
    let adapter = state
        .ingest_registry
        .adapter_for_venue(venue)
        .ok_or_else(|| ApiError::not_found("no ingest adapter registered for this venue"))?;
    let account_id = body
        .account_id
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "acc_mvp_paper".to_string());
    let params = pipeline::UniverseRunParams {
        account_id,
        venue,
        ts_ms: now_ms(),
    };
    let report = pipeline::run_universe_cycle(
        &state.database,
        adapter.as_ref(),
        &state.execution_router,
        state.strategy.as_ref(),
        state.risk_limits,
        &params,
    )
    .await
    .map_err(|error| match error {
        pipeline::PipelineError::UnsupportedStrategy | pipeline::PipelineError::EmptyAllowlist => {
            ApiError::bad_request(error.to_string())
        }
        pipeline::PipelineError::Db(inner) => ApiError::internal(inner),
        other => ApiError::internal_message(other.to_string()),
    })?;
    Ok(Json(report))
}

pub async fn get_latest_runtime_cycle(
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let report = pipeline::load_last_universe_cycle(&state.database)
        .await
        .map_err(|error| ApiError::internal_message(error.to_string()))?;
    let body = match report {
        Some(report) => serde_json::to_value(report)
            .map_err(|error| ApiError::internal_message(error.to_string()))?,
        None => serde_json::json!({ "report": null }),
    };
    Ok(Json(body))
}

pub async fn get_runtime_cycle_history(
    State(state): State<Arc<AppState>>,
    Query(query): Query<RuntimeCycleHistoryQuery>,
) -> Result<Json<Vec<pipeline::UniverseCycleReport>>, ApiError> {
    let limit = query.limit.unwrap_or(10);
    if !(1..=100).contains(&limit) {
        return Err(ApiError::bad_request("limit must be between 1 and 100"));
    }
    let reports = pipeline::load_universe_cycle_history(&state.database, limit)
        .await
        .map_err(|error| ApiError::internal_message(error.to_string()))?;
    Ok(Json(reports))
}

pub async fn get_runtime_execution_state(
    State(state): State<Arc<AppState>>,
    Query(query): Query<RuntimeExecutionStateQuery>,
) -> Result<Json<RuntimeExecutionStateBody>, ApiError> {
    if query.account_id.is_empty() {
        return Err(ApiError::bad_request("account_id must not be empty"));
    }

    let positions = db::list_local_positions_for_account(state.database.pool(), &query.account_id)
        .await
        .map_err(ApiError::internal)?;
    let open_orders = db::list_open_orders_for_account(state.database.pool(), &query.account_id)
        .await
        .map_err(ApiError::internal)?;
    let latest_cycle = pipeline::load_last_universe_cycle(&state.database)
        .await
        .map_err(|error| ApiError::internal_message(error.to_string()))?
        .filter(|report| report.account_id == query.account_id)
        .map(|report| RuntimeExecutionCycleSummary {
            mode: report.mode,
            venue: report.venue,
            triggered_at_ms: report.triggered_at_ms,
            accepted: report.accepted,
            placed: report.placed,
            skipped: report.skipped,
        });

    Ok(Json(RuntimeExecutionStateBody {
        account_id: query.account_id,
        positions,
        open_orders,
        latest_cycle,
    }))
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
    let venue = domain::Venue::parse(&body.venue).ok_or_else(|| {
        ApiError::bad_request("unknown venue; use US_EQUITY, HK_EQUITY, CRYPTO, POLYMARKET")
    })?;

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
        None,
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

#[derive(Serialize)]
pub struct StrategyConfigBody {
    pub account_id: String,
    pub config: serde_json::Value,
}

#[derive(Deserialize)]
pub struct StrategyConfigUpdate {
    pub account_id: String,
    pub config: serde_json::Value,
}

pub async fn get_strategy_config(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<StrategyConfigBody>>, ApiError> {
    let rows = db::list_system_config_by_prefix(state.database.pool(), "strategy.")
        .await
        .map_err(ApiError::internal)?;

    let configs: Vec<StrategyConfigBody> = rows
        .into_iter()
        .filter_map(|(key, value)| {
            let account_id = key.strip_prefix("strategy.")?.to_string();
            let config: serde_json::Value =
                serde_json::from_str(value.as_deref().unwrap_or("{}")).ok()?;
            Some(StrategyConfigBody { account_id, config })
        })
        .collect();

    Ok(Json(configs))
}

pub async fn put_strategy_config(
    State(state): State<Arc<AppState>>,
    Json(body): Json<StrategyConfigUpdate>,
) -> Result<StatusCode, ApiError> {
    let key = format!("strategy.{}", body.account_id);
    let value = serde_json::to_string(&body.config)
        .map_err(|e| ApiError::bad_request(format!("invalid config JSON: {e}")))?;
    db::set_system_config(state.database.pool(), &key, &value)
        .await
        .map_err(ApiError::internal)?;
    Ok(StatusCode::NO_CONTENT)
}
