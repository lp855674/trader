# 策略系统架构设计

**日期**: 2024-04-04  
**优先级**: P0  
**状态**: 待审批

---

## 1. 概述

### 1.1 目标

构建一个**声明式、多数据源、高级回测**的策略系统，支持：
- **多数据源输入**：K 线、Tick、订单簿、外部 API
- **混合触发**：定期调度 + 事件驱动
- **轻量状态**：纯函数决策，通过 Context 传递缓存/记忆
- **复杂组合**：加权平均、轮动、条件组合
- **高级回测**：参数优化、蒙特卡洛、敏感性分析
- **模拟执行**：Paper trading 模式
- **热加载**：运行时重新加载配置

### 1.2 设计原则

- **YAGNI**：无状态策略是默认，状态通过 Context 显式传递
- **单一职责**：每个组件只负责一个功能
- **组合优于继承**：通过 trait 组合实现策略能力
- **回测优先**：设计时考虑回测的确定性执行

---

## 2. 核心架构

### 2.1 数据流

```
┌─────────────┐    ┌─────────────┐    ┌─────────────┐
│ InputSource │───>│ Strategy    │───>│ Signal      │
└─────────────┘    └─────────────┘    └─────────────┘
                         │
                         v
                  ┌─────────────┐
                  │ Strategy    │
                  │ Context     │
                  │ (Cache,     │
                  │  Memory)    │
                  └─────────────┘
```

### 2.2 核心组件

| 组件 | 职责 | 关键特性 |
|------|------|----------|
| **Strategy** | 信号生成 | 纯函数 `Context -> Signal` |
| **StrategyContext** | 状态容器 | 缓存、记忆、参数 |
| **InputSource** | 数据源 | K 线、Tick、订单簿 |
| **Scheduler** | 触发器 | 定期调度 |
| **EventBus** | 事件总线 | 事件分发 |
| **StrategyCombinator** | 组合器 | 加权、轮动、条件 |
| **BacktestEngine** | 回测引擎 | 确定性执行 |

---

## 3. 详细设计

### 3.1 核心 Trait 定义

#### 3.1.1 Strategy

```rust
// 纯函数：无副作用，确定性输出
pub trait Strategy: Send + Sync + 'static {
    fn evaluate(&self, context: &StrategyContext) -> Option<Signal>;
}

// 支持组合
pub trait StrategyCombinator {
    type Output: Strategy;
    fn combine(self, other: Self::Output) -> Self::Output;
}
```

#### 3.1.2 StrategyContext

```rust
pub struct StrategyContext {
    pub instrument: InstrumentId,
    pub ts_ms: i64,
    
    // 轻量状态：缓存、记忆
    pub cache: LruCache<Key, Value>,
    pub memory: Vec<HistoricalData>,
    
    // 参数：运行时可配置
    pub params: HashMap<String, Value>,
    
    // 输入源引用
    pub kline_source: Arc<dyn KlineSource>,
    pub tick_source: Arc<dyn TickSource>,
}
```

#### 3.1.3 InputSource

```rust
// K 线源
pub trait KlineSource: Send + Sync {
    fn get(&self, instrument: InstrumentId, ts_ms: i64) -> Option<NormalizedBar>;
    fn get_range(&self, instrument: InstrumentId, from: i64, to: i64) -> Vec<NormalizedBar>;
}

// Tick 源
pub trait TickSource: Send + Sync {
    fn get_latest(&self, instrument: InstrumentId) -> Option<Tick>;
    fn subscribe(&self, instrument: InstrumentId, callback: Box<dyn Fn(Tick) + Send>);
}

// 订单簿源
pub trait OrderBookSource: Send + Sync {
    fn get_depth(&self, instrument: InstrumentId, levels: usize) -> OrderBook;
}
```

### 3.2 触发机制

#### 3.2.1 Scheduler（定期触发）

```rust
pub struct Scheduler {
    pub interval_ms: u64,
    pub strategies: Vec<Arc<dyn Strategy>>,
}

impl Scheduler {
    pub fn run(&self) -> JoinHandle<()>;
    
    // 混合触发：定期 + 事件
    pub fn run_hybrid(&self, event_bus: Arc<EventBus>) -> JoinHandle<()>;
}
```

#### 3.2.2 EventBus（事件驱动）

```rust
pub enum StrategyEvent {
    NewKline { instrument: InstrumentId, bar: NormalizedBar },
    NewTick { instrument: InstrumentId, tick: Tick },
    NewOrderBook { instrument: InstrumentId, book: OrderBook },
    Reconfig { strategy: Arc<dyn Strategy>, params: HashMap<String, Value> },
}

pub struct EventBus {
    pub sender: broadcast::Sender<StrategyEvent>,
}

impl EventBus {
    pub fn emit(&self, event: StrategyEvent);
}
```

### 3.3 策略组合（Combinator）

#### 3.3.1 基础组合

```rust
// 加权平均：Signal = w1*s1 + w2*s2
pub struct WeightedAverage {
    pub weights: Vec<f64>,
}

// 条件组合：仅当条件满足时输出
pub struct Conditional {
    pub condition: Box<dyn Fn(&StrategyContext) -> bool>,
    pub strategy: Arc<dyn Strategy>,
}

// 轮动策略：按时间片轮动不同策略
pub struct RoundRobin {
    pub strategies: Vec<Arc<dyn Strategy>>,
    pub interval_ms: u64,
}
```

#### 3.3.2 组合 Trait

```rust
pub trait StrategyCombinator {
    type Output: Strategy;
    
    fn combine(self, other: Self::Output) -> Self::Output;
    
    fn with_condition<F>(self, cond: F) -> Conditional
    where
        F: Fn(&StrategyContext) -> bool + 'static;
    
    fn with_weight(self, weight: f64) -> WeightedAverage;
}
```

### 3.4 回测引擎

#### 3.4.1 核心设计

```rust
// 确定性执行：给定相同输入，必定产生相同输出
pub struct BacktestEngine {
    pub strategy: Arc<dyn Strategy>,
    pub data_source: Arc<dyn HistoricalData>,
    pub config: BacktestConfig,
}

pub struct BacktestConfig {
    pub start_ts: i64,
    pub end_ts: i64,
    pub initial_capital: f64,
    pub commission_rate: f64,
    pub slippage: f64,
    pub tick_interval_ms: u64,
}

impl BacktestEngine {
    pub fn run(&self) -> BacktestResult;
}

pub struct BacktestResult {
    pub equity_curve: Vec<(i64, f64)>,
    pub total_return: f64,
    pub sharpe_ratio: f64,
    pub max_drawdown: f64,
    pub trades: Vec<Trade>,
}
```

#### 3.4.2 高级功能

**参数优化**

```rust
pub struct ParameterOptimizer {
    pub param_grid: HashMap<String, Vec<Value>>,
    pub validation_split: (i64, i64), // 训练/验证时间分割
}

impl ParameterOptimizer {
    pub fn grid_search(&self, engine: &BacktestEngine) -> Vec<OptimizationResult>;
    
    pub fn bayesian_optimization(&self, engine: &BacktestEngine) -> OptimizationResult;
}
```

**蒙特卡洛模拟**

```rust
pub struct MonteCarlo {
    pub iterations: usize,
    pub random_seed: u64,
}

impl MonteCarlo {
    pub fn simulate(&self, engine: &BacktestEngine) -> MonteCarloResult;
}

pub struct MonteCarloResult {
    pub median_return: f64,
    pub confidence_interval: (f64, f64),
    pub worst_case: f64,
}
```

**敏感性分析**

```rust
pub struct SensitivityAnalyzer {
    pub params_to_vary: Vec<(String, f64)>, // (param_name, variation_range)
}

impl SensitivityAnalyzer {
    pub fn analyze(&self, engine: &BacktestEngine) -> SensitivityReport;
}
```

### 3.5 模拟执行器

```rust
pub enum ExecutionMode {
    Paper,      // 模拟交易
    Live,       // 真实交易
    Backtest,   // 回测
}

pub struct PaperAdapter {
    pub position: HashMap<InstrumentId, Position>,
    pub cash: f64,
    pub trades: Vec<Trade>,
}

impl PaperAdapter {
    pub fn simulate_order(&mut self, intent: OrderIntent) -> Result<Order, PaperError>;
    pub fn get_pnl(&self) -> f64;
}
```

### 3.6 热加载机制

```rust
pub struct StrategyManager {
    pub loaded_strategies: HashMap<String, Arc<dyn Strategy>>,
    pub config_path: PathBuf,
    pub reload_interval_ms: u64,
}

impl StrategyManager {
    pub fn load(&mut self) -> Result<(), LoadError>;
    
    pub fn reload(&mut self) -> Result<(), LoadError> {
        // 简单热加载：读取配置，重新构建策略，替换
    }
    
    pub fn get(&self, name: &str) -> Option<Arc<dyn Strategy>>;
}
```

### 3.7 日志与监控

```rust
pub struct StrategyLogger {
    pub logger: Logger,
    pub metrics: MetricsCollector,
}

impl StrategyLogger {
    fn log_signal(&self, context: &StrategyContext, signal: &Signal) {
        // 详细日志：输入、参数、中间状态
        self.logger.info!(
            "Strategy signal",
            instrument = %context.instrument,
            params = ?context.params,
            signal = ?signal
        );
    }
    
    fn log_performance(&self, result: &BacktestResult) {
        self.metrics.record("backtest.sharpe", result.sharpe_ratio);
        self.metrics.record("backtest.max_drawdown", result.max_drawdown);
    }
}
```

---

## 4. 数据模型

### 4.1 核心类型

```rust
// 信号
pub struct Signal {
    pub strategy_id: String,
    pub instrument: InstrumentId,
    pub side: Side,
    pub qty: f64,
    pub limit_price: f64,
    pub ts_ms: i64,
    pub params: HashMap<String, Value>, // 决策参数
}

// 交易
pub struct Trade {
    pub order_id: String,
    pub instrument: InstrumentId,
    pub side: Side,
    pub filled_qty: f64,
    pub avg_price: f64,
    pub ts_ms: i64,
    pub commission: f64,
}

// 仓位
pub struct Position {
    pub instrument: InstrumentId,
    pub qty: f64,
    pub avg_price: f64,
    pub side: Side,
}
```

### 4.2 数据库 Schema

```sql
-- 策略配置
CREATE TABLE strategies (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    module_path TEXT NOT NULL,  -- 用于动态加载
    params JSONB,
    created_at TIMESTAMP,
    updated_at TIMESTAMP
);

-- 回测结果
CREATE TABLE backtest_results (
    id TEXT PRIMARY KEY,
    strategy_id TEXT NOT NULL,
    start_ts INTEGER NOT NULL,
    end_ts INTEGER NOT NULL,
    total_return REAL NOT NULL,
    sharpe_ratio REAL,
    max_drawdown REAL,
    trades_count INTEGER,
    result_json JSONB,
    created_at TIMESTAMP
);

-- 策略日志
CREATE TABLE strategy_logs (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    strategy_id TEXT NOT NULL,
    event_type TEXT NOT NULL,  -- signal, error, config_change
    context JSONB,
    ts_ms INTEGER NOT NULL
);
```

---

## 5. 执行流程

### 5.1 实盘/模拟执行

```
1. Scheduler 每 N 秒触发 或 EventBus 收到事件
2. 构建 StrategyContext（加载数据、状态）
3. 调用 strategy.evaluate(context)
4. 生成 Signal
5. 通过 StrategyCombinator 组合多个信号
6. 转换为 OrderIntent
7. PaperAdapter 模拟执行 或 真实交易所执行
8. 记录日志和指标
```

### 5.2 回测执行

```
1. BacktestEngine 加载历史数据
2. 模拟时间推进：start_ts -> end_ts
3. 对每个时间点：
   a. 构建历史 Context（包含历史数据）
   b. 执行策略，生成信号
   c. 模拟订单执行（考虑滑点、手续费）
   d. 更新仓位和资金
4. 生成性能统计和交易记录
```

---

## 6. 错误处理

```rust
// 策略错误
#[derive(Debug, thiserror::Error)]
pub enum StrategyError {
    #[error("Data source error: {0}")]
    DataSource(String),
    #[error("Invalid parameters: {0}")]
    InvalidParams(String),
    #[error("Execution error: {0}")]
    Execution(String),
}

// 回测错误
#[derive(Debug, thiserror::Error)]
pub enum BacktestError {
    #[error("Data gap detected at {ts_ms}")]
    DataGap { ts_ms: i64 },
    #[error("Invalid parameters for optimization")]
    InvalidParams,  
    #[error("Monte Carlo simulation failed: {0}")]
    Simulation(String),
}
```

---

## 7. 测试策略

### 7.1 单元测试

```rust
#[test]
fn test_strategy_pure_function() {
    // 相同输入必定相同输出
    let ctx = StrategyContext::new(...);
    let sig1 = strategy.evaluate(&ctx);
    let sig2 = strategy.evaluate(&ctx);
    assert_eq!(sig1, sig2);
}

#[test]
fn test_strategy_with_cache() {
    // 测试缓存命中率
    let ctx = StrategyContext::new_with_cache(...);
    let sig = strategy.evaluate(&ctx);
    assert!(ctx.cache.hit_rate > 0.9);
}
```

### 7.2 集成测试

```rust
#[test]
fn test_backtest_deterministic() {
    // 相同配置必定相同结果
    let result1 = engine.run();
    let result2 = engine.run();
    assert_eq!(result1.equity_curve, result2.equity_curve);
}
```

### 7.3 性能基准

```rust
#[bench]
fn bench_strategy_evaluation(b: &mut Bencher) {
    let ctx = StrategyContext::new(...);
    b.iter(|| strategy.evaluate(&ctx));
}
```

---

## 8. 使用示例

```rust
// 1. 定义策略
pub struct MovingAverageCrossover {
    pub fast_period: usize,
    pub slow_period: usize,
}

impl Strategy for MovingAverageCrossover {
    fn evaluate(&self, ctx: &StrategyContext) -> Option<Signal> {
        // 计算均线... 生成信号
    }
}

// 2. 创建回测引擎
let engine = BacktestEngine::new(
    Arc::new(MovingAverageCrossover { fast: 10, slow: 30 }),
    Arc::new(HistoricalData::new(PathBuf::from("data"))),
    BacktestConfig {
        start_ts: 1700000000000,
        end_ts: 1700000000000 + 86400000, // 24 小时
        initial_capital: 10000.0,
        ..Default::default()
    },
);

// 3. 运行回测
let result = engine.run();
println!("Sharpe: {}", result.sharpe_ratio);

// 4. 参数优化
let optimizer = ParameterOptimizer {
    param_grid: vec![
        ("fast_period".to_string(), vec![10, 20, 30]),
        ("slow_period".to_string(), vec![30, 60, 120]),
    ],
    ..Default::default()
};
let best_params = optimizer.grid_search(&engine);
```

---

## 9. 实施计划

### 阶段 1：核心框架（1-2 周）
- [x] 核心 Trait 定义（Strategy, Context, InputSource）
- [x] 基础策略实现（MovingAverage, RSI, MACD）
- [x] Scheduler 和 EventBus
- [x] 简单回测引擎（基础回放）

### 阶段 2：高级功能（2 周）
- [ ] StrategyCombinator 实现
- [ ] 参数优化系统
- [ ] 蒙特卡洛和敏感性分析
- [ ] 详细日志和监控

### 阶段 3：执行增强（1 周）
- [ ] PaperAdapter 完善
- [ ] 热加载机制
- [ ] 数据库持久化

---

## 10. 依赖

```toml
[dependencies]
# 核心
thiserror = "1.0"
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"

# 数据结构和并发
hashbrown = "0.14"
tracing = "0.1"

# 回测和优化
polars = { version = "0.40", features = ["lazy"] }
rayon = "1.8"  # 并行优化

# 热加载和配置
toml = "0.8"
```

---

**审批问题**：
1. 架构设计是否符合预期？
2. 优先级和范围是否合理？
3. 是否有需要调整的功能点？

请确认是否批准此设计。