use broker::BrokerKind;
use runtime::{CancellationFlag, LiveRuntime, LiveRuntimeSettings};
use storage::Db;

#[tokio::test]
async fn live_runtime_starts_reports_broker_status_and_stops_without_orders() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    let settings = LiveRuntimeSettings {
        run_id: "live-1".to_string(),
        broker_kind: BrokerKind::Futu,
    };
    let live = LiveRuntime::new(db.clone(), settings);
    let cancel = CancellationFlag::default();
    let task_cancel = cancel.clone();

    let handle = tokio::spawn(async move { live.run(task_cancel).await.unwrap() });

    wait_for_status(&db, "live-1", "running").await;
    let status = LiveRuntime::new(
        db.clone(),
        LiveRuntimeSettings {
            run_id: "live-1".to_string(),
            broker_kind: BrokerKind::Futu,
        },
    )
    .broker_status()
    .await
    .unwrap();
    assert_eq!(status.kind, BrokerKind::Futu);
    assert!(status.connected);
    assert!(db.list_orders("live-1").await.unwrap().is_empty());

    let events = db.list_events_by_source("live-1").await.unwrap();
    assert!(events.iter().any(|event| event.category == "live.started"));

    cancel.cancel();
    handle.await.unwrap();

    let run = db.get_strategy_run("live-1").await.unwrap().unwrap();
    assert_eq!(run.status, "stopped");
    let events = db.list_events_by_source("live-1").await.unwrap();
    assert!(events.iter().any(|event| event.category == "live.stopped"));
}

async fn wait_for_status(db: &Db, run_id: &str, expected: &str) {
    for _ in 0..50 {
        if let Some(run) = db.get_strategy_run(run_id).await.unwrap()
            && run.status == expected
        {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    panic!("run {run_id} did not reach {expected}");
}
