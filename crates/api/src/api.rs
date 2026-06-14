#![forbid(unsafe_code)]

mod state;
mod ws;

use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
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
use runtime::{LiveRuntime, LiveRuntimeSettings};
use rust_decimal::Decimal;
use serde::Serialize;
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
        .route("/api/v1/account-balances", get(list_account_balances))
        .route("/api/v1/portfolio/snapshots", get(list_portfolio_snapshots))
        .route("/api/v1/metrics", get(metrics_summary))
        .route("/api/v1/brokers/status", get(broker_status))
        .route("/api/v1/brokers/account/{account_id}", get(broker_account))
        .route("/api/v1/runs", get(list_runs))
        .route("/api/v1/runs/{run_id}", get(get_run))
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

async fn run_backtest(
    State(state): State<AppState>,
) -> Result<(StatusCode, Json<backtest::BacktestSummary>), ApiError> {
    let app_config = config::AppConfig::from_toml_file(&state.config_path)?;
    insert_event(
        &state.db,
        &app_config.runtime.run_id,
        "backtest.started",
        &serde_json::json!({ "run_id": &app_config.runtime.run_id }).to_string(),
    )
    .await?;
    let market_slices = load_configured_market_slices(&app_config)?;
    let summary = BacktestRuntime::new(state.db.clone(), backtest_settings(&app_config)?)
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
    Ok((StatusCode::CREATED, Json(summary)))
}

async fn run_paper(
    State(state): State<AppState>,
) -> Result<(StatusCode, Json<RunStartResponse>), ApiError> {
    let app_config = config::AppConfig::from_toml_file(&state.config_path)?;
    let settings = paper_settings(&app_config)?;
    let started_at_ms = chrono::Utc::now().timestamp_millis();

    state
        .db
        .start_strategy_run(storage::StrategyRunStartCommand {
            run_id: settings.run_id.clone(),
            name: settings.strategy_name.clone(),
            mode: "paper".to_string(),
            started_at_ms,
            config: serde_json::json!({}),
        })
        .await?;
    insert_event(
        &state.db,
        &settings.run_id,
        "paper.started",
        &serde_json::json!({ "run_id": &settings.run_id }).to_string(),
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
    state
        .db
        .start_strategy_run(storage::StrategyRunStartCommand {
            run_id: app_config.runtime.run_id.clone(),
            name: app_config.strategy.name.clone(),
            mode: "replay".to_string(),
            started_at_ms,
            config: serde_json::json!({}),
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

    Ok((StatusCode::CREATED, Json(summary)))
}

async fn start_live_run(
    State(state): State<AppState>,
) -> Result<(StatusCode, Json<RunStartResponse>), ApiError> {
    let app_config = config::AppConfig::from_toml_file(&state.config_path)?;
    let run_id = app_config.runtime.run_id.clone();
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
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in bytes {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    Ok(format!("fnv1a64:{hash:016x}"))
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

fn backtest_settings(app_config: &config::AppConfig) -> Result<BacktestSettings, ApiError> {
    Ok(BacktestSettings {
        run_id: app_config.runtime.run_id.clone(),
        strategy_name: app_config.strategy.name.clone(),
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
    Ok(PaperSettings {
        run_id: app_config.runtime.run_id.clone(),
        strategy_name: app_config.strategy.name.clone(),
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
        (
            StatusCode::INTERNAL_SERVER_ERROR,
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
