use std::sync::{Arc, Mutex};

use events::{
    LogSink, LogWriter, LogWriterMetrics, LogWriterSettings, StructuredLogEntry, SystemLogLayer,
};
use tracing_subscriber::prelude::*;

#[tokio::test]
async fn log_writer_flushes_on_interval() {
    let sink = InMemoryLogSink::default();
    let writer = LogWriter::new(sink.clone(), 16, 10, 50);

    writer
        .sender()
        .send(test_log("interval-1", "interval message"))
        .await
        .unwrap();

    tokio::time::sleep(std::time::Duration::from_millis(80)).await;
    writer.shutdown().await;

    let logs = sink.take();
    assert_eq!(logs.len(), 1);
    assert_eq!(logs[0].message, "interval message");
}

#[tokio::test]
async fn log_writer_flushes_when_batch_is_full() {
    let sink = InMemoryLogSink::default();
    let writer = LogWriter::new(sink.clone(), 16, 2, 1_000);

    writer
        .sender()
        .send(test_log("batch-1", "first"))
        .await
        .unwrap();
    writer
        .sender()
        .send(test_log("batch-2", "second"))
        .await
        .unwrap();
    writer
        .sender()
        .send(test_log("batch-3", "third"))
        .await
        .unwrap();

    writer.shutdown().await;

    let logs = sink.take();
    assert_eq!(logs.len(), 3);
    assert_eq!(
        logs.iter().map(|log| log.id.as_str()).collect::<Vec<_>>(),
        vec!["batch-1", "batch-2", "batch-3"]
    );
}

#[tokio::test]
async fn tracing_layer_captures_structured_events() {
    let sink = InMemoryLogSink::default();
    let writer = LogWriter::new(sink.clone(), 16, 10, 10);
    let layer = SystemLogLayer::new(writer.sender(), Some("run-default".to_string()));
    let subscriber = tracing_subscriber::registry().with(layer);
    let dispatch = tracing::Dispatch::new(subscriber);

    tracing::dispatcher::with_default(&dispatch, || {
        tracing::info!(
            run_id = "run-123",
            order_id = "ord-9",
            answer = 42,
            "execution started"
        );
    });

    writer.shutdown().await;

    let logs = sink.take();
    assert_eq!(logs.len(), 1);
    let log = &logs[0];
    assert_eq!(log.run_id.as_deref(), Some("run-123"));
    assert_eq!(log.level, "INFO");
    assert_eq!(log.target, "log_writer_tests");
    assert_eq!(log.message, "execution started");
    let fields =
        serde_json::from_str::<serde_json::Value>(log.fields_json.as_deref().unwrap()).unwrap();
    assert_eq!(fields["order_id"], "ord-9");
    assert_eq!(fields["answer"], 42);
}

#[tokio::test]
async fn tracing_layer_filters_by_level_and_category() {
    let sink = InMemoryLogSink::default();
    let writer = LogWriter::new(sink.clone(), 16, 10, 10);
    let layer = SystemLogLayer::new(writer.sender(), Some("run-default".to_string()))
        .with_settings(LogWriterSettings {
            min_level: "WARN".to_string(),
            categories: vec!["risk".to_string()],
            ..LogWriterSettings::default()
        });
    let subscriber = tracing_subscriber::registry().with(layer);
    let dispatch = tracing::Dispatch::new(subscriber);

    tracing::dispatcher::with_default(&dispatch, || {
        tracing::info!(category = "risk", "info risk should be dropped");
        tracing::warn!(category = "api", "warn api should be dropped");
        tracing::error!(category = "risk", "error risk should be captured");
    });

    writer.shutdown().await;

    let logs = sink.take();
    assert_eq!(logs.len(), 1);
    assert_eq!(logs[0].level, "ERROR");
    assert_eq!(logs[0].message, "error risk should be captured");
}

#[tokio::test]
async fn tracing_layer_counts_dropped_logs_when_channel_is_full() {
    let sink = InMemoryLogSink::default();
    let metrics = LogWriterMetrics::default();
    let writer = LogWriter::new_with_metrics(sink.clone(), 1, 10, 1_000, metrics.clone());
    let layer = SystemLogLayer::new(writer.sender(), Some("run-default".to_string()))
        .with_metrics(metrics.clone());
    let subscriber = tracing_subscriber::registry().with(layer);
    let dispatch = tracing::Dispatch::new(subscriber);

    tracing::dispatcher::with_default(&dispatch, || {
        tracing::info!("first log fills channel");
        tracing::info!("second log is dropped");
    });

    assert_eq!(metrics.dropped_logs(), 1);
    writer.shutdown().await;
}

#[tokio::test]
async fn log_writer_settings_can_share_metrics_with_layer_and_writer() {
    let settings = LogWriterSettings {
        metrics: LogWriterMetrics::default(),
        ..LogWriterSettings::default()
    };
    let (layer_tx, _rx_guard) = {
        let (tx, rx) = tokio::sync::mpsc::channel(1);
        (tx, rx)
    };
    let layer = SystemLogLayer::new(layer_tx, Some("run-default".to_string()))
        .with_settings(settings.clone())
        .with_metrics(settings.metrics.clone());
    let subscriber = tracing_subscriber::registry().with(layer);
    let dispatch = tracing::Dispatch::new(subscriber);

    tracing::dispatcher::with_default(&dispatch, || {
        tracing::info!("first log fills shared metrics channel");
        tracing::info!("second log increments shared dropped count");
    });

    assert_eq!(settings.metrics.snapshot().dropped_logs, 1);
}

#[derive(Clone, Default)]
struct InMemoryLogSink {
    logs: Arc<Mutex<Vec<StructuredLogEntry>>>,
}

impl InMemoryLogSink {
    fn take(&self) -> Vec<StructuredLogEntry> {
        self.logs.lock().unwrap().clone()
    }
}

#[async_trait::async_trait]
impl LogSink for InMemoryLogSink {
    async fn write_batch(&self, logs: &[StructuredLogEntry]) -> Result<(), events::LogSinkError> {
        self.logs.lock().unwrap().extend_from_slice(logs);
        Ok(())
    }
}

fn test_log(id: &str, message: &str) -> StructuredLogEntry {
    StructuredLogEntry {
        id: id.to_string(),
        run_id: Some("run-test".to_string()),
        ts_ms: 1,
        level: "INFO".to_string(),
        target: "test.target".to_string(),
        message: message.to_string(),
        fields_json: None,
        created_at_ms: 1,
    }
}
