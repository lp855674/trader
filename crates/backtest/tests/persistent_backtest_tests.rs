use backtest::{BacktestRuntime, BacktestSettings};
use data::Bar;
use rust_decimal_macros::dec;
use storage::Db;

#[tokio::test]
async fn backtest_persists_orders_and_positions() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    let bars = vec![
        Bar::new(1, dec!(1), dec!(1), dec!(1), dec!(10), dec!(1)),
        Bar::new(2, dec!(1), dec!(1), dec!(1), dec!(11), dec!(1)),
        Bar::new(3, dec!(1), dec!(1), dec!(1), dec!(20), dec!(1)),
    ];

    let summary = BacktestRuntime::new(db.clone(), BacktestSettings::sample())
        .run(bars)
        .await
        .unwrap();

    assert_eq!(summary.signals, 1);
    assert_eq!(db.list_orders("sample-ma-cross").await.unwrap().len(), 1);
    assert_eq!(db.list_positions("sample-ma-cross").await.unwrap().len(), 1);
    let insights = db.list_insights("sample-ma-cross").await.unwrap();
    assert_eq!(insights.len(), 1);
    assert_eq!(insights[0].symbol, "US:NASDAQ:AAPL:EQUITY");
    let targets = db.list_portfolio_targets("sample-ma-cross").await.unwrap();
    assert_eq!(targets.len(), 1);
    assert_eq!(targets[0].target_qty, "1");
}
