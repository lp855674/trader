pub mod binance_funding;
pub mod binance_meta;
pub mod corporate_actions;
pub mod tracker;

use std::time::Instant;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CryptoMarketMeta {
    pub exchange: String,
    pub symbol: String,
    pub base_asset: String,
    pub quote_asset: String,
    pub instrument_type: String,
    pub contract_type: Option<String>,
    pub contract_size: Option<String>,
    pub settlement_asset: Option<String>,
    pub min_notional: Option<String>,
    pub min_qty: Option<String>,
    pub max_qty: Option<String>,
    pub price_precision: Option<i64>,
    pub qty_precision: Option<i64>,
    pub price_tick: Option<String>,
    pub qty_step: Option<String>,
    pub maker_fee_rate: Option<String>,
    pub taker_fee_rate: Option<String>,
    pub funding_interval_hours: Option<i64>,
    pub max_leverage: Option<String>,
    pub margin_modes: Option<Vec<String>>,
    pub is_inverse: bool,
    pub is_active: bool,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FundingRate {
    pub id: String,
    pub exchange: String,
    pub symbol: String,
    pub funding_time_ms: i64,
    pub funding_rate: String,
    pub mark_price: Option<String>,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CorporateAction {
    pub market: String,
    pub exchange: String,
    pub symbol: String,
    pub action_type: String,
    pub ex_date_ms: i64,
    pub record_date_ms: Option<i64>,
    pub payable_date_ms: Option<i64>,
    pub ratio: Option<String>,
    pub cash_amount: Option<String>,
    pub currency: Option<String>,
    pub source: Option<String>,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

#[derive(Debug, thiserror::Error)]
pub enum IngestionError {
    #[error("failed to fetch reference data: {0}")]
    Http(#[from] reqwest::Error),
    #[error("failed to parse reference data: {0}")]
    Json(#[from] serde_json::Error),
    #[error("failed to parse decimal value {value}: {source}")]
    Decimal {
        value: String,
        source: rust_decimal::Error,
    },
    #[error("failed to persist reference data: {0}")]
    Storage(#[from] storage::StorageError),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IngestionResult {
    pub source: String,
    pub table: String,
    pub rows_fetched: usize,
    pub rows_upserted: usize,
}

pub async fn run_scheduled_ingestion(
    db: &storage::Db,
    client: &reqwest::Client,
    config: &config::IngestionConfig,
) -> Result<Vec<IngestionResult>, IngestionError> {
    if !config.enabled {
        return Ok(Vec::new());
    }

    let mut results = Vec::new();
    for source in &config.sources {
        match source.as_str() {
            "binance" => {
                let started = Instant::now();
                let result = binance_meta::ingest_binance_market_meta(db, client).await?;
                tracker::IngestionTracker::log_ingestion(db, &result, elapsed_millis_i64(started))
                    .await?;
                results.push(result);

                for symbol in &config.symbols {
                    let started = Instant::now();
                    let result =
                        binance_funding::ingest_binance_funding_rates(db, client, symbol).await?;
                    tracker::IngestionTracker::log_ingestion(
                        db,
                        &result,
                        elapsed_millis_i64(started),
                    )
                    .await?;
                    results.push(result);
                }
            }
            "yahoo" => {
                for symbol in &config.symbols {
                    let started = Instant::now();
                    let result =
                        corporate_actions::ingest_yahoo_corporate_actions(db, client, symbol)
                            .await?;
                    tracker::IngestionTracker::log_ingestion(
                        db,
                        &result,
                        elapsed_millis_i64(started),
                    )
                    .await?;
                    results.push(result);
                }
            }
            _ => {}
        }
    }

    Ok(results)
}

fn elapsed_millis_i64(started: Instant) -> i64 {
    i64::try_from(started.elapsed().as_millis()).unwrap_or(i64::MAX)
}
