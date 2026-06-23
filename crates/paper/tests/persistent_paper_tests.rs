use data::Bar;
use paper::{PaperRuntime, PaperSettings};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use std::str::FromStr;
use storage::Db;

#[tokio::test]
async fn paper_runtime_persists_account_and_portfolio_state() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    let settings = PaperSettings::sample();
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

#[tokio::test]
async fn paper_runtime_persists_cash_and_position_snapshots() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    let settings = PaperSettings::sample();
    let bars = vec![
        Bar::new(1, dec!(1), dec!(1), dec!(1), dec!(10), dec!(1)),
        Bar::new(2, dec!(1), dec!(1), dec!(1), dec!(11), dec!(1)),
        Bar::new(3, dec!(1), dec!(1), dec!(1), dec!(20), dec!(1)),
    ];

    PaperRuntime::new(db.clone(), settings.clone())
        .run_bars(bars)
        .await
        .unwrap();

    let portfolio_snapshots = db.list_portfolio_snapshots(&settings.run_id).await.unwrap();
    let cash_snapshots = db.list_cash_snapshots(&settings.run_id).await.unwrap();
    assert_eq!(cash_snapshots.len(), portfolio_snapshots.len());
    let final_cash = cash_snapshots.last().unwrap();
    assert_eq!(final_cash.currency, "USD");
    assert_eq!(final_cash.cash, "99980");
    assert_eq!(final_cash.available_cash, "99980");
    assert_eq!(final_cash.frozen_cash, "0");

    let position_snapshots = db.list_position_snapshots(&settings.run_id).await.unwrap();
    assert!(!position_snapshots.is_empty());
    let final_position = position_snapshots.last().unwrap();
    assert_eq!(final_position.market, "US");
    assert_eq!(final_position.exchange, "NASDAQ");
    assert_eq!(final_position.symbol, "US:NASDAQ:AAPL:EQUITY");
    assert_eq!(final_position.asset_class, "EQUITY");
    assert_eq!(final_position.qty, "1");
    assert_eq!(final_position.available_qty, "1");
    assert_eq!(final_position.avg_price.as_deref(), Some("20"));
    assert_eq!(final_position.currency, "USD");
}

#[tokio::test]
async fn paper_runtime_uses_initial_cash_and_broker_settings() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    let mut settings = PaperSettings::sample();
    settings.initial_cash = dec!(100000);
    settings.slippage_bps = dec!(100);
    settings.fee_bps = dec!(10);
    let bars = vec![
        Bar::new(1, dec!(1), dec!(1), dec!(1), dec!(10), dec!(1)),
        Bar::new(2, dec!(1), dec!(1), dec!(1), dec!(11), dec!(1)),
        Bar::new(3, dec!(1), dec!(1), dec!(1), dec!(20), dec!(1)),
    ];

    PaperRuntime::new(db.clone(), settings.clone())
        .run_bars(bars)
        .await
        .unwrap();

    let balances = db.list_account_balances(&settings.run_id).await.unwrap();
    assert_eq!(
        Decimal::from_str(&balances[0].total).unwrap(),
        dec!(99979.7798)
    );
    let fills = db.list_fills(&settings.run_id).await.unwrap();
    assert_eq!(Decimal::from_str(&fills[0].price).unwrap(), dec!(20.20));
    assert_eq!(Decimal::from_str(&fills[0].fee).unwrap(), dec!(0.0202));
}

#[tokio::test]
async fn paper_runtime_persists_realized_and_unrealized_pnl() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    let mut settings = PaperSettings::sample();
    settings.initial_cash = dec!(100000);
    settings.allow_short = true;
    let bars = vec![
        Bar::new(1, dec!(1), dec!(1), dec!(1), dec!(10), dec!(1)),
        Bar::new(2, dec!(1), dec!(1), dec!(1), dec!(11), dec!(1)),
        Bar::new(3, dec!(1), dec!(1), dec!(1), dec!(20), dec!(1)),
        Bar::new(4, dec!(1), dec!(1), dec!(1), dec!(1), dec!(1)),
    ];

    let summary = PaperRuntime::new(db.clone(), settings.clone())
        .run_bars(bars)
        .await
        .unwrap();

    assert_eq!(summary.orders, 2);
    let snapshots = db.list_portfolio_snapshots(&settings.run_id).await.unwrap();
    let last = snapshots.last().unwrap();
    assert_eq!(last.realized_pnl, "-19");
    assert_eq!(last.unrealized_pnl, "0");
}

#[tokio::test]
async fn paper_runtime_captures_structured_system_logs() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    let settings = PaperSettings::sample();
    let bars = vec![
        Bar::new(1, dec!(1), dec!(1), dec!(1), dec!(10), dec!(1)),
        Bar::new(2, dec!(1), dec!(1), dec!(1), dec!(11), dec!(1)),
        Bar::new(3, dec!(1), dec!(1), dec!(1), dec!(20), dec!(1)),
    ];

    PaperRuntime::new(db.clone(), settings.clone())
        .run_bars(bars)
        .await
        .unwrap();

    let logs = db
        .list_system_logs_filtered(storage::SystemLogFilter {
            run_id: Some(settings.run_id.clone()),
            ..Default::default()
        })
        .await
        .unwrap();
    let messages = logs
        .iter()
        .map(|log| log.message.as_str())
        .collect::<Vec<_>>();
    assert!(messages.contains(&"paper run started"), "{messages:?}");
    assert!(
        messages.contains(&"algorithm alpha generated"),
        "{messages:?}"
    );
    assert!(
        messages.contains(&"algorithm portfolio target generated"),
        "{messages:?}"
    );
    assert!(
        messages.contains(&"algorithm risk approved"),
        "{messages:?}"
    );
    assert!(
        messages.contains(&"algorithm execution order generated"),
        "{messages:?}"
    );
    assert!(messages.contains(&"paper order submitted"), "{messages:?}");
    assert!(messages.contains(&"paper order filled"), "{messages:?}");
    assert!(
        messages.contains(&"algorithm execution applied"),
        "{messages:?}"
    );
    assert!(messages.contains(&"paper run completed"), "{messages:?}");
}
