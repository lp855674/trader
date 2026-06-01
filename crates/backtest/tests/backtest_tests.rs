use backtest::{BacktestRuntime, BacktestSummary};
use data::Bar;
use rust_decimal_macros::dec;

#[tokio::test]
async fn backtest_counts_signals() {
    let bars = vec![
        Bar::new(1, dec!(1), dec!(1), dec!(1), dec!(10), dec!(1)),
        Bar::new(2, dec!(1), dec!(1), dec!(1), dec!(11), dec!(1)),
        Bar::new(3, dec!(1), dec!(1), dec!(1), dec!(20), dec!(1)),
    ];
    let summary = BacktestRuntime.run(bars).await.unwrap();

    assert_eq!(
        summary,
        BacktestSummary {
            signals: 1,
            orders: 1
        }
    );
}
