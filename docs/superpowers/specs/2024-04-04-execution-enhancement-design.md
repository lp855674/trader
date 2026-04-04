# 执行增强系统架构设计

**日期**: 2024-04-04  
**优先级**: P0  
**状态**: 待审批

---

## 1. 概述

### 1.1 目标

构建**完整订单生命周期管理**的执行系统，支持：
- **多订单类型**：市价、限价、止损、止损限价、冰山单、TWAP/VWAP
- **完整状态跟踪**：订单状态、部分成交、取消、修改
- **仓位管理**：实时仓位、PnL（已实现/未实现）、盈亏平衡
- **执行质量**：滑点控制、手续费优化、执行报告
- **批量执行**：订单队列、优先级调度
- **三种模式**：实盘、Paper、回测

### 1.2 设计原则

- **订单即资源**：每个订单是独立实体，支持全生命周期跟踪
- **状态机驱动**：订单状态流转，支持并发安全
- **执行质量优先**：最小化滑点，最大化成交率
- **零丢失设计**：幂等性保证，断点续传

---

## 2. 核心架构

### 2.1 分层架构

```
┌─────────────────────────────────────────────────────────────┐
│                    执行引擎 (Execution Engine)               │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌────────────┐  │
│  │ Order    │  │ Position │  │ PnL      │  │ Execution  │  │
│  │ Manager  │  │ Manager  │  │ Manager  │  │ Reporter   │  │
│  └──────────┘  └──────────┘  └──────────┘  └────────────┘  │
└─────────────────────────────────────────────────────────────┘
                             │
┌─────────────────────────────────────────────────────────────┐
│                    适配器层 (Adapter Layer)                   │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌────────────┐  │
│  │ Paper    │  │ Longbridge│ │ Binance  │ │ Custom     │  │
│  │ Adapter  │  │ Adapter  │ │ Adapter  │ │ Adapter    │  │
│  └──────────┘  └──────────┘  └──────────┘  └────────────┘  │
└─────────────────────────────────────────────────────────────┘
                             │
┌─────────────────────────────────────────────────────────────┐
│                    策略层 (Strategy Layer)                    │
│  Strategy → Signal → OrderIntent → OrderRequest              │
└─────────────────────────────────────────────────────────────┘
```

### 2.2 数据流

```
1. Signal 生成 → 2. OrderIntent 创建 → 3. OrderRequest 封装
4. 执行引擎处理（仓位检查、滑点计算）
5. 适配器执行（实盘/Paper）
6. OrderStatus 回调 → 7. 仓位更新 → 8. PnL 计算
```

---

## 3. 详细设计

### 3.1 核心 Trait 定义

#### 3.1.1 订单请求

```rust
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct OrderRequest {
    pub order_id: String,        // 客户端订单 ID（幂等键）
    pub instrument: InstrumentId,
    pub side: Side,
    pub qty: f64,
    pub order_type: OrderType,
    pub time_in_force: TimeInForce,
    
    // 价格相关
    pub limit_price: Option<f64>,         // 限价单
    pub stop_price: Option<f64>,          // 止损价
    pub trigger_price: Option<f64>,       // 触发价
    
    // 执行参数
    pub commission_type: CommissionType,
    pub commission_rate: f64,             // 手续费率
    pub slippage_tolerance: f64,          // 滑点容忍度
    
    // 高级功能
    pub iceberg_qty: Option<f64>,         // 冰山单总量
    pub display_qty: Option<f64>,         // 显示量
    pub twap_interval_ms: Option<u64>,    // TWAP 间隔
    pub duration_ms: Option<u64>,         // 执行时长
}

pub enum OrderType {
    Market,           // 市价单
    Limit,            // 限价单
    Stop,             // 止损单
    StopLimit,        // 止损限价单
    Trailing,         // 追踪止损
    Iceberg,          // 冰山单
    TWAP,             // 时间加权平均
    VWAP,             // 成交量加权平均
}
```

#### 3.1.2 订单状态机

```rust
pub enum OrderStatus {
    Pending,          // 待处理
    Submitted,        // 已提交
    PartiallyFilled,  // 部分成交
    Filled,           // 已成交
    Cancelled,        // 已取消
    Rejected,         // 被拒绝
    Expired,          // 已过期
}

pub struct OrderState {
    pub request: OrderRequest,
    pub status: OrderStatus,
    pub fills: Vec<Fill>,
    pub created_at: i64,
    pub updated_at: i64,
    pub exchange_order_id: Option<String>,
    pub reject_reason: Option<String>,
}

impl OrderState {
    fn transition(&mut self, event: OrderEvent) -> Result<(), StateTransitionError>;
}

pub enum OrderEvent {
    Created,           // 新订单
    Submitted,         // 已提交交易所
    PartialFill { fill: Fill },
    Filled,            // 全部成交
    Cancelled,         // 已取消
    Rejected { reason: String },
    Error { error: ExecError },
}
```

#### 3.1.3 仓位管理

```rust
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Position {
    pub instrument: InstrumentId,
    pub qty: f64,              // 持仓数量
    pub avg_price: f64,        // 平均成本价
    pub side: Side,            // 多头/空头
    pub open_time: i64,
    pub commission_paid: f64,
}

impl Position {
    pub fn pnl(&self, current_price: f64) -> PnL {
        let unrealized = (current_price - self.avg_price) * self.qty;
        PnL {
            unrealized: unrealized,
            realized: self.commission_paid,  // 简化，实际应从 fills 计算
            total: unrealized + self.commission_paid,
        }
    }
}

pub struct Portfolio {
    pub positions: HashMap<InstrumentId, Position>,
    pub cash: f64,
    pub margin_used: f64,       // 已用保证金
    pub margin_free: f64,      // 可用保证金
}

impl Portfolio {
    pub fn update_position(&mut self, fill: &Fill) {
        // 更新仓位
    }
    
    pub fn get_margin_requirement(&self) -> f64;
}
```

#### 3.1.4 执行适配器

```rust
#[async_trait]
pub trait ExecutionAdapter: Send + Sync {
    // 下单
    async fn execute_order(
        &self,
        request: OrderRequest,
    ) -> Result<ExecutionResult, ExecError>;
    
    // 查询状态
    async fn query_order(&self, order_id: &str) -> Result<OrderState, ExecError>;
    
    // 取消订单
    async fn cancel_order(&self, order_id: &str) -> Result<(), ExecError>;
    
    // 批量查询
    async fn query_orders(&self, filter: OrderFilter) -> Result<Vec<OrderState>, ExecError>;
    
    // 获取市场数据（用于计算滑点）
    async fn get_market_data(&self, instrument: InstrumentId) -> MarketData;
}

pub struct ExecutionResult {
    pub order_id: String,
    pub exchange_order_id: Option<String>,
    pub fill: Option<Fill>,  // 如果是市价单，可能立即成交
    pub status: OrderStatus,
    pub timestamp: i64,
}
```

### 3.2 执行引擎

#### 3.2.1 订单管理器

```rust
pub struct OrderManager {
    pub orders: HashMap<String, OrderState>,  // 内存中的活跃订单
    pub order_history: Vec<OrderState>,       // 历史订单
    pub pending_fills: HashMap<String, PendingFill>,
}

impl OrderManager {
    pub fn submit(&mut self, request: OrderRequest) -> Result<OrderState, SubmitError>;
    
    pub fn get(&self, order_id: &str) -> Option<&OrderState>;
    
    pub fn update_status(&mut self, order_id: &str, event: OrderEvent) -> Result<(), StateTransitionError>;
    
    pub fn cancel(&mut self, order_id: &str) -> Result<(), CancelError>;
    
    pub fn get_open_orders(&self) -> Vec<&OrderState>;
    
    pub fn get_closed_orders(&self, since: i64) -> Vec<&OrderState>;
}

pub struct SubmitError {
    pub order_id: String,
    pub reason: SubmitReason,
}

pub enum SubmitReason {
    DuplicateId,           // 重复 ID
    InvalidState,          // 状态转换错误
    InsufficientMargin,    // 保证金不足
    OrderLimitExceeded,   // 超出订单限额
}
```

#### 3.2.2 仓位管理器

```rust
pub struct PositionManager {
    pub positions: HashMap<InstrumentId, Position>,
    pub unrealized_pnl: HashMap<InstrumentId, f64>,
    pub margin_config: MarginConfig,
}

impl PositionManager {
    pub fn initialize(&mut self, portfolio: &Portfolio);
    
    pub fn update(&mut self, fill: &Fill) -> Result<(), PositionError>;
    
    pub fn get_position(&self, instrument: &InstrumentId) -> Option<&Position>;
    
    pub fn calculate_pnl(&self) -> PortfolioPnL;
    
    pub fn check_margin(&self, order: &OrderRequest) -> Result<(), MarginError>;
}

pub struct PortfolioPnL {
    pub total_unrealized: f64,
    pub total_realized: f64,
    pub daily_pnl: f64,
    pub watermark: f64,        // 最高净值
}
```

#### 3.2.3 执行质量优化器

```rust
pub struct ExecutionOptimizer {
    pub slippage_model: SlippageModel,
    pub commission_model: CommissionModel,
    pub market_impact_model: MarketImpactModel,
}

impl ExecutionOptimizer {
    pub fn estimate_cost(&self, order: &OrderRequest, current_price: f64) -> ExecutionCost {
        // 计算预期滑点、手续费、市场冲击
    }
    
    pub fn select_best_execution(&self, order: &OrderRequest, strategies: &[ExecutionStrategy]) -> BestExecution {
        // 选择最优执行策略
    }
}

pub struct ExecutionCost {
    pub slippage_cost: f64,
    pub commission: f64,
    pub market_impact: f64,
    pub total: f64,
}

pub enum ExecutionStrategy {
    Immediate,           // 立即执行
    Iceberg,             // 冰山单
    TWAP,                // 时间加权
    VWAP,                // 成交量加权
    SmartRouting,        // 智能路由
}
```

### 3.3 滑点与手续费控制

#### 3.3.1 滑点模型

```rust
pub enum SlippageModel {
    // 固定滑点
    Fixed { base_bps: f64, volatility_multiplier: f64 },
    
    // 基于订单大小
    VolumeBased { 
        small_order_bps: f64, 
        large_order_bps: f64, 
        exponent: f64,
    },
    
    // 基于市场深度
    MarketDepth { 
        depth_source: Arc<dyn DepthSource>,
        impact_factor: f64,
    },
    
    // 混合模型
    Hybrid { strategies: Vec<Box<dyn SlippageModel>> },
}

impl SlippageModel {
    fn calculate(&self, instrument: InstrumentId, qty: f64, current_price: f64) -> f64;
}
```

#### 3.3.2 手续费模型

```rust
pub enum CommissionModel {
    // 固定费率
    Percentage { rate: f64, min_fee: f64, max_fee: f64 },
    
    // 阶梯费率
    Tiered { tiers: Vec<CommissionTier> },
    
    // 交易所特定
    ExchangeSpecific { exchange: String, config: ExchangeCommissionConfig },
}

pub struct CommissionTier {
    pub volume_threshold: f64,
    pub rate: f64,
}
```

### 3.4 高级订单类型

#### 3.4.1 冰山单

```rust
pub struct IcebergOrder {
    pub total_qty: f64,
    pub display_qty: f64,       // 每次显示量
    pub remaining_qty: f64,
    pub active_display_orders: Vec<String>,
}

impl IcebergOrder {
    pub fn create_display_order(&mut self) -> OrderRequest;
    
    pub fn consume_fill(&mut self, fill_qty: f64) -> Result<(), IcebergError>;
    
    pub fn is_complete(&self) -> bool;
}
```

#### 3.4.2 TWAP/VWAP

```rust
pub struct TWAPOrder {
    pub total_qty: f64,
    pub duration_ms: u64,
    pub interval_ms: u64,
    pub remaining_qty: f64,
    pub start_time: i64,
    pub execution_count: usize,
}

impl TWAPOrder {
    pub fn get_next_execution(&self, current_time: i64) -> Option<TWAPExecution>;
    
    pub fn is_complete(&self, current_time: i64) -> bool;
}

pub struct VWAPOrder {
    pub total_qty: f64,
    pub duration_ms: u64,
    pub interval_ms: u64,
    pub remaining_qty: f64,
    pub start_time: i64,
    pub executions: Vec<VWAPExecution>,
}

pub struct VWAPExecution {
    pub timestamp: i64,
    pub qty: f64,
    pub avg_price: f64,
    pub volume_weighted_price: f64,  // 当前市场 VWAP
}
```

### 3.5 批量执行与队列

```rust
pub struct ExecutionQueue {
    pub orders: VecDeque<ExecutionJob>,
    pub priority: PriorityFIFO,
    pub max_concurrent: usize,
}

pub enum ExecutionJob {
    Order { request: OrderRequest, priority: u32 },
    Cancel { order_id: String, priority: u32 },
    Query { filter: OrderFilter, callback: Box<dyn Fn(Vec<OrderState>) + Send> },
}

impl ExecutionQueue {
    pub fn submit(&mut self, job: ExecutionJob);
    
    pub fn process(&mut self) -> Result<(), QueueError>;
    
    pub fn get_pending_count(&self) -> usize;
}

pub struct PriorityFIFO {
    pub orders: HashMap<u32, VecDeque<ExecutionJob>>,
}
```

### 3.6 监控与告警

```rust
pub struct ExecutionMetrics {
    pub order_latency: Histogram,  // 订单处理延迟
    pub fill_rate: Counter,        // 成交率
    pub slippage: Histogram,       // 滑点分布
    pub rejected_orders: Counter,  // 拒绝订单数
    pub position_pnl: Gauge,       // 实时 PnL
}

pub struct ExecutionAlert {
    pub severity: AlertSeverity,
    pub message: String,
    pub metrics_snapshot: MetricsSnapshot,
}

pub enum AlertSeverity {
    Info,
    Warning,
    Critical,
    Emergency,
}
```

---

## 4. 数据模型

### 4.1 数据库 Schema

```sql
-- 订单主表
CREATE TABLE orders (
    order_id TEXT PRIMARY KEY,          -- 客户端订单 ID
    exchange_order_id TEXT,             -- 交易所订单 ID
    account_id TEXT NOT NULL,
    instrument_id TEXT NOT NULL,
    side TEXT NOT NULL,                 -- BUY/SELL
    qty REAL NOT NULL,
    limit_price REAL,
    stop_price REAL,
    order_type TEXT NOT NULL,           -- market, limit, stop, etc.
    time_in_force TEXT NOT NULL,        -- day, gtc, etc.
    
    status TEXT NOT NULL DEFAULT 'PENDING',
    
    -- 时间戳
    created_at_ms INTEGER NOT NULL,
    updated_at_ms INTEGER NOT NULL,
    submitted_ms INTEGER,
    filled_at_ms INTEGER,
    cancelled_at_ms INTEGER,
    
    -- 财务
    commission_paid REAL DEFAULT 0.0,
    commission_currency TEXT,
    
    -- 索引
    INDEX(account_id, status),
    INDEX(created_at_ms),
    UNIQUE(account_id, exchange_order_id)
);

-- 成交记录
CREATE TABLE fills (
    fill_id TEXT PRIMARY KEY,
    order_id TEXT NOT NULL REFERENCES orders(order_id),
    exchange_fill_id TEXT,
    
    qty REAL NOT NULL,
    price REAL NOT NULL,
    side TEXT NOT NULL,
    commission REAL DEFAULT 0.0,
    
    ts_ms INTEGER NOT NULL,
    
    INDEX(order_id),
    INDEX(ts_ms)
);

-- 仓位快照（按时间分区）
CREATE TABLE positions_history (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    instrument_id TEXT NOT NULL,
    qty REAL,
    avg_price REAL,
    side TEXT,
    ts_ms INTEGER NOT NULL,
    
    UNIQUE(instrument_id, ts_ms),
    INDEX(ts_ms)
);

-- 执行质量日志
CREATE TABLE execution_quality (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    order_id TEXT REFERENCES orders(order_id),
    instrument_id TEXT,
    
    -- 执行指标
    slippage_bps REAL,              -- 基点滑点
    fill_rate REAL,                 -- 成交比例
    vwap_deviation_bps REAL,        -- 偏离 VWAP 的基点
    
    -- 市场条件
    market_volatility REAL,
    spread_bps REAL,
    
    ts_ms INTEGER NOT NULL
);
```

### 4.2 核心实体

```rust
// 成交
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Fill {
    pub fill_id: String,
    pub order_id: String,
    pub qty: f64,
    pub price: f64,
    pub side: Side,
    pub commission: f64,
    pub ts_ms: i64,
}

// 订单过滤器
#[derive(Debug, Clone)]
pub struct OrderFilter {
    pub instrument_id: Option<InstrumentId>,
    pub status: Option<OrderStatus>,
    pub since: Option<i64>,
    pub until: Option<i64>,
    pub exchange_order_id: Option<String>,
}
```

---

## 5. 执行流程

### 5.1 完整下单流程

```
1. Strategy 生成 Signal
2. OrderManager 创建 OrderRequest（幂等检查）
3. PositionManager 检查保证金（软限制，允许超买）
4. ExecutionOptimizer 计算滑点和手续费
5. ExecutionAdapter 提交订单
6. OrderManager 更新状态为 Submitted
7. 异步轮询状态（或推送回调）
8. Fill 到达 → PositionManager 更新仓位
9. PnL 计算 → 监控告警
```

### 5.2 订单取消流程

```
1. 收到取消请求（用户/风控/错误）
2. OrderManager 检查状态（仅 PENDING/Submitted 可取消）
3. ExecutionAdapter 发送取消请求
4. OrderManager 更新状态为 Cancelled
5. 记录取消原因和时间
```

### 5.3 批量执行流程

```
1. ExecutionQueue 收集订单
2. 按优先级排序
3. 并发执行（max_concurrent 限制）
4. 错误重试（指数退避）
5. 结果汇总
```

---

## 6. 错误处理

```rust
#[derive(Debug, thiserror::Error)]
pub enum ExecError {
    #[error("Order rejected: {0}")]
    OrderRejected(String),
    
    #[error("Insufficient margin: required={req:.2f}, available={avail:.2f}")]
    InsufficientMargin { req: f64, avail: f64 },
    
    #[error("Market data unavailable for {instrument}")]
    MarketDataUnavailable { instrument: InstrumentId },
    
    #[error("Adapter error: {0}")]
    Adapter(String),
    
    #[error("Network error: {0}")]
    Network(String),
    
    #[error("Position update error: {0}")]
    PositionUpdate(String),
}
```

---

## 7. 配置管理

```yaml
# execution_config.yaml
execution:
  max_concurrent_orders: 100
  order_timeout_ms: 30000  # 30 秒超时
  retry_max_attempts: 3
  retry_backoff_ms: 1000

slippage:
  default_bps: 10.0
  max_bps: 100.0
  volatility_multiplier: 2.0

commission:
  default_rate: 0.001  # 0.1%
  min_fee: 0.01

monitoring:
  alert_thresholds:
    slippage_warning_bps: 50
    slippage_critical_bps: 100
    fill_rate_warning: 0.8
    fill_rate_critical: 0.5
```

---

## 8. 测试策略

### 8.1 单元测试

```rust
#[test]
fn test_position_update() {
    let mut manager = PositionManager::new();
    let fill = Fill {
        qty: 10.0,
        price: 100.0,
        ..Default::default()
    };
    manager.update(&fill).unwrap();
    assert_eq!(manager.get_position(&instrument).unwrap().qty, 10.0);
}

#[test]
fn test_slippage_model() {
    let model = SlippageModel::Fixed { base_bps: 5.0, ..Default::default() };
    let slippage = model.calculate(instrument, 100.0, 100.0);
    assert_eq!(slippage, 0.0005);  // 0.05%
}
```

### 8.2 集成测试

```rust
#[test]
fn test_paper_adapter_full_flow() {
    // 1. 创建订单
    // 2. 验证立即成交
    // 3. 验证仓位更新
    // 4. 验证 PnL 计算
}
```

---

## 9. 使用示例

```rust
// 1. 创建订单
let order = OrderRequest {
    order_id: "order-123".to_string(),
    instrument: InstrumentId::new(Venue::Crypto, "BTC-USD"),
    side: Side::Buy,
    qty: 0.1,
    order_type: OrderType::Limit,
    limit_price: Some(45000.0),
    time_in_force: TimeInForce::Day,
    slippage_tolerance: 0.01,  // 1%
};

// 2. 执行
let adapter = PaperAdapter::new();
let result = adapter.execute_order(order).await?;

// 3. 查询状态
let state = adapter.query_order("order-123").await?;
assert_eq!(state.status, OrderStatus::Filled);

// 4. 获取 PnL
let portfolio = adapter.get_portfolio();
println!("Unrealized PnL: ${}", portfolio.unrealized_pnl);
```

---

## 10. 实施计划

### 阶段 1：核心框架（1 周）
- [ ] 核心 Trait 定义和基础实现
- [ ] 订单状态机
- [ ] 仓位管理器
- [ ] PaperAdapter 增强（多订单类型）

### 阶段 2：高级功能（1 周）
- [ ] 执行优化器（滑点模型）
- [ ] 冰山单、TWAP/VWAP
- [ ] 批量执行队列

### 阶段 3：监控与优化（1 周）
- [ ] 完整监控指标
- [ ] 执行报告生成
- [ ] 性能优化

---

## 11. 依赖

```toml
[dependencies]
# 并发
concurrent-queue = "2.0"
priority-queue = "2.0"

# 时间
chrono = "0.4"

# 数学计算
num-traits = "0.2"
```

---

**审批问题**：
1. 分层架构（引擎/适配器/策略）是否符合预期？
2. 订单状态机和滑点模型设计是否合理？
3. 功能范围是否需要调整？

请确认是否批准此设计。