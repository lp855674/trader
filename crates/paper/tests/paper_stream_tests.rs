use data::Bar;
use paper::{PaperRunError, PaperRuntime, PaperSettings};
use runtime::CancellationFlag;
use rust_decimal_macros::dec;
use storage::Db;
use tokio::sync::mpsc;

#[tokio::test]
async fn paper_runtime_runs_bars_from_stream() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    let settings = PaperSettings::sample();
    let run_id = settings.run_id.clone();
    let (sender, receiver) = mpsc::channel(4);
    for bar in signal_bars() {
        sender.send(bar).await.unwrap();
    }
    drop(sender);

    let summary = PaperRuntime::new(db.clone(), settings)
        .run_bar_stream_with_cancel(receiver, CancellationFlag::default())
        .await
        .unwrap();

    assert_eq!(summary.orders, 1);
    assert_eq!(
        db.get_strategy_run(&run_id).await.unwrap().unwrap().status,
        "completed"
    );
    assert_eq!(db.list_orders(&run_id).await.unwrap().len(), 1);
    assert_eq!(db.list_portfolio_snapshots(&run_id).await.unwrap().len(), 3);
}

#[tokio::test]
async fn paper_runtime_stream_stops_when_cancelled_before_first_bar() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    let mut settings = PaperSettings::sample();
    settings.run_id = "cancelled-paper-stream".to_string();
    let run_id = settings.run_id.clone();
    let (sender, receiver) = mpsc::channel(4);
    sender.send(signal_bars()[0].clone()).await.unwrap();
    drop(sender);
    let cancel = CancellationFlag::default();
    cancel.cancel();

    let result = PaperRuntime::new(db.clone(), settings)
        .run_bar_stream_with_cancel(receiver, cancel)
        .await;

    let error = result.unwrap_err();
    assert_eq!(
        error.downcast_ref::<PaperRunError>(),
        Some(&PaperRunError::Cancelled)
    );
    assert!(db.get_strategy_run(&run_id).await.unwrap().is_none());
}

fn signal_bars() -> Vec<Bar> {
    vec![
        Bar::new(1, dec!(1), dec!(1), dec!(1), dec!(10), dec!(1)),
        Bar::new(2, dec!(1), dec!(1), dec!(1), dec!(11), dec!(1)),
        Bar::new(3, dec!(1), dec!(1), dec!(1), dec!(20), dec!(1)),
    ]
}
