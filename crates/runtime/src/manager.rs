use crate::CancellationFlag;
use std::{collections::HashMap, future::Future, sync::Arc};
use tokio::{sync::Mutex, task::JoinHandle};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunSpawnError {
    AlreadyRunning,
}

#[derive(Clone, Default)]
pub struct RuntimeManager {
    inner: Arc<Mutex<HashMap<String, RunHandle>>>,
}

struct RunHandle {
    cancel: CancellationFlag,
    join: JoinHandle<()>,
}

impl RuntimeManager {
    pub async fn is_active(&self, run_id: &str) -> bool {
        self.inner.lock().await.contains_key(run_id)
    }

    pub async fn cancel(&self, run_id: &str) -> bool {
        let Some(cancel) = self
            .inner
            .lock()
            .await
            .get(run_id)
            .map(|handle| handle.cancel.clone())
        else {
            return false;
        };
        cancel.cancel();
        true
    }

    pub async fn spawn<F, Fut>(&self, run_id: String, task: F) -> Result<(), RunSpawnError>
    where
        F: FnOnce(CancellationFlag) -> Fut + Send + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        let mut runs = self.inner.lock().await;
        if runs.contains_key(&run_id) {
            return Err(RunSpawnError::AlreadyRunning);
        }

        let manager = self.clone();
        let cancel = CancellationFlag::default();
        let task_cancel = cancel.clone();
        let task_run_id = run_id.clone();
        let join = tokio::spawn(async move {
            task(task_cancel).await;
            manager.inner.lock().await.remove(&task_run_id);
        });

        runs.insert(run_id, RunHandle { cancel, join });
        Ok(())
    }

    pub async fn wait_for_idle(&self, run_id: &str) {
        loop {
            let join = self
                .inner
                .lock()
                .await
                .remove(run_id)
                .map(|handle| handle.join);
            if let Some(join) = join {
                let _ = join.await;
                return;
            }
            if !self.is_active(run_id).await {
                return;
            }
        }
    }
}
