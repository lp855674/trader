use data::Bar;
use events::{EventBus, TraderEvent};
use replay::ReplayRuntime;
use rust_decimal_macros::dec;

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
