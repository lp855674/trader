use backtest::BacktestSettings;
use data::Bar;
use paper::PaperRuntime;
use rust_decimal_macros::dec;
use storage::Db;

#[tokio::test]
async fn paper_runtime_counts_orders() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    let bars = vec![
        Bar::new(1, dec!(1), dec!(1), dec!(1), dec!(10), dec!(1)),
        Bar::new(2, dec!(1), dec!(1), dec!(1), dec!(11), dec!(1)),
        Bar::new(3, dec!(1), dec!(1), dec!(1), dec!(20), dec!(1)),
    ];

    let summary = PaperRuntime::new(db, BacktestSettings::sample())
        .run_bars(bars)
        .await
        .unwrap();

    assert_eq!(summary.orders, 1);
}
