use std::collections::BTreeMap;

use serde::Deserialize;

use crate::ingestion::{IngestionError, IngestionResult};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IngestionStatus {
    pub source: String,
    pub table: String,
    pub rows_fetched: usize,
    pub rows_upserted: usize,
    pub duration_ms: i64,
    pub ts_ms: i64,
    pub age_ms: i64,
    pub stale_after_ms: i64,
    pub is_stale: bool,
}

#[derive(Debug, Deserialize)]
struct IngestionFields {
    source: String,
    table: String,
    rows_fetched: usize,
    rows_upserted: usize,
    duration_ms: i64,
}

pub struct IngestionTracker;

impl IngestionTracker {
    pub async fn log_ingestion(
        db: &storage::Db,
        result: &IngestionResult,
        duration_ms: i64,
    ) -> Result<(), IngestionError> {
        let ts_ms = chrono::Utc::now().timestamp_millis();
        db.record_system_log(storage::SystemLogCommand {
            run_id: None,
            ts_ms,
            level: "INFO".to_string(),
            target: "ingestion".to_string(),
            message: format!(
                "ingested {} rows into {} from {}",
                result.rows_upserted, result.table, result.source
            ),
            fields: Some(serde_json::json!({
                "source": result.source,
                "table": result.table,
                "rows_fetched": result.rows_fetched,
                "rows_upserted": result.rows_upserted,
                "duration_ms": duration_ms,
            })),
        })
        .await?;
        Ok(())
    }
}

pub async fn last_ingestions(db: &storage::Db) -> Result<Vec<IngestionStatus>, IngestionError> {
    let now_ms = chrono::Utc::now().timestamp_millis();
    let logs = db.list_system_logs(None).await?;
    parse_last_ingestions(&logs, now_ms)
}

pub async fn last_ingestions_with_staleness(
    db: &storage::Db,
    now_ms: i64,
) -> Result<Vec<IngestionStatus>, IngestionError> {
    let logs = db.list_system_logs(None).await?;
    let statuses = parse_last_ingestions(&logs, now_ms)?;
    for status in statuses.iter().filter(|status| status.is_stale) {
        if !has_stale_alert(&logs, status)? {
            log_stale_alert(db, status, now_ms).await?;
        }
    }
    Ok(statuses)
}

fn parse_last_ingestions(
    logs: &[storage::StoredSystemLog],
    now_ms: i64,
) -> Result<Vec<IngestionStatus>, IngestionError> {
    let mut latest_by_key = BTreeMap::<(String, String), IngestionStatus>::new();

    for log in logs {
        if log.target != "ingestion" {
            continue;
        }
        let Some(fields_json) = log.fields_json.as_deref() else {
            continue;
        };
        let fields = serde_json::from_str::<IngestionFields>(fields_json)?;
        let stale_after_ms = stale_after_ms_for_table(&fields.table);
        let age_ms = now_ms.saturating_sub(log.ts_ms);
        latest_by_key.insert(
            (fields.source.clone(), fields.table.clone()),
            IngestionStatus {
                source: fields.source,
                table: fields.table,
                rows_fetched: fields.rows_fetched,
                rows_upserted: fields.rows_upserted,
                duration_ms: fields.duration_ms,
                ts_ms: log.ts_ms,
                age_ms,
                stale_after_ms,
                is_stale: age_ms > stale_after_ms,
            },
        );
    }

    Ok(latest_by_key.into_values().collect())
}

fn stale_after_ms_for_table(table: &str) -> i64 {
    match table {
        "funding_rates" => 12 * HOUR_MS,
        "crypto_market_meta" => 24 * HOUR_MS,
        "corporate_actions_meta" => 7 * DAY_MS,
        _ => 24 * HOUR_MS,
    }
}

fn has_stale_alert(
    logs: &[storage::StoredSystemLog],
    status: &IngestionStatus,
) -> Result<bool, IngestionError> {
    for log in logs {
        if log.target != "runtime.alert" || log.message != "reference_data_stale.alert" {
            continue;
        }
        let Some(fields_json) = log.fields_json.as_deref() else {
            continue;
        };
        let fields = serde_json::from_str::<serde_json::Value>(fields_json)?;
        if fields.get("source").and_then(serde_json::Value::as_str) == Some(status.source.as_str())
            && fields.get("table").and_then(serde_json::Value::as_str)
                == Some(status.table.as_str())
            && fields
                .get("ingestion_ts_ms")
                .and_then(serde_json::Value::as_i64)
                == Some(status.ts_ms)
        {
            return Ok(true);
        }
    }
    Ok(false)
}

async fn log_stale_alert(
    db: &storage::Db,
    status: &IngestionStatus,
    now_ms: i64,
) -> Result<(), IngestionError> {
    db.record_system_log(storage::SystemLogCommand {
        run_id: None,
        ts_ms: now_ms,
        level: "WARN".to_string(),
        target: "runtime.alert".to_string(),
        message: "reference_data_stale.alert".to_string(),
        fields: Some(serde_json::json!({
            "source": status.source,
            "table": status.table,
            "ingestion_ts_ms": status.ts_ms,
            "age_ms": status.age_ms,
            "stale_after_ms": status.stale_after_ms,
        })),
    })
    .await?;
    Ok(())
}

const HOUR_MS: i64 = 60 * 60 * 1000;
const DAY_MS: i64 = 24 * HOUR_MS;
