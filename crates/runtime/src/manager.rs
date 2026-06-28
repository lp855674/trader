use crate::CancellationFlag;
use std::{collections::HashMap, future::Future, sync::Arc};
use tokio::{sync::Mutex, task::JoinHandle};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunSpawnError {
    AlreadyRunning,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeRunStatus {
    Running,
    CancelRequested,
    Completed,
    Canceled,
}

impl RuntimeRunStatus {
    pub fn is_active(self) -> bool {
        matches!(self, Self::Running | Self::CancelRequested)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuntimeRunInfo {
    pub status: RuntimeRunStatus,
    pub started_at_ms: i64,
    pub last_state_change_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeRunMetadata {
    pub mode: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeRunSnapshot {
    pub info: RuntimeRunInfo,
    pub metadata: RuntimeRunMetadata,
}

#[derive(Clone, Default)]
pub struct RuntimeManager {
    inner: Arc<Mutex<HashMap<String, RunHandle>>>,
}

struct RunHandle {
    cancel: CancellationFlag,
    status: RuntimeRunStatus,
    started_at_ms: i64,
    last_state_change_at_ms: i64,
    metadata: RuntimeRunMetadata,
    join: Option<JoinHandle<()>>,
}

impl RuntimeManager {
    pub async fn is_active(&self, run_id: &str) -> bool {
        self.inner
            .lock()
            .await
            .get(run_id)
            .is_some_and(|handle| handle.status.is_active())
    }

    pub async fn status(&self, run_id: &str) -> Option<RuntimeRunStatus> {
        self.inner
            .lock()
            .await
            .get(run_id)
            .map(|handle| handle.status)
    }

    pub async fn info(&self, run_id: &str) -> Option<RuntimeRunInfo> {
        self.inner
            .lock()
            .await
            .get(run_id)
            .map(|handle| RuntimeRunInfo {
                status: handle.status,
                started_at_ms: handle.started_at_ms,
                last_state_change_at_ms: handle.last_state_change_at_ms,
            })
    }

    pub async fn metadata(&self, run_id: &str) -> Option<RuntimeRunMetadata> {
        self.inner
            .lock()
            .await
            .get(run_id)
            .map(|handle| handle.metadata.clone())
    }

    pub async fn snapshot(&self, run_id: &str) -> Option<RuntimeRunSnapshot> {
        self.inner
            .lock()
            .await
            .get(run_id)
            .map(|handle| RuntimeRunSnapshot {
                info: RuntimeRunInfo {
                    status: handle.status,
                    started_at_ms: handle.started_at_ms,
                    last_state_change_at_ms: handle.last_state_change_at_ms,
                },
                metadata: handle.metadata.clone(),
            })
    }

    pub async fn list_active(&self) -> Vec<(String, RuntimeRunSnapshot)> {
        let mut snapshots = self
            .inner
            .lock()
            .await
            .iter()
            .filter(|(_, handle)| handle.status.is_active())
            .map(|(run_id, handle)| {
                (
                    run_id.clone(),
                    RuntimeRunSnapshot {
                        info: RuntimeRunInfo {
                            status: handle.status,
                            started_at_ms: handle.started_at_ms,
                            last_state_change_at_ms: handle.last_state_change_at_ms,
                        },
                        metadata: handle.metadata.clone(),
                    },
                )
            })
            .collect::<Vec<_>>();
        snapshots.sort_by(|left, right| left.0.cmp(&right.0));
        snapshots
    }

    pub async fn cancel(&self, run_id: &str) -> bool {
        let mut runs = self.inner.lock().await;
        let Some(handle) = runs.get_mut(run_id) else {
            return false;
        };
        handle.cancel.cancel();
        handle.status = RuntimeRunStatus::CancelRequested;
        handle.last_state_change_at_ms = chrono::Utc::now().timestamp_millis();
        true
    }

    pub async fn spawn<F, Fut>(&self, run_id: String, task: F) -> Result<(), RunSpawnError>
    where
        F: FnOnce(CancellationFlag) -> Fut + Send + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        self.spawn_with_metadata(run_id, RuntimeRunMetadata { mode: None }, task)
            .await
    }

    pub async fn spawn_with_metadata<F, Fut>(
        &self,
        run_id: String,
        metadata: RuntimeRunMetadata,
        task: F,
    ) -> Result<(), RunSpawnError>
    where
        F: FnOnce(CancellationFlag) -> Fut + Send + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        let mut runs = self.inner.lock().await;
        if runs
            .get(&run_id)
            .is_some_and(|handle| handle.status.is_active())
        {
            return Err(RunSpawnError::AlreadyRunning);
        }

        let started_at_ms = chrono::Utc::now().timestamp_millis();
        let manager = self.clone();
        let cancel = CancellationFlag::default();
        let task_cancel = cancel.clone();
        let task_run_id = run_id.clone();
        let join = tokio::spawn(async move {
            task(task_cancel).await;
            manager.mark_terminal(&task_run_id).await;
        });

        runs.insert(
            run_id,
            RunHandle {
                cancel,
                status: RuntimeRunStatus::Running,
                started_at_ms,
                last_state_change_at_ms: started_at_ms,
                metadata,
                join: Some(join),
            },
        );
        Ok(())
    }

    async fn mark_terminal(&self, run_id: &str) {
        let mut runs = self.inner.lock().await;
        let Some(handle) = runs.get_mut(run_id) else {
            return;
        };
        handle.status = if handle.status == RuntimeRunStatus::CancelRequested {
            RuntimeRunStatus::Canceled
        } else {
            RuntimeRunStatus::Completed
        };
        handle.last_state_change_at_ms = chrono::Utc::now().timestamp_millis();
    }

    pub async fn wait_for_idle(&self, run_id: &str) {
        loop {
            let join = {
                let mut runs = self.inner.lock().await;
                let Some(handle) = runs.get_mut(run_id) else {
                    return;
                };
                if !handle.status.is_active() {
                    return;
                }
                handle.join.take()
            };
            if let Some(join) = join {
                let _ = join.await;
                return;
            }
            if !self.is_active(run_id).await {
                return;
            }
            tokio::task::yield_now().await;
        }
    }
}
