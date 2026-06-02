use data::Bar;
use paper::{PaperRuntime, PaperSettings};
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

    let summary = PaperRuntime::new(db, PaperSettings::sample())
        .run_bars(bars)
        .await
        .unwrap();

    assert_eq!(summary.orders, 1);
}

#[tokio::test]
async fn paper_runtime_rejects_order_above_max_order_qty() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    let mut settings = PaperSettings::sample();
    settings.order_qty = dec!(2);
    settings.max_order_qty = dec!(1);
    let bars = vec![
        Bar::new(1, dec!(1), dec!(1), dec!(1), dec!(10), dec!(1)),
        Bar::new(2, dec!(1), dec!(1), dec!(1), dec!(11), dec!(1)),
        Bar::new(3, dec!(1), dec!(1), dec!(1), dec!(20), dec!(1)),
    ];

    let result = PaperRuntime::new(db, settings).run_bars(bars).await;

    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("max order quantity")
    );
}
