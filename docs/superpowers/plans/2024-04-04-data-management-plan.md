# Data Management Implementation Plan

**Version**: 1.0.0  
**Priority**: P0  
**Estimated Duration**: 12 weeks  
**Dependencies**: None

---

## 1. Implementation Phases

### Phase 1: Core Framework & Data Sources (Weeks 1-2)
**Goal**: Establish data source abstractions and basic ingestion

| Task | Duration | Dependencies | Deliverables |
|------|----------|--------------|--------------|
| 1.1 DataSource Trait & DataItem | 2 days | None | `src/core/data/mod.rs` |
| 1.2 DataQuery & Granularity | 2 days | 1.1 | `src/core/data/mod.rs` |
| 1.3 FileParser (CSV/Parquet) | 3 days | 1.2 | `src/parser/file.rs` |
| 1.4 ApiParser with Rate Limiting | 3 days | 1.2 | `src/parser/api.rs` |
| 1.5 DataCleaner Rules | 3 days | 1.3 | `src/clean/mod.rs` |
| 1.6 TimeAligner & Gap Filler | 3 days | 1.5 | `src/align/mod.rs` |
| 1.7 MetadataManager | 2 days | 1.1 | `src/metadata/mod.rs` |
| 1.8 Integration Tests | 2 days | 1.1-1.7 | `tests/integration/data/*.rs` |

**Rollback Plan**: If parser performance degrades, switch to streaming processing instead of batch.

---

### Phase 2: Caching & Storage (Weeks 3-4)
**Goal**: Implement tiered caching and database storage

| Task | Duration | Dependencies | Deliverables |
|------|----------|--------------|--------------|
| 2.1 LruCache Implementation | 3 days | None | `src/cache/lru.rs` |
| 2.2 MmapCache for Large Files | 2 days | 2.1 | `src/cache/mmap.rs` |
| 2.3 TieredCache (Memory+Disk+DB) | 3 days | 2.1, 2.2 | `src/cache/mod.rs` |
| 2.4 PartitionedStorage (SQLite) | 3 days | 2.3 | `src/storage/sqlite.rs` |
| 2.5 BatchProcessor | 2 days | 2.4 | `src/storage/batch.rs` |
| 2.6 IndexOptimizer | 2 days | 2.4 | `src/storage/index.rs` |
| 2.7 Performance Benchmarks | 2 days | 2.1-2.6 | `benches/data/*.rs` |
| 2.8 Memory Profiling | 2 days | 2.1-2.6 | `benches/data/*.rs` |

**Rollback Plan**: If disk cache causes I/O bottlenecks, disable and use memory-only cache.

---

### Phase 3: Replay Engine & Quality (Weeks 5-6)
**Goal**: Build historical replay and data quality system

| Task | Duration | Dependencies | Deliverables |
|------|----------|--------------|--------------|
| 3.1 ReplayController | 4 days | 2.3 | `src/replay/mod.rs` |
| 3.2 ArbitraryGranularityReplay | 3 days | 3.1 | `src/replay/granularity.rs` |
| 3.3 ReplayCallback System | 2 days | 3.1 | `src/replay/callback.rs` |
| 3.4 QualityChecker | 3 days | 1.5 | `src/quality/mod.rs` |
| 3.5 QualityReport Generation | 2 days | 3.4 | `src/quality/report.rs` |
| 3.6 DataGapDetector | 2 days | 3.4 | `src/quality/gaps.rs` |
| 3.7 Integration Tests | 2 days | 3.1-3.6 | `tests/integration/data/*.rs` |
| 3.8 Chaos Testing | 2 days | 3.1-3.6 | `tests/chaos/data/*.rs` |

**Rollback Plan**: If replay performance degrades, switch to sequential processing.

---

### Phase 4: Advanced Features & Optimization (Weeks 7-8)
**Goal**: Implement data quality optimization and advanced features

| Task | Duration | Dependencies | Deliverables |
|------|----------|--------------|--------------|
| 4.1 CorrelationMatrix | 3 days | 2.3 | `src/analysis/correlation.rs` |
| 4.2 LiquidityRisk Calculator | 3 days | 4.1 | `src/analysis/liquidity.rs` |
| 4.3 MarketDepth Integration | 2 days | 4.2 | `src/analysis/market_depth.rs` |
| 4.4 OutlierDetection | 2 days | 1.5 | `src/clean/outliers.rs` |
| 4.5 DataNormalization | 2 days | 1.5 | `src/clean/normalize.rs` |
| 4.6 PolarsIntegration | 3 days | 2.3 | `src/polars/mod.rs` |
| 4.7 ZeroCopy DataItem | 2 days | 1.1 | `src/core/data/mod.rs` |
| 4.8 Performance Optimization | 2 days | 4.1-4.7 | `benches/data/*.rs` |

**Rollback Plan**: If Polars integration causes compilation issues, fall back to Arrow.

---

### Phase 5: Execution & API (Weeks 9-10)
**Goal**: Connect data system to execution and provide APIs

| Task | Duration | Dependencies | Deliverables |
|------|----------|--------------|--------------|
| 5.1 PaperAdapter Integration | 3 days | 3.1 | `src/trading/paper.rs` |
| 5.2 OrderBookSource | 3 days | 1.3 | `src/data/orderbook.rs` |
| 5.3 TickSource | 2 days | 1.3 | `src/data/tick.rs` |
| 5.4 gRPC Server | 3 days | 2.3 | `src/api/grpc.rs` |
| 5.5 HTTP REST API | 2 days | 5.4 | `src/api/http.rs` |
| 5.6 WebSocket Streaming | 2 days | 5.4 | `src/api/ws.rs` |
| 5.7 Configuration Schema | 2 days | 1.1 | `src/config/mod.rs` |
| 5.8 System Integration | 2 days | All | `src/main.rs` |

**Rollback Plan**: If streaming causes memory issues, implement buffer-based streaming.

---

### Phase 6: Production Readiness & Monitoring (Weeks 11-12)
**Goal**: Add monitoring, alerting, and production features

| Task | Duration | Dependencies | Deliverables |
|------|----------|--------------|--------------|
| 6.1 MetricsCollector | 3 days | 2.3 | `src/monitor/metrics.rs` |
| 6.2 AlertManager | 3 days | 6.1 | `src/monitor/alert.rs` |
| 6.3 DistributedTracing | 3 days | 1.1 | `src/monitor/tracing.rs` |
| 6.4 HealthCheck | 2 days | 6.1 | `src/api/health.rs` |
| 6.5 GracefulShutdown | 2 days | 2.3 | `src/lifecycle/mod.rs` |
| 6.6 Configuration HotReload | 2 days | 1.7 | `src/config/hot_reload.rs` |
| 6.7 Docker Compose | 2 days | 6.1-6.6 | `docker-compose.yml` |
| 6.8 Documentation | 2 days | All | `docs/data/*.md` |

**Rollback Plan**: If monitoring overhead is too high, disable detailed tracing.

---

## 2. Technical Architecture

### 2.1 Core Design Decisions

| Decision | Rationale | Trade-offs |
|----------|-----------|------------|
| **#[repr(C)] DataItem** | Zero-copy, predictable memory layout | Limited to known types |
| **Polars + SQLite** | High performance + persistence | External dependencies |
| **Lru+Disk+DB Cache** | Multi-tier for different access patterns | Complexity in eviction |
| **TimePartitioned Storage** | Efficient for time-series queries | Schema overhead |
| **Hybrid Parser** | Flexibility for different formats | Code complexity |
| **Event-Driven Architecture** | Decouples ingestion from processing | Latency concerns |

### 2.2 Architecture Diagram

```
┌─────────────────────────────────────────────────────────────┐
│                    Application Layer                         │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐          │
│  │ Replay      │  │ Analysis    │  │ Quality     │          │
│  │ Engine      │  │ Engine      │  │ Checker     │          │
│  └─────────────┘  └─────────────┘  └─────────────┘          │
└─────────────────────────────────────────────────────────────┘
                              │
┌─────────────────────────────────────────────────────────────┐
│                     Service Layer                            │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐          │
│  │ Data        │  │ Cache       │  │ Metadata    │          │
│  │ Fetcher     │  │ Manager     │  │ Manager     │          │
│  └─────────────┘  └─────────────┘  └─────────────┘          │
└─────────────────────────────────────────────────────────────┘
                              │
┌─────────────────────────────────────────────────────────────┐
│                      Core Layer                              │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐          │
│  │ Raw Parser  │  │ Aligner     │  │ LruCache    │          │
│  │ (CSV/API)   │  │ (Gap Fill)  │  │ (Memory)    │          │
│  └─────────────┘  └─────────────┘  └─────────────┘          │
└─────────────────────────────────────────────────────────────┘
                              │
┌─────────────────────────────────────────────────────────────┐
│                    Storage Layer                             │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐          │
│  │ Files       │  │ SQLite      │  │ Mmap        │          │
│  │ (CSV/       │  │ (Partitioned)│ │ (Large      │          │
│  │ Parquet)    │  │             │  │ Files)      │          │
│  └─────────────┘  └─────────────┘  └─────────────┘          │
└─────────────────────────────────────────────────────────────┘
```

### 2.3 Key Implementation Details

#### 2.3.1 DataItem (Zero-Copy)
```rust
#[repr(C)]
#[derive(Clone, Copy, PartialEq)]
pub enum DataItem {
    Bar { ts_ms: i64, open: f64, high: f64, low: f64, close: f64, volume: f64 },
    Tick { ts_ms: i64, price: f64, volume: f64, side: Side },
    OrderBook { ts_ms: i64, bids: Vec<(f64, f64)>, asks: Vec<(f64, f64)> },
}
```

#### 2.3.2 LruCache
```rust
pub struct LruCache<K, V> {
    pub capacity: usize,
    pub shrink_ratio: f64,
}

impl<K, V> LruCache<K, V> {
    pub fn get(&self, key: &K) -> Option<&V>;
    pub fn insert(&mut self, key: K, value: V);
}
```

#### 2.3.3 TimeAligner
```rust
pub struct TimeAligner {
    pub tolerance_ms: u64,
    pub gap_fill: GapFillStrategy,
}

impl TimeAligner {
    pub fn align(&self, data: &[DataItem]) -> Vec<DataItem>;
    pub fn detect_gaps(&self, data: &[DataItem]) -> Vec<TimeGap>;
}
```

---

## 3. Database Schema

### 3.1 Migration Files

#### 001_data_core.sql
```sql
-- Data items (partitioned by time)
CREATE TABLE data_items (
    id TEXT PRIMARY KEY,
    instrument_id TEXT NOT NULL,
    data_source_id TEXT NOT NULL,
    ts_ms INTEGER NOT NULL,
    data_type TEXT NOT NULL CHECK (data_type IN ('bar', 'tick', 'orderbook')),
    
    -- Field data (avoid JSON)
    o REAL, h REAL, l REAL, c REAL, v REAL,  -- bar only
    price REAL, volume REAL, side TEXT,       -- tick only
    
    quality_score REAL DEFAULT 1.0,
    created_at INTEGER NOT NULL,
    
    UNIQUE(instrument_id, data_source_id, ts_ms, data_type),
    INDEX(data_source_id, ts_ms),
    INDEX(instrument_id, ts_ms),
    PARTITION BY RANGE(ts_ms) (
        PARTITION p202401 VALUES LESS THAN (1704067200000),
        PARTITION p202402 VALUES LESS THAN (1706745600000),
        -- ...
    )
);

-- Instruments metadata
CREATE TABLE instruments (
    instrument_id TEXT PRIMARY KEY,
    venue TEXT NOT NULL,
    symbol TEXT NOT NULL,
    base_currency TEXT,
    quote_currency TEXT,
    tick_size REAL,
    lot_size REAL,
    trading_hours_json JSONB,
    created_at TIMESTAMP,
    updated_at TIMESTAMP
);

-- Data quality logs
CREATE TABLE quality_logs (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    instrument_id TEXT,
    data_source_id TEXT,
    check_type TEXT,
    score REAL,
    issues_json JSONB,
    ts_ms INTEGER
);
```

#### 002_data_optimization.sql
```sql
-- Data quality checks history
CREATE TABLE data_quality_checks (
    id BIGSERIAL PRIMARY KEY,
    instrument_id TEXT NOT NULL,
    data_type TEXT,
    from_ts INTEGER NOT NULL,
    to_ts INTEGER NOT NULL,
    gaps INTEGER DEFAULT 0,
    duplicates INTEGER DEFAULT 0,
    out_of_order INTEGER DEFAULT 0,
    last_checked_at TIMESTAMP DEFAULT NOW()
);

CREATE INDEX idx_quality_checks_instrument ON data_quality_checks(instrument_id);
```

---

## 4. Test Strategy

### 4.1 Unit Tests
```rust
#[test]
fn test_time_aligner_gap_detection() {
    let aligner = TimeAligner::new();
    let data = vec![Item(1000), Item(2000), Item(5000)];
    let gaps = aligner.detect_gaps(&data);
    assert_eq!(gaps.len(), 1);
}

#[test]
fn test_lru_cache_eviction() {
    let mut cache = LruCache::new(3);
    cache.insert(1, "a");
    cache.insert(2, "b");
    cache.insert(3, "c");
    cache.get(&1);
    cache.insert(4, "d");
    assert!(cache.contains_key(&1) == false);
}
```

### 4.2 Integration Tests
```rust
#[tokio::test]
async fn test_full_data_pipeline() {
    // 1. Import CSV data
    // 2. Verify quality
    // 3. Replay and validate signals
    // 4. Verify database consistency
}
```

---

## 5. API Contracts

### 5.1 gRPC
```proto
service DataService {
  rpc GetData(GetDataRequest) returns (GetDataResponse);
  rpc SubscribeData(SubscribeRequest) returns (stream DataEvent);
  rpc QualityCheck(QualityRequest) returns (QualityReport);
}

message GetDataRequest {
  string instrument_id = 1;
  DataType data_type = 2;
  int64 start_ts = 3;
  int64 end_ts = 4;
  Granularity granularity = 5;
}

message GetDataResponse {
  repeated DataItem items = 1;
  int64 total_count = 2;
}
```

---

## 6. Configuration Schema

```yaml
data:
  fetch:
    parallelism: 4
    batch_size: 1000
    timeout_ms: 5000
    retry_max: 3
    
  cache:
    lru_capacity: 1000000  # 1M items
    memory_limit_mb: 512
    disk_path: "/tmp/data_cache"
    
  cleaning:
    enabled: true
    dedupe_window_ms: 100
    outlier_threshold: 3.0
    
  quality:
    min_score: 0.8
    alert_on_drop: true
    
  metadata:
    refresh_interval_ms: 60000
    validate_on_load: true
```

---

## 7. Rollback Plan

- **Phase 1**: Revert to simple CSV parser, disable API fetching
- **Phase 2**: Disable disk cache, use memory-only
- **Phase 3**: Sequential replay, disable quality checks
- **Phase 4**: Remove Polars, use Arrow only
- **Phase 5**: Disable streaming, use batch API
- **Phase 6**: Disable tracing, basic monitoring only

---

## 8. Dependencies

```toml
polars = "0.40"
arrow = "5.0"
rayon = "1.8"
lru = "0.12"
chrono = "0.4"
```

---

This plan provides a comprehensive roadmap for building the Data Management System with multi-source support, tiered caching, and replay capabilities.
