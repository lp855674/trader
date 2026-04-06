use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq)]
pub enum SpanKind {
    OrderSubmit,
    OrderFill,
    RiskCheck,
    RouteDecision,
    PersistOrder,
}

#[derive(Debug, Clone)]
pub struct TraceSpan {
    pub span_id: String,
    pub parent_id: Option<String>,
    pub kind: SpanKind,
    pub start_ts_us: u64,
    pub end_ts_us: Option<u64>,
    pub tags: HashMap<String, String>,
}

impl TraceSpan {
    pub fn finish(&mut self, ts_us: u64) {
        self.end_ts_us = Some(ts_us);
    }

    pub fn duration_us(&self) -> Option<u64> {
        self.end_ts_us.map(|end| end.saturating_sub(self.start_ts_us))
    }

    pub fn tag(&mut self, key: &str, value: &str) {
        self.tags.insert(key.to_string(), value.to_string());
    }
}

pub struct ExecutionTracer {
    pub spans: Vec<TraceSpan>,
    pub max_spans: usize,
    next_id: u64,
}

impl ExecutionTracer {
    pub fn new(max_spans: usize) -> Self {
        Self { spans: Vec::new(), max_spans, next_id: 1 }
    }

    pub fn start_span(
        &mut self,
        kind: SpanKind,
        parent_id: Option<String>,
        ts_us: u64,
    ) -> String {
        let span_id = format!("span-{}", self.next_id);
        self.next_id += 1;
        let span = TraceSpan {
            span_id: span_id.clone(),
            parent_id,
            kind,
            start_ts_us: ts_us,
            end_ts_us: None,
            tags: HashMap::new(),
        };
        if self.spans.len() >= self.max_spans {
            self.spans.remove(0);
        }
        self.spans.push(span);
        span_id
    }

    pub fn finish_span(&mut self, span_id: &str, ts_us: u64) {
        if let Some(span) = self.spans.iter_mut().find(|s| s.span_id == span_id) {
            span.finish(ts_us);
        }
    }

    pub fn tag_span(&mut self, span_id: &str, key: &str, value: &str) {
        if let Some(span) = self.spans.iter_mut().find(|s| s.span_id == span_id) {
            span.tag(key, value);
        }
    }

    pub fn recent_spans(&self, n: usize) -> &[TraceSpan] {
        let len = self.spans.len();
        if n >= len {
            &self.spans
        } else {
            &self.spans[len - n..]
        }
    }

    pub fn spans_by_kind(&self, kind: &SpanKind) -> Vec<&TraceSpan> {
        self.spans.iter().filter(|s| &s.kind == kind).collect()
    }

    /// Export spans as OTLP-compatible JSON lines (OpenTelemetry integration stub).
    /// In production, replace with real OTLP exporter via `opentelemetry-otlp` crate.
    pub fn export_otlp_json(&self) -> String {
        let entries: Vec<String> = self.spans.iter()
            .filter(|s| s.end_ts_us.is_some())
            .map(|s| {
                let dur = s.duration_us().unwrap_or(0);
                let parent = s.parent_id.as_deref().unwrap_or("none");
                let tags: Vec<String> = s.tags.iter()
                    .map(|(k, v)| format!("\"{}\":\"{}\"", k, v))
                    .collect();
                format!(
                    "{{\"span_id\":\"{}\",\"parent\":\"{}\",\"kind\":\"{:?}\",\"start_us\":{},\"dur_us\":{},\"tags\":{{{}}}}}",
                    s.span_id, parent, s.kind, s.start_ts_us, dur, tags.join(",")
                )
            })
            .collect();
        entries.join("\n")
    }

    /// Average span duration in microseconds for a given kind.
    pub fn avg_duration_us(&self, kind: &SpanKind) -> Option<f64> {
        let finished: Vec<u64> = self.spans.iter()
            .filter(|s| &s.kind == kind)
            .filter_map(|s| s.duration_us())
            .collect();
        if finished.is_empty() { return None; }
        Some(finished.iter().sum::<u64>() as f64 / finished.len() as f64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn span_lifecycle() {
        let mut tracer = ExecutionTracer::new(100);
        let id = tracer.start_span(SpanKind::OrderSubmit, None, 1000);
        assert_eq!(tracer.spans.len(), 1);
        tracer.finish_span(&id, 1500);
        let span = &tracer.spans[0];
        assert_eq!(span.end_ts_us, Some(1500));
    }

    #[test]
    fn duration_computed() {
        let mut tracer = ExecutionTracer::new(100);
        let id = tracer.start_span(SpanKind::RiskCheck, None, 1000);
        tracer.finish_span(&id, 1200);
        assert_eq!(tracer.spans[0].duration_us(), Some(200));
    }

    #[test]
    fn tag_set() {
        let mut tracer = ExecutionTracer::new(100);
        let id = tracer.start_span(SpanKind::OrderFill, None, 100);
        tracer.tag_span(&id, "order_id", "o-123");
        assert_eq!(tracer.spans[0].tags.get("order_id").unwrap(), "o-123");
    }

    #[test]
    fn spans_by_kind_filters() {
        let mut tracer = ExecutionTracer::new(100);
        tracer.start_span(SpanKind::OrderSubmit, None, 100);
        tracer.start_span(SpanKind::RiskCheck, None, 200);
        tracer.start_span(SpanKind::OrderSubmit, None, 300);
        let submit_spans = tracer.spans_by_kind(&SpanKind::OrderSubmit);
        assert_eq!(submit_spans.len(), 2);
        let risk_spans = tracer.spans_by_kind(&SpanKind::RiskCheck);
        assert_eq!(risk_spans.len(), 1);
    }

    #[test]
    fn max_spans_evicts_oldest() {
        let mut tracer = ExecutionTracer::new(2);
        let id1 = tracer.start_span(SpanKind::OrderSubmit, None, 100);
        tracer.start_span(SpanKind::OrderFill, None, 200);
        tracer.start_span(SpanKind::RiskCheck, None, 300);
        assert_eq!(tracer.spans.len(), 2);
        // id1 should have been evicted
        assert!(tracer.spans.iter().all(|s| s.span_id != id1));
    }

    #[test]
    fn recent_spans() {
        let mut tracer = ExecutionTracer::new(100);
        for i in 0..5 {
            tracer.start_span(SpanKind::OrderSubmit, None, i as u64 * 100);
        }
        let recent = tracer.recent_spans(3);
        assert_eq!(recent.len(), 3);
    }
}
