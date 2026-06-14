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
