use data::Bar;
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
