use data::{Bar, MarketSlice, SymbolBar};
use events::{EventBus, TraderEvent};
use paper::{PaperRunError, PaperRuntime, PaperSettings};
use runtime::CancellationFlag;
use rust_decimal_macros::dec;
use storage::{Db, NewMarketCalendar, NewTradingSessionRule};
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
    assert_eq!(db.list_portfolio_snapshots(&run_id).await.unwrap().len(), 4);
}

#[tokio::test]
async fn paper_runtime_runs_market_slices_from_stream() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    let mut settings = PaperSettings::sample();
    settings.run_id = "sample-market-slice-stream".to_string();
    settings.symbols = vec![
        "US:NASDAQ:AAPL:EQUITY".to_string(),
        "US:NASDAQ:MSFT:EQUITY".to_string(),
    ];
    let run_id = settings.run_id.clone();
    let (sender, receiver) = mpsc::channel(4);
    for market_slice in signal_market_slices() {
        sender.send(market_slice).await.unwrap();
    }
    drop(sender);

    let summary = PaperRuntime::new(db.clone(), settings)
        .run_market_slice_stream_with_cancel(receiver, CancellationFlag::default())
        .await
        .unwrap();

    assert_eq!(summary.orders, 2);
    assert_eq!(
        db.get_strategy_run(&run_id).await.unwrap().unwrap().status,
        "completed"
    );
    assert_eq!(db.list_orders(&run_id).await.unwrap().len(), 2);
    assert_eq!(db.list_positions(&run_id).await.unwrap().len(), 2);
    assert_eq!(db.list_portfolio_snapshots(&run_id).await.unwrap().len(), 4);
}

#[tokio::test]
async fn paper_runtime_publishes_algorithm_events_to_event_bus() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    let event_bus = EventBus::new(32);
    let mut receiver = event_bus.subscribe();

    let summary = PaperRuntime::new_with_event_bus(db, PaperSettings::sample(), event_bus)
        .run_bars(signal_bars())
        .await
        .unwrap();

    assert_eq!(summary.orders, 1);
    let mut categories = Vec::new();
    while categories.len() < 12 {
        let event = tokio::time::timeout(std::time::Duration::from_secs(1), receiver.recv())
            .await
            .unwrap()
            .unwrap();
        if let TraderEvent::Runtime(runtime_event) = event.payload {
            categories.push(runtime_event.category);
        }
    }
    assert!(categories.contains(&"algorithm.universe.selected".to_string()));
    assert!(categories.contains(&"algorithm.alpha.generated".to_string()));
    assert!(categories.contains(&"algorithm.oms.accepted".to_string()));
}

#[tokio::test]
async fn paper_runtime_stream_refreshes_storage_backed_market_calendar() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    let mut settings = PaperSettings::sample();
    settings.run_id = "stream-dynamic-calendar".to_string();
    let run_id = settings.run_id.clone();
    let (sender, receiver) = mpsc::channel(4);
    let runtime = PaperRuntime::new(db.clone(), settings);
    let task = tokio::spawn(async move {
        runtime
            .run_bar_stream_with_cancel(receiver, CancellationFlag::default())
            .await
    });

    sender.send(july_3_bar(0, dec!(10))).await.unwrap();
    sender.send(july_3_bar(60, dec!(11))).await.unwrap();
    db.upsert_market_calendar(NewMarketCalendar {
        id: "us-stream-dynamic-holiday".to_string(),
        market: "US".to_string(),
        trading_day: "2026-07-03".to_string(),
        is_open: false,
        session_template: Some("holiday".to_string()),
    })
    .await
    .unwrap();
    sender.send(july_3_bar(120, dec!(20))).await.unwrap();
    drop(sender);

    let summary = task.await.unwrap().unwrap();

    assert_eq!(summary.orders, 0);
    assert!(db.list_orders(&run_id).await.unwrap().is_empty());
    let events = db.list_events_by_source(&run_id).await.unwrap();
    assert!(events.iter().any(|event| {
        event.category == "algorithm.risk.rejected"
            && event.payload_json.contains("\"trading_session_closed\"")
            && event.payload_json.contains("market calendar closed")
    }));
}

#[tokio::test]
async fn paper_runtime_stream_refreshes_storage_backed_trading_sessions() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    let mut settings = PaperSettings::sample();
    settings.run_id = "stream-dynamic-session".to_string();
    let run_id = settings.run_id.clone();
    let (sender, receiver) = mpsc::channel(4);
    let runtime = PaperRuntime::new(db.clone(), settings);
    let task = tokio::spawn(async move {
        runtime
            .run_bar_stream_with_cancel(receiver, CancellationFlag::default())
            .await
    });

    sender.send(july_3_bar(0, dec!(10))).await.unwrap();
    sender.send(july_3_bar(60, dec!(11))).await.unwrap();
    db.upsert_market_calendar(NewMarketCalendar {
        id: "us-stream-dynamic-open".to_string(),
        market: "US".to_string(),
        trading_day: "2026-07-03".to_string(),
        is_open: true,
        session_template: Some("short".to_string()),
    })
    .await
    .unwrap();
    db.insert_trading_session_rule(NewTradingSessionRule {
        id: "us-stream-dynamic-short-session".to_string(),
        market: "US".to_string(),
        trading_day: "2026-07-03".to_string(),
        session_name: "short".to_string(),
        open_time: "00:00".to_string(),
        close_time: "02:00".to_string(),
        timezone: "UTC".to_string(),
    })
    .await
    .unwrap();
    sender.send(july_3_bar(120, dec!(20))).await.unwrap();
    drop(sender);

    let summary = task.await.unwrap().unwrap();

    assert_eq!(summary.orders, 0);
    assert!(db.list_orders(&run_id).await.unwrap().is_empty());
    let events = db.list_events_by_source(&run_id).await.unwrap();
    assert!(events.iter().any(|event| {
        event.category == "algorithm.risk.rejected"
            && event.payload_json.contains("\"trading_session_closed\"")
            && event
                .payload_json
                .contains("outside configured trading sessions")
    }));
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

fn july_3_bar(minute_offset: i64, close: rust_decimal::Decimal) -> Bar {
    Bar::new(
        1_783_036_800_000 + minute_offset * 60 * 1000,
        close,
        close,
        close,
        close,
        dec!(1),
    )
}

fn signal_market_slices() -> Vec<MarketSlice> {
    vec![
        market_slice(1, dec!(10), dec!(30)),
        market_slice(2, dec!(11), dec!(31)),
        market_slice(3, dec!(20), dec!(40)),
    ]
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
