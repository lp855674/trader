use backtest::{BacktestRuntime, BacktestSettings, BacktestSummary};
use data::{Bar, MarketSlice, SymbolBar};
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

#[tokio::test]
async fn backtest_runtime_runs_market_slices_for_multiple_symbols() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    let mut settings = BacktestSettings::sample();
    settings.symbols = vec![
        "US:NASDAQ:AAPL:EQUITY".to_string(),
        "US:NASDAQ:MSFT:EQUITY".to_string(),
    ];
    let slices = vec![
        market_slice(1, dec!(10), dec!(30)),
        market_slice(2, dec!(11), dec!(31)),
        market_slice(3, dec!(20), dec!(40)),
    ];

    let summary = BacktestRuntime::new(db.clone(), settings)
        .run_market_slices(slices)
        .await
        .unwrap();

    assert_eq!(summary.signals, 2);
    assert_eq!(summary.orders, 2);
    let orders = db.list_orders("sample-ma-cross").await.unwrap();
    assert_eq!(orders.len(), 2);
    assert_eq!(orders[0].symbol, "US:NASDAQ:AAPL:EQUITY");
    assert_eq!(orders[1].symbol, "US:NASDAQ:MSFT:EQUITY");
    let positions = db.list_positions("sample-ma-cross").await.unwrap();
    assert_eq!(positions.len(), 2);
}

fn market_slice(
    ts_ms: i64,
    aapl_close: rust_decimal::Decimal,
    msft_close: rust_decimal::Decimal,
) -> MarketSlice {
    MarketSlice::new(
        ts_ms,
        vec![
            SymbolBar::new(
                "US:NASDAQ:AAPL:EQUITY",
                Bar::new(
                    ts_ms,
                    aapl_close,
                    aapl_close,
                    aapl_close,
                    aapl_close,
                    dec!(1),
                ),
            ),
            SymbolBar::new(
                "US:NASDAQ:MSFT:EQUITY",
                Bar::new(
                    ts_ms,
                    msft_close,
                    msft_close,
                    msft_close,
                    msft_close,
                    dec!(1),
                ),
            ),
        ],
    )
}
