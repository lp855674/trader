use serde::Deserialize;
use std::str::FromStr;

use crate::ingestion::{FundingRate, IngestionError, IngestionResult, http_retry};

const BINANCE_FUNDING_RATE_URL: &str = "https://fapi.binance.com/fapi/v1/fundingRate";

#[derive(Debug, Deserialize)]
struct BinanceFundingRate {
    symbol: String,
    #[serde(rename = "fundingRate")]
    funding_rate: String,
    #[serde(rename = "fundingTime")]
    funding_time_ms: i64,
    #[serde(rename = "markPrice")]
    mark_price: Option<String>,
}

pub async fn fetch_binance_funding_history(
    client: &reqwest::Client,
    symbol: &str,
    start_ms: Option<i64>,
    end_ms: Option<i64>,
    limit: Option<u32>,
) -> Result<Vec<FundingRate>, IngestionError> {
    let mut query = vec![("symbol", symbol.to_string())];
    if let Some(start_ms) = start_ms {
        query.push(("startTime", start_ms.to_string()));
    }
    if let Some(end_ms) = end_ms {
        query.push(("endTime", end_ms.to_string()));
    }
    if let Some(limit) = limit {
        query.push(("limit", limit.to_string()));
    }

    let payload = http_retry::get_text_with_retry(client, BINANCE_FUNDING_RATE_URL, &query).await?;
    parse_binance_funding_history(&payload)
}

pub async fn ingest_binance_funding_rates(
    db: &storage::Db,
    client: &reqwest::Client,
    symbol: &str,
) -> Result<IngestionResult, IngestionError> {
    let latest_seen_ms = db
        .get_latest_funding_rate("BINANCE", symbol)
        .await?
        .map(|rate| rate.funding_time_ms);
    let start_ms = latest_seen_ms.map(|timestamp| timestamp + 1);
    let rates = fetch_binance_funding_history(client, symbol, start_ms, None, Some(1000)).await?;
    let rows_fetched = rates.len();
    let rates = filter_funding_rates_after(rates, latest_seen_ms);
    let rows_upserted = rates.len();

    for rate in rates {
        db.record_funding_rate(storage::FundingRateCommand {
            id: rate.id,
            exchange: rate.exchange,
            symbol: rate.symbol,
            funding_time_ms: rate.funding_time_ms,
            funding_rate: parse_decimal(&rate.funding_rate)?,
            mark_price: rate.mark_price.as_deref().map(parse_decimal).transpose()?,
            source: rate.source,
        })
        .await?;
    }

    Ok(IngestionResult {
        source: "binance".to_string(),
        table: "funding_rates".to_string(),
        rows_fetched,
        rows_upserted,
    })
}

pub fn parse_binance_funding_history(payload: &str) -> Result<Vec<FundingRate>, IngestionError> {
    let rates = serde_json::from_str::<Vec<BinanceFundingRate>>(payload)?;
    Ok(rates
        .into_iter()
        .map(|rate| FundingRate {
            id: format!("binance-{}-{}", rate.symbol, rate.funding_time_ms),
            exchange: "BINANCE".to_string(),
            symbol: rate.symbol,
            funding_time_ms: rate.funding_time_ms,
            funding_rate: rate.funding_rate,
            mark_price: rate.mark_price,
            source: "binance_fapi_fundingRate".to_string(),
        })
        .collect())
}

fn parse_decimal(value: &str) -> Result<rust_decimal::Decimal, IngestionError> {
    rust_decimal::Decimal::from_str(value).map_err(|source| IngestionError::Decimal {
        value: value.to_string(),
        source,
    })
}

pub fn filter_funding_rates_after(
    rates: Vec<FundingRate>,
    latest_seen_ms: Option<i64>,
) -> Vec<FundingRate> {
    let Some(latest_seen_ms) = latest_seen_ms else {
        return rates;
    };
    rates
        .into_iter()
        .filter(|rate| rate.funding_time_ms > latest_seen_ms)
        .collect()
}
