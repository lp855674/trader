use data::Bar;
use paper::{PaperRunError, PaperRuntime, PaperSettings};
use runtime::CancellationFlag;
use rust_decimal::Decimal;
use storage::Db;

#[tokio::test]
async fn paper_runtime_stops_when_cancelled_before_next_bar() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    let mut settings = PaperSettings::sample();
    settings.run_id = "cancelled-paper".to_string();
    settings.bar_delay_ms = 1;
    let cancel = CancellationFlag::default();
    cancel.cancel();

    let result = PaperRuntime::new(db.clone(), settings)
        .run_bars_with_cancel(bars(), cancel)
        .await;

    let error = result.unwrap_err();
    assert_eq!(
        error.downcast_ref::<PaperRunError>(),
        Some(&PaperRunError::Cancelled)
    );
    assert!(
        db.get_strategy_run("cancelled-paper")
            .await
            .unwrap()
            .is_none()
    );
}

fn bars() -> Vec<Bar> {
    vec![
        Bar {
            ts_ms: 1,
            open: Decimal::from(100),
            high: Decimal::from(100),
            low: Decimal::from(100),
            close: Decimal::from(100),
            volume: Decimal::from(10),
        },
        Bar {
            ts_ms: 2,
            open: Decimal::from(101),
            high: Decimal::from(101),
            low: Decimal::from(101),
            close: Decimal::from(101),
            volume: Decimal::from(10),
        },
    ]
}
