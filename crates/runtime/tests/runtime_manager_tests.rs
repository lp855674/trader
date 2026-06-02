use runtime::{RunSpawnError, RuntimeManager};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use tokio::sync::Notify;

#[tokio::test]
async fn manager_tracks_active_run_and_cancels_it() {
    let manager = RuntimeManager::default();
    let started = Arc::new(Notify::new());
    let released = Arc::new(Notify::new());
    let observed_cancel = Arc::new(AtomicBool::new(false));

    let started_for_task = started.clone();
    let released_for_task = released.clone();
    let observed_for_task = observed_cancel.clone();
    manager
        .spawn("run-1".to_string(), move |cancel| async move {
            started_for_task.notify_one();
            released_for_task.notified().await;
            observed_for_task.store(cancel.is_cancelled(), Ordering::SeqCst);
        })
        .await
        .unwrap();

    started.notified().await;
    assert!(manager.is_active("run-1").await);
    assert!(manager.cancel("run-1").await);
    released.notify_one();
    manager.wait_for_idle("run-1").await;

    assert!(observed_cancel.load(Ordering::SeqCst));
    assert!(!manager.is_active("run-1").await);
}

#[tokio::test]
async fn manager_rejects_duplicate_active_run_id() {
    let manager = RuntimeManager::default();
    let released = Arc::new(Notify::new());
    let released_for_task = released.clone();

    manager
        .spawn("run-1".to_string(), move |_cancel| async move {
            released_for_task.notified().await;
        })
        .await
        .unwrap();

    let duplicate = manager.spawn("run-1".to_string(), |_cancel| async {}).await;
    assert_eq!(duplicate.unwrap_err(), RunSpawnError::AlreadyRunning);

    released.notify_one();
    manager.wait_for_idle("run-1").await;
}
