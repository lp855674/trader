use runtime::{RunSpawnError, RuntimeManager, RuntimeRunMetadata, RuntimeRunStatus};
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
    assert_eq!(
        manager.status("run-1").await,
        Some(RuntimeRunStatus::Running)
    );
    let running_info = manager.info("run-1").await.unwrap();
    assert_eq!(running_info.status, RuntimeRunStatus::Running);
    let metadata = manager.metadata("run-1").await.unwrap();
    assert_eq!(metadata.mode, None);
    let snapshot = manager.snapshot("run-1").await.unwrap();
    assert_eq!(snapshot.info, running_info);
    assert_eq!(snapshot.metadata.mode, None);
    assert!(running_info.started_at_ms > 0);
    assert_eq!(
        running_info.started_at_ms,
        running_info.last_state_change_at_ms
    );
    assert!(manager.cancel("run-1").await);
    assert_eq!(
        manager.status("run-1").await,
        Some(RuntimeRunStatus::CancelRequested)
    );
    let cancel_info = manager.info("run-1").await.unwrap();
    assert_eq!(cancel_info.status, RuntimeRunStatus::CancelRequested);
    assert_eq!(cancel_info.started_at_ms, running_info.started_at_ms);
    assert!(cancel_info.last_state_change_at_ms >= running_info.last_state_change_at_ms);
    released.notify_one();
    manager.wait_for_idle("run-1").await;

    assert!(observed_cancel.load(Ordering::SeqCst));
    assert!(!manager.is_active("run-1").await);
    assert_eq!(
        manager.status("run-1").await,
        Some(RuntimeRunStatus::Canceled)
    );
    let terminal_info = manager.info("run-1").await.unwrap();
    assert_eq!(terminal_info.status, RuntimeRunStatus::Canceled);
    assert!(terminal_info.last_state_change_at_ms >= cancel_info.last_state_change_at_ms);
    assert_eq!(manager.metadata("run-1").await.unwrap().mode, metadata.mode);
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

#[tokio::test]
async fn manager_tracks_runtime_metadata_for_spawned_run() {
    let manager = RuntimeManager::default();
    let released = Arc::new(Notify::new());
    let released_for_task = released.clone();

    manager
        .spawn_with_metadata(
            "run-2".to_string(),
            RuntimeRunMetadata {
                mode: Some("paper".to_string()),
            },
            move |_cancel| async move {
                released_for_task.notified().await;
            },
        )
        .await
        .unwrap();

    let metadata = manager.metadata("run-2").await.unwrap();
    assert_eq!(metadata.mode.as_deref(), Some("paper"));
    let snapshot = manager.snapshot("run-2").await.unwrap();
    assert_eq!(snapshot.metadata.mode.as_deref(), Some("paper"));
    assert_eq!(snapshot.info.status, RuntimeRunStatus::Running);

    released.notify_one();
    manager.wait_for_idle("run-2").await;
    assert!(!manager.is_active("run-2").await);
    assert_eq!(
        manager.status("run-2").await,
        Some(RuntimeRunStatus::Completed)
    );
    assert_eq!(
        manager.metadata("run-2").await.unwrap().mode.as_deref(),
        Some("paper")
    );
}

#[tokio::test]
async fn manager_lists_only_active_runs() {
    let manager = RuntimeManager::default();
    let released = Arc::new(Notify::new());
    let released_for_task = released.clone();

    manager
        .spawn("run-active".to_string(), move |_cancel| async move {
            released_for_task.notified().await;
        })
        .await
        .unwrap();

    manager
        .spawn("run-completed".to_string(), |_cancel| async {})
        .await
        .unwrap();
    manager.wait_for_idle("run-completed").await;

    let active = manager.list_active().await;
    assert_eq!(active.len(), 1);
    assert_eq!(active[0].0, "run-active");
    assert_eq!(active[0].1.info.status, RuntimeRunStatus::Running);

    released.notify_one();
    manager.wait_for_idle("run-active").await;
}
