use data::Bar;
use replay::ReplayRuntime;
use rust_decimal_macros::dec;

#[tokio::test]
async fn replay_emits_all_bars() {
    let bars = vec![
        Bar::new(1, dec!(1), dec!(1), dec!(1), dec!(10), dec!(1)),
        Bar::new(2, dec!(1), dec!(1), dec!(1), dec!(11), dec!(1)),
    ];
    let summary = ReplayRuntime::new(100).replay_bars(bars).await;

    assert_eq!(summary.bars, 2);
    assert_eq!(summary.speed, 100);
}
