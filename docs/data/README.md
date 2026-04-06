# Data Management System Documentation

## Architecture

The `marketdata` crate provides high-performance market data ingestion, storage, analysis, and streaming.

### Core Modules

| Module | Purpose |
|--------|---------|
| `core/` | `DataItem`, `DataQuery`, `DataSource` trait |
| `parser/` | CSV/Parquet file parsing |
| `clean/` | Data cleaning and gap filling |
| `align/` | Time-series alignment and resampling |
| `cache/` | LRU, mmap, tiered caching |
| `storage/` | SQLite partitioned storage with WAL |
| `replay/` | Historical data replay engine |
| `quality/` | Data quality checking and reporting |
| `analysis/` | Correlation, liquidity, market depth, normalization |
| `data_sources/` | Paper, orderbook, tick data sources |
| `data_api/` | gRPC, HTTP, WebSocket APIs |
| `monitor/` | Metrics, alerts, distributed tracing |
| `lifecycle/` | Graceful shutdown |

## Data Format Specifications

### NormalizedBar
```rust
struct NormalizedBar {
    ts_ms: i64,    // Unix timestamp in milliseconds
    open: f64,
    high: f64,
    low: f64,
    close: f64,
    volume: f64,
}
```

### DataQuery
```rust
struct DataQuery {
    instrument: String,
    start_ms: u64,
    end_ms: u64,
    granularity: Granularity,  // Tick, Min1, Min5, Hour1, Day1, ...
}
```

## API Reference

### HTTP Endpoints
- `GET /data/bars?instrument=AAPL&start=...&end=...` — Query bars
- `POST /data/upload` — Upload CSV/Parquet file
- `GET /data/metadata` — List available instruments
- `GET /data/export?format=csv` — Export data

### WebSocket
- `/ws/stream?instrument=AAPL` — Real-time bar streaming

### gRPC
- `DataService/QueryBars` — Query historical bars
- `DataService/StreamBars` — Stream real-time bars

## Performance

| Operation | Latency (p95) | Throughput |
|-----------|---------------|------------|
| Bar ingestion | < 1ms | 100k+ events/sec |
| Query (1k bars) | < 5ms | — |
| Query (100k bars) | < 50ms | — |
| Correlation (1k bars) | < 2ms | — |

## Configuration

```json
{
  "cache": {
    "max_bytes": 536870912,
    "ttl_secs": 3600
  },
  "storage": {
    "db_path": "data/marketdata.db",
    "partition_by": "month"
  },
  "quality": {
    "min_quality_score": 0.95,
    "max_gap_secs": 3600
  }
}
```

## Troubleshooting

| Symptom | Cause | Fix |
|---------|-------|-----|
| Stale data | Cache not invalidated | Restart with `--flush-cache` |
| Missing bars | Gap in source data | Check gap report in `quality/` |
| Slow queries | Missing index | Run `IndexOptimizer::rebuild()` |
| High memory | LRU cache too large | Reduce `cache.max_bytes` |
