use backtest::BacktestSettings;
use data::Bar;
use paper::PaperRuntime;
use rust_decimal_macros::dec;
use storage::Db;

#[tokio::test]
async fn paper_runtime_persists_account_and_portfolio_state() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    let mut settings = BacktestSettings::sample();
    settings.account_id = "paper".to_string();
    let bars = vec![
        Bar::new(1, dec!(1), dec!(1), dec!(1), dec!(10), dec!(1)),
        Bar::new(2, dec!(1), dec!(1), dec!(1), dec!(11), dec!(1)),
        Bar::new(3, dec!(1), dec!(1), dec!(1), dec!(20), dec!(1)),
    ];

    let summary = PaperRuntime::new(db.clone(), settings.clone())
        .run_bars(bars)
        .await
        .unwrap();

    assert_eq!(summary.signals, 1);
    assert_eq!(summary.orders, 1);
    assert_eq!(db.list_orders(&settings.run_id).await.unwrap().len(), 1);
    assert_eq!(db.list_fills(&settings.run_id).await.unwrap().len(), 1);
    assert_eq!(db.list_positions(&settings.run_id).await.unwrap().len(), 1);
    assert_eq!(
        db.list_account_balances(&settings.run_id)
            .await
            .unwrap()
            .len(),
        1
    );
    assert!(
        !db.list_portfolio_snapshots(&settings.run_id)
            .await
            .unwrap()
            .is_empty()
    );
}
