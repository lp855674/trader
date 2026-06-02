#![forbid(unsafe_code)]

mod state;

use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    routing::{get, post},
};
use backtest::{BacktestRuntime, BacktestSettings};
use rust_decimal::Decimal;
use serde::Serialize;
use std::str::FromStr;

pub use state::AppState;

#[derive(Serialize)]
struct HealthResponse {
    status: &'static str,
}

pub fn router() -> Router {
    Router::new().route("/api/v1/health", get(health))
}

pub fn router_with_state(state: AppState) -> Router {
    Router::new()
        .route("/api/v1/health", get(health))
        .route("/api/v1/backtests", post(run_backtest))
        .route("/api/v1/orders", get(list_orders))
        .route("/api/v1/positions", get(list_positions))
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

async fn list_orders(
    State(state): State<AppState>,
) -> Result<Json<Vec<storage::NewOrder>>, ApiError> {
    let app_config = config::AppConfig::from_toml_file(&state.config_path)?;
    Ok(Json(
        state.db.list_orders(&app_config.runtime.run_id).await?,
    ))
}

async fn list_positions(
    State(state): State<AppState>,
) -> Result<Json<Vec<storage::NewPosition>>, ApiError> {
    let app_config = config::AppConfig::from_toml_file(&state.config_path)?;
    Ok(Json(
        state.db.list_positions(&app_config.runtime.run_id).await?,
    ))
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
