#![forbid(unsafe_code)]

mod state;
mod ws;

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post, put},
};
use backtest::{BacktestRuntime, BacktestSettings};
use broker::{
    BinanceSpotTestnetAdapter, BinanceSpotTestnetSettings, Broker, BrokerAccountSnapshot,
    BrokerKind, BrokerStatus, FakeBrokerAdapter, IbkrPaperGatewayAdapter, IbkrPaperGatewaySettings,
};
use metrics::{MetricsSummary, equity_returns, paper_summary};
use paper::{
    BinancePaperOrderExecutor, IbkrPaperGatewayOrderClient, IbkrPaperOrderExecutor, PaperRuntime,
    PaperSettings,
};
use replay::{ReplayController, ReplayRuntime, ReplayState, ReplaySummary};
use runtime::{AlertSinkSettings, LiveRuntime, LiveRuntimeSettings};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::str::FromStr;
use std::sync::Arc;
use tokio::sync::Mutex;

pub use state::AppState;

#[derive(Serialize)]
struct HealthResponse {
    status: &'static str,
}

#[derive(Serialize)]
struct RunStatusResponse {
    run_id: String,
    status: String,
    error: Option<String>,
}

#[derive(Serialize)]
struct RunStartResponse {
    run_id: String,
    status: String,
}

#[derive(Serialize)]
struct RunResponse {
    id: String,
    name: String,
    mode: String,
    status: String,
    started_at_ms: i64,
    ended_at_ms: Option<i64>,
    error: Option<String>,
    config: serde_json::Value,
}

#[derive(Serialize)]
struct PaperPreflightResponse {
    status: &'static str,
    run_id: String,
    strategy: String,
    symbol: String,
    bars: usize,
    database: String,
    broker: &'static str,
    broker_mode: &'static str,
    account: String,
    max_order_notional: String,
    max_exposure: String,
    trading_halted: bool,
    real_broker_connection: bool,
    order_submit_enabled: bool,
}

#[derive(Serialize)]
struct OrderResponse {
    id: String,
    run_id: String,
    client_order_id: String,
    broker_order_id: Option<String>,
    account_id: String,
    symbol: String,
    side: String,
    order_type: String,
    price: Option<String>,
    qty: String,
    filled_qty: String,
    status: String,
    created_at_ms: i64,
    updated_at_ms: i64,
}

#[derive(Serialize)]
struct FillResponse {
    id: String,
    order_id: String,
    run_id: String,
    symbol: String,
    side: String,
    price: String,
    qty: String,
    fee: String,
    ts_ms: i64,
}

#[derive(Serialize)]
struct PositionResponse {
    run_id: String,
    account_id: String,
    symbol: String,
    qty: String,
    avg_price: String,
    updated_at_ms: i64,
}

#[derive(Serialize)]
struct CryptoPositionResponse {
    run_id: String,
    account_id: String,
    exchange: String,
    symbol: String,
    asset_class: String,
    margin_mode: String,
    position_side: String,
    leverage: String,
    qty: String,
    avg_price: String,
    margin_used: String,
    funding_fee: String,
    realized_pnl: String,
    unrealized_pnl: String,
    updated_at_ms: i64,
}

#[derive(Serialize)]
struct FundingRateResponse {
    id: String,
    exchange: String,
    symbol: String,
    funding_time_ms: i64,
    funding_rate: String,
    mark_price: Option<String>,
    source: String,
}

#[derive(Deserialize)]
struct FundingRatesQuery {
    exchange: String,
    symbol: Option<String>,
    start_ms: Option<i64>,
    end_ms: Option<i64>,
}

#[derive(Deserialize)]
struct CashSnapshotsQuery {
    currency: Option<String>,
    from_ms: Option<i64>,
    to_ms: Option<i64>,
}

#[derive(Deserialize)]
struct PositionSnapshotsQuery {
    symbol: Option<String>,
    position_side: Option<String>,
    from_ms: Option<i64>,
    to_ms: Option<i64>,
}

#[derive(Deserialize)]
struct SystemLogsQuery {
    run_id: Option<String>,
    level: Option<String>,
    target: Option<String>,
    from_ms: Option<i64>,
    to_ms: Option<i64>,
    limit: Option<i64>,
}

#[derive(Deserialize)]
struct ReconciliationDriftsQuery {
    run_id: Option<String>,
    account_id: Option<String>,
    symbol: Option<String>,
    from_ms: Option<i64>,
    to_ms: Option<i64>,
    limit: Option<i64>,
}

#[derive(Deserialize)]
struct ReconciliationAlertsSummaryQuery {
    run_id: Option<String>,
    account_id: Option<String>,
    symbol: Option<String>,
    from_ms: Option<i64>,
    to_ms: Option<i64>,
    limit: Option<i64>,
}

#[derive(Serialize)]
struct CryptoMarketMetaResponse {
    id: i64,
    exchange: String,
    symbol: String,
    base_asset: String,
    quote_asset: String,
    instrument_type: String,
    contract_type: Option<String>,
    contract_size: Option<String>,
    settlement_asset: Option<String>,
    min_notional: Option<String>,
    min_qty: Option<String>,
    max_qty: Option<String>,
    price_precision: Option<i64>,
    qty_precision: Option<i64>,
    price_tick: Option<String>,
    qty_step: Option<String>,
    maker_fee_rate: Option<String>,
    taker_fee_rate: Option<String>,
    funding_interval_hours: Option<i64>,
    max_leverage: Option<String>,
    margin_modes: serde_json::Value,
    is_inverse: bool,
    is_active: bool,
    created_at_ms: i64,
    updated_at_ms: i64,
}

#[derive(Deserialize)]
struct CryptoMarketMetaQuery {
    exchange: String,
    symbol: String,
}

#[derive(Serialize)]
struct CorporateActionResponse {
    id: i64,
    market: String,
    exchange: String,
    symbol: String,
    action_type: String,
    ex_date_ms: i64,
    record_date_ms: Option<i64>,
    payable_date_ms: Option<i64>,
    ratio: Option<String>,
    cash_amount: Option<String>,
    currency: Option<String>,
    source: Option<String>,
    created_at_ms: i64,
    updated_at_ms: i64,
}

#[derive(Deserialize)]
struct CorporateActionsQuery {
    market: String,
    symbol: String,
    start_ms: i64,
    end_ms: i64,
}

#[derive(Serialize)]
struct AccountBalanceResponse {
    run_id: String,
    account_id: String,
    asset: String,
    total: String,
    available: String,
    frozen: String,
    updated_at_ms: i64,
}

#[derive(Serialize)]
struct CashSnapshotResponse {
    id: i64,
    run_id: String,
    ts_ms: i64,
    currency: String,
    cash: String,
    available_cash: String,
    frozen_cash: String,
    created_at_ms: i64,
}

#[derive(Serialize)]
struct PositionSnapshotResponse {
    id: i64,
    run_id: String,
    ts_ms: i64,
    market: String,
    exchange: String,
    symbol: String,
    asset_class: String,
    position_side: Option<String>,
    qty: String,
    available_qty: String,
    avg_price: Option<String>,
    entry_price: Option<String>,
    market_price: Option<String>,
    mark_price: Option<String>,
    market_value: Option<String>,
    unrealized_pnl: Option<String>,
    realized_pnl: Option<String>,
    currency: String,
    created_at_ms: i64,
}

#[derive(Serialize)]
struct PortfolioSnapshotResponse {
    id: String,
    run_id: String,
    account_id: String,
    ts_ms: i64,
    cash: String,
    market_value: String,
    equity: String,
    realized_pnl: String,
    unrealized_pnl: String,
}

#[derive(Serialize)]
struct EventResponse {
    event_id: String,
    ts_ms: i64,
    source: String,
    category: String,
    payload: serde_json::Value,
}

#[derive(Serialize)]
struct OrderEventResponse {
    id: String,
    event_id: String,
    run_id: String,
    order_id: Option<String>,
    client_order_id: Option<String>,
    broker_order_id: Option<String>,
    account_id: Option<String>,
    symbol: Option<String>,
    status: String,
    event_type: String,
    message: Option<String>,
    ts_ms: i64,
    payload: serde_json::Value,
}

#[derive(Serialize)]
struct RiskEventResponse {
    id: String,
    event_id: String,
    run_id: String,
    account_id: Option<String>,
    symbol: Option<String>,
    risk_type: String,
    decision: String,
    reason: Option<String>,
    threshold: Option<String>,
    observed_value: Option<String>,
    ts_ms: i64,
    payload: serde_json::Value,
}

#[derive(Serialize)]
struct ReconciliationStatusResponse {
    run_id: String,
    status: String,
    cash_snapshots: usize,
    position_snapshots: usize,
    drift_events: Vec<RiskEventResponse>,
    latest_cash_ts_ms: Option<i64>,
    latest_position_ts_ms: Option<i64>,
}

#[derive(Serialize)]
struct ReconciliationAlertSummaryResponse {
    run_id: Option<String>,
    alert_count: usize,
    latest_alert_ts_ms: Option<i64>,
    runs: Vec<String>,
    accounts: Vec<String>,
    symbols: Vec<String>,
    reasons: Vec<String>,
}

#[derive(Serialize)]
struct ReconciliationAlertDeliverySummaryResponse {
    run_id: Option<String>,
    delivery_count: usize,
    latest_delivery_ts_ms: Option<i64>,
    sent_count: usize,
    failed_count: usize,
    statuses: Vec<String>,
    sinks: Vec<String>,
}

#[derive(Serialize)]
struct InsightResponse {
    id: String,
    event_id: String,
    run_id: String,
    strategy: String,
    symbol: String,
    side: String,
    confidence: String,
    ts_ms: i64,
    payload: serde_json::Value,
}

#[derive(Serialize)]
struct PortfolioTargetResponse {
    id: String,
    event_id: String,
    run_id: String,
    account_id: String,
    symbol: String,
    target_qty: String,
    ts_ms: i64,
    payload: serde_json::Value,
}

#[derive(Serialize)]
struct ConfigResponse {
    id: String,
    name: String,
    config_type: String,
    content: String,
    format: String,
    checksum: Option<String>,
    created_at_ms: i64,
    updated_at_ms: i64,
}

#[derive(Serialize)]
struct ConfigReleaseResponse {
    id: String,
    config_id: String,
    version: String,
    status: String,
    released_by: Option<String>,
    notes: Option<String>,
    created_at_ms: i64,
    updated_at_ms: i64,
}

#[derive(Serialize)]
struct RunConfigVersionBindingResponse {
    run_id: String,
    config_id: String,
    version: String,
    bound_at_ms: i64,
}

#[derive(Serialize)]
struct ConfigAuditResponse {
    id: String,
    config_id: String,
    version: Option<String>,
    action: String,
    actor: Option<String>,
    reason: Option<String>,
    ts_ms: i64,
}

#[derive(Deserialize)]
struct CreateConfigVersionRequest {
    name: String,
    content: serde_json::Value,
    created_by: String,
    parent_version: Option<u32>,
    target_env: Option<String>,
    rollout: Option<String>,
    ts_ms: Option<i64>,
}

#[derive(Deserialize)]
struct UpdateConfigStateRequest {
    new_state: storage::ConfigState,
    changed_by: String,
    actor_role: Option<String>,
    reason: Option<String>,
    ts_ms: Option<i64>,
}

#[derive(Deserialize)]
struct PendingConfigApprovalsQuery {
    target_env: Option<String>,
}

#[derive(Deserialize)]
struct RollbackConfigRequest {
    actor: String,
    reason: Option<String>,
    ts_ms: Option<i64>,
}

#[derive(Deserialize)]
struct ConfigDiffQuery {
    v1: u32,
    v2: u32,
}

#[derive(Serialize)]
struct ConfigVersionResponse {
    id: String,
    name: String,
    version: u32,
    content: serde_json::Value,
    state: storage::ConfigState,
    parent_version: Option<u32>,
    created_by: String,
    created_at_ms: i64,
    state_changed_at_ms: i64,
    state_changed_by: String,
    state_change_reason: Option<String>,
    target_env: Option<String>,
    rollout: Option<String>,
    approved_by: Option<String>,
    approved_at_ms: Option<i64>,
    published_by: Option<String>,
    published_at_ms: Option<i64>,
}

#[derive(Serialize)]
struct SystemLogResponse {
    id: String,
    run_id: Option<String>,
    ts_ms: i64,
    level: String,
    target: String,
    message: String,
    fields: serde_json::Value,
    created_at_ms: i64,
}

#[derive(Serialize)]
struct IngestionStatusResponse {
    sources: Vec<IngestionSourceStatusResponse>,
}

#[derive(Serialize)]
struct IngestionSourceStatusResponse {
    name: String,
    table: String,
    ts_ms: i64,
    rows_fetched: usize,
    rows_upserted: usize,
    duration_ms: i64,
}

pub fn router() -> Router {
    Router::new().route("/api/v1/health", get(health))
}

pub fn router_with_state(state: AppState) -> Router {
    Router::new()
        .route("/api/v1/health", get(health))
        .route("/api/v1/preflight/paper", get(paper_preflight))
        .route("/api/v1/backtests", post(run_backtest))
        .route("/api/v1/paper-runs", post(run_paper))
        .route("/api/v1/replays", post(run_replay))
        .route("/api/v1/live-runs", post(start_live_run))
        .route("/api/v1/live-runs/{run_id}/status", get(get_run_status))
        .route("/api/v1/live-runs/{run_id}/stop", post(stop_live_run))
        .route("/api/v1/orders", get(list_orders))
        .route("/api/v1/fills", get(list_fills))
        .route("/api/v1/positions", get(list_positions))
        .route("/api/v1/funding-rates", get(list_funding_rates))
        .route("/api/v1/crypto-market-meta", get(list_crypto_market_meta))
        .route("/api/v1/corporate-actions", get(list_corporate_actions))
        .route("/api/v1/ingestion/status", get(ingestion_status))
        .route("/api/v1/account-balances", get(list_account_balances))
        .route("/api/v1/portfolio/snapshots", get(list_portfolio_snapshots))
        .route("/api/v1/cash/snapshots", get(list_cash_snapshots))
        .route("/api/v1/positions/snapshots", get(list_position_snapshots))
        .route("/api/v1/metrics", get(metrics_summary))
        .route("/api/v1/brokers/status", get(broker_status))
        .route("/api/v1/brokers/account/{account_id}", get(broker_account))
        .route("/api/v1/runs", get(list_runs))
        .route("/api/v1/runs/{run_id}", get(get_run))
        .route(
            "/api/v1/configs",
            get(list_configs).post(create_config_version),
        )
        .route("/api/v1/configs/{name}", get(list_config_versions_by_name))
        .route("/api/v1/configs/{name}/latest", get(get_latest_config))
        .route(
            "/api/v1/configs/{name}/published",
            get(get_published_config),
        )
        .route("/api/v1/configs/{name}/diff", get(diff_config_versions))
        .route("/api/v1/configs/{name}/{version}", get(get_config_version))
        .route(
            "/api/v1/configs/{name}/{version}/state",
            put(update_config_state),
        )
        .route(
            "/api/v1/configs/{name}/{version}/rollback",
            post(rollback_config_version),
        )
        .route(
            "/api/v1/config-approvals/pending",
            get(list_pending_config_approvals),
        )
        .route("/api/v1/system-logs", get(list_system_logs))
        .route(
            "/api/v1/reconciliation-drifts",
            get(list_reconciliation_drifts),
        )
        .route(
            "/api/v1/reconciliation-alerts/summary",
            get(get_reconciliation_alerts_summary),
        )
        .route(
            "/api/v1/reconciliation-alert-deliveries/summary",
            get(get_reconciliation_alert_deliveries_summary),
        )
        .route(
            "/api/v1/configs/{config_id}/releases",
            get(list_config_releases),
        )
        .route(
            "/api/v1/configs/{config_id}/audits",
            get(list_config_audits),
        )
        .route(
            "/api/v1/runs/{run_id}/config-version",
            get(get_run_config_version),
        )
        .route("/api/v1/events", get(list_events))
        .route("/api/v1/runs/{run_id}/events", get(list_run_events))
        .route(
            "/api/v1/runs/{run_id}/order-events",
            get(list_run_order_events),
        )
        .route(
            "/api/v1/runs/{run_id}/risk-events",
            get(list_run_risk_events),
        )
        .route("/api/v1/runs/{run_id}/insights", get(list_run_insights))
        .route(
            "/api/v1/runs/{run_id}/cash-snapshots",
            get(list_run_cash_snapshots),
        )
        .route(
            "/api/v1/runs/{run_id}/position-snapshots",
            get(list_run_position_snapshots),
        )
        .route(
            "/api/v1/runs/{run_id}/reconciliation",
            get(get_run_reconciliation),
        )
        .route(
            "/api/v1/runs/{run_id}/reconciliation-drifts",
            get(list_run_reconciliation_drifts),
        )
        .route(
            "/api/v1/runs/{run_id}/reconciliation-alerts/summary",
            get(get_run_reconciliation_alerts_summary),
        )
        .route(
            "/api/v1/runs/{run_id}/reconciliation-alert-deliveries/summary",
            get(get_run_reconciliation_alert_deliveries_summary),
        )
        .route(
            "/api/v1/runs/{run_id}/portfolio-targets",
            get(list_run_portfolio_targets),
        )
        .route(
            "/api/v1/runs/{run_id}/system-logs",
            get(list_run_system_logs),
        )
        .route(
            "/api/v1/runs/{run_id}/crypto-positions",
            get(list_run_crypto_positions),
        )
        .route("/api/v1/runs/{run_id}/status", get(get_run_status))
        .route("/api/v1/runs/{run_id}/cancel", post(cancel_run))
        .route("/api/v1/replay/{run_id}/pause", post(pause_replay))
        .route("/api/v1/replay/{run_id}/resume", post(resume_replay))
        .route("/api/v1/replay/{run_id}/seek/{offset}", post(seek_replay))
        .route("/api/v1/replay/{run_id}/speed/{speed}", post(speed_replay))
        .route("/ws", get(ws::ws_handler))
        .with_state(state)
}

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse { status: "ok" })
}

async fn ingestion_status(
    State(state): State<AppState>,
) -> Result<Json<IngestionStatusResponse>, ApiError> {
    let statuses = data::ingestion::tracker::last_ingestions(&state.db).await?;
    Ok(Json(IngestionStatusResponse {
        sources: statuses
            .into_iter()
            .map(|status| IngestionSourceStatusResponse {
                name: status.source,
                table: status.table,
                ts_ms: status.ts_ms,
                rows_fetched: status.rows_fetched,
                rows_upserted: status.rows_upserted,
                duration_ms: status.duration_ms,
            })
            .collect(),
    }))
}

async fn broker_status() -> Result<Json<Vec<BrokerStatus>>, ApiError> {
    let mut statuses = Vec::new();
    for kind in [
        BrokerKind::Futu,
        BrokerKind::Binance,
        BrokerKind::Okx,
        BrokerKind::InteractiveBrokers,
    ] {
        statuses.push(
            FakeBrokerAdapter::new(kind)
                .status()
                .await
                .map_err(|error| ApiError(anyhow::anyhow!(error)))?,
        );
    }
    Ok(Json(statuses))
}

async fn broker_account(
    State(state): State<AppState>,
    Path(account_id): Path<String>,
) -> Result<Json<BrokerAccountSnapshot>, ApiError> {
    let app_config = config::AppConfig::from_toml_file(&state.config_path)?;
    let snapshot = FakeBrokerAdapter::new(broker_kind(app_config.broker.kind))
        .account_snapshot(&account_id)
        .await
        .map_err(|error| ApiError(anyhow::anyhow!(error)))?;
    Ok(Json(snapshot))
}

async fn paper_preflight(
    State(state): State<AppState>,
) -> Result<Json<PaperPreflightResponse>, ApiError> {
    let app_config = config::AppConfig::from_toml_file(&state.config_path)?;
    let settings = paper_settings(&app_config)?;
    if app_config.runtime.mode != config::RuntimeMode::Paper {
        return Err(ApiError(anyhow::anyhow!(
            "paper preflight requires runtime.mode = paper"
        )));
    }
    if app_config.broker.mode != config::BrokerMode::Paper {
        return Err(ApiError(anyhow::anyhow!(
            "paper preflight requires broker.mode = paper"
        )));
    }
    let real_broker_connection = paper_real_broker_connection_ready(&app_config).await?;
    let market_slices = load_configured_market_slices(&app_config)?;
    Ok(Json(PaperPreflightResponse {
        status: "ok",
        run_id: settings.run_id,
        strategy: settings.strategy_name,
        symbol: settings.symbol,
        bars: market_slices.len(),
        database: app_config.database.url,
        broker: broker_kind_slug(app_config.broker.kind),
        broker_mode: broker_mode_slug(app_config.broker.mode),
        account: settings.account_id,
        max_order_notional: settings.max_order_notional.to_string(),
        max_exposure: settings.max_exposure.to_string(),
        trading_halted: settings.trading_halted,
        real_broker_connection,
        order_submit_enabled: app_config.broker.order_submit_enabled,
    }))
}

async fn persist_run_config_snapshot(
    state: &AppState,
    app_config: &config::AppConfig,
    timestamp_ms: i64,
) -> Result<String, ApiError> {
    let content = std::fs::read_to_string(&state.config_path).map_err(|source| {
        ApiError(anyhow::anyhow!(
            "failed to read config snapshot {}: {source}",
            state.config_path
        ))
    })?;
    let config_value: toml::Value = toml::from_str(&content)
        .map_err(|error| ApiError(anyhow::anyhow!("failed to parse config snapshot: {error}")))?;
    let config_json = serde_json::to_value(config_value)
        .map_err(|error| ApiError(anyhow::anyhow!("failed to encode config snapshot: {error}")))?;

    state
        .db
        .record_run_config_snapshot(storage::RunConfigSnapshotCommand {
            run_id: app_config.runtime.run_id.clone(),
            content: content.clone(),
            format: "TOML".to_string(),
            checksum: Some(stable_bytes_hash(content.as_bytes())),
            ts_ms: timestamp_ms,
        })
        .await?;

    Ok(config_json.to_string())
}

async fn record_system_log(
    db: &storage::Db,
    run_id: &str,
    level: &str,
    message: &str,
    fields: serde_json::Value,
) -> Result<(), ApiError> {
    db.record_system_log(storage::SystemLogCommand {
        run_id: Some(run_id.to_string()),
        ts_ms: chrono::Utc::now().timestamp_millis(),
        level: level.to_string(),
        target: "api.run".to_string(),
        message: message.to_string(),
        fields: Some(fields),
    })
    .await?;
    Ok(())
}

async fn run_backtest(
    State(state): State<AppState>,
) -> Result<(StatusCode, Json<backtest::BacktestSummary>), ApiError> {
    let app_config = config::AppConfig::from_toml_file(&state.config_path)?;
    let started_at_ms = chrono::Utc::now().timestamp_millis();
    let config_json = persist_run_config_snapshot(&state, &app_config, started_at_ms).await?;
    insert_event(
        &state.db,
        &app_config.runtime.run_id,
        "backtest.started",
        &serde_json::json!({ "run_id": &app_config.runtime.run_id }).to_string(),
    )
    .await?;
    record_system_log(
        &state.db,
        &app_config.runtime.run_id,
        "INFO",
        "backtest run started",
        serde_json::json!({
            "mode": "backtest",
            "status": "running"
        }),
    )
    .await?;
    let market_slices = load_configured_market_slices(&app_config)?;
    let summary = BacktestRuntime::new(
        state.db.clone(),
        backtest_settings_with_config_json(&app_config, config_json)?,
    )
    .with_event_bus(state.event_bus.clone())
    .run_market_slices(market_slices)
    .await?;
    let payload = serde_json::json!({
        "run_id": &app_config.runtime.run_id,
        "signals": summary.signals,
        "orders": summary.orders
    })
    .to_string();
    insert_event(
        &state.db,
        &app_config.runtime.run_id,
        "backtest.completed",
        &payload,
    )
    .await?;
    record_system_log(
        &state.db,
        &app_config.runtime.run_id,
        "INFO",
        "backtest run completed",
        serde_json::json!({
            "mode": "backtest",
            "status": "completed",
            "signals": summary.signals,
            "orders": summary.orders
        }),
    )
    .await?;
    Ok((StatusCode::CREATED, Json(summary)))
}

async fn run_paper(
    State(state): State<AppState>,
) -> Result<(StatusCode, Json<RunStartResponse>), ApiError> {
    let app_config = config::AppConfig::from_toml_file(&state.config_path)?;
    let started_at_ms = chrono::Utc::now().timestamp_millis();
    let config_json = persist_run_config_snapshot(&state, &app_config, started_at_ms).await?;
    let settings = paper_settings_with_config_json(&app_config, config_json.clone())?;

    state
        .db
        .start_strategy_run(storage::StrategyRunStartCommand {
            run_id: settings.run_id.clone(),
            name: settings.strategy_name.clone(),
            mode: "paper".to_string(),
            started_at_ms,
            config: payload_response(config_json),
        })
        .await?;
    insert_event(
        &state.db,
        &settings.run_id,
        "paper.started",
        &serde_json::json!({ "run_id": &settings.run_id }).to_string(),
    )
    .await?;
    record_system_log(
        &state.db,
        &settings.run_id,
        "INFO",
        "paper run accepted",
        serde_json::json!({
            "mode": "paper",
            "status": "running"
        }),
    )
    .await?;

    let market_slices = match load_configured_market_slices(&app_config) {
        Ok(market_slices) => market_slices,
        Err(error) => {
            let message = error.0.to_string();
            record_failed_run(&state, &settings.run_id, message).await?;
            return Err(error);
        }
    };

    let run_id = settings.run_id.clone();
    let db = state.db.clone();
    let task_settings = settings.clone();
    let runtime = paper_runtime(&app_config, db.clone(), task_settings.clone())
        .await?
        .with_event_bus(state.event_bus.clone());
    state
        .runtime_manager
        .spawn(run_id.clone(), move |cancel| async move {
            let result = runtime
                .run_market_slices_with_cancel(market_slices, cancel)
                .await;

            match result {
                Ok(summary) => {
                    let payload = serde_json::json!({
                        "run_id": &task_settings.run_id,
                        "signals": summary.signals,
                        "orders": summary.orders
                    })
                    .to_string();
                    let _ =
                        insert_event(&db, &task_settings.run_id, "paper.completed", &payload).await;
                    let _ = record_system_log(
                        &db,
                        &task_settings.run_id,
                        "INFO",
                        "paper run completed",
                        serde_json::json!({
                            "mode": "paper",
                            "status": "completed",
                            "signals": summary.signals,
                            "orders": summary.orders
                        }),
                    )
                    .await;
                }
                Err(error) => {
                    if let Ok(Some(existing)) = db.get_strategy_run(&task_settings.run_id).await
                        && existing.status == "cancelled"
                    {
                        return;
                    }
                    let status = if error
                        .downcast_ref::<paper::PaperRunError>()
                        .is_some_and(|error| error == &paper::PaperRunError::Cancelled)
                    {
                        "cancelled"
                    } else {
                        "failed"
                    };
                    let _ = db
                        .update_strategy_run_status(
                            &task_settings.run_id,
                            status,
                            Some(chrono::Utc::now().timestamp_millis()),
                            Some(&error.to_string()),
                        )
                        .await;
                    let _ = record_system_log(
                        &db,
                        &task_settings.run_id,
                        "ERROR",
                        "paper run failed",
                        serde_json::json!({
                            "mode": "paper",
                            "status": status,
                            "error": error.to_string()
                        }),
                    )
                    .await;
                }
            }
        })
        .await
        .map_err(|error| anyhow::anyhow!("{error:?}"))?;

    Ok((
        StatusCode::ACCEPTED,
        Json(RunStartResponse {
            run_id,
            status: "running".to_string(),
        }),
    ))
}

async fn run_replay(
    State(state): State<AppState>,
) -> Result<(StatusCode, Json<ReplaySummary>), ApiError> {
    let app_config = config::AppConfig::from_toml_file(&state.config_path)?;
    let started_at_ms = chrono::Utc::now().timestamp_millis();
    let config_json = persist_run_config_snapshot(&state, &app_config, started_at_ms).await?;
    state
        .db
        .start_strategy_run(storage::StrategyRunStartCommand {
            run_id: app_config.runtime.run_id.clone(),
            name: app_config.strategy.name.clone(),
            mode: "replay".to_string(),
            started_at_ms,
            config: payload_response(config_json),
        })
        .await?;
    let replay_controller = Arc::new(Mutex::new(ReplayController::new(
        app_config.runtime.run_id.clone(),
        100_000,
    )));
    state
        .replay_controllers
        .lock()
        .await
        .insert(app_config.runtime.run_id.clone(), replay_controller.clone());
    insert_event(
        &state.db,
        &app_config.runtime.run_id,
        "replay.started",
        &serde_json::json!({ "run_id": &app_config.runtime.run_id }).to_string(),
    )
    .await?;
    record_system_log(
        &state.db,
        &app_config.runtime.run_id,
        "INFO",
        "replay run started",
        serde_json::json!({
            "mode": "replay",
            "status": "running"
        }),
    )
    .await?;

    let bars = data::load_bars(&app_config.data.source, &app_config.data.path)?;
    let summary = ReplayRuntime::new_for_run(app_config.runtime.run_id.clone(), 100_000)
        .with_event_bus(state.event_bus.clone())
        .with_controller(replay_controller)
        .replay_bars(bars)
        .await;
    state
        .db
        .update_strategy_run_status(
            &app_config.runtime.run_id,
            "completed",
            Some(chrono::Utc::now().timestamp_millis()),
            None,
        )
        .await?;
    let payload = serde_json::json!({
        "run_id": &app_config.runtime.run_id,
        "bars": summary.bars,
        "speed": summary.speed
    })
    .to_string();
    insert_event(
        &state.db,
        &app_config.runtime.run_id,
        "replay.completed",
        &payload,
    )
    .await?;
    record_system_log(
        &state.db,
        &app_config.runtime.run_id,
        "INFO",
        "replay run completed",
        serde_json::json!({
            "mode": "replay",
            "status": "completed",
            "bars": summary.bars,
            "speed": summary.speed
        }),
    )
    .await?;

    Ok((StatusCode::CREATED, Json(summary)))
}

async fn start_live_run(
    State(state): State<AppState>,
) -> Result<(StatusCode, Json<RunStartResponse>), ApiError> {
    let app_config = config::AppConfig::from_toml_file(&state.config_path)?;
    let run_id = app_config.runtime.run_id.clone();
    let initial_cash = Decimal::from_str(&app_config.portfolio.initial_cash)?;
    let started_at_ms = chrono::Utc::now().timestamp_millis();
    persist_run_config_snapshot(&state, &app_config, started_at_ms).await?;
    let db = state.db.clone();
    let task_run_id = run_id.clone();
    state
        .runtime_manager
        .spawn(run_id.clone(), move |cancel| async move {
            let runtime = LiveRuntime::new(
                db,
                LiveRuntimeSettings {
                    run_id: task_run_id,
                    broker_kind: broker_kind(app_config.broker.kind),
                    account_id: app_config.paper.account_id,
                    base_currency: app_config.portfolio.base_currency,
                    initial_cash,
                    broker_snapshot_interval_ms: app_config.live.broker_snapshot_interval_ms,
                    alert_sink: alert_sink_settings(&app_config.live.alerts),
                },
            );
            let _ = runtime.run(cancel).await;
        })
        .await
        .map_err(|error| ApiError(anyhow::anyhow!("{error:?}")))?;

    Ok((
        StatusCode::ACCEPTED,
        Json(RunStartResponse {
            run_id,
            status: "running".to_string(),
        }),
    ))
}

async fn stop_live_run(
    State(state): State<AppState>,
    Path(run_id): Path<String>,
) -> Result<axum::response::Response, ApiError> {
    state.runtime_manager.cancel(&run_id).await;
    state.runtime_manager.wait_for_idle(&run_id).await;
    get_run_status(State(state), Path(run_id)).await
}

async fn list_orders(State(state): State<AppState>) -> Result<Json<Vec<OrderResponse>>, ApiError> {
    let app_config = config::AppConfig::from_toml_file(&state.config_path)?;
    let orders = state
        .db
        .list_orders(&app_config.runtime.run_id)
        .await?
        .into_iter()
        .map(|order| OrderResponse {
            id: order.id,
            run_id: order.run_id,
            client_order_id: order.client_order_id,
            broker_order_id: order.broker_order_id,
            account_id: order.account_id,
            symbol: order.symbol,
            side: order.side,
            order_type: order.order_type,
            price: order.price,
            qty: order.qty,
            filled_qty: order.filled_qty,
            status: order.status,
            created_at_ms: order.created_at_ms,
            updated_at_ms: order.updated_at_ms,
        })
        .collect();
    Ok(Json(orders))
}

async fn list_fills(State(state): State<AppState>) -> Result<Json<Vec<FillResponse>>, ApiError> {
    let app_config = config::AppConfig::from_toml_file(&state.config_path)?;
    let fills = state
        .db
        .list_fills(&app_config.runtime.run_id)
        .await?
        .into_iter()
        .map(|fill| FillResponse {
            id: fill.id,
            order_id: fill.order_id,
            run_id: fill.run_id,
            symbol: fill.symbol,
            side: fill.side,
            price: fill.price,
            qty: fill.qty,
            fee: fill.fee,
            ts_ms: fill.ts_ms,
        })
        .collect();
    Ok(Json(fills))
}

async fn list_positions(
    State(state): State<AppState>,
) -> Result<Json<Vec<PositionResponse>>, ApiError> {
    let app_config = config::AppConfig::from_toml_file(&state.config_path)?;
    let positions = state
        .db
        .list_positions(&app_config.runtime.run_id)
        .await?
        .into_iter()
        .map(|position| PositionResponse {
            run_id: position.run_id,
            account_id: position.account_id,
            symbol: position.symbol,
            qty: position.qty,
            avg_price: position.avg_price,
            updated_at_ms: position.updated_at_ms,
        })
        .collect();
    Ok(Json(positions))
}

async fn list_run_crypto_positions(
    State(state): State<AppState>,
    Path(run_id): Path<String>,
) -> Result<Json<Vec<CryptoPositionResponse>>, ApiError> {
    let positions = state
        .db
        .list_crypto_positions(&run_id)
        .await?
        .into_iter()
        .map(crypto_position_response)
        .collect();
    Ok(Json(positions))
}

async fn list_funding_rates(
    State(state): State<AppState>,
    Query(query): Query<FundingRatesQuery>,
) -> Result<Json<Vec<FundingRateResponse>>, ApiError> {
    let rates = state
        .db
        .list_funding_rates(
            &query.exchange,
            query.symbol.as_deref(),
            query.start_ms,
            query.end_ms,
        )
        .await?
        .into_iter()
        .map(funding_rate_response)
        .collect();
    Ok(Json(rates))
}

async fn list_crypto_market_meta(
    State(state): State<AppState>,
    Query(query): Query<CryptoMarketMetaQuery>,
) -> Result<Json<Vec<CryptoMarketMetaResponse>>, ApiError> {
    let metas = state
        .db
        .find_crypto_market_meta(&query.exchange, &query.symbol)
        .await?
        .into_iter()
        .map(crypto_market_meta_response)
        .collect();
    Ok(Json(metas))
}

async fn list_corporate_actions(
    State(state): State<AppState>,
    Query(query): Query<CorporateActionsQuery>,
) -> Result<Json<Vec<CorporateActionResponse>>, ApiError> {
    let actions = state
        .db
        .list_corporate_actions(&query.market, &query.symbol, query.start_ms, query.end_ms)
        .await?
        .into_iter()
        .map(corporate_action_response)
        .collect();
    Ok(Json(actions))
}

async fn list_account_balances(
    State(state): State<AppState>,
) -> Result<Json<Vec<AccountBalanceResponse>>, ApiError> {
    let app_config = config::AppConfig::from_toml_file(&state.config_path)?;
    let balances = state
        .db
        .list_account_balances(&app_config.runtime.run_id)
        .await?
        .into_iter()
        .map(|balance| AccountBalanceResponse {
            run_id: balance.run_id,
            account_id: balance.account_id,
            asset: balance.asset,
            total: balance.total,
            available: balance.available,
            frozen: balance.frozen,
            updated_at_ms: balance.updated_at_ms,
        })
        .collect();
    Ok(Json(balances))
}

async fn list_cash_snapshots(
    State(state): State<AppState>,
) -> Result<Json<Vec<CashSnapshotResponse>>, ApiError> {
    let app_config = config::AppConfig::from_toml_file(&state.config_path)?;
    snapshot_cash_response(&state.db, &app_config.runtime.run_id, None, None, None).await
}

async fn list_run_cash_snapshots(
    State(state): State<AppState>,
    Path(run_id): Path<String>,
    Query(query): Query<CashSnapshotsQuery>,
) -> Result<Json<Vec<CashSnapshotResponse>>, ApiError> {
    snapshot_cash_response(
        &state.db,
        &run_id,
        query.currency.as_deref(),
        query.from_ms,
        query.to_ms,
    )
    .await
}

async fn snapshot_cash_response(
    db: &storage::Db,
    run_id: &str,
    currency: Option<&str>,
    from_ms: Option<i64>,
    to_ms: Option<i64>,
) -> Result<Json<Vec<CashSnapshotResponse>>, ApiError> {
    let snapshots = db
        .list_cash_snapshots_filtered(run_id, currency, from_ms, to_ms)
        .await?
        .into_iter()
        .map(|snapshot| CashSnapshotResponse {
            id: snapshot.id,
            run_id: snapshot.run_id,
            ts_ms: snapshot.ts_ms,
            currency: snapshot.currency,
            cash: snapshot.cash,
            available_cash: snapshot.available_cash,
            frozen_cash: snapshot.frozen_cash,
            created_at_ms: snapshot.created_at_ms,
        })
        .collect();
    Ok(Json(snapshots))
}

async fn list_position_snapshots(
    State(state): State<AppState>,
) -> Result<Json<Vec<PositionSnapshotResponse>>, ApiError> {
    let app_config = config::AppConfig::from_toml_file(&state.config_path)?;
    snapshot_position_response(
        &state.db,
        &app_config.runtime.run_id,
        None,
        None,
        None,
        None,
    )
    .await
}

async fn list_run_position_snapshots(
    State(state): State<AppState>,
    Path(run_id): Path<String>,
    Query(query): Query<PositionSnapshotsQuery>,
) -> Result<Json<Vec<PositionSnapshotResponse>>, ApiError> {
    snapshot_position_response(
        &state.db,
        &run_id,
        query.symbol.as_deref(),
        query.position_side.as_deref(),
        query.from_ms,
        query.to_ms,
    )
    .await
}

async fn snapshot_position_response(
    db: &storage::Db,
    run_id: &str,
    symbol: Option<&str>,
    position_side: Option<&str>,
    from_ms: Option<i64>,
    to_ms: Option<i64>,
) -> Result<Json<Vec<PositionSnapshotResponse>>, ApiError> {
    let snapshots = db
        .list_position_snapshots_filtered(run_id, symbol, position_side, from_ms, to_ms)
        .await?
        .into_iter()
        .map(|snapshot| PositionSnapshotResponse {
            id: snapshot.id,
            run_id: snapshot.run_id,
            ts_ms: snapshot.ts_ms,
            market: snapshot.market,
            exchange: snapshot.exchange,
            symbol: snapshot.symbol,
            asset_class: snapshot.asset_class,
            position_side: snapshot.position_side,
            qty: snapshot.qty,
            available_qty: snapshot.available_qty,
            avg_price: snapshot.avg_price,
            entry_price: snapshot.entry_price,
            market_price: snapshot.market_price,
            mark_price: snapshot.mark_price,
            market_value: snapshot.market_value,
            unrealized_pnl: snapshot.unrealized_pnl,
            realized_pnl: snapshot.realized_pnl,
            currency: snapshot.currency,
            created_at_ms: snapshot.created_at_ms,
        })
        .collect();
    Ok(Json(snapshots))
}

async fn get_run_reconciliation(
    State(state): State<AppState>,
    Path(run_id): Path<String>,
) -> Result<Json<ReconciliationStatusResponse>, ApiError> {
    let cash_snapshots = state.db.list_cash_snapshots(&run_id).await?;
    let position_snapshots = state.db.list_position_snapshots(&run_id).await?;
    let drift_events = state
        .db
        .list_risk_events(&run_id)
        .await?
        .into_iter()
        .filter(|event| event.risk_type == "reconciliation_drift")
        .map(risk_event_response)
        .collect::<Vec<_>>();
    let latest_cash_ts_ms = cash_snapshots.iter().map(|snapshot| snapshot.ts_ms).max();
    let latest_position_ts_ms = position_snapshots
        .iter()
        .map(|snapshot| snapshot.ts_ms)
        .max();
    let status = if drift_events.is_empty() {
        "ok"
    } else {
        "drift"
    };

    Ok(Json(ReconciliationStatusResponse {
        run_id,
        status: status.to_string(),
        cash_snapshots: cash_snapshots.len(),
        position_snapshots: position_snapshots.len(),
        drift_events,
        latest_cash_ts_ms,
        latest_position_ts_ms,
    }))
}

async fn list_portfolio_snapshots(
    State(state): State<AppState>,
) -> Result<Json<Vec<PortfolioSnapshotResponse>>, ApiError> {
    let app_config = config::AppConfig::from_toml_file(&state.config_path)?;
    let snapshots = state
        .db
        .list_portfolio_snapshots(&app_config.runtime.run_id)
        .await?
        .into_iter()
        .map(|snapshot| PortfolioSnapshotResponse {
            id: snapshot.id,
            run_id: snapshot.run_id,
            account_id: snapshot.account_id,
            ts_ms: snapshot.ts_ms,
            cash: snapshot.cash,
            market_value: snapshot.market_value,
            equity: snapshot.equity,
            realized_pnl: snapshot.realized_pnl,
            unrealized_pnl: snapshot.unrealized_pnl,
        })
        .collect();
    Ok(Json(snapshots))
}

async fn metrics_summary(State(state): State<AppState>) -> Result<Json<MetricsSummary>, ApiError> {
    let app_config = config::AppConfig::from_toml_file(&state.config_path)?;
    let run_id = &app_config.runtime.run_id;
    let orders = state.db.list_orders(run_id).await?;
    let fills = state.db.list_fills(run_id).await?;
    let snapshots = state.db.list_portfolio_snapshots(run_id).await?;
    let equity = snapshots
        .iter()
        .map(|snapshot| Decimal::from_str(&snapshot.equity))
        .collect::<Result<Vec<_>, _>>()?;
    let returns = equity_returns(&equity);

    Ok(Json(paper_summary(
        orders.len(),
        fills.len(),
        &equity,
        &returns,
    )))
}

async fn list_runs(State(state): State<AppState>) -> Result<Json<Vec<RunResponse>>, ApiError> {
    let runs = state
        .db
        .list_strategy_runs()
        .await?
        .into_iter()
        .map(run_response)
        .collect();
    Ok(Json(runs))
}

async fn get_run(
    State(state): State<AppState>,
    Path(run_id): Path<String>,
) -> Result<axum::response::Response, ApiError> {
    let Some(run) = state.db.get_strategy_run(&run_id).await? else {
        return Ok(StatusCode::NOT_FOUND.into_response());
    };
    Ok(Json(run_response(run)).into_response())
}

async fn list_configs(
    State(state): State<AppState>,
) -> Result<Json<Vec<ConfigResponse>>, ApiError> {
    let configs = state
        .db
        .list_configs()
        .await?
        .into_iter()
        .map(config_response)
        .collect();
    Ok(Json(configs))
}

async fn create_config_version(
    State(state): State<AppState>,
    Json(request): Json<CreateConfigVersionRequest>,
) -> Result<(StatusCode, Json<ConfigVersionResponse>), ApiError> {
    let ts_ms = request.ts_ms.unwrap_or_else(now_ms);
    let version = state
        .db
        .create_config_version(storage::NewConfigVersion {
            name: request.name.clone(),
            content_json: serde_json::to_string(&request.content)
                .map_err(|error| ApiError(anyhow::anyhow!(error)))?,
            created_by: request.created_by,
            parent_version: request.parent_version,
            target_env: request.target_env,
            rollout: request.rollout,
            ts_ms,
        })
        .await?;
    let config = state
        .db
        .get_config(&request.name, version)
        .await?
        .ok_or_else(|| ApiError(anyhow::anyhow!("created config version was not found")))?;
    Ok((StatusCode::CREATED, Json(config_version_response(config))))
}

async fn list_config_versions_by_name(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<Vec<ConfigVersionResponse>>, ApiError> {
    let versions = state
        .db
        .list_config_versions(&name)
        .await?
        .into_iter()
        .map(config_version_response)
        .collect();
    Ok(Json(versions))
}

async fn get_latest_config(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<axum::response::Response, ApiError> {
    let Some(config) = state.db.get_latest_config(&name).await? else {
        return Ok(StatusCode::NOT_FOUND.into_response());
    };
    Ok(Json(config_version_response(config)).into_response())
}

async fn get_published_config(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<axum::response::Response, ApiError> {
    let Some(config) = state.db.get_published_config(&name).await? else {
        return Ok(StatusCode::NOT_FOUND.into_response());
    };
    Ok(Json(config_version_response(config)).into_response())
}

async fn get_config_version(
    State(state): State<AppState>,
    Path((name, version)): Path<(String, u32)>,
) -> Result<axum::response::Response, ApiError> {
    let Some(config) = state.db.get_config(&name, version).await? else {
        return Ok(StatusCode::NOT_FOUND.into_response());
    };
    Ok(Json(config_version_response(config)).into_response())
}

async fn update_config_state(
    State(state): State<AppState>,
    Path((name, version)): Path<(String, u32)>,
    Json(request): Json<UpdateConfigStateRequest>,
) -> Result<Json<ConfigVersionResponse>, ApiError> {
    let ts_ms = request.ts_ms.unwrap_or_else(now_ms);
    if let Some(actor_role) = request.actor_role.as_deref() {
        state
            .db
            .update_config_state_with_policy(
                &name,
                version,
                request.new_state,
                &request.changed_by,
                actor_role,
                request.reason.as_deref(),
                ts_ms,
            )
            .await?;
    } else {
        state
            .db
            .update_config_state(
                &name,
                version,
                request.new_state,
                &request.changed_by,
                request.reason.as_deref(),
                ts_ms,
            )
            .await?;
    }
    let config = state
        .db
        .get_config(&name, version)
        .await?
        .ok_or_else(|| ApiError(anyhow::anyhow!("updated config version was not found")))?;
    Ok(Json(config_version_response(config)))
}

async fn list_pending_config_approvals(
    State(state): State<AppState>,
    Query(query): Query<PendingConfigApprovalsQuery>,
) -> Result<Json<Vec<ConfigVersionResponse>>, ApiError> {
    let approvals = state
        .db
        .list_pending_config_approvals(query.target_env.as_deref())
        .await?
        .into_iter()
        .map(config_version_response)
        .collect();
    Ok(Json(approvals))
}

async fn diff_config_versions(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Query(query): Query<ConfigDiffQuery>,
) -> Result<Json<storage::ConfigDiff>, ApiError> {
    Ok(Json(
        state.db.diff_configs(&name, query.v1, query.v2).await?,
    ))
}

async fn rollback_config_version(
    State(state): State<AppState>,
    Path((name, version)): Path<(String, u32)>,
    Json(request): Json<RollbackConfigRequest>,
) -> Result<(StatusCode, Json<ConfigVersionResponse>), ApiError> {
    let ts_ms = request.ts_ms.unwrap_or_else(now_ms);
    let rollback_version = state
        .db
        .rollback_config_version(
            &name,
            version,
            &request.actor,
            request.reason.as_deref(),
            ts_ms,
        )
        .await?;
    let config = state
        .db
        .get_config(&name, rollback_version)
        .await?
        .ok_or_else(|| ApiError(anyhow::anyhow!("rollback config version was not found")))?;
    Ok((StatusCode::CREATED, Json(config_version_response(config))))
}

async fn list_config_releases(
    State(state): State<AppState>,
    Path(config_id): Path<String>,
) -> Result<Json<Vec<ConfigReleaseResponse>>, ApiError> {
    let releases = state
        .db
        .list_config_releases(&config_id)
        .await?
        .into_iter()
        .map(config_release_response)
        .collect();
    Ok(Json(releases))
}

async fn list_config_audits(
    State(state): State<AppState>,
    Path(config_id): Path<String>,
) -> Result<Json<Vec<ConfigAuditResponse>>, ApiError> {
    let audits = state
        .db
        .list_config_audits(&config_id)
        .await?
        .into_iter()
        .map(config_audit_response)
        .collect();
    Ok(Json(audits))
}

async fn get_run_config_version(
    State(state): State<AppState>,
    Path(run_id): Path<String>,
) -> Result<axum::response::Response, ApiError> {
    let Some(binding) = state.db.get_run_config_version_binding(&run_id).await? else {
        return Ok(StatusCode::NOT_FOUND.into_response());
    };
    Ok(Json(run_config_version_binding_response(binding)).into_response())
}

async fn list_events(State(state): State<AppState>) -> Result<Json<Vec<EventResponse>>, ApiError> {
    let events = state
        .db
        .list_events()
        .await?
        .into_iter()
        .map(event_response)
        .collect();
    Ok(Json(events))
}

async fn list_run_events(
    State(state): State<AppState>,
    Path(run_id): Path<String>,
) -> Result<Json<Vec<EventResponse>>, ApiError> {
    let events = state
        .db
        .list_events_by_source(&run_id)
        .await?
        .into_iter()
        .map(event_response)
        .collect();
    Ok(Json(events))
}

async fn list_run_order_events(
    State(state): State<AppState>,
    Path(run_id): Path<String>,
) -> Result<Json<Vec<OrderEventResponse>>, ApiError> {
    let events = state
        .db
        .list_order_events(&run_id)
        .await?
        .into_iter()
        .map(order_event_response)
        .collect();
    Ok(Json(events))
}

async fn list_run_risk_events(
    State(state): State<AppState>,
    Path(run_id): Path<String>,
) -> Result<Json<Vec<RiskEventResponse>>, ApiError> {
    let events = state
        .db
        .list_risk_events(&run_id)
        .await?
        .into_iter()
        .map(risk_event_response)
        .collect();
    Ok(Json(events))
}

async fn list_run_reconciliation_drifts(
    State(state): State<AppState>,
    Path(run_id): Path<String>,
    Query(query): Query<ReconciliationDriftsQuery>,
) -> Result<Json<Vec<RiskEventResponse>>, ApiError> {
    reconciliation_drifts_response(
        &state.db,
        storage::RiskEventFilter {
            run_id: Some(run_id),
            risk_type: Some("reconciliation_drift".to_string()),
            account_id: query.account_id,
            symbol: query.symbol,
            from_ms: query.from_ms,
            to_ms: query.to_ms,
            limit: query.limit,
        },
    )
    .await
}

async fn list_run_insights(
    State(state): State<AppState>,
    Path(run_id): Path<String>,
) -> Result<Json<Vec<InsightResponse>>, ApiError> {
    let insights = state
        .db
        .list_insights(&run_id)
        .await?
        .into_iter()
        .map(insight_response)
        .collect();
    Ok(Json(insights))
}

async fn list_run_portfolio_targets(
    State(state): State<AppState>,
    Path(run_id): Path<String>,
) -> Result<Json<Vec<PortfolioTargetResponse>>, ApiError> {
    let targets = state
        .db
        .list_portfolio_targets(&run_id)
        .await?
        .into_iter()
        .map(portfolio_target_response)
        .collect();
    Ok(Json(targets))
}

async fn list_run_system_logs(
    State(state): State<AppState>,
    Path(run_id): Path<String>,
    Query(query): Query<SystemLogsQuery>,
) -> Result<Json<Vec<SystemLogResponse>>, ApiError> {
    system_logs_response(
        &state.db,
        storage::SystemLogFilter {
            run_id: Some(run_id),
            level: query.level,
            target: query.target,
            from_ms: query.from_ms,
            to_ms: query.to_ms,
            limit: query.limit,
        },
    )
    .await
}

async fn list_system_logs(
    State(state): State<AppState>,
    Query(query): Query<SystemLogsQuery>,
) -> Result<Json<Vec<SystemLogResponse>>, ApiError> {
    system_logs_response(
        &state.db,
        storage::SystemLogFilter {
            run_id: query.run_id,
            level: query.level,
            target: query.target,
            from_ms: query.from_ms,
            to_ms: query.to_ms,
            limit: query.limit,
        },
    )
    .await
}

async fn list_reconciliation_drifts(
    State(state): State<AppState>,
    Query(query): Query<ReconciliationDriftsQuery>,
) -> Result<Json<Vec<RiskEventResponse>>, ApiError> {
    reconciliation_drifts_response(
        &state.db,
        storage::RiskEventFilter {
            run_id: query.run_id,
            risk_type: Some("reconciliation_drift".to_string()),
            account_id: query.account_id,
            symbol: query.symbol,
            from_ms: query.from_ms,
            to_ms: query.to_ms,
            limit: query.limit,
        },
    )
    .await
}

async fn get_reconciliation_alerts_summary(
    State(state): State<AppState>,
    Query(query): Query<ReconciliationAlertsSummaryQuery>,
) -> Result<Json<ReconciliationAlertSummaryResponse>, ApiError> {
    reconciliation_alerts_summary_response(
        &state.db,
        query.run_id,
        query.account_id,
        query.symbol,
        query.from_ms,
        query.to_ms,
        query.limit,
    )
    .await
}

async fn get_run_reconciliation_alerts_summary(
    State(state): State<AppState>,
    Path(run_id): Path<String>,
    Query(query): Query<ReconciliationAlertsSummaryQuery>,
) -> Result<Json<ReconciliationAlertSummaryResponse>, ApiError> {
    reconciliation_alerts_summary_response(
        &state.db,
        Some(run_id),
        query.account_id,
        query.symbol,
        query.from_ms,
        query.to_ms,
        query.limit,
    )
    .await
}

async fn get_reconciliation_alert_deliveries_summary(
    State(state): State<AppState>,
    Query(query): Query<ReconciliationAlertsSummaryQuery>,
) -> Result<Json<ReconciliationAlertDeliverySummaryResponse>, ApiError> {
    reconciliation_alert_deliveries_summary_response(
        &state.db,
        query.run_id,
        query.account_id,
        query.symbol,
        query.from_ms,
        query.to_ms,
        query.limit,
    )
    .await
}

async fn get_run_reconciliation_alert_deliveries_summary(
    State(state): State<AppState>,
    Path(run_id): Path<String>,
    Query(query): Query<ReconciliationAlertsSummaryQuery>,
) -> Result<Json<ReconciliationAlertDeliverySummaryResponse>, ApiError> {
    reconciliation_alert_deliveries_summary_response(
        &state.db,
        Some(run_id),
        query.account_id,
        query.symbol,
        query.from_ms,
        query.to_ms,
        query.limit,
    )
    .await
}

async fn system_logs_response(
    db: &storage::Db,
    filter: storage::SystemLogFilter,
) -> Result<Json<Vec<SystemLogResponse>>, ApiError> {
    let logs = db
        .list_system_logs_filtered(filter)
        .await?
        .into_iter()
        .map(system_log_response)
        .collect();
    Ok(Json(logs))
}

async fn reconciliation_drifts_response(
    db: &storage::Db,
    filter: storage::RiskEventFilter,
) -> Result<Json<Vec<RiskEventResponse>>, ApiError> {
    let events = db
        .list_risk_events_filtered(filter)
        .await?
        .into_iter()
        .map(risk_event_response)
        .collect();
    Ok(Json(events))
}

async fn reconciliation_alerts_summary_response(
    db: &storage::Db,
    run_id: Option<String>,
    account_id: Option<String>,
    symbol: Option<String>,
    from_ms: Option<i64>,
    to_ms: Option<i64>,
    limit: Option<i64>,
) -> Result<Json<ReconciliationAlertSummaryResponse>, ApiError> {
    let logs = db
        .list_system_logs_filtered(storage::SystemLogFilter {
            run_id: run_id.clone(),
            level: None,
            target: Some("runtime.alert".to_string()),
            from_ms,
            to_ms,
            limit,
        })
        .await?;

    let mut latest_alert_ts_ms = None;
    let mut runs = BTreeSet::new();
    let mut accounts = BTreeSet::new();
    let mut symbols = BTreeSet::new();
    let mut reasons = BTreeSet::new();
    let mut alert_count = 0usize;

    for log in logs {
        if log.message != "reconciliation_drift.alert" {
            continue;
        }
        let fields = log
            .fields_json
            .as_deref()
            .and_then(parse_log_fields)
            .unwrap_or(serde_json::Value::Null);
        let log_account_id = json_string_field(&fields, "account_id");
        let log_symbol = json_string_field(&fields, "symbol");
        let log_reason = json_string_field(&fields, "reason");

        if account_id
            .as_deref()
            .is_some_and(|expected| log_account_id.as_deref() != Some(expected))
        {
            continue;
        }
        if symbol
            .as_deref()
            .is_some_and(|expected| log_symbol.as_deref() != Some(expected))
        {
            continue;
        }

        alert_count += 1;
        latest_alert_ts_ms =
            Some(latest_alert_ts_ms.map_or(log.ts_ms, |current: i64| current.max(log.ts_ms)));
        if let Some(run_id) = log.run_id {
            runs.insert(run_id);
        }
        if let Some(account_id) = log_account_id {
            accounts.insert(account_id);
        }
        if let Some(symbol) = log_symbol {
            symbols.insert(symbol);
        }
        if let Some(reason) = log_reason {
            reasons.insert(reason);
        }
    }

    Ok(Json(ReconciliationAlertSummaryResponse {
        run_id,
        alert_count,
        latest_alert_ts_ms,
        runs: runs.into_iter().collect(),
        accounts: accounts.into_iter().collect(),
        symbols: symbols.into_iter().collect(),
        reasons: reasons.into_iter().collect(),
    }))
}

async fn reconciliation_alert_deliveries_summary_response(
    db: &storage::Db,
    run_id: Option<String>,
    account_id: Option<String>,
    symbol: Option<String>,
    from_ms: Option<i64>,
    to_ms: Option<i64>,
    limit: Option<i64>,
) -> Result<Json<ReconciliationAlertDeliverySummaryResponse>, ApiError> {
    let logs = db
        .list_system_logs_filtered(storage::SystemLogFilter {
            run_id: run_id.clone(),
            level: None,
            target: Some("runtime.alert_delivery".to_string()),
            from_ms,
            to_ms,
            limit,
        })
        .await?;

    let mut latest_delivery_ts_ms = None;
    let mut statuses = BTreeSet::new();
    let mut sinks = BTreeSet::new();
    let mut delivery_count = 0usize;
    let mut sent_count = 0usize;
    let mut failed_count = 0usize;

    for log in logs {
        if log.message != "alert.delivery" {
            continue;
        }
        let fields = log
            .fields_json
            .as_deref()
            .and_then(parse_log_fields)
            .unwrap_or(serde_json::Value::Null);
        let log_account_id = json_string_field(&fields, "account_id");
        let log_symbol = json_string_field(&fields, "symbol");
        if account_id
            .as_deref()
            .is_some_and(|expected| log_account_id.as_deref() != Some(expected))
        {
            continue;
        }
        if symbol
            .as_deref()
            .is_some_and(|expected| log_symbol.as_deref() != Some(expected))
        {
            continue;
        }

        delivery_count += 1;
        latest_delivery_ts_ms =
            Some(latest_delivery_ts_ms.map_or(log.ts_ms, |current: i64| current.max(log.ts_ms)));
        if let Some(status) = json_string_field(&fields, "status") {
            if status == "sent" {
                sent_count += 1;
            }
            if status == "failed" {
                failed_count += 1;
            }
            statuses.insert(status);
        }
        if let Some(sink) = json_string_field(&fields, "sink") {
            sinks.insert(sink);
        }
    }

    Ok(Json(ReconciliationAlertDeliverySummaryResponse {
        run_id,
        delivery_count,
        latest_delivery_ts_ms,
        sent_count,
        failed_count,
        statuses: statuses.into_iter().collect(),
        sinks: sinks.into_iter().collect(),
    }))
}

fn parse_log_fields(fields_json: &str) -> Option<serde_json::Value> {
    serde_json::from_str(fields_json).ok()
}

fn json_string_field(value: &serde_json::Value, key: &str) -> Option<String> {
    value.get(key)?.as_str().map(str::to_string)
}

fn alert_sink_settings(alerts: &config::LiveAlertsConfig) -> AlertSinkSettings {
    if !alerts.enabled {
        return AlertSinkSettings::Noop;
    }
    match (
        alerts.sink.as_deref(),
        alerts.file_path.as_ref().filter(|path| !path.is_empty()),
        alerts.webhook_url.as_ref().filter(|url| !url.is_empty()),
    ) {
        (Some("file"), Some(path), _) => AlertSinkSettings::File {
            path: path.clone(),
            cooldown_ms: alerts.cooldown_ms.unwrap_or(300_000),
        },
        (Some("webhook"), _, Some(url)) => AlertSinkSettings::Webhook {
            url: url.clone(),
            cooldown_ms: alerts.cooldown_ms.unwrap_or(300_000),
            timeout_ms: alerts.webhook_timeout_ms.unwrap_or(3_000),
            max_retries: alerts.webhook_max_retries.unwrap_or(2),
            auth_token: alerts.webhook_auth_token.clone(),
        },
        _ => AlertSinkSettings::Noop,
    }
}

fn run_response(run: storage::StrategyRunRecord) -> RunResponse {
    let config = serde_json::from_str(&run.config_json)
        .unwrap_or(serde_json::Value::String(run.config_json));
    RunResponse {
        id: run.id,
        name: run.name,
        mode: run.mode,
        status: run.status,
        started_at_ms: run.started_at_ms,
        ended_at_ms: run.ended_at_ms,
        error: run.error,
        config,
    }
}

fn payload_response(payload_json: String) -> serde_json::Value {
    serde_json::from_str(&payload_json).unwrap_or(serde_json::Value::String(payload_json))
}

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

fn config_response(config: storage::StoredConfigRecord) -> ConfigResponse {
    ConfigResponse {
        id: config.id,
        name: config.name,
        config_type: config.config_type,
        content: config.content,
        format: config.format,
        checksum: config.checksum,
        created_at_ms: config.created_at_ms,
        updated_at_ms: config.updated_at_ms,
    }
}

fn config_version_response(config: storage::ConfigVersion) -> ConfigVersionResponse {
    ConfigVersionResponse {
        id: config.id,
        name: config.name,
        version: config.version,
        content: payload_response(config.content_json),
        state: config.state,
        parent_version: config.parent_version,
        created_by: config.created_by,
        created_at_ms: config.created_at_ms,
        state_changed_at_ms: config.state_changed_at_ms,
        state_changed_by: config.state_changed_by,
        state_change_reason: config.state_change_reason,
        target_env: config.target_env,
        rollout: config.rollout,
        approved_by: config.approved_by,
        approved_at_ms: config.approved_at_ms,
        published_by: config.published_by,
        published_at_ms: config.published_at_ms,
    }
}

fn config_release_response(release: storage::StoredConfigRelease) -> ConfigReleaseResponse {
    ConfigReleaseResponse {
        id: release.id,
        config_id: release.config_id,
        version: release.version,
        status: release.status,
        released_by: release.released_by,
        notes: release.notes,
        created_at_ms: release.created_at_ms,
        updated_at_ms: release.updated_at_ms,
    }
}

fn run_config_version_binding_response(
    binding: storage::StoredRunConfigVersionBinding,
) -> RunConfigVersionBindingResponse {
    RunConfigVersionBindingResponse {
        run_id: binding.run_id,
        config_id: binding.config_id,
        version: binding.version,
        bound_at_ms: binding.bound_at_ms,
    }
}

fn config_audit_response(audit: storage::StoredConfigAudit) -> ConfigAuditResponse {
    ConfigAuditResponse {
        id: audit.id,
        config_id: audit.config_id,
        version: audit.version,
        action: audit.action,
        actor: audit.actor,
        reason: audit.reason,
        ts_ms: audit.ts_ms,
    }
}

fn crypto_position_response(position: storage::StoredCryptoPosition) -> CryptoPositionResponse {
    CryptoPositionResponse {
        run_id: position.run_id,
        account_id: position.account_id,
        exchange: position.exchange,
        symbol: position.symbol,
        asset_class: position.asset_class,
        margin_mode: position.margin_mode,
        position_side: position.position_side,
        leverage: position.leverage,
        qty: position.qty,
        avg_price: position.avg_price,
        margin_used: position.margin_used,
        funding_fee: position.funding_fee,
        realized_pnl: position.realized_pnl,
        unrealized_pnl: position.unrealized_pnl,
        updated_at_ms: position.updated_at_ms,
    }
}

fn funding_rate_response(rate: storage::StoredFundingRate) -> FundingRateResponse {
    FundingRateResponse {
        id: rate.id,
        exchange: rate.exchange,
        symbol: rate.symbol,
        funding_time_ms: rate.funding_time_ms,
        funding_rate: rate.funding_rate,
        mark_price: rate.mark_price,
        source: rate.source,
    }
}

fn crypto_market_meta_response(meta: storage::StoredCryptoMarketMeta) -> CryptoMarketMetaResponse {
    CryptoMarketMetaResponse {
        id: meta.id,
        exchange: meta.exchange,
        symbol: meta.symbol,
        base_asset: meta.base_asset,
        quote_asset: meta.quote_asset,
        instrument_type: meta.instrument_type,
        contract_type: meta.contract_type,
        contract_size: meta.contract_size,
        settlement_asset: meta.settlement_asset,
        min_notional: meta.min_notional,
        min_qty: meta.min_qty,
        max_qty: meta.max_qty,
        price_precision: meta.price_precision,
        qty_precision: meta.qty_precision,
        price_tick: meta.price_tick,
        qty_step: meta.qty_step,
        maker_fee_rate: meta.maker_fee_rate,
        taker_fee_rate: meta.taker_fee_rate,
        funding_interval_hours: meta.funding_interval_hours,
        max_leverage: meta.max_leverage,
        margin_modes: meta
            .margin_modes
            .map(payload_response)
            .unwrap_or(serde_json::Value::Null),
        is_inverse: meta.is_inverse,
        is_active: meta.is_active,
        created_at_ms: meta.created_at_ms,
        updated_at_ms: meta.updated_at_ms,
    }
}

fn corporate_action_response(
    action: storage::StoredCorporateActionMeta,
) -> CorporateActionResponse {
    CorporateActionResponse {
        id: action.id,
        market: action.market,
        exchange: action.exchange,
        symbol: action.symbol,
        action_type: action.action_type,
        ex_date_ms: action.ex_date_ms,
        record_date_ms: action.record_date_ms,
        payable_date_ms: action.payable_date_ms,
        ratio: action.ratio,
        cash_amount: action.cash_amount,
        currency: action.currency,
        source: action.source,
        created_at_ms: action.created_at_ms,
        updated_at_ms: action.updated_at_ms,
    }
}

fn system_log_response(log: storage::StoredSystemLog) -> SystemLogResponse {
    SystemLogResponse {
        id: log.id,
        run_id: log.run_id,
        ts_ms: log.ts_ms,
        level: log.level,
        target: log.target,
        message: log.message,
        fields: log
            .fields_json
            .map(payload_response)
            .unwrap_or_else(|| serde_json::json!({})),
        created_at_ms: log.created_at_ms,
    }
}

fn event_response(event: storage::EventRecord) -> EventResponse {
    EventResponse {
        event_id: event.event_id,
        ts_ms: event.ts_ms,
        source: event.source,
        category: event.category,
        payload: payload_response(event.payload_json),
    }
}

fn order_event_response(event: storage::StoredOrderEvent) -> OrderEventResponse {
    OrderEventResponse {
        id: event.id,
        event_id: event.event_id,
        run_id: event.run_id,
        order_id: event.order_id,
        client_order_id: event.client_order_id,
        broker_order_id: event.broker_order_id,
        account_id: event.account_id,
        symbol: event.symbol,
        status: event.status,
        event_type: event.event_type,
        message: event.message,
        ts_ms: event.ts_ms,
        payload: payload_response(event.payload_json),
    }
}

fn risk_event_response(event: storage::StoredRiskEvent) -> RiskEventResponse {
    RiskEventResponse {
        id: event.id,
        event_id: event.event_id,
        run_id: event.run_id,
        account_id: event.account_id,
        symbol: event.symbol,
        risk_type: event.risk_type,
        decision: event.decision,
        reason: event.reason,
        threshold: event.threshold,
        observed_value: event.observed_value,
        ts_ms: event.ts_ms,
        payload: payload_response(event.payload_json),
    }
}

fn insight_response(insight: storage::StoredInsight) -> InsightResponse {
    InsightResponse {
        id: insight.id,
        event_id: insight.event_id,
        run_id: insight.run_id,
        strategy: insight.strategy,
        symbol: insight.symbol,
        side: insight.side,
        confidence: insight.confidence,
        ts_ms: insight.ts_ms,
        payload: payload_response(insight.payload_json),
    }
}

fn portfolio_target_response(target: storage::StoredPortfolioTarget) -> PortfolioTargetResponse {
    PortfolioTargetResponse {
        id: target.id,
        event_id: target.event_id,
        run_id: target.run_id,
        account_id: target.account_id,
        symbol: target.symbol,
        target_qty: target.target_qty,
        ts_ms: target.ts_ms,
        payload: payload_response(target.payload_json),
    }
}

async fn get_run_status(
    State(state): State<AppState>,
    Path(run_id): Path<String>,
) -> Result<axum::response::Response, ApiError> {
    let Some(run) = state.db.get_strategy_run(&run_id).await? else {
        return Ok(StatusCode::NOT_FOUND.into_response());
    };
    Ok(Json(RunStatusResponse {
        run_id: run.id,
        status: run.status,
        error: run.error,
    })
    .into_response())
}

async fn cancel_run(
    State(state): State<AppState>,
    Path(run_id): Path<String>,
) -> Result<axum::response::Response, ApiError> {
    if let Some(run) = state.db.get_strategy_run(&run_id).await?
        && is_terminal_run_status(&run.status)
    {
        return Ok(run_status_response(run));
    }

    if state.runtime_manager.cancel(&run_id).await {
        if let Some(run) = state.db.get_strategy_run(&run_id).await?
            && is_terminal_run_status(&run.status)
        {
            return Ok(run_status_response(run));
        }

        state
            .db
            .update_strategy_run_status(
                &run_id,
                "cancelled",
                Some(chrono::Utc::now().timestamp_millis()),
                None,
            )
            .await?;
        return get_run_status(State(state), Path(run_id)).await;
    }

    let Some(run) = state.db.get_strategy_run(&run_id).await? else {
        return Ok(StatusCode::NOT_FOUND.into_response());
    };

    Ok(run_status_response(run))
}

fn is_terminal_run_status(status: &str) -> bool {
    matches!(status, "completed" | "failed" | "cancelled" | "stopped")
}

fn run_status_response(run: storage::StrategyRunRecord) -> axum::response::Response {
    Json(RunStatusResponse {
        run_id: run.id,
        status: run.status,
        error: run.error,
    })
    .into_response()
}

async fn pause_replay(
    State(state): State<AppState>,
    Path(run_id): Path<String>,
) -> Result<Json<ReplayState>, ApiError> {
    update_replay_controller(state, run_id, "replay.pause", |controller| {
        controller.pause();
    })
    .await
}

async fn resume_replay(
    State(state): State<AppState>,
    Path(run_id): Path<String>,
) -> Result<Json<ReplayState>, ApiError> {
    update_replay_controller(state, run_id, "replay.resume", |controller| {
        controller.resume();
    })
    .await
}

async fn seek_replay(
    State(state): State<AppState>,
    Path((run_id, offset)): Path<(String, usize)>,
) -> Result<Json<ReplayState>, ApiError> {
    update_replay_controller(state, run_id, "replay.seek", |controller| {
        controller.seek(offset);
    })
    .await
}

async fn speed_replay(
    State(state): State<AppState>,
    Path((run_id, speed)): Path<(String, u32)>,
) -> Result<Json<ReplayState>, ApiError> {
    update_replay_controller(state, run_id, "replay.speed", |controller| {
        controller.set_speed(speed);
    })
    .await
}

async fn record_failed_run(
    state: &AppState,
    run_id: &str,
    error: String,
) -> storage::StorageResult<()> {
    state
        .db
        .update_strategy_run_status(
            run_id,
            "failed",
            Some(chrono::Utc::now().timestamp_millis()),
            Some(&error),
        )
        .await
}

async fn insert_event(
    db: &storage::Db,
    source: &str,
    category: &str,
    payload_json: &str,
) -> storage::StorageResult<()> {
    let payload = serde_json::from_str(payload_json)
        .unwrap_or_else(|_| serde_json::Value::String(payload_json.to_string()));
    db.record_runtime_event(storage::RuntimeEventCommand {
        ts_ms: chrono::Utc::now().timestamp_millis(),
        source: source.to_string(),
        category: category.to_string(),
        payload,
    })
    .await
}

async fn update_replay_controller(
    state: AppState,
    run_id: String,
    category: &str,
    update: impl FnOnce(&mut ReplayController),
) -> Result<Json<ReplayState>, ApiError> {
    let replay_state = {
        let mut controllers = state.replay_controllers.lock().await;
        let controller = controllers
            .entry(run_id.clone())
            .or_insert_with(|| Arc::new(Mutex::new(ReplayController::new(run_id.clone(), 1))))
            .clone();
        drop(controllers);
        let mut controller = controller.lock().await;
        update(&mut controller);
        controller.state().clone()
    };
    let payload =
        serde_json::to_string(&replay_state).map_err(|error| ApiError(anyhow::anyhow!(error)))?;
    insert_event(&state.db, &run_id, category, &payload).await?;
    Ok(Json(replay_state))
}

fn load_configured_market_slices(
    app_config: &config::AppConfig,
) -> Result<Vec<data::MarketSlice>, ApiError> {
    let inputs = configured_bar_inputs(app_config)?;
    Ok(data::load_market_slices(&inputs)?)
}

fn configured_bar_inputs(app_config: &config::AppConfig) -> Result<Vec<data::BarInput>, ApiError> {
    if app_config.data.inputs.is_empty() {
        return Ok(vec![data::BarInput::new(
            primary_strategy_symbol(app_config),
            app_config.data.source.clone(),
            app_config.data.path.clone(),
        )]);
    }

    let input_symbols = app_config
        .data
        .inputs
        .iter()
        .map(|input| input.symbol.as_str())
        .collect::<BTreeSet<_>>();
    for symbol in &app_config.strategy.symbols {
        if !input_symbols.contains(symbol.as_str()) {
            return Err(ApiError(anyhow::anyhow!(
                "missing data input for strategy symbol {symbol}"
            )));
        }
    }

    Ok(app_config
        .data
        .inputs
        .iter()
        .map(|input| {
            data::BarInput::new(
                input.symbol.clone(),
                input.source.clone(),
                input.path.clone(),
            )
        })
        .collect())
}

fn feature_manifest_input_from_bar_input(
    input: &data::BarInput,
) -> feature_store::FeatureManifestInput {
    feature_store::FeatureManifestInput {
        symbol: input.symbol.clone(),
        source: input.source.clone(),
        path: input.path.clone(),
        content_hash: None,
        bar_count: None,
        first_ts_ms: None,
        last_ts_ms: None,
    }
}

fn feature_manifest_input_from_bar_input_and_bars(
    input: &data::BarInput,
    bars: &[data::Bar],
) -> Result<feature_store::FeatureManifestInput, ApiError> {
    let mut manifest_input = feature_manifest_input_from_bar_input(input);
    manifest_input.content_hash =
        Some(stable_file_content_hash(&input.path).map_err(|error| ApiError(error.into()))?);
    manifest_input.bar_count = Some(bars.len());
    manifest_input.first_ts_ms = bars.first().map(|bar| bar.ts_ms);
    manifest_input.last_ts_ms = bars.last().map(|bar| bar.ts_ms);
    Ok(manifest_input)
}

fn stable_file_content_hash(path: &str) -> Result<String, std::io::Error> {
    let bytes = std::fs::read(path)?;
    Ok(stable_bytes_hash(&bytes))
}

fn stable_bytes_hash(bytes: &[u8]) -> String {
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("fnv1a64:{hash:016x}")
}

fn validate_feature_manifest_input_contract(
    manifest: &feature_store::FeatureManifest,
    app_config: &config::AppConfig,
) -> Result<(), ApiError> {
    let inputs = configured_bar_inputs(app_config)?;
    let mut manifest_inputs = Vec::with_capacity(inputs.len());
    for input in &inputs {
        let bars = data::load_bars(&input.source, &input.path)
            .map_err(|error| ApiError(anyhow::anyhow!(error)))?;
        manifest_inputs.push(feature_manifest_input_from_bar_input_and_bars(
            input, &bars,
        )?);
    }
    feature_store::validate_feature_manifest_for_input_contract(manifest, &manifest_inputs)
        .map_err(|error| ApiError(anyhow::anyhow!(error)))?;
    Ok(())
}

fn validate_feature_manifest_build_contract(
    manifest: &feature_store::FeatureManifest,
    indicator: Option<String>,
    period: Option<usize>,
    value_column: Option<String>,
) -> Result<(), ApiError> {
    feature_store::validate_feature_manifest_for_build_contract(
        manifest,
        &feature_store::FeatureBuildContractExpectation {
            indicator,
            value_column,
            period,
        },
    )
    .map_err(|error| ApiError(anyhow::anyhow!(error)))?;
    Ok(())
}

fn primary_strategy_symbol(app_config: &config::AppConfig) -> String {
    app_config
        .strategy
        .symbols
        .first()
        .cloned()
        .unwrap_or_else(|| "US:NASDAQ:AAPL:EQUITY".to_string())
}

fn backtest_settings_with_config_json(
    app_config: &config::AppConfig,
    config_json: String,
) -> Result<BacktestSettings, ApiError> {
    Ok(BacktestSettings {
        run_id: app_config.runtime.run_id.clone(),
        strategy_name: app_config.strategy.name.clone(),
        config_json,
        universe_name: app_config.strategy.universe.clone(),
        alpha_name: app_config.strategy.alpha.clone(),
        symbols: app_config.strategy.symbols.clone(),
        universe_filter: strategy_universe_filter(app_config)?,
        alpha_components: strategy_alpha_components(app_config),
        alpha_conflict_resolution: strategy_alpha_conflict_resolution(app_config)?,
        alpha_gate: strategy_alpha_gate(app_config)?,
        symbol: app_config
            .strategy
            .symbols
            .first()
            .cloned()
            .unwrap_or_else(|| "US:NASDAQ:AAPL:EQUITY".to_string()),
        account_id: "backtest".to_string(),
        order_qty: Decimal::from_str(&app_config.portfolio.order_qty)?,
        max_abs_qty: Decimal::from_str(&app_config.portfolio.max_abs_qty)?,
        max_exposure: Decimal::from_str(&app_config.risk.max_exposure)?,
        max_drawdown: Decimal::from_str(&app_config.risk.max_drawdown)?,
        max_leverage: Decimal::from_str(&app_config.risk.max_leverage)?,
        max_margin_used: Decimal::from_str(&app_config.risk.max_margin_used)?,
        trading_halted: app_config.risk.trading_halted,
        allow_short: app_config.effective_allow_short(),
        shortable_symbols: app_config.shortable_symbols(),
        initial_equity: Decimal::from_str(&app_config.portfolio.initial_cash)?,
        fast_window: app_config.strategy.fast_window,
        slow_window: app_config.strategy.slow_window,
    })
}

fn paper_settings(app_config: &config::AppConfig) -> Result<PaperSettings, ApiError> {
    paper_settings_with_config_json(app_config, "{}".to_string())
}

fn paper_settings_with_config_json(
    app_config: &config::AppConfig,
    config_json: String,
) -> Result<PaperSettings, ApiError> {
    Ok(PaperSettings {
        run_id: app_config.runtime.run_id.clone(),
        strategy_name: app_config.strategy.name.clone(),
        config_json,
        universe_name: app_config.strategy.universe.clone(),
        alpha_name: app_config.strategy.alpha.clone(),
        symbols: app_config.strategy.symbols.clone(),
        universe_filter: strategy_universe_filter(app_config)?,
        alpha_components: strategy_alpha_components(app_config),
        alpha_conflict_resolution: strategy_alpha_conflict_resolution(app_config)?,
        alpha_gate: strategy_alpha_gate(app_config)?,
        symbol: app_config
            .strategy
            .symbols
            .first()
            .cloned()
            .unwrap_or_else(|| "US:NASDAQ:AAPL:EQUITY".to_string()),
        account_id: app_config.paper.account_id.clone(),
        order_qty: Decimal::from_str(&app_config.portfolio.order_qty)?,
        max_abs_qty: Decimal::from_str(&app_config.portfolio.max_abs_qty)?,
        max_order_qty: Decimal::from_str(&app_config.portfolio.max_abs_qty)?,
        max_order_notional: Decimal::from_str(&app_config.risk.max_order_notional)?,
        min_cash_after_order: Decimal::from_str(&app_config.risk.min_cash_after_order)?,
        max_exposure: Decimal::from_str(&app_config.risk.max_exposure)?,
        max_drawdown: Decimal::from_str(&app_config.risk.max_drawdown)?,
        max_leverage: Decimal::from_str(&app_config.risk.max_leverage)?,
        max_margin_used: Decimal::from_str(&app_config.risk.max_margin_used)?,
        trading_halted: app_config.risk.trading_halted,
        allow_short: app_config.effective_allow_short(),
        shortable_symbols: app_config.shortable_symbols(),
        initial_cash: Decimal::from_str(&app_config.portfolio.initial_cash)?,
        base_currency: app_config.portfolio.base_currency.clone(),
        slippage_bps: Decimal::from_str(&app_config.paper.slippage_bps)?,
        fee_bps: Decimal::from_str(&app_config.paper.fee_bps)?,
        simulated_funding_rate: None,
        fast_window: app_config.strategy.fast_window,
        slow_window: app_config.strategy.slow_window,
        bar_delay_ms: app_config.paper.bar_delay_ms.unwrap_or(0),
    })
}

fn strategy_universe_filter(
    app_config: &config::AppConfig,
) -> Result<strategies::StrategyUniverseFilterConfig, ApiError> {
    Ok(strategies::StrategyUniverseFilterConfig {
        include_symbols: app_config.strategy.universe_filter.include_symbols.clone(),
        exclude_symbols: app_config.strategy.universe_filter.exclude_symbols.clone(),
        symbol_prefixes: app_config.strategy.universe_filter.symbol_prefixes.clone(),
        require_current_data: app_config.strategy.universe_filter.require_current_data,
        max_symbols: app_config.strategy.universe_filter.max_symbols,
        feature_rank: strategy_universe_rank(app_config)?,
    })
}

fn strategy_universe_rank(
    app_config: &config::AppConfig,
) -> Result<Option<strategies::StrategyUniverseRankConfig>, ApiError> {
    let Some(rank) = &app_config.strategy.universe_rank else {
        return Ok(None);
    };
    if rank.source != "parquet" {
        return Err(ApiError(anyhow::anyhow!(
            "unsupported universe rank feature source {}; expected parquet",
            rank.source
        )));
    }
    if let Some(manifest_path) = &rank.manifest_path {
        let manifest = feature_store::load_feature_manifest(manifest_path)
            .map_err(|error| ApiError(anyhow::anyhow!(error)))?;
        feature_store::validate_feature_manifest_for_contract(
            &manifest,
            &rank.path,
            &rank.run_id,
            &app_config.strategy.symbols,
            &rank.feature_name,
            rank.version.as_deref(),
        )
        .map_err(|error| ApiError(anyhow::anyhow!(error)))?;
        validate_feature_manifest_input_contract(&manifest, app_config)?;
        validate_feature_manifest_build_contract(
            &manifest,
            rank.build_indicator.clone(),
            rank.build_period,
            rank.build_value_column.clone(),
        )?;
    }
    Ok(Some(strategies::StrategyUniverseRankConfig {
        run_id: rank.run_id.clone(),
        feature_name: rank.feature_name.clone(),
        version: rank.version.clone(),
        descending: rank.descending,
        records: feature_store::load_feature_records_from_parquet(&rank.path)
            .map_err(|error| ApiError(anyhow::anyhow!(error)))?,
    }))
}

fn strategy_alpha_components(
    app_config: &config::AppConfig,
) -> Vec<strategies::StrategyAlphaComponentConfig> {
    app_config
        .strategy
        .alpha_components
        .iter()
        .map(|component| strategies::StrategyAlphaComponentConfig {
            name: component.name.clone(),
            category: component.category.clone(),
            fast_window: component.fast_window,
            slow_window: component.slow_window,
            weight: component.weight,
        })
        .collect()
}

fn strategy_alpha_conflict_resolution(
    app_config: &config::AppConfig,
) -> Result<strategies::StrategyAlphaConflictResolution, ApiError> {
    match app_config.strategy.alpha_conflict_resolution.as_str() {
        "highest_confidence" => Ok(strategies::StrategyAlphaConflictResolution::HighestConfidence),
        "net_signal" => Ok(strategies::StrategyAlphaConflictResolution::NetSignal),
        "majority_vote" => Ok(strategies::StrategyAlphaConflictResolution::MajorityVote),
        "category_majority" => Ok(strategies::StrategyAlphaConflictResolution::CategoryMajority),
        other => Err(ApiError(anyhow::anyhow!(
            "unknown alpha conflict resolution {other}"
        ))),
    }
}

fn strategy_alpha_gate(
    app_config: &config::AppConfig,
) -> Result<Option<strategies::StrategyAlphaGateConfig>, ApiError> {
    let Some(gate) = &app_config.strategy.alpha_gate else {
        return Ok(None);
    };
    if gate.source != "parquet" {
        return Err(ApiError(anyhow::anyhow!(
            "unsupported alpha gate feature source {}; expected parquet",
            gate.source
        )));
    }
    if let Some(manifest_path) = &gate.manifest_path {
        let manifest = feature_store::load_feature_manifest(manifest_path)
            .map_err(|error| ApiError(anyhow::anyhow!(error)))?;
        feature_store::validate_feature_manifest_for_contract(
            &manifest,
            &gate.path,
            &gate.run_id,
            &app_config.strategy.symbols,
            &gate.feature_name,
            gate.version.as_deref(),
        )
        .map_err(|error| ApiError(anyhow::anyhow!(error)))?;
        validate_feature_manifest_input_contract(&manifest, app_config)?;
        validate_feature_manifest_build_contract(
            &manifest,
            gate.build_indicator.clone(),
            gate.build_period,
            gate.build_value_column.clone(),
        )?;
    }
    Ok(Some(strategies::StrategyAlphaGateConfig {
        run_id: gate.run_id.clone(),
        feature_name: gate.feature_name.clone(),
        version: gate.version.clone(),
        min_value: gate
            .min_value
            .as_deref()
            .map(Decimal::from_str)
            .transpose()?,
        max_value: gate
            .max_value
            .as_deref()
            .map(Decimal::from_str)
            .transpose()?,
        records: feature_store::load_feature_records_from_parquet(&gate.path)
            .map_err(|error| ApiError(anyhow::anyhow!(error)))?,
    }))
}

fn broker_kind(kind: config::BrokerKind) -> BrokerKind {
    match kind {
        config::BrokerKind::Simulated => BrokerKind::Simulated,
        config::BrokerKind::Futu => BrokerKind::Futu,
        config::BrokerKind::Binance => BrokerKind::Binance,
        config::BrokerKind::Okx => BrokerKind::Okx,
        config::BrokerKind::InteractiveBrokers => BrokerKind::InteractiveBrokers,
    }
}

fn broker_kind_slug(kind: config::BrokerKind) -> &'static str {
    match kind {
        config::BrokerKind::Simulated => "simulated",
        config::BrokerKind::Futu => "futu",
        config::BrokerKind::Binance => "binance",
        config::BrokerKind::Okx => "okx",
        config::BrokerKind::InteractiveBrokers => "ibkr",
    }
}

fn broker_mode_slug(mode: config::BrokerMode) -> &'static str {
    match mode {
        config::BrokerMode::Paper => "paper",
        config::BrokerMode::Live => "live",
    }
}

async fn paper_real_broker_connection_ready(
    app_config: &config::AppConfig,
) -> Result<bool, ApiError> {
    match app_config.broker.kind {
        config::BrokerKind::Simulated => Ok(false),
        config::BrokerKind::Binance => {
            let base_url = app_config.broker.base_url.as_deref().unwrap_or_default();
            if !base_url.contains("testnet.binance.vision") {
                return Err(ApiError(anyhow::anyhow!(
                    "Binance paper preflight requires Spot testnet base_url"
                )));
            }
            let api_key_env = app_config
                .broker
                .api_key_env
                .as_deref()
                .unwrap_or("BINANCE_TESTNET_API_KEY");
            let secret_key_env = app_config
                .broker
                .secret_key_env
                .as_deref()
                .unwrap_or("BINANCE_TESTNET_SECRET_KEY");
            std::env::var(api_key_env).map_err(|_| {
                ApiError(anyhow::anyhow!(
                    "missing Binance testnet API key env {api_key_env}"
                ))
            })?;
            std::env::var(secret_key_env).map_err(|_| {
                ApiError(anyhow::anyhow!(
                    "missing Binance testnet secret key env {secret_key_env}"
                ))
            })?;
            Ok(true)
        }
        config::BrokerKind::InteractiveBrokers => {
            if !app_config.broker.order_submit_enabled {
                return Ok(false);
            }
            let adapter =
                IbkrPaperGatewayAdapter::try_new(ibkr_paper_gateway_settings(app_config)?)
                    .map_err(|error| ApiError(anyhow::anyhow!(error)))?;
            adapter
                .validate_paper_account(&app_config.paper.account_id)
                .await
                .map_err(|error| ApiError(anyhow::anyhow!(error)))?;
            Ok(true)
        }
        config::BrokerKind::Futu | config::BrokerKind::Okx => Ok(false),
    }
}

async fn paper_runtime(
    app_config: &config::AppConfig,
    db: storage::Db,
    settings: PaperSettings,
) -> Result<PaperRuntime, ApiError> {
    if !app_config.broker.order_submit_enabled {
        return Ok(PaperRuntime::new(db, settings));
    }
    if app_config.runtime.mode != config::RuntimeMode::Paper {
        return Err(ApiError(anyhow::anyhow!(
            "broker order submit requires runtime.mode = paper"
        )));
    }
    if app_config.broker.mode != config::BrokerMode::Paper {
        return Err(ApiError(anyhow::anyhow!(
            "broker order submit requires broker.mode = paper"
        )));
    }
    match app_config.broker.kind {
        config::BrokerKind::Binance => {
            let adapter = BinanceSpotTestnetAdapter::try_new(binance_testnet_settings(app_config)?)
                .map_err(|error| ApiError(anyhow::anyhow!(error)))?;
            let account = adapter
                .account_snapshot(&app_config.paper.account_id)
                .await
                .map_err(|error| ApiError(anyhow::anyhow!(error)))?;
            Ok(PaperRuntime::new_with_executor(
                db,
                settings_with_broker_initial_cash(settings, account.cash),
                Box::new(BinancePaperOrderExecutor::new_with_client_order_prefix(
                    adapter,
                    app_config.runtime.run_id.clone(),
                )),
            ))
        }
        config::BrokerKind::InteractiveBrokers => {
            let adapter =
                IbkrPaperGatewayAdapter::try_new(ibkr_paper_gateway_settings(app_config)?)
                    .map_err(|error| ApiError(anyhow::anyhow!(error)))?;
            adapter
                .validate_paper_account(&app_config.paper.account_id)
                .await
                .map_err(|error| ApiError(anyhow::anyhow!(error)))?;
            Ok(PaperRuntime::new_with_executor(
                db,
                settings,
                Box::new(IbkrPaperOrderExecutor::new_with_client_order_prefix(
                    IbkrPaperGatewayOrderClient::new(adapter, app_config.paper.account_id.clone()),
                    app_config.runtime.run_id.clone(),
                )),
            ))
        }
        config::BrokerKind::Simulated | config::BrokerKind::Futu | config::BrokerKind::Okx => {
            Err(ApiError(anyhow::anyhow!(
                "paper-run broker order submit only supports Binance Spot Testnet and IBKR paper in this phase"
            )))
        }
    }
}

fn settings_with_broker_initial_cash(
    mut settings: PaperSettings,
    broker_cash: Decimal,
) -> PaperSettings {
    settings.initial_cash = broker_cash;
    settings
}

fn binance_testnet_settings(
    app_config: &config::AppConfig,
) -> Result<BinanceSpotTestnetSettings, ApiError> {
    let api_key_env = app_config
        .broker
        .api_key_env
        .as_deref()
        .unwrap_or("BINANCE_TESTNET_API_KEY");
    let secret_key_env = app_config
        .broker
        .secret_key_env
        .as_deref()
        .unwrap_or("BINANCE_TESTNET_SECRET_KEY");
    let api_key = std::env::var(api_key_env).map_err(|_| {
        ApiError(anyhow::anyhow!(
            "missing Binance testnet API key env {api_key_env}"
        ))
    })?;
    let secret_key = std::env::var(secret_key_env).map_err(|_| {
        ApiError(anyhow::anyhow!(
            "missing Binance testnet secret key env {secret_key_env}"
        ))
    })?;

    Ok(BinanceSpotTestnetSettings {
        base_url: app_config
            .broker
            .base_url
            .clone()
            .unwrap_or_else(|| "https://testnet.binance.vision/api".to_string()),
        api_key,
        secret_key,
        recv_window_ms: app_config.broker.recv_window_ms.unwrap_or(5000),
    })
}

fn ibkr_paper_gateway_settings(
    app_config: &config::AppConfig,
) -> Result<IbkrPaperGatewaySettings, ApiError> {
    Ok(IbkrPaperGatewaySettings {
        host: app_config
            .broker
            .host
            .clone()
            .unwrap_or_else(|| "127.0.0.1".to_string()),
        port: app_config.broker.port.unwrap_or(7497),
        client_id: app_config.broker.client_id.unwrap_or(1),
        connect_timeout: std::time::Duration::from_secs(2),
    })
}

struct ApiError(anyhow::Error);

impl axum::response::IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        let status = match self.0.downcast_ref::<storage::StorageError>() {
            Some(storage::StorageError::Protocol(_)) => StatusCode::BAD_REQUEST,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        };
        (
            status,
            Json(ErrorResponse {
                error: self.0.to_string(),
            }),
        )
            .into_response()
    }
}

#[derive(Serialize)]
struct ErrorResponse {
    error: String,
}

impl From<anyhow::Error> for ApiError {
    fn from(error: anyhow::Error) -> Self {
        Self(error)
    }
}

impl From<config::ConfigError> for ApiError {
    fn from(error: config::ConfigError) -> Self {
        Self(error.into())
    }
}

impl From<data::DataError> for ApiError {
    fn from(error: data::DataError) -> Self {
        Self(error.into())
    }
}

impl From<data::ingestion::IngestionError> for ApiError {
    fn from(error: data::ingestion::IngestionError) -> Self {
        Self(error.into())
    }
}

impl From<rust_decimal::Error> for ApiError {
    fn from(error: rust_decimal::Error) -> Self {
        Self(error.into())
    }
}

impl From<storage::StorageError> for ApiError {
    fn from(error: storage::StorageError) -> Self {
        Self(error.into())
    }
}
