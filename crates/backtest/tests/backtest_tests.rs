use backtest::{BacktestRuntime, BacktestSettings, BacktestSummary};
use data::{Bar, MarketSlice, SymbolBar};
use feature_store::FeatureRecord;
use rust_decimal_macros::dec;
use storage::Db;
use strategies::{StrategyAlphaGateConfig, StrategyUniverseFilterConfig};

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

#[tokio::test]
async fn backtest_runtime_applies_filtered_universe_rules() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    let mut settings = BacktestSettings::sample();
    settings.universe_name = "filtered".to_string();
    settings.symbols = vec![
        "US:NASDAQ:AAPL:EQUITY".to_string(),
        "US:NASDAQ:MSFT:EQUITY".to_string(),
    ];
    settings.universe_filter = StrategyUniverseFilterConfig {
        include_symbols: Vec::new(),
        exclude_symbols: vec!["US:NASDAQ:MSFT:EQUITY".to_string()],
        symbol_prefixes: Vec::new(),
        require_current_data: false,
        max_symbols: None,
        feature_rank: None,
    };
    let slices = vec![
        market_slice(1, dec!(10), dec!(30)),
        market_slice(2, dec!(11), dec!(31)),
        market_slice(3, dec!(20), dec!(40)),
    ];

    let summary = BacktestRuntime::new(db.clone(), settings)
        .run_market_slices(slices)
        .await
        .unwrap();

    assert_eq!(summary.signals, 1);
    assert_eq!(summary.orders, 1);
    let orders = db.list_orders("sample-ma-cross").await.unwrap();
    assert_eq!(orders.len(), 1);
    assert_eq!(orders[0].symbol, "US:NASDAQ:AAPL:EQUITY");
    let positions = db.list_positions("sample-ma-cross").await.unwrap();
    assert_eq!(positions.len(), 1);
    assert_eq!(positions[0].symbol, "US:NASDAQ:AAPL:EQUITY");
}

#[tokio::test]
async fn backtest_runtime_applies_alpha_feature_gate() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    let mut settings = BacktestSettings::sample();
    settings.alpha_gate = Some(StrategyAlphaGateConfig {
        run_id: "research-run".to_string(),
        feature_name: "quality_score".to_string(),
        version: None,
        min_value: Some(dec!(0.7)),
        max_value: None,
        records: vec![FeatureRecord::new(
            "research-run",
            "US:NASDAQ:AAPL:EQUITY",
            3,
            "quality_score",
            dec!(0.2),
            "v1",
        )],
    });
    let bars = vec![
        Bar::new(1, dec!(1), dec!(1), dec!(1), dec!(10), dec!(1)),
        Bar::new(2, dec!(1), dec!(1), dec!(1), dec!(11), dec!(1)),
        Bar::new(3, dec!(1), dec!(1), dec!(1), dec!(20), dec!(1)),
    ];

    let summary = BacktestRuntime::new(db.clone(), settings)
        .run(bars)
        .await
        .unwrap();

    assert_eq!(summary.signals, 0);
    assert_eq!(summary.orders, 0);
    let orders = db.list_orders("sample-ma-cross").await.unwrap();
    assert!(orders.is_empty());
}

#[tokio::test]
async fn backtest_runtime_opens_short_position_for_sell_alpha_signal() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    let mut settings = BacktestSettings::sample();
    settings.strategy_name = "price_channel_reversion".to_string();
    settings.alpha_name = "price_channel_reversion".to_string();
    settings.fast_window = 1;
    settings.slow_window = 2;
    settings.allow_short = true;
    let bars = vec![
        Bar::new(1, dec!(10), dec!(10), dec!(10), dec!(10), dec!(1)),
        Bar::new(2, dec!(11), dec!(11), dec!(11), dec!(11), dec!(1)),
        Bar::new(3, dec!(20), dec!(20), dec!(20), dec!(20), dec!(1)),
    ];

    let summary = BacktestRuntime::new(db.clone(), settings)
        .run(bars)
        .await
        .unwrap();

    assert_eq!(summary.signals, 1);
    assert_eq!(summary.orders, 1);
    let orders = db.list_orders("sample-ma-cross").await.unwrap();
    assert_eq!(orders[0].side, "SELL");
    let positions = db.list_positions("sample-ma-cross").await.unwrap();
    assert_eq!(positions[0].qty, "-1");
    assert_eq!(positions[0].avg_price, "20");
}

#[tokio::test]
async fn backtest_runtime_rejects_short_position_when_shorting_is_disabled() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    let mut settings = BacktestSettings::sample();
    settings.strategy_name = "price_channel_reversion".to_string();
    settings.alpha_name = "price_channel_reversion".to_string();
    settings.fast_window = 1;
    settings.slow_window = 2;
    let bars = vec![
        Bar::new(1, dec!(10), dec!(10), dec!(10), dec!(10), dec!(1)),
        Bar::new(2, dec!(11), dec!(11), dec!(11), dec!(11), dec!(1)),
        Bar::new(3, dec!(20), dec!(20), dec!(20), dec!(20), dec!(1)),
    ];

    let error = BacktestRuntime::new(db.clone(), settings)
        .run(bars)
        .await
        .unwrap_err();

    assert!(error.to_string().contains("short selling is disabled"));
    let orders = db.list_orders("sample-ma-cross").await.unwrap();
    assert!(orders.is_empty());
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
