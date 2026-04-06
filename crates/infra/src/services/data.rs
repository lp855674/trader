use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct DataQuery {
    pub instrument: String,
    pub start_ms: u64,
    pub end_ms: u64,
    pub granularity: String,
}

#[derive(Debug, Clone)]
pub struct DataRecord {
    pub instrument: String,
    pub timestamp_ms: u64,
    pub close: f64,
    pub volume: f64,
}

#[derive(Default)]
pub struct DataServiceStub {
    records: HashMap<String, Vec<DataRecord>>,
    ingested_count: u64,
    quality_score: f64,
}

impl DataServiceStub {
    pub fn new() -> Self {
        Self { records: HashMap::new(), ingested_count: 0, quality_score: 1.0 }
    }

    pub fn ingest(&mut self, record: DataRecord) {
        self.records.entry(record.instrument.clone()).or_default().push(record);
        self.ingested_count += 1;
    }

    pub fn query(&self, q: &DataQuery) -> Vec<&DataRecord> {
        self.records
            .get(&q.instrument)
            .map(|recs| {
                recs.iter()
                    .filter(|r| r.timestamp_ms >= q.start_ms && r.timestamp_ms <= q.end_ms)
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn ingested_count(&self) -> u64 {
        self.ingested_count
    }

    pub fn set_quality_score(&mut self, score: f64) {
        self.quality_score = score.clamp(0.0, 1.0);
    }

    pub fn quality_score(&self) -> f64 {
        self.quality_score
    }

    pub fn metadata(&self) -> Vec<String> {
        self.records.keys().cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ingest_and_query() {
        let mut svc = DataServiceStub::new();
        svc.ingest(DataRecord { instrument: "AAPL".to_string(), timestamp_ms: 1000, close: 150.0, volume: 1000.0 });
        svc.ingest(DataRecord { instrument: "AAPL".to_string(), timestamp_ms: 2000, close: 152.0, volume: 1200.0 });
        svc.ingest(DataRecord { instrument: "AAPL".to_string(), timestamp_ms: 3000, close: 148.0, volume: 800.0 });

        let q = DataQuery { instrument: "AAPL".to_string(), start_ms: 1500, end_ms: 2500, granularity: "1m".to_string() };
        let results = svc.query(&q);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].timestamp_ms, 2000);
    }

    #[test]
    fn metadata_lists_instruments() {
        let mut svc = DataServiceStub::new();
        svc.ingest(DataRecord { instrument: "AAPL".to_string(), timestamp_ms: 0, close: 100.0, volume: 1.0 });
        svc.ingest(DataRecord { instrument: "GOOG".to_string(), timestamp_ms: 0, close: 200.0, volume: 1.0 });
        let meta = svc.metadata();
        assert_eq!(meta.len(), 2);
    }
}
