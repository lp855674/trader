use backtest::{BacktestRuntime, BacktestSettings, BacktestSummary};
use data::Bar;
use rust_decimal_macros::dec;
use storage::Db;

#[tokio::test]
async fn backtest_counts_signals() {
    let bars = vec![
        Bar::new(1, dec!(1), dec!(1), dec!(1), dec!(10), dec!(1)),
        Bar::new(2, dec!(1), dec!(1), dec!(1), dec!(11), dec!(1)),
        Bar::new(3, dec!(1), dec!(1), dec!(1), dec!(20), dec!(1)),
    ];
    let summary = BacktestRuntime::default().run(bars).await.unwrap();

    assert_eq!(
        summary,
        BacktestSummary {
            signals: 1,
            orders: 1
        }
    );
}

#[tokio::test]
async fn backtest_runtime_rejects_projected_exposure_above_limit() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    let mut settings = BacktestSettings::sample();
    settings.max_exposure = dec!(10);
    let bars = vec![
        Bar::new(1, dec!(1), dec!(1), dec!(1), dec!(10), dec!(1)),
        Bar::new(2, dec!(1), dec!(1), dec!(1), dec!(11), dec!(1)),
        Bar::new(3, dec!(1), dec!(1), dec!(1), dec!(20), dec!(1)),
    ];

    let result = BacktestRuntime::new(db, settings).run(bars).await;

    assert!(result.unwrap_err().to_string().contains("max exposure"));
}

#[tokio::test]
async fn backtest_runtime_uses_configured_universe_and_alpha_names() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    let mut settings = BacktestSettings::sample();
    settings.universe_name = "unknown_universe".to_string();
    let bars = vec![Bar::new(1, dec!(1), dec!(1), dec!(1), dec!(10), dec!(1))];

    let error = BacktestRuntime::new(db.clone(), settings)
        .run(bars.clone())
        .await
        .unwrap_err();
    assert!(error.to_string().contains("unknown universe"));

    let mut settings = BacktestSettings::sample();
    settings.alpha_name = "unknown_alpha".to_string();
    let error = BacktestRuntime::new(db, settings)
        .run(bars)
        .await
        .unwrap_err();
    assert!(error.to_string().contains("unknown strategy unknown_alpha"));
}
