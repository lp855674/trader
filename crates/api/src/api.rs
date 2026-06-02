#![forbid(unsafe_code)]

mod state;

use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
};
use backtest::{BacktestRuntime, BacktestSettings};
use metrics::{MetricsSummary, paper_summary};
use paper::{PaperRuntime, PaperSettings};
use rust_decimal::Decimal;
use serde::Serialize;
use std::str::FromStr;

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

pub fn router() -> Router {
    Router::new().route("/api/v1/health", get(health))
}

pub fn router_with_state(state: AppState) -> Router {
    Router::new()
        .route("/api/v1/health", get(health))
        .route("/api/v1/backtests", post(run_backtest))
        .route("/api/v1/paper-runs", post(run_paper))
        .route("/api/v1/orders", get(list_orders))
        .route("/api/v1/fills", get(list_fills))
        .route("/api/v1/positions", get(list_positions))
        .route("/api/v1/account-balances", get(list_account_balances))
        .route("/api/v1/portfolio/snapshots", get(list_portfolio_snapshots))
        .route("/api/v1/metrics", get(metrics_summary))
        .route("/api/v1/runs", get(list_runs))
        .route("/api/v1/runs/{run_id}", get(get_run))
        .route("/api/v1/runs/{run_id}/status", get(get_run_status))
        .route("/api/v1/runs/{run_id}/cancel", post(cancel_run))
        .with_state(state)
}

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse { status: "ok" })
}

async fn run_backtest(
    State(state): State<AppState>,
) -> Result<(StatusCode, Json<backtest::BacktestSummary>), ApiError> {
    let app_config = config::AppConfig::from_toml_file(&state.config_path)?;
    let bars = data::load_bars_from_csv(&app_config.data.path)?;
    let summary = BacktestRuntime::new(state.db.clone(), backtest_settings(&app_config)?)
        .run(bars)
        .await?;
    Ok((StatusCode::CREATED, Json(summary)))
}

async fn run_paper(
    State(state): State<AppState>,
) -> Result<(StatusCode, Json<backtest::BacktestSummary>), ApiError> {
    let app_config = config::AppConfig::from_toml_file(&state.config_path)?;
    let settings = paper_settings(&app_config)?;
    let started_at_ms = chrono::Utc::now().timestamp_millis();

    state
        .db
        .insert_strategy_run(storage::NewStrategyRun {
            id: settings.run_id.clone(),
            name: settings.strategy_name.clone(),
            mode: "paper".to_string(),
            status: "running".to_string(),
            started_at_ms,
            ended_at_ms: None,
            error: None,
            config_json: "{}".to_string(),
        })
        .await?;

    let bars = match data::load_bars_from_csv(&app_config.data.path) {
        Ok(bars) => bars,
        Err(error) => {
            record_failed_run(&state, &settings.run_id, error.to_string()).await?;
            return Err(error.into());
        }
    };

    let summary = match PaperRuntime::new(state.db.clone(), settings.clone())
        .run_bars(bars)
        .await
    {
        Ok(summary) => summary,
        Err(error) => {
            record_failed_run(&state, &settings.run_id, error.to_string()).await?;
            return Err(error.into());
        }
    };
    Ok((StatusCode::CREATED, Json(summary)))
}

async fn list_orders(
    State(state): State<AppState>,
) -> Result<Json<Vec<storage::NewOrder>>, ApiError> {
    let app_config = config::AppConfig::from_toml_file(&state.config_path)?;
    Ok(Json(
        state.db.list_orders(&app_config.runtime.run_id).await?,
    ))
}

async fn list_fills(
    State(state): State<AppState>,
) -> Result<Json<Vec<storage::NewFill>>, ApiError> {
    let app_config = config::AppConfig::from_toml_file(&state.config_path)?;
    Ok(Json(state.db.list_fills(&app_config.runtime.run_id).await?))
}

async fn list_positions(
    State(state): State<AppState>,
) -> Result<Json<Vec<storage::NewPosition>>, ApiError> {
    let app_config = config::AppConfig::from_toml_file(&state.config_path)?;
    Ok(Json(
        state.db.list_positions(&app_config.runtime.run_id).await?,
    ))
}

async fn list_account_balances(
    State(state): State<AppState>,
) -> Result<Json<Vec<storage::NewAccountBalance>>, ApiError> {
    let app_config = config::AppConfig::from_toml_file(&state.config_path)?;
    Ok(Json(
        state
            .db
            .list_account_balances(&app_config.runtime.run_id)
            .await?,
    ))
}

async fn list_portfolio_snapshots(
    State(state): State<AppState>,
) -> Result<Json<Vec<storage::NewPortfolioSnapshot>>, ApiError> {
    let app_config = config::AppConfig::from_toml_file(&state.config_path)?;
    Ok(Json(
        state
            .db
            .list_portfolio_snapshots(&app_config.runtime.run_id)
            .await?,
    ))
}

async fn metrics_summary(State(state): State<AppState>) -> Result<Json<MetricsSummary>, ApiError> {
    let app_config = config::AppConfig::from_toml_file(&state.config_path)?;
    let run_id = &app_config.runtime.run_id;
    let orders = state.db.list_orders(run_id).await?;
    let fills = state.db.list_fills(run_id).await?;
    let snapshots = state.db.list_portfolio_snapshots(run_id).await?;
    let Some(first_snapshot) = snapshots.first() else {
        return Ok(Json(MetricsSummary {
            total_return: Decimal::ZERO.to_string(),
            order_count: orders.len(),
            fill_count: fills.len(),
        }));
    };
    let last_snapshot = snapshots.last().unwrap_or(first_snapshot);
    let initial_equity = Decimal::from_str(&first_snapshot.equity)?;
    let final_equity = Decimal::from_str(&last_snapshot.equity)?;

    Ok(Json(paper_summary(
        orders.len(),
        fills.len(),
        initial_equity,
        final_equity,
    )))
}

async fn list_runs(
    State(state): State<AppState>,
) -> Result<Json<Vec<storage::StrategyRunRecord>>, ApiError> {
    Ok(Json(state.db.list_strategy_runs().await?))
}

async fn get_run(
    State(state): State<AppState>,
    Path(run_id): Path<String>,
) -> Result<axum::response::Response, ApiError> {
    let Some(run) = state.db.get_strategy_run(&run_id).await? else {
        return Ok(StatusCode::NOT_FOUND.into_response());
    };
    Ok(Json(run).into_response())
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
    if state.db.get_strategy_run(&run_id).await?.is_none() {
        return Ok(StatusCode::NOT_FOUND.into_response());
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

    get_run_status(State(state), Path(run_id)).await
}

async fn record_failed_run(
    state: &AppState,
    run_id: &str,
    error: String,
) -> Result<(), sqlx::Error> {
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

fn backtest_settings(app_config: &config::AppConfig) -> Result<BacktestSettings, ApiError> {
    Ok(BacktestSettings {
        run_id: app_config.runtime.run_id.clone(),
        strategy_name: app_config.strategy.name.clone(),
        symbol: app_config
            .strategy
            .symbols
            .first()
            .cloned()
            .unwrap_or_else(|| "US:NASDAQ:AAPL:EQUITY".to_string()),
        account_id: "backtest".to_string(),
        order_qty: Decimal::from_str(&app_config.portfolio.order_qty)?,
        max_abs_qty: Decimal::from_str(&app_config.portfolio.max_abs_qty)?,
    })
}

fn paper_settings(app_config: &config::AppConfig) -> Result<PaperSettings, ApiError> {
    Ok(PaperSettings {
        run_id: app_config.runtime.run_id.clone(),
        strategy_name: app_config.strategy.name.clone(),
        symbol: app_config
            .strategy
            .symbols
            .first()
            .cloned()
            .unwrap_or_else(|| "US:NASDAQ:AAPL:EQUITY".to_string()),
        account_id: app_config.paper.account_id.clone(),
        order_qty: Decimal::from_str(&app_config.portfolio.order_qty)?,
        max_abs_qty: Decimal::from_str(&app_config.portfolio.max_abs_qty)?,
        initial_cash: Decimal::from_str(&app_config.portfolio.initial_cash)?,
        base_currency: app_config.portfolio.base_currency.clone(),
        slippage_bps: Decimal::from_str(&app_config.paper.slippage_bps)?,
        fee_bps: Decimal::from_str(&app_config.paper.fee_bps)?,
        fast_window: app_config.strategy.fast_window,
        slow_window: app_config.strategy.slow_window,
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

impl From<sqlx::Error> for ApiError {
    fn from(error: sqlx::Error) -> Self {
        Self(error.into())
    }
}

impl From<rust_decimal::Error> for ApiError {
    fn from(error: rust_decimal::Error) -> Self {
        Self(error.into())
    }
}
