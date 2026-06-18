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
    let logs = db.list_system_logs(None).await?;
    let mut latest_by_key = BTreeMap::<(String, String), IngestionStatus>::new();

    for log in logs {
        if log.target != "ingestion" {
            continue;
        }
        let Some(fields_json) = log.fields_json else {
            continue;
        };
        let fields = serde_json::from_str::<IngestionFields>(&fields_json)?;
        latest_by_key.insert(
            (fields.source.clone(), fields.table.clone()),
            IngestionStatus {
                source: fields.source,
                table: fields.table,
                rows_fetched: fields.rows_fetched,
                rows_upserted: fields.rows_upserted,
                duration_ms: fields.duration_ms,
                ts_ms: log.ts_ms,
            },
        );
    }

    Ok(latest_by_key.into_values().collect())
}
