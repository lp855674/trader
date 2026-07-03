use data::ingestion::{
    IngestionResult,
    binance_funding::{filter_funding_rates_after, parse_binance_funding_history},
    binance_meta::parse_binance_market_meta,
    corporate_actions::parse_yahoo_corporate_actions,
    http_retry::{FetchRetryPolicy, get_text_with_retry_policy},
    run_scheduled_ingestion,
    tracker::{IngestionTracker, last_ingestions, last_ingestions_with_staleness},
};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use std::time::Duration;

#[test]
fn ingestion_parses_binance_exchange_info_into_market_meta() {
    let payload = r#"
    {
      "symbols": [
        {
          "symbol": "BTCUSDT",
          "status": "TRADING",
          "baseAsset": "BTC",
          "quoteAsset": "USDT",
          "baseAssetPrecision": 8,
          "quoteAssetPrecision": 8,
          "filters": [
            { "filterType": "PRICE_FILTER", "tickSize": "0.01000000" },
            { "filterType": "LOT_SIZE", "minQty": "0.00001000", "maxQty": "9000.00000000", "stepSize": "0.00001000" },
            { "filterType": "MIN_NOTIONAL", "minNotional": "5.00000000" }
          ]
        }
      ]
    }
    "#;

    let records = parse_binance_market_meta(payload, 42).unwrap();

    assert_eq!(records.len(), 1);
    let meta = &records[0];
    assert_eq!(meta.exchange, "BINANCE");
    assert_eq!(meta.symbol, "BTCUSDT");
    assert_eq!(meta.base_asset, "BTC");
    assert_eq!(meta.quote_asset, "USDT");
    assert_eq!(meta.instrument_type, "SPOT");
    assert_eq!(meta.price_tick.as_deref(), Some("0.01000000"));
    assert_eq!(meta.qty_step.as_deref(), Some("0.00001000"));
    assert_eq!(meta.min_qty.as_deref(), Some("0.00001000"));
    assert_eq!(meta.max_qty.as_deref(), Some("9000.00000000"));
    assert_eq!(meta.min_notional.as_deref(), Some("5.00000000"));
    assert_eq!(meta.qty_precision, Some(8));
    assert!(meta.is_active);
    assert_eq!(meta.created_at_ms, 42);
    assert_eq!(meta.updated_at_ms, 42);
}

#[test]
fn ingestion_parses_binance_funding_history() {
    let payload = r#"
    [
      {
        "symbol": "BTCUSDT",
        "fundingRate": "0.00010000",
        "fundingTime": 1710000000000,
        "markPrice": "65000.12340000"
      },
      {
        "symbol": "BTCUSDT",
        "fundingRate": "-0.00005000",
        "fundingTime": 1710003600000
      }
    ]
    "#;

    let rates = parse_binance_funding_history(payload).unwrap();

    assert_eq!(rates.len(), 2);
    assert_eq!(rates[0].id, "binance-BTCUSDT-1710000000000");
    assert_eq!(rates[0].exchange, "BINANCE");
    assert_eq!(rates[0].symbol, "BTCUSDT");
    assert_eq!(rates[0].funding_time_ms, 1710000000000);
    assert_eq!(rates[0].funding_rate, "0.00010000");
    assert_eq!(rates[0].mark_price.as_deref(), Some("65000.12340000"));
    assert_eq!(rates[0].source, "binance_fapi_fundingRate");
    assert_eq!(rates[1].mark_price, None);
}

#[test]
fn ingestion_filters_funding_rates_after_latest_seen_timestamp() {
    let rates = vec![
        data::ingestion::FundingRate {
            id: "old".to_string(),
            exchange: "BINANCE".to_string(),
            symbol: "BTCUSDT".to_string(),
            funding_time_ms: 100,
            funding_rate: "0.1".to_string(),
            mark_price: None,
            source: "fixture".to_string(),
        },
        data::ingestion::FundingRate {
            id: "new".to_string(),
            exchange: "BINANCE".to_string(),
            symbol: "BTCUSDT".to_string(),
            funding_time_ms: 200,
            funding_rate: "0.2".to_string(),
            mark_price: None,
            source: "fixture".to_string(),
        },
    ];

    let filtered = filter_funding_rates_after(rates, Some(100));

    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].id, "new");
}

#[test]
fn ingestion_parses_yahoo_corporate_actions() {
    let payload = r#"
    {
      "chart": {
        "result": [
          {
            "events": {
              "dividends": {
                "1715731200": { "amount": 0.25, "date": 1715731200 }
              },
              "splits": {
                "1598832000": { "date": 1598832000, "numerator": 4.0, "denominator": 1.0, "splitRatio": "4:1" }
              }
            }
          }
        ],
        "error": null
      }
    }
    "#;

    let actions = parse_yahoo_corporate_actions("AAPL", payload, 42).unwrap();

    assert_eq!(actions.len(), 2);
    let dividend = actions
        .iter()
        .find(|action| action.action_type == "DIVIDEND")
        .unwrap();
    let split = actions
        .iter()
        .find(|action| action.action_type == "SPLIT")
        .unwrap();
    assert_eq!(dividend.symbol, "AAPL");
    assert_eq!(dividend.ex_date_ms, 1715731200000);
    assert_eq!(dividend.cash_amount.as_deref(), Some("0.25"));
    assert_eq!(dividend.source.as_deref(), Some("yahoo_chart"));
    assert_eq!(split.ratio.as_deref(), Some("4:1"));
}

#[tokio::test]
async fn ingestion_tracker_logs_status() {
    let db = storage::Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();

    IngestionTracker::log_ingestion(
        &db,
        &IngestionResult {
            source: "binance".to_string(),
            table: "funding_rates".to_string(),
            rows_fetched: 3,
            rows_upserted: 2,
        },
        25,
    )
    .await
    .unwrap();

    let statuses = last_ingestions(&db).await.unwrap();

    assert_eq!(statuses.len(), 1);
    assert_eq!(statuses[0].source, "binance");
    assert_eq!(statuses[0].table, "funding_rates");
    assert_eq!(statuses[0].rows_fetched, 3);
    assert_eq!(statuses[0].rows_upserted, 2);
    assert_eq!(statuses[0].duration_ms, 25);
}

#[tokio::test]
async fn ingestion_tracker_marks_stale_reference_data_and_logs_alert() {
    let db = storage::Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    let stale_after_ms = 12 * 60 * 60 * 1000;

    db.record_system_log(storage::SystemLogCommand {
        run_id: None,
        ts_ms: 1_000,
        level: "INFO".to_string(),
        target: "ingestion".to_string(),
        message: "ingested 2 rows into funding_rates from binance".to_string(),
        fields: Some(serde_json::json!({
            "source": "binance",
            "table": "funding_rates",
            "rows_fetched": 3,
            "rows_upserted": 2,
            "duration_ms": 25
        })),
    })
    .await
    .unwrap();

    let statuses = last_ingestions_with_staleness(&db, 1_000 + stale_after_ms + 1)
        .await
        .unwrap();

    assert_eq!(statuses.len(), 1);
    assert!(statuses[0].is_stale);
    assert_eq!(statuses[0].age_ms, stale_after_ms + 1);
    assert_eq!(statuses[0].stale_after_ms, stale_after_ms);

    let alerts = db
        .list_system_logs_filtered(storage::SystemLogFilter {
            target: Some("runtime.alert".to_string()),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(alerts.len(), 1);
    assert_eq!(alerts[0].message, "reference_data_stale.alert");
    let fields: serde_json::Value =
        serde_json::from_str(alerts[0].fields_json.as_ref().unwrap()).unwrap();
    assert_eq!(fields["source"], "binance");
    assert_eq!(fields["table"], "funding_rates");
    assert_eq!(fields["age_ms"], stale_after_ms + 1);
    assert_eq!(fields["stale_after_ms"], stale_after_ms);
}

#[tokio::test]
async fn ingestion_http_retry_recovers_after_rate_limit_response() {
    let attempts = Arc::new(AtomicUsize::new(0));
    let url = spawn_sequence_server(
        attempts.clone(),
        vec![
            TestHttpResponse::status(429, "rate limited"),
            TestHttpResponse::status(200, "{\"ok\":true}"),
        ],
    );
    let client = reqwest::Client::new();

    let body = get_text_with_retry_policy(
        &client,
        &url,
        &[],
        FetchRetryPolicy {
            max_attempts: 3,
            initial_backoff: Duration::from_millis(0),
        },
    )
    .await
    .unwrap();

    assert_eq!(body, "{\"ok\":true}");
    assert_eq!(attempts.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn ingestion_http_retry_recovers_after_server_error_response() {
    let attempts = Arc::new(AtomicUsize::new(0));
    let url = spawn_sequence_server(
        attempts.clone(),
        vec![
            TestHttpResponse::status(503, "temporarily unavailable"),
            TestHttpResponse::status(200, "{\"ok\":true}"),
        ],
    );
    let client = reqwest::Client::new();

    let body = get_text_with_retry_policy(
        &client,
        &url,
        &[],
        FetchRetryPolicy {
            max_attempts: 3,
            initial_backoff: Duration::from_millis(0),
        },
    )
    .await
    .unwrap();

    assert_eq!(body, "{\"ok\":true}");
    assert_eq!(attempts.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn ingestion_http_retry_recovers_after_timeout() {
    let attempts = Arc::new(AtomicUsize::new(0));
    let url = spawn_sequence_server(
        attempts.clone(),
        vec![
            TestHttpResponse::delayed_status(200, "too late", Duration::from_millis(100)),
            TestHttpResponse::status(200, "{\"ok\":true}"),
        ],
    );
    let client = reqwest::Client::builder()
        .timeout(Duration::from_millis(20))
        .build()
        .unwrap();

    let body = get_text_with_retry_policy(
        &client,
        &url,
        &[],
        FetchRetryPolicy {
            max_attempts: 3,
            initial_backoff: Duration::from_millis(0),
        },
    )
    .await
    .unwrap();

    assert_eq!(body, "{\"ok\":true}");
    assert_eq!(attempts.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn ingestion_scheduled_disabled_returns_no_work() {
    let db = storage::Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    let client = reqwest::Client::new();
    let config = config::IngestionConfig::default();

    let results = run_scheduled_ingestion(&db, &client, &config)
        .await
        .unwrap();

    assert!(results.is_empty());
}

struct TestHttpResponse {
    status: u16,
    body: &'static str,
    delay: Duration,
}

impl TestHttpResponse {
    fn status(status: u16, body: &'static str) -> Self {
        Self {
            status,
            body,
            delay: Duration::from_millis(0),
        }
    }

    fn delayed_status(status: u16, body: &'static str, delay: Duration) -> Self {
        Self {
            status,
            body,
            delay,
        }
    }
}

fn spawn_sequence_server(attempts: Arc<AtomicUsize>, responses: Vec<TestHttpResponse>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    std::thread::spawn(move || {
        for response in responses {
            let (mut stream, _) = listener.accept().unwrap();
            attempts.fetch_add(1, Ordering::SeqCst);
            std::thread::spawn(move || {
                let mut buffer = [0_u8; 1024];
                let _ = stream.read(&mut buffer);
                std::thread::sleep(response.delay);
                let payload = format!(
                    "HTTP/1.1 {} OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    response.status,
                    response.body.len(),
                    response.body
                );
                let _ = stream.write_all(payload.as_bytes());
            });
        }
    });
    url
}
