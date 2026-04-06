use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct Span {
    pub id: u64,
    pub name: String,
    pub start_ms: u64,
    pub end_ms: Option<u64>,
    pub tags: HashMap<String, String>,
}

#[derive(Default)]
pub struct TracingSystem {
    spans: Vec<Span>,
    next_id: u64,
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

impl TracingSystem {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn start_span(&mut self, name: &str) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        self.spans.push(Span {
            id,
            name: name.to_string(),
            start_ms: now_ms(),
            end_ms: None,
            tags: HashMap::new(),
        });
        id
    }

    pub fn finish_span(&mut self, id: u64) {
        if let Some(span) = self.spans.iter_mut().find(|s| s.id == id) {
            span.end_ms = Some(now_ms());
        }
    }

    pub fn add_tag(&mut self, id: u64, key: &str, value: &str) {
        if let Some(span) = self.spans.iter_mut().find(|s| s.id == id) {
            span.tags.insert(key.to_string(), value.to_string());
        }
    }

    pub fn completed_spans(&self) -> Vec<&Span> {
        self.spans.iter().filter(|s| s.end_ms.is_some()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn start_finish_span() {
        let mut ts = TracingSystem::new();
        let id = ts.start_span("db.query");
        assert_eq!(ts.completed_spans().len(), 0);
        ts.finish_span(id);
        let completed = ts.completed_spans();
        assert_eq!(completed.len(), 1);
        assert_eq!(completed[0].name, "db.query");
    }

    #[test]
    fn add_tag_to_span() {
        let mut ts = TracingSystem::new();
        let id = ts.start_span("http.request");
        ts.add_tag(id, "method", "GET");
        ts.finish_span(id);
        let span = &ts.completed_spans()[0];
        assert_eq!(span.tags["method"], "GET");
    }
}
