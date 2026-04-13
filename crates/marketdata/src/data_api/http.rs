use crate::core::data::{DataQuery, DataSource, Granularity};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Debug, Clone, Serialize)]
pub struct DataApiError {
    pub code: u16,
    pub message: String,
}

impl std::fmt::Display for DataApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "HTTP {} - {}", self.code, self.message)
    }
}

pub struct DataHttpHandler {
    source: Arc<dyn DataSource + Send + Sync>,
}

impl DataHttpHandler {
    pub fn new(source: Arc<dyn DataSource + Send + Sync>) -> Self {
        Self { source }
    }

    pub fn handle_query(&self, query_json: &str) -> Result<String, DataApiError> {
        #[derive(Deserialize)]
        struct QueryPayload {
            instrument: String,
            start_ts_ms: i64,
            end_ts_ms: i64,
            #[serde(default)]
            limit: Option<usize>,
        }

        let payload: QueryPayload = serde_json::from_str(query_json).map_err(|e| DataApiError {
            code: 400,
            message: format!("Invalid query JSON: {}", e),
        })?;

        let query = DataQuery {
            instrument: payload.instrument,
            start_ts_ms: payload.start_ts_ms,
            end_ts_ms: payload.end_ts_ms,
            granularity: Granularity::Minutes(1),
            limit: payload.limit,
        };

        let items = self.source.query(&query).map_err(|e| DataApiError {
            code: 500,
            message: e.to_string(),
        })?;

        serde_json::to_string(&items).map_err(|e| DataApiError {
            code: 500,
            message: format!("Serialization error: {}", e),
        })
    }

    pub fn handle_health(&self) -> String {
        r#"{"status":"ok"}"#.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::data::InMemoryDataSource;

    #[test]
    fn health_returns_ok() {
        let source = Arc::new(InMemoryDataSource::new("test", vec![]));
        let handler = DataHttpHandler::new(source);
        let resp = handler.handle_health();
        assert!(resp.contains("ok"));
    }

    #[test]
    fn invalid_json_returns_error() {
        let source = Arc::new(InMemoryDataSource::new("test", vec![]));
        let handler = DataHttpHandler::new(source);
        let result = handler.handle_query("not json");
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code, 400);
    }
}
