use marketdata::analysis::market_depth::DepthSnapshot;
use marketdata::cache::TieredCache;
use marketdata::core::data::{DataItem, DataQuery, DataSource};
use marketdata::data_api::{DataGrpcService, DataServiceRequest};
use marketdata::data_config::DataConfigLoader;
use marketdata::data_sources::orderbook::to_depth_snapshot;
use marketdata::data_sources::{OrderBookConfig, OrderBookSource, PaperDataSource, TickAggregator};
use serde_json::Value;
use std::sync::Arc;

// 1. PaperDataSource → TieredCache pipeline
#[test]
fn paper_to_cache_pipeline() {
    let source = PaperDataSource::new_random("BTC", 100, 0, 60_000, 42);
    let query = DataQuery::new("BTC", 0, 100 * 60_000);
    let items = source.query(&query).unwrap();
    assert_eq!(items.len(), 100);

    let mut cache = TieredCache::new(10_000_000);
    cache.insert("BTC_0_6000000".to_string(), items.clone());
    let cached = cache.get("BTC_0_6000000");
    assert!(cached.is_some());
    assert_eq!(cached.unwrap().len(), 100);
}

// 2. OrderBookSource generates valid snapshots, DepthSnapshot metrics correct
#[test]
fn order_book_source_generates_valid_snapshots() {
    let config = OrderBookConfig {
        instrument: "ETH".to_string(),
        n_levels: 5,
        base_price: 2000.0,
        spread_bps: 4.0,
    };
    let source = OrderBookSource::new(config).with_generated_snapshots(0, 10, 1000);
    let query = DataQuery::new("ETH", 0, 10_000);
    let items = source.query(&query).unwrap();
    assert_eq!(items.len(), 10);

    for item in &items {
        let snap = to_depth_snapshot(item).unwrap();
        assert_eq!(snap.bids.len(), 5);
        assert_eq!(snap.asks.len(), 5);
        let spread = snap.spread_bps().unwrap();
        assert!(spread > 0.0, "spread should be positive");
        // best bid < best ask
        assert!(snap.best_bid().unwrap() < snap.best_ask().unwrap());
    }

    // DepthSnapshot imbalance in range
    let first_snap = to_depth_snapshot(&items[0]).unwrap();
    let imb = first_snap.imbalance(3);
    assert!(imb >= -1.0 && imb <= 1.0);
}

// 3. TickAggregator assembles bars from ticks
#[test]
fn tick_aggregator_assembles_bars() {
    let mut agg = TickAggregator::new(60_000);
    // Push 20 ticks across 3 different minute buckets
    for i in 0..20i64 {
        agg.push(DataItem::Tick {
            ts_ms: i * 10_000, // 10s intervals → 6 ticks per minute
            bid: 99.0,
            ask: 101.0,
            last: 100.0 + i as f64 * 0.1,
            volume: 10.0,
        });
    }
    let bars = agg.flush_all();
    assert!(!bars.is_empty(), "Should produce at least one bar");
    for bar in &bars {
        assert!(bar.volume > 0.0);
        assert!(bar.high >= bar.low);
        assert!(bar.open > 0.0);
        assert!(bar.close > 0.0);
    }
}

// 4. gRPC service handles query action
#[test]
fn grpc_service_handles_query_action() {
    let source = Arc::new(PaperDataSource::new_random("BTC", 50, 0, 60_000, 1));
    let service = DataGrpcService::new(source);

    // Test health
    let health_req = DataServiceRequest {
        action: "health".to_string(),
        payload: Value::Null,
    };
    let resp = service.handle(&health_req);
    assert!(resp.success);
    assert_eq!(resp.data["status"], "ok");

    // Test query
    let query_req = DataServiceRequest {
        action: "query".to_string(),
        payload: serde_json::json!({
            "instrument": "BTC",
            "start_ts_ms": 0,
            "end_ts_ms": 50 * 60_000
        }),
    };
    let resp = service.handle(&query_req);
    assert!(resp.success, "query should succeed");
    assert!(resp.data.is_array());
}

// 5. Config loader parses and validates
#[test]
fn config_loader_parses_and_validates() {
    let json = r#"{
        "sources": [
            {"name": "btc", "source_type": "paper", "params": {"n_bars": 100}}
        ],
        "cache": {"l1_capacity_mb": 128, "l2_capacity_mb": 1024, "ttl_ms": 300000},
        "quality": {"z_threshold": 3.5, "max_gap_ms": 120000, "min_volume": 1.0},
        "default_interval_ms": 60000
    }"#;

    let config = DataConfigLoader::from_json(json).unwrap();
    assert_eq!(config.sources[0].source_type, "paper");
    assert_eq!(config.cache.l1_capacity_mb, 128);
    assert!(DataConfigLoader::validate(&config).is_ok());

    // Invalid: l1 >= l2
    let mut bad_config = config.clone();
    bad_config.cache.l1_capacity_mb = 2000;
    assert!(DataConfigLoader::validate(&bad_config).is_err());
}
