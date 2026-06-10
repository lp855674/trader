use crate::CancellationFlag;
use broker::{Broker, BrokerKind, BrokerStatus, FakeBrokerAdapter};
use storage::{Db, LiveRunCommand, RuntimeEventCommand};
use tokio::time::{Duration, sleep};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiveRuntimeSettings {
    pub run_id: String,
    pub broker_kind: BrokerKind,
}

pub struct LiveRuntime {
    db: Db,
    settings: LiveRuntimeSettings,
}

impl LiveRuntime {
    pub fn new(db: Db, settings: LiveRuntimeSettings) -> Self {
        Self { db, settings }
    }

    pub async fn broker_status(&self) -> anyhow::Result<BrokerStatus> {
        Ok(FakeBrokerAdapter::new(self.settings.broker_kind)
            .status()
            .await?)
    }

    pub async fn run(&self, cancel: CancellationFlag) -> anyhow::Result<()> {
        let started_at_ms = chrono::Utc::now().timestamp_millis();
        self.db
            .start_live_run(LiveRunCommand {
                run_id: self.settings.run_id.clone(),
                started_at_ms,
                config: serde_json::json!({
                    "broker_kind": self.settings.broker_kind
                }),
            })
            .await?;
        self.record_event("live.started").await?;

        while !cancel.is_cancelled() {
            sleep(Duration::from_millis(10)).await;
        }

        let ended_at_ms = chrono::Utc::now().timestamp_millis();
        self.db
            .update_strategy_run_status(&self.settings.run_id, "stopped", Some(ended_at_ms), None)
            .await?;
        self.record_event("live.stopped").await?;
        Ok(())
    }

    async fn record_event(&self, category: &str) -> storage::StorageResult<()> {
        self.db
            .record_runtime_event(RuntimeEventCommand {
                ts_ms: chrono::Utc::now().timestamp_millis(),
                source: self.settings.run_id.clone(),
                category: category.to_string(),
                payload: serde_json::json!({
                    "run_id": &self.settings.run_id,
                    "broker_kind": self.settings.broker_kind
                }),
            })
            .await
    }
}
