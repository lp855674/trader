use std::collections::BTreeMap;
use std::str::FromStr;

use serde::Deserialize;

use crate::ingestion::{CorporateAction, IngestionError};

const YAHOO_CHART_URL_PREFIX: &str = "https://query1.finance.yahoo.com/v8/finance/chart";

#[derive(Debug, Deserialize)]
struct YahooChartResponse {
    chart: YahooChart,
}

#[derive(Debug, Deserialize)]
struct YahooChart {
    result: Option<Vec<YahooChartResult>>,
}

#[derive(Debug, Deserialize)]
struct YahooChartResult {
    events: Option<YahooEvents>,
}

#[derive(Debug, Deserialize)]
struct YahooEvents {
    dividends: Option<BTreeMap<String, YahooDividend>>,
    splits: Option<BTreeMap<String, YahooSplit>>,
}

#[derive(Debug, Deserialize)]
struct YahooDividend {
    amount: serde_json::Value,
    date: i64,
}

#[derive(Debug, Deserialize)]
struct YahooSplit {
    date: i64,
    numerator: Option<serde_json::Value>,
    denominator: Option<serde_json::Value>,
    #[serde(rename = "splitRatio")]
    split_ratio: Option<String>,
}

pub async fn fetch_yahoo_corporate_actions(
    client: &reqwest::Client,
    symbol: &str,
) -> Result<Vec<CorporateAction>, IngestionError> {
    let period2 = chrono::Utc::now().timestamp();
    let payload = client
        .get(format!("{YAHOO_CHART_URL_PREFIX}/{symbol}"))
        .query(&[
            ("period1", "0".to_string()),
            ("period2", period2.to_string()),
            ("events", "div,splits".to_string()),
        ])
        .send()
        .await?
        .error_for_status()?
        .text()
        .await?;
    parse_yahoo_corporate_actions(symbol, &payload, chrono::Utc::now().timestamp_millis())
}

pub async fn ingest_yahoo_corporate_actions(
    db: &storage::Db,
    client: &reqwest::Client,
    symbol: &str,
) -> Result<crate::ingestion::IngestionResult, IngestionError> {
    let actions = fetch_yahoo_corporate_actions(client, symbol).await?;
    let rows_fetched = actions.len();
    for action in actions {
        db.record_corporate_action_meta(storage::CorporateActionMetaCommand {
            market: action.market,
            exchange: action.exchange,
            symbol: action.symbol,
            action_type: action.action_type,
            ex_date_ms: action.ex_date_ms,
            record_date_ms: action.record_date_ms,
            payable_date_ms: action.payable_date_ms,
            ratio: action.ratio,
            cash_amount: action
                .cash_amount
                .as_deref()
                .map(parse_decimal)
                .transpose()?,
            currency: action.currency,
            source: action.source,
            created_at_ms: action.created_at_ms,
            updated_at_ms: action.updated_at_ms,
        })
        .await?;
    }

    Ok(crate::ingestion::IngestionResult {
        source: "yahoo".to_string(),
        table: "corporate_actions_meta".to_string(),
        rows_fetched,
        rows_upserted: rows_fetched,
    })
}

pub fn parse_yahoo_corporate_actions(
    symbol: &str,
    payload: &str,
    fetched_at_ms: i64,
) -> Result<Vec<CorporateAction>, IngestionError> {
    let response = serde_json::from_str::<YahooChartResponse>(payload)?;
    let mut actions = Vec::new();
    let Some(results) = response.chart.result else {
        return Ok(actions);
    };

    for result in results {
        let Some(events) = result.events else {
            continue;
        };
        if let Some(dividends) = events.dividends {
            actions.extend(dividends.into_values().map(|dividend| CorporateAction {
                market: "US".to_string(),
                exchange: "YAHOO".to_string(),
                symbol: symbol.to_string(),
                action_type: "DIVIDEND".to_string(),
                ex_date_ms: seconds_to_millis(dividend.date),
                record_date_ms: None,
                payable_date_ms: None,
                ratio: None,
                cash_amount: Some(json_scalar_to_string(&dividend.amount)),
                currency: Some("USD".to_string()),
                source: Some("yahoo_chart".to_string()),
                created_at_ms: fetched_at_ms,
                updated_at_ms: fetched_at_ms,
            }));
        }
        if let Some(splits) = events.splits {
            actions.extend(splits.into_values().map(|split| CorporateAction {
                market: "US".to_string(),
                exchange: "YAHOO".to_string(),
                symbol: symbol.to_string(),
                action_type: "SPLIT".to_string(),
                ex_date_ms: seconds_to_millis(split.date),
                record_date_ms: None,
                payable_date_ms: None,
                ratio: split.split_ratio.clone().or_else(|| split_ratio(&split)),
                cash_amount: None,
                currency: None,
                source: Some("yahoo_chart".to_string()),
                created_at_ms: fetched_at_ms,
                updated_at_ms: fetched_at_ms,
            }));
        }
    }

    actions.sort_by(|left, right| {
        left.ex_date_ms
            .cmp(&right.ex_date_ms)
            .then_with(|| left.action_type.cmp(&right.action_type))
    });
    Ok(actions)
}

fn seconds_to_millis(seconds: i64) -> i64 {
    seconds * 1000
}

fn split_ratio(split: &YahooSplit) -> Option<String> {
    Some(format!(
        "{}:{}",
        json_scalar_to_string(split.numerator.as_ref()?),
        json_scalar_to_string(split.denominator.as_ref()?)
    ))
}

fn json_scalar_to_string(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(value) => value.clone(),
        other => other.to_string(),
    }
}

fn parse_decimal(value: &str) -> Result<rust_decimal::Decimal, IngestionError> {
    rust_decimal::Decimal::from_str(value).map_err(|source| IngestionError::Decimal {
        value: value.to_string(),
        source,
    })
}
