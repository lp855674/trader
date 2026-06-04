use async_trait::async_trait;
use data::Bar;
use paper::{ExecutedPaperOrder, PaperOrderExecutor, PaperRuntime, PaperSettings};
use rust_decimal_macros::dec;
use storage::Db;
use trader_core::OrderRequest;

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

#[tokio::test]
async fn paper_runtime_uses_market_rules_for_crypto_spot_symbols() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    let mut settings = PaperSettings::sample();
    settings.symbol = "CRYPTO:BINANCE:BTCUSDT:CRYPTO_SPOT".to_string();
    settings.order_qty = dec!(0.001);
    settings.max_abs_qty = dec!(1);
    settings.max_order_qty = dec!(1);
    let bars = vec![
        Bar::new(1, dec!(1), dec!(1), dec!(1), dec!(10000), dec!(1)),
        Bar::new(2, dec!(1), dec!(1), dec!(1), dec!(11000), dec!(1)),
        Bar::new(3, dec!(1), dec!(1), dec!(1), dec!(12000), dec!(1)),
    ];

    let summary = PaperRuntime::new(db, settings)
        .run_bars(bars)
        .await
        .unwrap();

    assert_eq!(summary.orders, 1);
}

#[tokio::test]
async fn paper_runtime_rejects_projected_exposure_above_limit() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    let mut settings = PaperSettings::sample();
    settings.max_exposure = dec!(10);
    let bars = vec![
        Bar::new(1, dec!(1), dec!(1), dec!(1), dec!(10), dec!(1)),
        Bar::new(2, dec!(1), dec!(1), dec!(1), dec!(11), dec!(1)),
        Bar::new(3, dec!(1), dec!(1), dec!(1), dec!(20), dec!(1)),
    ];

    let result = PaperRuntime::new(db, settings).run_bars(bars).await;

    assert!(result.unwrap_err().to_string().contains("max exposure"));
}

#[tokio::test]
async fn paper_runtime_rejects_drawdown_above_limit() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    let mut settings = PaperSettings::sample();
    settings.max_drawdown = dec!(0);
    let bars = vec![
        Bar::new(1, dec!(1), dec!(1), dec!(1), dec!(10), dec!(1)),
        Bar::new(2, dec!(1), dec!(1), dec!(1), dec!(11), dec!(1)),
        Bar::new(3, dec!(1), dec!(1), dec!(1), dec!(20), dec!(1)),
        Bar::new(4, dec!(1), dec!(1), dec!(1), dec!(5), dec!(1)),
    ];

    let result = PaperRuntime::new(db, settings).run_bars(bars).await;

    assert!(result.unwrap_err().to_string().contains("max drawdown"));
}

#[tokio::test]
async fn paper_runtime_can_use_injected_order_executor() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    let bars = vec![
        Bar::new(1, dec!(1), dec!(1), dec!(1), dec!(10), dec!(1)),
        Bar::new(2, dec!(1), dec!(1), dec!(1), dec!(11), dec!(1)),
        Bar::new(3, dec!(1), dec!(1), dec!(1), dec!(20), dec!(1)),
    ];

    let summary = PaperRuntime::new_with_executor(
        db.clone(),
        PaperSettings::sample(),
        Box::new(FixedExecutor),
    )
    .run_bars(bars)
    .await
    .unwrap();

    assert_eq!(summary.orders, 1);
    let orders = db.list_orders("sample-ma-cross").await.unwrap();
    assert_eq!(orders[0].broker_order_id.as_deref(), Some("external-1"));
    assert_eq!(orders[0].status, "FILLED");
    let fills = db.list_fills("sample-ma-cross").await.unwrap();
    assert_eq!(fills[0].price, "19.5");
    assert_eq!(fills[0].fee, "0.01");
}

struct FixedExecutor;

#[async_trait]
impl PaperOrderExecutor for FixedExecutor {
    async fn execute_order(
        &self,
        order: OrderRequest,
        _mark_price: rust_decimal::Decimal,
        _order_number: usize,
    ) -> anyhow::Result<ExecutedPaperOrder> {
        Ok(ExecutedPaperOrder {
            broker_order_id: "external-1".to_string(),
            status: "FILLED".to_string(),
            price: dec!(19.5),
            qty: order.qty,
            fee: dec!(0.01),
        })
    }
}
