use std::{
    collections::BTreeMap,
    fmt,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

use async_trait::async_trait;
use chrono::Utc;
use serde_json::Value;
use thiserror::Error;
use tokio::{
    sync::{mpsc, oneshot},
    task::JoinHandle,
};
use tracing::{Event, Subscriber};
use tracing_subscriber::{Layer, layer::Context, registry::LookupSpan};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct StructuredLogEntry {
    pub id: String,
    pub run_id: Option<String>,
    pub ts_ms: i64,
    pub level: String,
    pub target: String,
    pub message: String,
    pub fields_json: Option<String>,
    pub created_at_ms: i64,
}

#[derive(Debug, Error)]
pub enum LogSinkError {
    #[error("log sink write failed: {0}")]
    Write(String),
}

#[async_trait]
pub trait LogSink: Send + Sync + 'static {
    async fn write_batch(&self, logs: &[StructuredLogEntry]) -> Result<(), LogSinkError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogWriterSettings {
    pub enabled: bool,
    pub buffer_size: usize,
    pub batch_size: usize,
    pub flush_interval_ms: u64,
    pub min_level: String,
    pub categories: Vec<String>,
    pub metrics: LogWriterMetrics,
}

impl Default for LogWriterSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            buffer_size: 1_024,
            batch_size: 100,
            flush_interval_ms: 25,
            min_level: "INFO".to_string(),
            categories: Vec::new(),
            metrics: LogWriterMetrics::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct LogWriterMetricsSnapshot {
    pub dropped_logs: u64,
}

pub struct LogWriter<S> {
    tx: mpsc::Sender<StructuredLogEntry>,
    shutdown_tx: Option<oneshot::Sender<()>>,
    handle: JoinHandle<()>,
    _sink: Arc<S>,
    metrics: LogWriterMetrics,
}

#[derive(Debug, Clone, Default)]
pub struct LogWriterMetrics {
    dropped_logs: Arc<AtomicU64>,
}

impl PartialEq for LogWriterMetrics {
    fn eq(&self, other: &Self) -> bool {
        self.dropped_logs() == other.dropped_logs()
    }
}

impl Eq for LogWriterMetrics {}

impl LogWriterMetrics {
    pub fn dropped_logs(&self) -> u64 {
        self.dropped_logs.load(Ordering::Relaxed)
    }

    pub fn snapshot(&self) -> LogWriterMetricsSnapshot {
        LogWriterMetricsSnapshot {
            dropped_logs: self.dropped_logs(),
        }
    }

    fn increment_dropped_logs(&self) {
        self.dropped_logs.fetch_add(1, Ordering::Relaxed);
    }
}

impl<S> LogWriter<S>
where
    S: LogSink,
{
    pub fn new(sink: S, buffer_size: usize, batch_size: usize, flush_interval_ms: u64) -> Self {
        Self::new_with_metrics(
            sink,
            buffer_size,
            batch_size,
            flush_interval_ms,
            LogWriterMetrics::default(),
        )
    }

    pub fn new_with_metrics(
        sink: S,
        buffer_size: usize,
        batch_size: usize,
        flush_interval_ms: u64,
        metrics: LogWriterMetrics,
    ) -> Self {
        let sink = Arc::new(sink);
        let (tx, rx) = mpsc::channel(buffer_size);
        let (shutdown_tx, shutdown_rx) = oneshot::channel();
        let handle = tokio::spawn(Self::write_loop(
            sink.clone(),
            rx,
            shutdown_rx,
            batch_size.max(1),
            flush_interval_ms.max(1),
        ));
        Self {
            tx,
            shutdown_tx: Some(shutdown_tx),
            handle,
            _sink: sink,
            metrics,
        }
    }

    pub fn sender(&self) -> mpsc::Sender<StructuredLogEntry> {
        self.tx.clone()
    }

    pub fn metrics(&self) -> LogWriterMetrics {
        self.metrics.clone()
    }

    pub async fn shutdown(mut self) {
        if let Some(shutdown_tx) = self.shutdown_tx.take() {
            let _ = shutdown_tx.send(());
        }
        let _ = self.handle.await;
    }

    async fn write_loop(
        sink: Arc<S>,
        mut rx: mpsc::Receiver<StructuredLogEntry>,
        mut shutdown_rx: oneshot::Receiver<()>,
        batch_size: usize,
        flush_interval_ms: u64,
    ) {
        let mut buffer = Vec::with_capacity(batch_size);
        let mut interval = tokio::time::interval(Duration::from_millis(flush_interval_ms));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

        loop {
            tokio::select! {
                maybe_log = rx.recv() => {
                    match maybe_log {
                        Some(log) => {
                            buffer.push(log);
                            if buffer.len() >= batch_size {
                                flush_buffer(&*sink, &mut buffer).await;
                            }
                        }
                        None => break,
                    }
                }
                _ = &mut shutdown_rx => {
                    while let Ok(log) = rx.try_recv() {
                        buffer.push(log);
                        if buffer.len() >= batch_size {
                            flush_buffer(&*sink, &mut buffer).await;
                        }
                    }
                    break;
                }
                _ = interval.tick() => {
                    if !buffer.is_empty() {
                        flush_buffer(&*sink, &mut buffer).await;
                    }
                }
            }
        }

        if !buffer.is_empty() {
            flush_buffer(&*sink, &mut buffer).await;
        }
    }
}

async fn flush_buffer<S>(sink: &S, buffer: &mut Vec<StructuredLogEntry>)
where
    S: LogSink,
{
    if sink.write_batch(buffer).await.is_ok() {
        buffer.clear();
    }
}

#[derive(Clone)]
pub struct SystemLogLayer {
    tx: mpsc::Sender<StructuredLogEntry>,
    default_run_id: Option<String>,
    settings: LogWriterSettings,
    metrics: LogWriterMetrics,
}

impl SystemLogLayer {
    pub fn new(tx: mpsc::Sender<StructuredLogEntry>, default_run_id: Option<String>) -> Self {
        Self {
            tx,
            default_run_id,
            settings: LogWriterSettings::default(),
            metrics: LogWriterMetrics::default(),
        }
    }

    pub fn with_settings(mut self, settings: LogWriterSettings) -> Self {
        self.settings = settings;
        self
    }

    pub fn with_metrics(mut self, metrics: LogWriterMetrics) -> Self {
        self.metrics = metrics;
        self
    }
}

impl<S> Layer<S> for SystemLogLayer
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        let metadata = event.metadata();
        if !level_allowed(metadata.level().as_str(), &self.settings.min_level) {
            return;
        }

        let mut visitor = JsonVisitor::default();
        event.record(&mut visitor);
        if !category_allowed(&visitor.fields, &self.settings.categories) {
            return;
        }

        let run_id = visitor
            .fields
            .get("run_id")
            .and_then(Value::as_str)
            .map(str::to_string)
            .or_else(|| self.default_run_id.clone());
        let message = visitor
            .fields
            .remove("message")
            .and_then(|value| match value {
                Value::String(value) => Some(value),
                other => Some(other.to_string()),
            })
            .unwrap_or_else(|| metadata.name().to_string());
        let fields_json = (!visitor.fields.is_empty())
            .then(|| serde_json::to_string(&visitor.fields).ok())
            .flatten();
        let now_ms = Utc::now().timestamp_millis();
        let entry = StructuredLogEntry {
            id: Uuid::new_v4().to_string(),
            run_id,
            ts_ms: now_ms,
            level: metadata.level().as_str().to_string(),
            target: metadata.target().to_string(),
            message,
            fields_json,
            created_at_ms: now_ms,
        };
        if self.tx.try_send(entry).is_err() {
            self.metrics.increment_dropped_logs();
        }
    }
}

fn level_allowed(actual: &str, minimum: &str) -> bool {
    level_rank(actual) >= level_rank(minimum)
}

fn level_rank(level: &str) -> u8 {
    match level.to_ascii_uppercase().as_str() {
        "TRACE" => 0,
        "DEBUG" => 1,
        "INFO" => 2,
        "WARN" | "WARNING" => 3,
        "ERROR" | "FATAL" => 4,
        _ => 2,
    }
}

fn category_allowed(fields: &BTreeMap<String, Value>, categories: &[String]) -> bool {
    if categories.is_empty() {
        return true;
    }
    let Some(category) = fields.get("category").and_then(Value::as_str) else {
        return false;
    };
    categories
        .iter()
        .any(|allowed| allowed.eq_ignore_ascii_case(category))
}

#[derive(Default)]
struct JsonVisitor {
    fields: BTreeMap<String, Value>,
}

impl tracing::field::Visit for JsonVisitor {
    fn record_i64(&mut self, field: &tracing::field::Field, value: i64) {
        self.fields
            .insert(field.name().to_string(), Value::from(value));
    }

    fn record_u64(&mut self, field: &tracing::field::Field, value: u64) {
        self.fields
            .insert(field.name().to_string(), Value::from(value));
    }

    fn record_bool(&mut self, field: &tracing::field::Field, value: bool) {
        self.fields
            .insert(field.name().to_string(), Value::from(value));
    }

    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        self.fields
            .insert(field.name().to_string(), Value::from(value.to_string()));
    }

    fn record_error(
        &mut self,
        field: &tracing::field::Field,
        value: &(dyn std::error::Error + 'static),
    ) {
        self.fields
            .insert(field.name().to_string(), Value::from(value.to_string()));
    }

    fn record_f64(&mut self, field: &tracing::field::Field, value: f64) {
        self.fields
            .insert(field.name().to_string(), Value::from(value));
    }

    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn fmt::Debug) {
        self.fields
            .insert(field.name().to_string(), Value::from(format!("{value:?}")));
    }
}
