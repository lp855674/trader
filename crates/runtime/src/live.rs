use crate::CancellationFlag;
use broker::{Broker, BrokerKind, BrokerStatus, FakeBrokerAdapter};
use storage::{Db, NewEventRecord, NewStrategyRun};
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
            .insert_strategy_run(NewStrategyRun {
                id: self.settings.run_id.clone(),
                name: "live".to_string(),
                mode: "live".to_string(),
                status: "running".to_string(),
                started_at_ms,
                ended_at_ms: None,
                error: None,
                config_json: serde_json::json!({
                    "broker_kind": self.settings.broker_kind
                })
                .to_string(),
            })
            .await?;
        self.insert_event("live.started").await?;

        while !cancel.is_cancelled() {
            sleep(Duration::from_millis(10)).await;
        }

        let ended_at_ms = chrono::Utc::now().timestamp_millis();
        self.db
            .update_strategy_run_status(&self.settings.run_id, "stopped", Some(ended_at_ms), None)
            .await?;
        self.insert_event("live.stopped").await?;
        Ok(())
    }

    async fn insert_event(&self, category: &str) -> Result<(), sqlx::Error> {
        self.db
            .insert_event(NewEventRecord {
                event_id: uuid::Uuid::new_v4().to_string(),
                ts_ms: chrono::Utc::now().timestamp_millis(),
                source: self.settings.run_id.clone(),
                category: category.to_string(),
                payload_json: serde_json::json!({
                    "run_id": &self.settings.run_id,
                    "broker_kind": self.settings.broker_kind
                })
                .to_string(),
            })
            .await
    }
}
