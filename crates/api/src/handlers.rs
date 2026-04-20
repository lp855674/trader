use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use axum::extract::Path;
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

#[derive(Deserialize)]
pub struct CreateOrderBody {
    pub account_id: String,
    pub symbol: String,
    pub side: String,
    pub qty: f64,
    pub order_type: String,
    pub limit_price: Option<f64>,
}

#[derive(Deserialize)]
pub struct CancelOrderBody {
    pub account_id: String,
}

#[derive(Deserialize)]
pub struct AmendOrderBody {
    pub account_id: String,
    pub qty: f64,
    pub limit_price: Option<f64>,
}

#[derive(Serialize)]
pub struct OrderActionResponse {
    pub order_id: String,
    pub status: String,
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

#[derive(Deserialize)]
pub struct RuntimeReconciliationQuery {
    pub account_id: String,
}

#[derive(Serialize)]
pub struct RuntimeReconciliationSnapshotBody {
    pub id: String,
    pub status: String,
    pub mismatch_count: i64,
    pub broker_cash: f64,
    pub local_cash: f64,
    pub broker_positions: serde_json::Value,
    pub local_positions: serde_json::Value,
    pub created_at: i64,
}

#[derive(Serialize)]
pub struct RuntimeReconciliationLatestBody {
    pub account_id: String,
    pub runtime_mode: String,
    pub local_positions: Vec<db::LocalPositionViewRow>,
    pub local_open_orders: Vec<db::OpenOrderViewRow>,
    pub latest_snapshot: Option<RuntimeReconciliationSnapshotBody>,
}

#[derive(Serialize)]
pub struct TerminalWatchRow {
    pub symbol: String,
    pub venue: String,
    pub last_price: Option<f64>,
}

#[derive(Serialize)]
pub struct TerminalOverviewBody {
    pub account_id: String,
    pub runtime_mode: String,
    pub watchlist: Vec<TerminalWatchRow>,
    pub positions: Vec<db::LocalPositionViewRow>,
    pub open_orders: Vec<db::OpenOrderViewRow>,
}

#[derive(Serialize)]
pub struct QuoteBody {
    pub symbol: String,
    pub venue: String,
    pub last_price: Option<f64>,
    pub day_high: Option<f64>,
    pub day_low: Option<f64>,
    pub bars: Vec<db::BarRow>,
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

pub async fn post_order(
    State(state): State<Arc<AppState>>,
    Json(body): Json<CreateOrderBody>,
) -> Result<(StatusCode, Json<OrderActionResponse>), ApiError> {
    let account_id = require_non_empty(body.account_id, "account_id")?;
    let symbol = require_non_empty(body.symbol, "symbol")?;
    let side = normalize_side(&body.side)?;
    let order_type = body.order_type.trim().to_ascii_lowercase();
    if order_type != "limit" {
        return Err(ApiError::bad_request("order_type must be limit"));
    }
    let limit_price = body
        .limit_price
        .ok_or_else(|| ApiError::bad_request("limit_price must be provided"))?;
    if limit_price <= 0.0 {
        return Err(ApiError::bad_request("limit_price must be positive"));
    }

    let (instrument, venue) = ensure_instrument_for_symbol(&state.database, &symbol).await?;
    let ack = state
        .execution_router
        .submit_manual_order(
            &account_id,
            &domain::OrderIntent {
                strategy_id: "manual_terminal".to_string(),
                instrument: domain::InstrumentId::new(venue, &symbol),
                instrument_db_id: instrument.id,
                side,
                qty: body.qty,
                limit_price,
            },
            None,
        )
        .await
        .map_err(ApiError::exec)?;
    let created_at_ms = now_ms();

    let _ = state.events.send(StreamEvent::OrderCreated {
        order_id: ack.order_id.clone(),
        venue,
        symbol,
        side: Some(body.side.trim().to_ascii_lowercase()),
        qty: Some(body.qty),
        status: Some(ack.status.clone()),
        order_type: Some(order_type),
        limit_price: Some(limit_price),
        exchange_ref: ack.exchange_ref.clone(),
        created_at_ms: Some(created_at_ms),
        updated_at_ms: Some(created_at_ms),
    });

    Ok((
        StatusCode::CREATED,
        Json(OrderActionResponse {
            order_id: ack.order_id,
            status: ack.status,
        }),
    ))
}

pub async fn post_cancel_order(
    Path(order_id): Path<String>,
    State(state): State<Arc<AppState>>,
    Json(body): Json<CancelOrderBody>,
) -> Result<Json<OrderActionResponse>, ApiError> {
    let account_id = require_non_empty(body.account_id, "account_id")?;
    state
        .execution_router
        .cancel_order(&account_id, &order_id)
        .await
        .map_err(ApiError::exec)?;

    let _ = state.events.send(StreamEvent::OrderCancelled {
        order_id: order_id.clone(),
    });

    Ok(Json(OrderActionResponse {
        order_id,
        status: "CANCELLED".to_string(),
    }))
}

pub async fn post_amend_order(
    Path(order_id): Path<String>,
    State(state): State<Arc<AppState>>,
    Json(body): Json<AmendOrderBody>,
) -> Result<Json<OrderActionResponse>, ApiError> {
    let account_id = require_non_empty(body.account_id, "account_id")?;
    let ack = state
        .execution_router
        .amend_order(&account_id, &order_id, body.qty, body.limit_price)
        .await
        .map_err(ApiError::exec)?;

    let _ = state.events.send(StreamEvent::OrderReplaced {
        order_id: order_id.clone(),
        qty: body.qty,
        limit_price: body.limit_price,
    });

    Ok(Json(OrderActionResponse {
        order_id: ack.order_id,
        status: ack.status,
    }))
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

fn normalize_side(value: &str) -> Result<domain::Side, ApiError> {
    match value.trim().to_ascii_lowercase().as_str() {
        "buy" => Ok(domain::Side::Buy),
        "sell" => Ok(domain::Side::Sell),
        _ => Err(ApiError::bad_request("side must be buy or sell")),
    }
}

fn require_non_empty(value: String, field_name: &str) -> Result<String, ApiError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(ApiError::bad_request(format!("{field_name} must not be empty")));
    }
    Ok(trimmed.to_string())
}

async fn resolve_instrument_by_symbol(
    database: &db::Db,
    symbol: &str,
) -> Result<db::InstrumentRow, ApiError> {
    db::list_instruments(database.pool())
        .await
        .map_err(ApiError::internal)?
        .into_iter()
        .find(|row| row.symbol == symbol)
        .ok_or_else(|| ApiError::not_found("instrument not found"))
}

async fn ensure_instrument_for_symbol(
    database: &db::Db,
    symbol: &str,
) -> Result<(db::InstrumentRow, domain::Venue), ApiError> {
    match resolve_instrument_by_symbol(database, symbol).await {
        Ok(instrument) => {
            let venue = domain::Venue::parse(&instrument.venue)
                .ok_or_else(|| ApiError::bad_request("instrument venue is invalid"))?;
            Ok((instrument, venue))
        }
        Err(error) if error.status == StatusCode::NOT_FOUND => {
            let venue = infer_venue_from_symbol(symbol)?;
            let instrument_id =
                db::upsert_instrument(database.pool(), venue.as_str(), symbol)
                    .await
                    .map_err(ApiError::internal)?;
            Ok((
                db::InstrumentRow {
                    id: instrument_id,
                    venue: venue.as_str().to_string(),
                    symbol: symbol.to_string(),
                },
                venue,
            ))
        }
        Err(error) => Err(error),
    }
}

fn infer_venue_from_symbol(symbol: &str) -> Result<domain::Venue, ApiError> {
    if symbol.ends_with(".US") {
        Ok(domain::Venue::UsEquity)
    } else if symbol.ends_with(".HK") {
        Ok(domain::Venue::HkEquity)
    } else {
        Err(ApiError::bad_request(
            "instrument not found and venue cannot be inferred from symbol suffix",
        ))
    }
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

pub async fn get_runtime_reconciliation_latest(
    State(state): State<Arc<AppState>>,
    Query(query): Query<RuntimeReconciliationQuery>,
) -> Result<Json<RuntimeReconciliationLatestBody>, ApiError> {
    if query.account_id.is_empty() {
        return Err(ApiError::bad_request("account_id must not be empty"));
    }

    let runtime_mode = db::get_runtime_control(state.database.pool(), RUNTIME_MODE_KEY)
        .await
        .map_err(ApiError::internal)?
        .unwrap_or_else(|| "observe_only".to_string());
    let local_positions =
        db::list_local_positions_for_account(state.database.pool(), &query.account_id)
            .await
            .map_err(ApiError::internal)?;
    let local_open_orders =
        db::list_open_orders_for_account(state.database.pool(), &query.account_id)
            .await
            .map_err(ApiError::internal)?;
    let latest_snapshot =
        db::load_latest_reconciliation_snapshot(state.database.pool(), &query.account_id)
            .await
            .map_err(ApiError::internal)?
            .map(|snapshot| RuntimeReconciliationSnapshotBody {
                id: snapshot.id,
                status: snapshot.status,
                mismatch_count: snapshot.mismatch_count,
                broker_cash: snapshot.broker_cash,
                local_cash: snapshot.local_cash,
                broker_positions: serde_json::from_str(&snapshot.broker_positions_json)
                    .unwrap_or_else(|_| serde_json::json!([])),
                local_positions: serde_json::from_str(&snapshot.local_positions_json)
                    .unwrap_or_else(|_| serde_json::json!([])),
                created_at: snapshot.created_at,
            });

    Ok(Json(RuntimeReconciliationLatestBody {
        account_id: query.account_id,
        runtime_mode,
        local_positions,
        local_open_orders,
        latest_snapshot,
    }))
}

pub async fn get_terminal_overview(
    State(state): State<Arc<AppState>>,
    Query(query): Query<RuntimeExecutionStateQuery>,
) -> Result<Json<TerminalOverviewBody>, ApiError> {
    if query.account_id.is_empty() {
        return Err(ApiError::bad_request("account_id must not be empty"));
    }

    let runtime_mode = db::get_runtime_control(state.database.pool(), RUNTIME_MODE_KEY)
        .await
        .map_err(ApiError::internal)?
        .unwrap_or_else(|| "observe_only".to_string());
    let watchlist = db::list_symbol_allowlist(state.database.pool())
        .await
        .map_err(ApiError::internal)?
        .into_iter()
        .filter(|(_, enabled)| *enabled)
        .map(|(symbol, _)| TerminalWatchRow {
            last_price: None,
            venue: resolve_watch_venue(&symbol),
            symbol,
        })
        .collect();
    let positions = db::list_local_positions_for_account(state.database.pool(), &query.account_id)
        .await
        .map_err(ApiError::internal)?;
    let open_orders = db::list_open_orders_for_account(state.database.pool(), &query.account_id)
        .await
        .map_err(ApiError::internal)?;

    Ok(Json(TerminalOverviewBody {
        account_id: query.account_id,
        runtime_mode,
        watchlist,
        positions,
        open_orders,
    }))
}

pub async fn get_quote(
    Path(symbol): Path<String>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<QuoteBody>, ApiError> {
    let symbol = require_non_empty(symbol, "symbol")?;
    let quote = load_quote_body(&state.database, &symbol).await?;
    Ok(Json(quote))
}

fn resolve_watch_venue(symbol: &str) -> String {
    if symbol.ends_with(".HK") {
        "HK_EQUITY".to_string()
    } else if symbol.ends_with(".US") {
        "US_EQUITY".to_string()
    } else {
        "UNKNOWN".to_string()
    }
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
            side: None,
            qty: None,
            status: None,
            order_type: None,
            limit_price: None,
            exchange_ref: Some(ack.exchange_ref),
            created_at_ms: None,
            updated_at_ms: None,
        });
    }

    if let Ok(quote) = load_quote_body(&state.database, &tick.symbol).await {
        let _ = state.events.send(StreamEvent::QuoteUpdated {
            symbol: quote.symbol,
            venue: quote.venue,
            last_price: quote.last_price,
            day_high: quote.day_high,
            day_low: quote.day_low,
            bars: quote.bars,
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

async fn load_quote_body(database: &db::Db, symbol: &str) -> Result<QuoteBody, ApiError> {
    let instrument = match resolve_instrument_by_symbol(database, symbol).await {
        Ok(instrument) => instrument,
        Err(error) if error.status == StatusCode::NOT_FOUND => {
            return Ok(QuoteBody {
                symbol: symbol.to_string(),
                venue: resolve_watch_venue(symbol),
                last_price: None,
                day_high: None,
                day_low: None,
                bars: Vec::new(),
            });
        }
        Err(error) => return Err(error),
    };
    let bars = db::get_recent_bars(
        database.pool(),
        instrument.id,
        db::PAPER_BARS_DATA_SOURCE_ID,
        20,
    )
    .await
    .map_err(ApiError::internal)?;
    let last_price = bars.last().map(|bar| bar.close);
    let day_high = bars
        .iter()
        .map(|bar| bar.high)
        .max_by(|left, right| left.total_cmp(right));
    let day_low = bars
        .iter()
        .map(|bar| bar.low)
        .min_by(|left, right| left.total_cmp(right));

    Ok(QuoteBody {
        symbol: symbol.to_string(),
        venue: instrument.venue,
        last_price,
        day_high,
        day_low,
        bars,
    })
}
