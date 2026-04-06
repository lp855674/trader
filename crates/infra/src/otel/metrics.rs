use std::collections::HashMap;

#[derive(Debug, Clone)]
pub enum MetricValue {
    Counter(u64),
    Gauge(f64),
    Histogram(Vec<f64>),
}

#[derive(Default)]
pub struct MetricsCollector {
    data: HashMap<String, MetricValue>,
}

impl MetricsCollector {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record_counter(&mut self, name: &str, delta: u64) {
        let entry = self.data.entry(name.to_string()).or_insert(MetricValue::Counter(0));
        if let MetricValue::Counter(c) = entry {
            *c += delta;
        } else {
            *entry = MetricValue::Counter(delta);
        }
    }

    pub fn record_gauge(&mut self, name: &str, value: f64) {
        self.data.insert(name.to_string(), MetricValue::Gauge(value));
    }

    pub fn record_histogram(&mut self, name: &str, value: f64) {
        let entry = self.data.entry(name.to_string()).or_insert(MetricValue::Histogram(vec![]));
        if let MetricValue::Histogram(v) = entry {
            v.push(value);
        } else {
            *entry = MetricValue::Histogram(vec![value]);
        }
    }

    pub fn snapshot(&self) -> HashMap<String, MetricValue> {
        self.data.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counter_accumulates() {
        let mut mc = MetricsCollector::new();
        mc.record_counter("orders", 3);
        mc.record_counter("orders", 5);
        let snap = mc.snapshot();
        match snap["orders"] {
            MetricValue::Counter(c) => assert_eq!(c, 8),
            _ => panic!("expected counter"),
        }
    }

    #[test]
    fn gauge_overwrites() {
        let mut mc = MetricsCollector::new();
        mc.record_gauge("cpu", 0.5);
        mc.record_gauge("cpu", 0.9);
        let snap = mc.snapshot();
        match snap["cpu"] {
            MetricValue::Gauge(v) => assert!((v - 0.9).abs() < 1e-9),
            _ => panic!("expected gauge"),
        }
    }

    #[test]
    fn histogram_collects_values() {
        let mut mc = MetricsCollector::new();
        mc.record_histogram("latency_ms", 1.2);
        mc.record_histogram("latency_ms", 3.4);
        let snap = mc.snapshot();
        match &snap["latency_ms"] {
            MetricValue::Histogram(v) => assert_eq!(v.len(), 2),
            _ => panic!("expected histogram"),
        }
    }
}
