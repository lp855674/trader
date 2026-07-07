use backtest::{BacktestRuntime, BacktestSettings, BacktestSummary};
use data::{Bar, MarketSlice, SymbolBar};
use feature_store::FeatureRecord;
use rust_decimal_macros::dec;
use storage::{Db, ExternalFillCommand, ExternalOrderCommand, NewFeeRule, NewFeeRuleTier};
use strategies::{StrategyAlphaGateConfig, StrategyUniverseFilterConfig};

async fn insert_historical_fee_fill(
    db: &Db,
    account_id: &str,
    symbol: &str,
    price: &str,
    qty: &str,
    ts_ms: i64,
) {
    let price = price.parse().unwrap();
    let qty = qty.parse().unwrap();
    db.record_external_order(ExternalOrderCommand {
        order_id: "historical-backtest-order".to_string(),
        run_id: "historical-backtest-run".to_string(),
        client_order_id: "historical-backtest-client-order".to_string(),
        broker_order_id: None,
        account_id: account_id.to_string(),
        symbol: symbol.to_string(),
        side: "BUY".to_string(),
        order_type: "MARKET".to_string(),
        price: None,
        qty,
        filled_qty: qty,
        status: "FILLED".to_string(),
        ts_ms,
    })
    .await
    .unwrap();
    db.record_external_fill(ExternalFillCommand {
        id: "historical-backtest-fill".to_string(),
        order_id: "historical-backtest-order".to_string(),
        run_id: "historical-backtest-run".to_string(),
        symbol: symbol.to_string(),
        side: "BUY".to_string(),
        price,
        qty,
        fee: dec!(0),
        ts_ms,
    })
    .await
    .unwrap();
}

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

    let summary = BacktestRuntime::new(db.clone(), settings)
        .run(bars)
        .await
        .unwrap();

    assert_eq!(summary.signals, 1);
    assert_eq!(summary.orders, 0);
    let risk_events = db.list_risk_events("sample-ma-cross").await.unwrap();
    assert_eq!(risk_events.len(), 1);
    assert_eq!(risk_events[0].risk_type, "max_exposure");
    assert_eq!(risk_events[0].decision, "rejected");
    assert!(
        risk_events[0]
            .reason
            .as_deref()
            .is_some_and(|reason| reason.contains("max exposure"))
    );
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
async fn backtest_runtime_uses_configured_fee_rules_for_fills() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    db.insert_fee_rule(NewFeeRule {
        id: "fee-us-equity-backtest".to_string(),
        market: "US".to_string(),
        exchange: "NASDAQ".to_string(),
        asset_class: "EQUITY".to_string(),
        symbol: None,
        volume_window: "run".to_string(),
        maker_bps: "1".to_string(),
        taker_bps: "25".to_string(),
        minimum_fee: None,
        tax_bps: None,
        exchange_fee_bps: None,
        effective_from_ms: 0,
        effective_to_ms: None,
    })
    .await
    .unwrap();
    let settings = BacktestSettings::sample();
    let bars = vec![
        Bar::new(1, dec!(1), dec!(1), dec!(1), dec!(10), dec!(1)),
        Bar::new(2, dec!(1), dec!(1), dec!(1), dec!(11), dec!(1)),
        Bar::new(3, dec!(1), dec!(1), dec!(1), dec!(20), dec!(1)),
    ];

    let summary = BacktestRuntime::new(db.clone(), settings)
        .run(bars)
        .await
        .unwrap();

    assert_eq!(summary.signals, 1);
    assert_eq!(summary.orders, 1);
    let fills = db.list_fills("sample-ma-cross").await.unwrap();
    assert_eq!(fills.len(), 1);
    assert_eq!(fills[0].price, "20");
    assert_eq!(fills[0].qty, "1");
    assert_eq!(fills[0].fee, "0.05");
}

#[tokio::test]
async fn backtest_runtime_advances_fee_tiers_from_run_fill_notional() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    db.insert_fee_rule(NewFeeRule {
        id: "fee-us-equity-backtest-tiered".to_string(),
        market: "US".to_string(),
        exchange: "NASDAQ".to_string(),
        asset_class: "EQUITY".to_string(),
        symbol: None,
        volume_window: "run".to_string(),
        maker_bps: "99".to_string(),
        taker_bps: "99".to_string(),
        minimum_fee: Some("0.01".to_string()),
        tax_bps: Some("1".to_string()),
        exchange_fee_bps: Some("1".to_string()),
        effective_from_ms: 0,
        effective_to_ms: None,
    })
    .await
    .unwrap();
    db.insert_fee_rule_tier(NewFeeRuleTier {
        id: "fee-us-equity-backtest-tier-2".to_string(),
        fee_rule_id: "fee-us-equity-backtest-tiered".to_string(),
        volume_from: "20".to_string(),
        volume_to: None,
        maker_bps: "0.5".to_string(),
        taker_bps: "2".to_string(),
    })
    .await
    .unwrap();
    db.insert_fee_rule_tier(NewFeeRuleTier {
        id: "fee-us-equity-backtest-tier-1".to_string(),
        fee_rule_id: "fee-us-equity-backtest-tiered".to_string(),
        volume_from: "0".to_string(),
        volume_to: Some("20".to_string()),
        maker_bps: "1".to_string(),
        taker_bps: "8".to_string(),
    })
    .await
    .unwrap();
    let bars = vec![
        Bar::new(1, dec!(1), dec!(1), dec!(1), dec!(10), dec!(1)),
        Bar::new(2, dec!(1), dec!(1), dec!(1), dec!(11), dec!(1)),
        Bar::new(3, dec!(1), dec!(1), dec!(1), dec!(20), dec!(1)),
        Bar::new(4, dec!(1), dec!(1), dec!(1), dec!(1), dec!(1)),
        Bar::new(5, dec!(1), dec!(1), dec!(1), dec!(1), dec!(1)),
        Bar::new(6, dec!(1), dec!(1), dec!(1), dec!(30), dec!(1)),
    ];

    let mut settings = BacktestSettings::sample();
    settings.allow_short = true;
    let summary = BacktestRuntime::new(db.clone(), settings)
        .run(bars)
        .await
        .unwrap();

    assert_eq!(summary.orders, 3);
    let fills = db.list_fills("sample-ma-cross").await.unwrap();
    assert_eq!(fills.len(), 3);
    assert_eq!(fills[0].price, "20");
    assert_eq!(fills[0].fee, "0.02");
    assert_eq!(fills[1].price, "1");
    assert_eq!(fills[1].fee, "0.01");
    assert_eq!(fills[2].price, "30");
    assert_eq!(fills[2].qty, "2");
    assert_eq!(fills[2].fee, "0.024");
}

#[tokio::test]
async fn backtest_runtime_does_not_seed_run_fee_tier_from_account_volume() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    db.insert_fee_rule(NewFeeRule {
        id: "fee-us-equity-backtest-run-tiered".to_string(),
        market: "US".to_string(),
        exchange: "NASDAQ".to_string(),
        asset_class: "EQUITY".to_string(),
        symbol: None,
        volume_window: "run".to_string(),
        maker_bps: "99".to_string(),
        taker_bps: "99".to_string(),
        minimum_fee: None,
        tax_bps: None,
        exchange_fee_bps: None,
        effective_from_ms: 0,
        effective_to_ms: None,
    })
    .await
    .unwrap();
    db.insert_fee_rule_tier(NewFeeRuleTier {
        id: "fee-us-equity-backtest-run-tier-1".to_string(),
        fee_rule_id: "fee-us-equity-backtest-run-tiered".to_string(),
        volume_from: "0".to_string(),
        volume_to: Some("20".to_string()),
        maker_bps: "1".to_string(),
        taker_bps: "10".to_string(),
    })
    .await
    .unwrap();
    db.insert_fee_rule_tier(NewFeeRuleTier {
        id: "fee-us-equity-backtest-run-tier-2".to_string(),
        fee_rule_id: "fee-us-equity-backtest-run-tiered".to_string(),
        volume_from: "20".to_string(),
        volume_to: None,
        maker_bps: "0.5".to_string(),
        taker_bps: "2".to_string(),
    })
    .await
    .unwrap();
    let start_ms = 1_700_000_000_000;
    insert_historical_fee_fill(
        &db,
        "backtest",
        "US:NASDAQ:AAPL:EQUITY",
        "20",
        "1",
        start_ms - 24 * 60 * 60 * 1_000,
    )
    .await;
    let bars = vec![
        Bar::new(start_ms, dec!(1), dec!(1), dec!(1), dec!(10), dec!(1)),
        Bar::new(start_ms + 1, dec!(1), dec!(1), dec!(1), dec!(11), dec!(1)),
        Bar::new(start_ms + 2, dec!(1), dec!(1), dec!(1), dec!(20), dec!(1)),
    ];

    let summary = BacktestRuntime::new(db.clone(), BacktestSettings::sample())
        .run(bars)
        .await
        .unwrap();

    assert_eq!(summary.orders, 1);
    let fills = db.list_fills("sample-ma-cross").await.unwrap();
    assert_eq!(fills.len(), 1);
    assert_eq!(fills[0].price, "20");
    assert_eq!(fills[0].fee, "0.02");
}

#[tokio::test]
async fn backtest_runtime_seeds_fee_tier_from_rolling_account_volume() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    db.insert_fee_rule(NewFeeRule {
        id: "fee-us-equity-backtest-rolling-tiered".to_string(),
        market: "US".to_string(),
        exchange: "NASDAQ".to_string(),
        asset_class: "EQUITY".to_string(),
        symbol: None,
        volume_window: "rolling_30d".to_string(),
        maker_bps: "99".to_string(),
        taker_bps: "99".to_string(),
        minimum_fee: None,
        tax_bps: None,
        exchange_fee_bps: None,
        effective_from_ms: 0,
        effective_to_ms: None,
    })
    .await
    .unwrap();
    db.insert_fee_rule_tier(NewFeeRuleTier {
        id: "fee-us-equity-backtest-rolling-tier-1".to_string(),
        fee_rule_id: "fee-us-equity-backtest-rolling-tiered".to_string(),
        volume_from: "0".to_string(),
        volume_to: Some("20".to_string()),
        maker_bps: "1".to_string(),
        taker_bps: "10".to_string(),
    })
    .await
    .unwrap();
    db.insert_fee_rule_tier(NewFeeRuleTier {
        id: "fee-us-equity-backtest-rolling-tier-2".to_string(),
        fee_rule_id: "fee-us-equity-backtest-rolling-tiered".to_string(),
        volume_from: "20".to_string(),
        volume_to: None,
        maker_bps: "0.5".to_string(),
        taker_bps: "2".to_string(),
    })
    .await
    .unwrap();
    let start_ms = 1_700_000_000_000;
    insert_historical_fee_fill(
        &db,
        "backtest",
        "US:NASDAQ:AAPL:EQUITY",
        "20",
        "1",
        start_ms - 24 * 60 * 60 * 1_000,
    )
    .await;
    let bars = vec![
        Bar::new(start_ms, dec!(1), dec!(1), dec!(1), dec!(10), dec!(1)),
        Bar::new(start_ms + 1, dec!(1), dec!(1), dec!(1), dec!(11), dec!(1)),
        Bar::new(start_ms + 2, dec!(1), dec!(1), dec!(1), dec!(20), dec!(1)),
    ];

    let summary = BacktestRuntime::new(db.clone(), BacktestSettings::sample())
        .run(bars)
        .await
        .unwrap();

    assert_eq!(summary.orders, 1);
    let fills = db.list_fills("sample-ma-cross").await.unwrap();
    assert_eq!(fills.len(), 1);
    assert_eq!(fills[0].price, "20");
    assert_eq!(fills[0].fee, "0.004");
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

    let summary = BacktestRuntime::new(db.clone(), settings)
        .run(bars)
        .await
        .unwrap();

    assert_eq!(summary.signals, 1);
    assert_eq!(summary.orders, 0);
    let orders = db.list_orders("sample-ma-cross").await.unwrap();
    assert!(orders.is_empty());
    let risk_events = db.list_risk_events("sample-ma-cross").await.unwrap();
    assert_eq!(risk_events.len(), 1);
    assert_eq!(risk_events[0].risk_type, "short_selling_disabled");
    assert_eq!(risk_events[0].decision, "rejected");
    assert!(
        risk_events[0]
            .reason
            .as_deref()
            .is_some_and(|reason| reason.contains("short selling is disabled"))
    );
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
