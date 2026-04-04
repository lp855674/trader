# 数据管理系统架构设计

**日期**: 2024-04-04  
**优先级**: P0  
**状态**: 待审批

---

## 1. 概述

### 1.1 目标

构建**多数据源、高吞吐、任意粒度**的数据管理系统，支持：
- **多数据源**：K 线、Tick、订单簿、外部 API、文件导入
- **数据清洗**：自动去重、时间对齐、异常值处理
- **历史回放**：任意粒度时间序列回放
- **高性能缓存**：Lru 缓存 + 数据库 + 内存映射
- **数据质量**：完整性校验、一致性检查
- **三种模式**：实盘、回测、离线分析

### 1.2 设计原则

- **统一抽象**：所有数据源实现统一 `DataSource` trait
- **增量处理**：支持全量导入和增量更新
- **时间分区**：数据按时间分区存储，支持并行处理
- **零拷贝**：内存布局优化，减少序列化开销

---

## 2. 核心架构

### 2.1 分层架构

```
┌─────────────────────────────────────────────────────────────┐
│                     应用层 (Application)                     │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌────────────┐  │
│  │ Replay   │  │ Analysis │  │ Backtest │  │ Strategy   │  │
│  │ Engine   │  │ Engine   │  │ Engine   │  │ Engine     │  │
│  └──────────┘  └──────────┘  └──────────┘  └────────────┘  │
└─────────────────────────────────────────────────────────────┘
                             │
┌─────────────────────────────────────────────────────────────┐
│                     服务层 (Service)                         │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌────────────┐  │
│  │ Data     │  │ Cache    │  │ Quality  │  │ Metadata   │  │
│  │ Fetcher  │  │ Manager  │  │ Checker  │  │ Manager    │  │
│  └──────────┘  └──────────┘  └──────────┘  └────────────┘  │
└─────────────────────────────────────────────────────────────┘
                             │
┌─────────────────────────────────────────────────────────────┐
│                     核心层 (Core)                             │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌────────────┐  │
│  │ Raw Data │  │ Normalized│  │ Time     │  │ Index      │  │
│  │ Parser   │  │ Converter│  │ Aligner  │  │ Manager    │  │
│  └──────────┘  └──────────┘  └──────────┘  └────────────┘  │
└─────────────────────────────────────────────────────────────┘
                             │
┌─────────────────────────────────────────────────────────────┐
│                    存储层 (Storage)                           │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌────────────┐  │
│  │ Files    │  │ Database │  │ Memory   │  │ External   │  │
│  │ (CSV/    │  │ (SQLite) │  │ (Lru)    │  │ (S3/GCS)  │  │
│  │ Parquet) │  │          │  │          │  │            │  │
│  └──────────┘  └──────────┘  └──────────┘  └────────────┘  │
└─────────────────────────────────────────────────────────────┘
```

### 2.2 数据流

```
┌──────────┐    ┌──────────┐    ┌──────────┐    ┌──────────┐
│ Raw Data │───>│ Parser   │───>│ Aligner  │───>│ Database │
│ (CSV/API)│    │          │    │ (Gap Fill)│    │          │
└──────────┘    └──────────┘    └──────────┘    └──────────┘
                              │
                              v
                    ┌──────────┐
                    │ Cache    │
                    │ (Lru)    │
                    └──────────┘
```

---

## 3. 详细设计

### 3.1 核心 Trait 定义

#### 3.1.1 数据源抽象

```rust
// 统一的数据源接口
pub trait DataSource: Send + Sync {
    type Item: DataItem + Send + Sync;
    
    // 获取数据
    fn get(&self, query: DataQuery) -> Result<DataResult, FetchError>;
    
    // 订阅实时数据
    fn subscribe(&self, callback: Box<dyn Fn(DataEvent) + Send + Sync>);
    
    // 检查数据质量
    fn quality_check(&self) -> QualityReport;
}

// 数据项抽象（统一内存布局）
#[repr(C)]
#[derive(Clone, Copy, PartialEq)]
pub enum DataItem {
    Bar { ts_ms: i64, open: f64, high: f64, low: f64, close: f64, volume: f64 },
    Tick { ts_ms: i64, price: f64, volume: f64, side: Side },
    OrderBook { ts_ms: i64, bids: Vec<(f64, f64)>, asks: Vec<(f64, f64)> },
}
```

#### 3.1.2 数据查询

```rust
pub struct DataQuery {
    pub instrument: InstrumentId,
    pub data_type: DataType,  // Bar, Tick, OrderBook
    pub start_ts: i64,
    pub end_ts: i64,
    pub granularity: Granularity, // 1s, 1m, 5m, 1h, 1d
    pub limit: Option<usize>,
    pub offset: usize,
}

pub enum Granularity {
    Second,     // 秒级
    Minute,     // 分钟级
    Hour,       // 小时级
    Day,        // 日级
    Custom,     // 自定义（任意粒度）
}
```

### 3.2 数据解析与清洗

#### 3.2.1 解析器

```rust
// CSV/Parquet 解析
pub struct FileParser {
    pub batch_size: usize,
    pub parallelism: usize,
}

impl FileParser {
    pub fn parse<T: AsRef<Path>>(path: T) -> Result<Vec<DataItem>, ParseError>;
    
    pub fn parse_stream<T: AsRef<Path>>(path: T, callback: Box<dyn Fn(DataItem) + Send>) -> Result<(), ParseError>;
}

// API 数据解析
pub struct ApiParser {
    pub rate_limit: RateLimiter,
    pub retry_config: RetryConfig,
}

impl ApiParser {
    pub fn fetch<T: AsRef<str>>(url: T) -> Result<Vec<DataItem>, FetchError>;
}

// 数据清洗规则
pub struct CleanRule {
    pub rule_type: RuleType,  // 去重、时间对齐、异常值过滤
    pub params: HashMap<String, Value>,
}

pub enum RuleType {
    // 去重：基于时间戳和价格
    Deduplicate {
        window_ms: u64,
        tolerance: f64,
    },
    
    // 时间对齐：填充缺失时间点
    AlignTime {
        granularity: Granularity,
        interpolation: InterpolationType,  // Linear, LastValue, Zero
    },
    
    // 异常值检测
    OutlierDetection {
        method: OutlierMethod,  // ZScore, IQR, Rolling
        threshold: f64,
    },
    
    // 字段标准化
    Normalize {
        min_max: bool,
        z_score: bool,
    },
}
```

#### 3.2.2 时间对齐

```rust
pub struct TimeAligner {
    pub tolerance_ms: u64,  // 时间容差
    pub gap_fill: GapFillStrategy,  // 线性插值、前向填充
}

impl TimeAligner {
    pub fn align(&self, data: &[DataItem]) -> Vec<DataItem>;
    
    pub fn detect_gaps(&self, data: &[DataItem]) -> Vec<TimeGap>;
}

pub struct TimeGap {
    pub instrument: InstrumentId,
    pub start_ts: i64,
    pub end_ts: i64,
    pub expected_count: usize,
    pub actual_count: usize,
}
```

### 3.3 缓存系统

#### 3.3.1 Lru 缓存

```rust
pub struct LruCache<K, V> {
    pub capacity: usize,
    pub shrink_ratio: f64,  // 内存超过阈值时收缩
}

impl<K, V> LruCache<K, V> {
    pub fn get(&self, key: &K) -> Option<&V>;
    
    pub fn insert(&mut self, key: K, value: V);
    
    pub fn remove(&mut self, key: &K) -> Option<V>;
    
    pub fn clear(&mut self);
    
    pub fn resize(&mut self, new_capacity: usize);
}

// 内存映射缓存（大文件）
pub struct MmapCache {
    pub file_path: PathBuf,
    pub mmap: MmapMut,
    pub offset: usize,
}
```

#### 3.3.2 分级缓存

```rust
pub struct TieredCache {
    pub memory: LruCache<CacheKey, DataBatch>,
    pub disk: DiskCache,  // 内存溢出时落盘
    pub db: DatabaseCache, // 持久化缓存
}

impl TieredCache {
    pub fn get(&self, key: &CacheKey) -> Option<DataBatch>;
    
    pub fn put(&mut self, key: CacheKey, value: DataBatch);
}
```

### 3.4 历史回放引擎

#### 3.4.1 核心设计

```rust
// 回放控制器
pub struct ReplayController {
    pub data_source: Arc<dyn DataSource>,
    pub granularity: Granularity,
    pub speed_multiplier: f64,  // 1.0 = 实时，0.1 = 10 倍速
    pub callbacks: Vec<Box<dyn ReplayCallback>>,  // 信号、订单等回调
}

impl ReplayController {
    pub fn run(&mut self) -> ReplayResult;
    
    pub fn run_with_pause(&mut self) -> ReplayWithPause;
}

pub enum ReplayResult {
    Success { stats: ReplayStats },
    Error { error: ReplayError },
    Paused { current_ts: i64 },
}

pub struct ReplayStats {
    pub total_data_points: usize,
    pub processed: usize,
    pub skipped: usize,  // 重复数据
    pub errors: usize,
    pub duration_ms: u64,
}

// 任意粒度回放
pub struct ArbitraryGranularityReplay {
    pub custom_granularity: Vec<(i64, Vec<DataItem>)>,  // 时间戳 -> 数据
    pub interpolation: bool,  // 自动插值缺失点
}
```

#### 3.4.2 回放回调

```rust
pub trait ReplayCallback: Send + Sync {
    fn on_data(&self, item: &DataItem, ts_ms: i64);
    
    fn on_signal(&self, signal: &Signal);
    
    fn on_order(&self, order: &Order);
    
    fn on_error(&self, error: &ReplayError);
}
```

### 3.5 数据质量检查

```rust
pub struct QualityChecker {
    pub rules: Vec<QualityRule>,
    pub severity_threshold: SeverityLevel,  // 低于此级别才告警
}

impl QualityChecker {
    pub fn check(&self, data: &[DataItem]) -> QualityReport;
}

pub struct QualityReport {
    pub overall_score: f64,  // 0-100
    pub issues: Vec<QualityIssue>,
    pub recommendations: Vec<String>,
}

pub enum QualityIssue {
    // 完整性
    MissingData { instrument: InstrumentId, count: usize },
    DuplicateData { count: usize },
    
    // 一致性
    TimeGap { start: i64, end: i64 },
    PriceAnomaly { min: f64, max: f64, threshold: f64 },
    
    // 性能
    FetchLatency { avg_ms: f64, threshold_ms: f64 },
    MemoryUsage { current_mb: f64, threshold_mb: f64 },
}
```

### 3.6 元数据管理

```rust
pub struct MetadataManager {
    pub instruments: HashMap<InstrumentId, InstrumentMeta>,
    pub data_sources: HashMap<String, DataSourceMeta>,
    pub schemas: SchemaRegistry,
}

impl MetadataManager {
    pub fn get_instrument(&self, id: &InstrumentId) -> Option<&InstrumentMeta>;
    
    pub fn validate_schema(&self, data: &[DataItem]) -> Result<(), SchemaError>;
}

pub struct InstrumentMeta {
    pub venue: Venue,
    pub symbol: String,
    pub base_currency: String,
    pub quote_currency: String,
    pub tick_size: f64,
    pub lot_size: f64,
    pub trading_hours: TradingHours,
}

pub struct TradingHours {
    pub open: Time,
    pub close: Time,
    pub timezone: String,
    pub holidays: Vec<Date>,
}
```

### 3.7 数据库优化

#### 3.7.1 分区策略

```rust
// 时间分区（按月或按周）
pub struct PartitionedStorage {
    pub table_name: &'static str,
    pub partition_interval: PartitionInterval,  // Month, Week
    pub max_partitions: usize,
}

impl PartitionedStorage {
    pub fn insert(&self, data: &[DataItem]) -> Result<(), DatabaseError>;
    
    pub fn query(&self, query: DataQuery) -> Result<Vec<DataItem>, DatabaseError>;
    
    pub fn drop_old_partitions(&self, older_than_days: u64) -> Result<usize, DatabaseError>;
}

// 索引优化
pub struct IndexOptimizer {
    pub indexes: Vec<IndexDefinition>,
}

impl IndexOptimizer {
    pub fn optimize(&self) -> Result<(), DatabaseError>;
}
```

#### 3.7.2 批量处理

```rust
pub struct BatchProcessor {
    pub batch_size: usize,
    pub parallelism: usize,
}

impl BatchProcessor {
    pub fn process(&self, data: Vec<DataItem>) -> Result<(), ProcessError>;
    
    pub fn stream(&self, source: impl Iterator<Item = DataItem>) -> Result<(), ProcessError>;
}
```

---

## 4. 数据模型

### 4.1 核心类型

```rust
// 数据库记录
#[derive(serde::Serialize, serde::Deserialize)]
pub struct DataRecord {
    pub instrument_id: InstrumentId,
    pub data_source_id: String,
    pub ts_ms: i64,
    pub data_type: DataType,
    pub data: DataPayload,  // JSON 或 二进制
    pub quality_score: f64,  // 数据质量评分
    pub created_at: i64,
}

// 数据载荷
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(untagged)]
pub enum DataPayload {
    Bar(NormalizedBar),
    Tick(Tick),
    OrderBook(OrderBook),
    Custom(serde_json::Value),
}

// Tick 数据结构
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Tick {
    pub ts_ms: i64,
    pub price: f64,
    pub volume: f64,
    pub side: Side,
    pub vwap: Option<f64>,  // 累计 VWAP
    pub depth: Option<OrderBook>,  // 快照
}
```

### 4.2 数据库 Schema

```sql
-- 数据表（分区表）
CREATE TABLE data_items (
    id TEXT PRIMARY KEY,  -- 自动生成
    instrument_id TEXT NOT NULL,
    data_source_id TEXT NOT NULL,
    ts_ms INTEGER NOT NULL,
    data_type TEXT NOT NULL,  -- bar, tick, orderbook
    
    -- 字段数据（避免 JSON 解析）
    o REAL, h REAL, l REAL, c REAL, v REAL,  -- 仅 bar
    price REAL, volume REAL, side TEXT,       -- 仅 tick
    
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

-- 元数据表
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

-- 数据质量日志
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

---

## 5. 执行流程

### 5.1 实时数据流

```
1. 数据源触发（API/WebSocket）
2. 解析器解析数据
3. 清洗规则应用（去重、对齐）
4. 质量检查
5. 缓存更新（Lru -> Database）
6. 事件广播（Strategy/Execution）
7. 元数据更新
```

### 5.2 历史数据导入

```
1. 读取源文件/API
2. 批量解析（并行）
3. 质量检查（预览）
4. 时间对齐（填充缺失）
5. 分区插入（按时间）
6. 索引优化
7. 生成质量报告
```

### 5.3 回放执行

```
1. 加载回放配置（时间范围、粒度）
2. 初始化回调（策略、执行器）
3. 时间推进循环：
   a. 获取下一个数据点
   b. 触发回调
   c. 执行信号/订单
   d. 更新状态
4. 生成回放报告
```

---

## 6. 错误处理

```rust
#[derive(Debug, thiserror::Error)]
pub enum DataError {
    #[error("Fetch error: {0}")]
    Fetch(String),
    
    #[error("Parse error: {0}")]
    Parse(String),
    
    #[error("Quality error: {0}")]
    Quality(String),
    
    #[error("Database error: {0}")]
    Database(String),
    
    #[error("Cache error: {0}")]
    Cache(String),
    
    #[error("Replay error: {0}")]
    Replay(String),
}
```

---

## 7. 配置管理

```yaml
# data_config.yaml
fetch:
  parallelism: 4
  batch_size: 1000
  timeout_ms: 5000
  retry_max: 3
  
cache:
  lru_capacity: 1000000  # 1M 条
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

## 8. 测试策略

### 8.1 单元测试

```rust
#[test]
fn test_time_aligner_gap_detection() {
    let aligner = TimeAligner::new();
    let data = vec![Item(1000), Item(2000), Item(5000)];  // 3000-4000 缺失
    let gaps = aligner.detect_gaps(&data);
    assert_eq!(gaps.len(), 1);
    assert_eq!(gaps[0].start_ts, 3000);
}

#[test]
fn test_lru_cache_eviction() {
    let mut cache = LruCache::new(3);
    cache.insert(1, "a");
    cache.insert(2, "b");
    cache.insert(3, "c");
    cache.get(&1);  // 访问 1
    cache.insert(4, "d");
    assert!(cache.contains_key(&1) == false);  // 1 被驱逐
}
```

### 8.2 集成测试

```rust
#[test]
fn test_full_data_pipeline() {
    // 1. 导入 CSV 数据
    // 2. 验证数据质量
    // 3. 回放并验证信号生成
    // 4. 验证数据库一致性
}
```

---

## 9. 使用示例

```rust
// 1. 配置数据源
let config = DataConfig {
    sources: vec![
        DataSourceConfig::File {
            path: PathBuf::from("data/bars.csv"),
            parse: CsvParser::new(),
        },
        DataSourceConfig::Api {
            url: "https://api.example.com/marketdata",
            auth: ApiKey::new("xxx"),
        },
    ],
    ..Default::default()
};

// 2. 创建管理器
let manager = DataManager::new(config);

// 3. 导入数据
let result = manager.import(DataImport {
    instruments: vec![InstrumentId::new(Venue::Crypto, "BTC-USD")],
    start_ts: 1700000000000,
    end_ts: 1700000000000 + 86400000,
    granularity: Granularity::Minute,
});

// 4. 回放策略
let mut controller = ReplayController::new(
    manager.data_source.clone(),
    Granularity::Minute,
    0.5,  // 0.5 倍速
);
controller.run();

// 5. 质量检查
let report = manager.quality_check();
assert!(report.overall_score > 0.9);
```

---

## 10. 实施计划

### 阶段 1：核心框架（1 周）
- [ ] 核心 Trait 定义和基础实现
- [ ] 文件解析器（CSV/Parquet）
- [ ] Lru 缓存实现
- [ ] 基础质量检查

### 阶段 2：高级功能（1 周）
- [ ] 历史回放引擎
- [ ] 时间对齐和清洗
- [ ] 数据库分区优化

### 阶段 3：优化与扩展（1 周）
- [ ] 分级缓存
- [ ] 元数据管理
- [ ] 监控和告警

---

## 11. 依赖

```toml
[dependencies]
# 数据处理
polars = "0.40"  # 高性能 DataFrame
arrow = "5.0"    # 内存布局优化

# 并发
rayon = "1.8"

# 缓存
lru = "0.12"

# 时间
chrono = "0.4"

# 序列化
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
```

---

**审批问题**：
1. 分层架构设计是否符合预期？
2. 任意粒度回放和缓存设计是否合理？
3. 功能范围是否需要调整？

请确认是否批准此设计。