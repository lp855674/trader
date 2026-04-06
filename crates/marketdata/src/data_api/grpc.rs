use std::sync::Arc;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use crate::core::data::{DataSource, DataQuery, Granularity};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataServiceRequest {
    pub action: String,
    pub payload: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataServiceResponse {
    pub success: bool,
    pub data: Value,
}

pub struct DataGrpcService {
    source: Arc<dyn DataSource + Send + Sync>,
}

impl DataGrpcService {
    pub fn new(source: Arc<dyn DataSource + Send + Sync>) -> Self {
        Self { source }
    }

    pub fn handle(&self, req: &DataServiceRequest) -> DataServiceResponse {
        match req.action.as_str() {
            "query" => {
                // Parse DataQuery from payload
                let instrument = req.payload.get("instrument")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");
                let start = req.payload.get("start_ts_ms")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0);
                let end = req.payload.get("end_ts_ms")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(i64::MAX);
                let limit = req.payload.get("limit")
                    .and_then(|v| v.as_u64())
                    .map(|v| v as usize);

                let query = DataQuery {
                    instrument: instrument.to_string(),
                    start_ts_ms: start,
                    end_ts_ms: end,
                    granularity: Granularity::Minutes(1),
                    limit,
                };

                match self.source.query(&query) {
                    Ok(items) => {
                        let data = serde_json::to_value(&items).unwrap_or(Value::Array(vec![]));
                        DataServiceResponse { success: true, data }
                    }
                    Err(e) => DataServiceResponse {
                        success: false,
                        data: Value::String(e.to_string()),
                    },
                }
            }
            "health" => DataServiceResponse {
                success: true,
                data: serde_json::json!({"status": "ok"}),
            },
            _ => DataServiceResponse {
                success: false,
                data: Value::String(format!("Unknown action: {}", req.action)),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::data::InMemoryDataSource;

    #[test]
    fn health_action_returns_ok() {
        let source = Arc::new(InMemoryDataSource::new("test", vec![]));
        let service = DataGrpcService::new(source);
        let req = DataServiceRequest {
            action: "health".to_string(),
            payload: Value::Null,
        };
        let resp = service.handle(&req);
        assert!(resp.success);
    }
}
