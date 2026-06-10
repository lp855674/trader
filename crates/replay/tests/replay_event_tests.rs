use data::Bar;
use events::{EventBus, TraderEvent};
use replay::{ReplayController, ReplayRuntime};
use rust_decimal_macros::dec;
use std::sync::Arc;
use tokio::sync::Mutex;

#[tokio::test]
async fn replay_runtime_returns_market_events_for_each_bar() {
    let runtime = ReplayRuntime::new(1000);

    let summary = runtime
        .replay_bars_with_events(vec![
            Bar::new(1, dec!(100), dec!(100), dec!(100), dec!(100), dec!(1)),
            Bar::new(2, dec!(101), dec!(101), dec!(101), dec!(101), dec!(1)),
        ])
        .await;

    assert_eq!(summary.bars, 2);
    assert_eq!(summary.events.len(), 2);
    assert_eq!(summary.events[0].category, "market.bar");
}

#[tokio::test]
async fn replay_runtime_publishes_market_events_to_event_bus() {
    let event_bus = EventBus::new(8);
    let mut receiver = event_bus.subscribe();
    let runtime = ReplayRuntime::new(1000).with_event_bus(event_bus);

    let summary = runtime
        .replay_bars_with_events(vec![
            Bar::new(1, dec!(100), dec!(100), dec!(100), dec!(100), dec!(1)),
            Bar::new(2, dec!(101), dec!(101), dec!(101), dec!(101), dec!(1)),
        ])
        .await;

    assert_eq!(summary.bars, 2);
    let mut categories = Vec::new();
    while categories.len() < 2 {
        let event = tokio::time::timeout(std::time::Duration::from_secs(1), receiver.recv())
            .await
            .unwrap()
            .unwrap();
        if let TraderEvent::Runtime(runtime_event) = event.payload {
            categories.push(runtime_event.category);
        }
    }
    assert_eq!(categories, vec!["market.bar", "market.bar"]);
}

#[tokio::test]
async fn replay_runtime_publishes_market_events_with_run_id_source() {
    let event_bus = EventBus::new(8);
    let mut receiver = event_bus.subscribe();
    let runtime = ReplayRuntime::new_for_run("run-replay", 1000).with_event_bus(event_bus);

    runtime
        .replay_bars_with_events(vec![Bar::new(
            1,
            dec!(100),
            dec!(100),
            dec!(100),
            dec!(100),
            dec!(1),
        )])
        .await;

    let event = tokio::time::timeout(std::time::Duration::from_secs(1), receiver.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(event.source, "run-replay");
}

#[tokio::test]
async fn replay_runtime_pause_stops_running_replay_from_publishing_next_bar() {
    let event_bus = EventBus::new(8);
    let mut receiver = event_bus.subscribe();
    let controller = Arc::new(Mutex::new(ReplayController::new("run-paused", 10)));
    let runtime = ReplayRuntime::new_for_run("run-paused", 10)
        .with_event_bus(event_bus)
        .with_controller(controller.clone());

    let replay_task = tokio::spawn(async move {
        runtime
            .replay_bars_with_events(vec![
                Bar::new(1, dec!(100), dec!(100), dec!(100), dec!(100), dec!(1)),
                Bar::new(2, dec!(101), dec!(101), dec!(101), dec!(101), dec!(1)),
            ])
            .await
    });

    tokio::time::timeout(std::time::Duration::from_millis(200), receiver.recv())
        .await
        .unwrap()
        .unwrap();

    controller.lock().await.pause();

    let next_event =
        tokio::time::timeout(std::time::Duration::from_millis(250), receiver.recv()).await;
    assert!(
        next_event.is_err(),
        "paused replay should not publish the next bar"
    );

    controller.lock().await.resume();
    replay_task.await.unwrap();
}

#[tokio::test]
async fn replay_runtime_seek_changes_next_published_bar() {
    let event_bus = EventBus::new(8);
    let mut receiver = event_bus.subscribe();
    let controller = Arc::new(Mutex::new(ReplayController::new("run-seek", 10)));
    let runtime = ReplayRuntime::new_for_run("run-seek", 10)
        .with_event_bus(event_bus)
        .with_controller(controller.clone());

    let replay_task = tokio::spawn(async move {
        runtime
            .replay_bars_with_events(vec![
                Bar::new(1, dec!(100), dec!(100), dec!(100), dec!(100), dec!(1)),
                Bar::new(2, dec!(101), dec!(101), dec!(101), dec!(101), dec!(1)),
                Bar::new(3, dec!(102), dec!(102), dec!(102), dec!(102), dec!(1)),
            ])
            .await
    });

    let first = tokio::time::timeout(std::time::Duration::from_millis(200), receiver.recv())
        .await
        .unwrap()
        .unwrap();
    let TraderEvent::Runtime(first_event) = first.payload else {
        panic!("expected runtime event");
    };
    assert!(first_event.payload_json.contains("\"ts_ms\":1"));

    controller.lock().await.seek(2);

    let second = tokio::time::timeout(std::time::Duration::from_millis(300), receiver.recv())
        .await
        .unwrap()
        .unwrap();
    let TraderEvent::Runtime(second_event) = second.payload else {
        panic!("expected runtime event");
    };
    assert!(second_event.payload_json.contains("\"ts_ms\":3"));

    replay_task.await.unwrap();
}

#[tokio::test(start_paused = true)]
async fn replay_runtime_speed_change_affects_next_bar_pacing() {
    let event_bus = EventBus::new(8);
    let mut receiver = event_bus.subscribe();
    let controller = Arc::new(Mutex::new(ReplayController::new("run-speed", 5)));
    let runtime = ReplayRuntime::new_for_run("run-speed", 5)
        .with_event_bus(event_bus)
        .with_controller(controller.clone());

    let replay_task = tokio::spawn(async move {
        runtime
            .replay_bars_with_events(vec![
                Bar::new(1, dec!(100), dec!(100), dec!(100), dec!(100), dec!(1)),
                Bar::new(2, dec!(101), dec!(101), dec!(101), dec!(101), dec!(1)),
            ])
            .await
    });

    tokio::task::yield_now().await;
    tokio::time::advance(std::time::Duration::from_millis(200)).await;
    tokio::task::yield_now().await;
    receiver.recv().await.unwrap();

    controller.lock().await.set_speed(100);

    tokio::task::yield_now().await;
    tokio::time::advance(std::time::Duration::from_millis(10)).await;
    tokio::task::yield_now().await;
    receiver.recv().await.unwrap();

    replay_task.await.unwrap();
}
