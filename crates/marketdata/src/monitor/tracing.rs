use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct DataTraceSpan {
    pub id: String,
    pub operation: String,
    pub start_us: u64,
    pub end_us: Option<u64>,
    pub tags: HashMap<String, String>,
}

impl DataTraceSpan {
    pub fn finish(&mut self, ts_us: u64) {
        self.end_us = Some(ts_us);
    }

    pub fn duration_us(&self) -> Option<u64> {
        self.end_us.map(|end| end.saturating_sub(self.start_us))
    }
}

pub struct DataTracer {
    pub spans: Vec<DataTraceSpan>,
    pub max_spans: usize,
    span_counter: u64,
}

impl DataTracer {
    pub fn new(max_spans: usize) -> Self {
        Self {
            spans: Vec::new(),
            max_spans,
            span_counter: 0,
        }
    }

    pub fn start(&mut self, operation: &str, ts_us: u64) -> String {
        self.span_counter += 1;
        let id = format!("span-{}", self.span_counter);
        let span = DataTraceSpan {
            id: id.clone(),
            operation: operation.to_string(),
            start_us: ts_us,
            end_us: None,
            tags: HashMap::new(),
        };
        if self.spans.len() >= self.max_spans {
            self.spans.remove(0);
        }
        self.spans.push(span);
        id
    }

    pub fn finish(&mut self, span_id: &str, ts_us: u64) {
        if let Some(span) = self.spans.iter_mut().find(|s| s.id == span_id) {
            span.finish(ts_us);
        }
    }

    pub fn tag(&mut self, span_id: &str, key: &str, value: &str) {
        if let Some(span) = self.spans.iter_mut().find(|s| s.id == span_id) {
            span.tags.insert(key.to_string(), value.to_string());
        }
    }

    pub fn avg_duration_for(&self, operation: &str) -> Option<f64> {
        let durations: Vec<u64> = self
            .spans
            .iter()
            .filter(|s| s.operation == operation)
            .filter_map(|s| s.duration_us())
            .collect();
        if durations.is_empty() {
            None
        } else {
            Some(durations.iter().sum::<u64>() as f64 / durations.len() as f64)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tracer_avg_duration() {
        let mut tracer = DataTracer::new(100);
        let id1 = tracer.start("query", 1000);
        tracer.finish(&id1, 1200);
        let id2 = tracer.start("query", 2000);
        tracer.finish(&id2, 2400);
        let avg = tracer.avg_duration_for("query").unwrap();
        assert!((avg - 300.0).abs() < 1e-9, "Expected 300, got {}", avg);
    }

    #[test]
    fn tagging_works() {
        let mut tracer = DataTracer::new(100);
        let id = tracer.start("fetch", 0);
        tracer.tag(&id, "instrument", "BTC");
        tracer.finish(&id, 500);
        let span = tracer.spans.iter().find(|s| s.id == id).unwrap();
        assert_eq!(span.tags["instrument"], "BTC");
    }
}
