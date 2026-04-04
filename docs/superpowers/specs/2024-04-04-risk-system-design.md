# 风控系统架构设计

**日期**: 2024-04-04  
**优先级**: P0  
**状态**: 待审批

---

## 1. 概述

### 1.1 目标

构建**分层声明式风控引擎**，覆盖全流程风险管控：
- **三层架构**：订单风险 → 仓位风险 → 组合风险
- **混合执行**：声明式规则（事前）+ 状态监控（事中/事后）
- **动态调整**：基于波动率自动调整参数
- **全流程**：事前检查 → 事中监控 → 事后分析
- **完整量化**：VaR、CVaR、相关性矩阵、压力测试
- **三种模式**：实盘、模拟、回测

### 1.2 设计原则

- **分层隔离**：订单层关注单笔，仓位层关注累积，组合层关注相关性
- **声明式优先**：规则即配置，纯函数检查，易于测试
- **动态自适应**：参数随市场波动率自动调整
- **确定性执行**：回测中可重现所有风险决策

---

## 2. 核心架构

### 2.1 分层架构

```
┌─────────────────────────────────────────────────────────────┐
│                    组合层 (Portfolio)                        │
│  ┌────────────────┐  ┌────────────────┐  ┌──────────────┐  │
│  │ 相关性风险     │  │ 流动性风险     │  │ 压力测试     │  │
│  └────────────────┘  └────────────────┘  └──────────────┘  │
└─────────────────────────────────────────────────────────────┘
           ↑              ↑              ↑
┌─────────────────────────────────────────────────────────────┐
│                    仓位层 (Position)                         │
│  ┌────────────────┐  ┌────────────────┐  ┌──────────────┐  │
│  │ 仓位限额       │  │ PnL 限额        │  │ 止损止盈     │  │
│  └────────────────┘  └────────────────┘  └──────────────┘  │
└─────────────────────────────────────────────────────────────┘
           ↑              ↑              ↑
┌─────────────────────────────────────────────────────────────┐
│                    订单层 (Order)                            │
│  ┌────────────────┐  ┌────────────────┐  ┌──────────────┐  │
│  │ 价格保护       │  │ 数量限制       │  │ 执行风险     │  │
│  └────────────────┘  └────────────────┘  └──────────────┘  │
└─────────────────────────────────────────────────────────────┘
```

### 2.2 数据流

```
┌──────────┐    ┌──────────┐    ┌──────────┐    ┌──────────┐
│  Signal  │───>│ Order    │───>│ Position │───>│ Portfolio│
│          │    │ Risk     │    │ Risk     │    │ Risk     │
└──────────┘    └──────────┘    └──────────┘    └──────────┘
                              │              │
                              v              v
                    ┌──────────┐    ┌──────────┐
                    │ 事件流    │    │ 实时监控  │
                    │ EventBus │    │ Metrics  │
                    └──────────┘    └──────────┘
```

---

## 3. 详细设计

### 3.1 核心 Trait 定义

#### 3.1.1 风险检查器

```rust
// 声明式规则：纯函数，无副作用
pub trait RiskChecker: Send + Sync {
    type Input;
    type Output;
    type Error;
    
    fn check(&self, input: Self::Input) -> Result<Self::Output, Self::Error>;
    
    // 动态参数调整
    fn update_params(&mut self, market_volatility: f64) -> Result<(), ConfigError>;
}
```

#### 3.1.2 订单风险

```rust
pub struct OrderRisk {
    // 静态规则
    pub price_guard: PriceGuard,
    pub quantity_limits: QuantityLimits,
    
    // 动态调整
    pub volatility_adjuster: VolatilityAdjuster,
}

impl OrderRisk {
    pub fn check(&self, order: &OrderIntent) -> OrderRiskResult;
}

pub struct OrderRiskResult {
    pub allowed: bool,
    pub risk_score: f64, // 0-100，风险评分
    pub reasons: Vec<String>,
    pub adjusted_limit: Option<OrderIntent>, // 如果触发软限制，返回调整后的订单
}
```

#### 3.1.3 仓位风险

```rust
pub struct PositionRisk {
    // 实时状态
    pub positions: HashMap<InstrumentId, Position>,
    
    // 限制规则
    pub position_limits: PositionLimits,
    pub pnl_limits: PnLLimits,
    pub stop_loss: StopLossConfig,
}

impl PositionRisk {
    pub fn check(&self, signal: &Signal) -> PositionRiskResult;
    
    // 计算实时 PnL
    pub fn calculate_pnl(&self) -> PortfolioPnL;
}

pub struct PositionRiskResult {
    pub allowed: bool,
    pub margin_used: f64,
    pub margin_free: f64,
    pub risk_exposure: f64, // 风险敞口
}
```

#### 3.1.4 组合风险

```rust
pub struct PortfolioRisk {
    // 相关性矩阵
    pub correlation_matrix: CorrelationMatrix,
    
    // 风险指标
    pub var_calculator: VaRCalculator,
    pub cva_calculator: CVACalculator,
    pub stress_tester: StressTester,
}

impl PortfolioRisk {
    pub fn check(&self, portfolio: &PortfolioState) -> PortfolioRiskResult;
    
    // 实时风险指标
    pub fn get_metrics(&self) -> RiskMetrics;
}

pub struct PortfolioRiskResult {
    pub allowed: bool,
    pub total_var: f64,  // 95% VaR
    pub total_cvar: f64, // 95% CVaR
    pub sector_exposure: HashMap<String, f64>,
}
```

### 3.2 动态参数调整

#### 3.2.1 波动率调整算法

```rust
pub struct VolatilityAdjuster {
    pub half_life: Duration,  // 波动率半衰期，默认 20 分钟
    pub target_volatility: f64, // 目标波动率，默认 10%
    pub adjustment_factor: f64, // 调整因子，默认 0.1
}

impl VolatilityAdjuster {
    // 计算当前波动率（指数加权移动平均）
    fn current_volatility(&self, returns: &[f64]) -> f64;
    
    // 动态调整限制参数
    fn adjust_limit(&self, current_vol: f64, target_vol: f64) -> f64;
}

// 应用示例：波动率升高时收紧限制
pub struct DynamicLimits {
    pub base_qty: f64,
    pub volatility_adjuster: VolatilityAdjuster,
}

impl DynamicLimits {
    pub fn get_current_qty(&self, volatility: f64) -> f64 {
        let factor = self.volatility_adjuster.adjust_limit(volatility, 1.0);
        self.base_qty * factor
    }
}
```

#### 3.2.2 算法调整策略

```rust
enum AdjustmentStrategy {
    // 固定半衰期调整
    HalfLife { half_life: Duration, target: f64 },
    
    // 基于历史波动率
    HistoricalVolatility { window: usize, percentile: f64 },
    
    // 基于隐含波动率（如果可用）
    ImpliedVolatility { source: Arc<dyn ImpliedVolSource> },
    
    // 组合策略
    Hybrid { strategies: Vec<Box<dyn AdjustmentStrategy>> },
}
```

### 3.3 风险规则引擎

#### 3.3.1 声明式规则

```rust
// 基础规则
pub struct LimitRule {
    pub field: Field,
    pub operator: Operator,  // Lt, Le, Gt, Ge, Eq, Ne
    pub threshold: f64,
}

// 条件组合
pub struct Condition {
    pub rules: Vec<LimitRule>,
    pub logic: Logic,  // And, Or, Xor
}

// 动作
pub enum Action {
    Reject,           // 拒绝交易
    Approve,          // 批准
    ApproveWithLimit, // 批准但降低限额
    ApproveWithDelay, // 批准但延迟执行
    RequestReview,    // 请求人工审核
}

// 风控规则
pub struct RiskRule {
    pub name: String,
    pub condition: Condition,
    pub action: Action,
    pub priority: u32,
}
```

#### 3.3.2 规则组合器

```rust
pub struct RiskRuleCombinator {
    pub rules: Vec<RiskRule>,
    pub default_action: Action,
    pub short_circuit: bool, // 是否匹配第一个就停止
}

impl RiskRuleCombinator {
    pub fn evaluate(&self, context: &RiskContext) -> RiskDecision;
}

pub enum RiskDecision {
    Allow { risk_score: f64 },
    Deny { reason: String },
    Modify { modified: RiskContext, reason: String },
}
```

### 3.4 实时监控

#### 3.4.1 指标收集

```rust
pub struct RiskMetrics {
    // 实时指标
    pub current_var: f64,           // 当前 VaR
    pub current_cvar: f64,          // 当前 CVaR
    pub max_drawdown: f64,          // 最大回撤
    pub daily_pnl: f64,             // 今日 PnL
    pub position_count: usize,      // 持仓数量
    
    // 风险暴露
    pub total_exposure: f64,        // 总敞口
    pub sector_exposure: HashMap<String, f64>,
    pub correlation_changes: f64,   // 相关性变化率
    
    // 规则触发
    pub rules_triggered: HashMap<String, usize>,
}

pub struct MetricsCollector {
    pub metrics: HashMap<String, MetricValue>,
    pub histograms: Histograms,
}
```

#### 3.4.2 告警系统

```rust
pub enum RiskAlert {
    // 阈值告警
    ThresholdExceeded {
        metric: String,
        current: f64,
        threshold: f64,
        severity: AlertSeverity,
    },
    
    // 规则触发
    RuleTriggered {
        rule_id: String,
        context: RiskContext,
        severity: AlertSeverity,
    },
    
    // 系统告警
    System {
        message: String,
        severity: AlertSeverity,
    },
}

pub struct AlertManager {
    pub alerts: Vec<RiskAlert>,
    pub thresholds: HashMap<String, (f64, AlertSeverity)>,
}
```

### 3.5 风险量化

#### 3.5.1 VaR 计算

```rust
pub enum VaRMethod {
    // 历史模拟法
    Historical { window: usize, confidence: f64 },
    
    // 参数法
    Parametric { distribution: DistributionType, confidence: f64 },
    
    // 蒙特卡洛
    MonteCarlo { iterations: usize, confidence: f64 },
}

pub struct VaRCalculator {
    pub method: VaRMethod,
    pub correlation_matrix: CorrelationMatrix,
}

impl VaRCalculator {
    pub fn calculate(&self, portfolio: &Portfolio) -> f64;
}
```

#### 3.5.2 相关性分析

```rust
pub struct CorrelationMatrix {
    pub matrix: Matrix<f64>,  // N×N 矩阵
    pub last_updated: i64,
}

impl CorrelationMatrix {
    fn update(&mut self, returns: &[Vec<f64>]);
    
    fn get_sector_correlation(&self, sector_a: &str, sector_b: &str) -> f64;
}

// 流动性风险
pub struct LiquidityRisk {
    pub market_depth: MarketDepth,
    pub spread: f64,
    pub volume_profile: VolumeProfile,
}

impl LiquidityRisk {
    fn calculate_impact_cost(&self, qty: f64) -> f64;
}
```

### 3.6 压力测试

```rust
pub struct StressTest {
    pub scenarios: Vec<StressScenario>,
    pub baseline: BaselineMetrics,
}

pub enum StressScenario {
    // 市场冲击
    MarketCrash { drop_percent: f64 },
    MarketRally { rise_percent: f64 },
    
    // 流动性枯竭
    LiquidityDryUp { spread_multiplier: f64, volume_ratio: f64 },
    
    // 相关性激增
    CorrelationSpike { correlation: f64 },
    
    // 极端事件
    BlackSwan { custom: Box<dyn CustomScenario> },
}

impl StressTest {
    pub fn run(&self) -> StressResult;
}

pub struct StressResult {
    pub portfolio_value: f64,
    pub max_loss: f64,
    pub violations: Vec<Violation>,
}
```

### 3.7 报告系统

```rust
pub enum ReportType {
    // 实时报告
    RealTime {
        metrics: RiskMetrics,
        timestamp: i64,
    },
    
    // 日报
    Daily {
        period: DateRange,
        pnl: DailyPnL,
        risk_limits: DailyLimits,
        violations: DailyViolations,
    },
    
    // 监管报告
    Regulatory {
        format: RegulatoryFormat,  // Basel, SEC, etc.
        data: RegulatoryData,
    },
    
    // 回溯报告
    Backtest {
        strategy_id: String,
        results: BacktestResults,
    },
}

impl ReportType {
    fn generate(&self) -> Report;
}
```

---

## 4. 数据模型

### 4.1 核心实体

```rust
// 订单
pub struct Order {
    pub id: String,
    pub instrument: InstrumentId,
    pub side: Side,
    pub qty: f64,
    pub limit_price: f64,
    pub status: OrderStatus,
    pub risk_score: f64,
}

// 仓位
pub struct Position {
    pub instrument: InstrumentId,
    pub qty: f64,
    pub avg_price: f64,
    pub side: Side,
    pub unrealized_pnl: f64,
    pub realized_pnl: f64,
}

// 组合状态
pub struct PortfolioState {
    pub positions: HashMap<InstrumentId, Position>,
    pub cash: f64,
    pub total_value: f64,
    pub daily_pnl: f64,
    pub max_drawdown: f64,
}
```

### 4.2 数据库 Schema

```sql
-- 风险规则配置
CREATE TABLE risk_rules (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    rule_type TEXT NOT NULL,  -- order, position, portfolio
    condition_json JSONB,
    action TEXT NOT NULL,     -- reject, approve, modify
    priority INTEGER DEFAULT 0,
    is_active BOOLEAN DEFAULT true,
    created_at TIMESTAMP
);

-- 风险事件
CREATE TABLE risk_events (
    id TEXT PRIMARY KEY,
    event_type TEXT NOT NULL,  -- order_check, pnl_alert, var_exceeded
    instrument TEXT,
    current_value REAL,
    threshold_value REAL,
    severity TEXT,
    decision TEXT,             -- allow, deny, modify
    context_json JSONB,
    ts_ms INTEGER NOT NULL
);

-- 风险指标历史
CREATE TABLE risk_metrics_history (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    timestamp INTEGER NOT NULL,
    var_95 REAL,
    cvar_95 REAL,
    max_drawdown REAL,
    total_exposure REAL,
    rules_triggered JSONB,
    volatility_adjusted BOOLEAN
);

-- 压力测试记录
CREATE TABLE stress_test_results (
    id TEXT PRIMARY KEY,
    scenario TEXT NOT NULL,
    baseline_value REAL,
    stressed_value REAL,
    loss_percent REAL,
    violations JSONB,
    ts_ms INTEGER NOT NULL
);
```

---

## 5. 执行流程

### 5.1 实时执行（实盘/模拟）

```
1. 订单生成：Signal -> OrderIntent
2. 订单层检查：
   - PriceGuard: 检查价格合理性
   - QuantityLimits: 检查数量限制
   - VolatilityAdjuster: 动态调整限制
3. 仓位层检查：
   - PositionLimits: 检查仓位限额
   - PnLLimits: 检查 PnL 限额
   - StopLoss: 检查止损
4. 组合层检查：
   - VaR: 计算风险敞口
   - Correlation: 检查相关性风险
5. 决策：
   - 允许：执行订单
   - 拒绝：返回原因
   - 调整：修改订单后重试
6. 监控：
   - 记录风险评分
   - 更新实时指标
   - 发送告警（如需要）
```

### 5.2 回测执行

```
1. 加载历史数据
2. 模拟时间推进
3. 对每个时间点：
   a. 构建历史 Context（历史仓位、历史价格）
   b. 执行风控检查（使用历史波动率调整参数）
   c. 记录风控决策
4. 生成回测报告：
   - 风险指标曲线
   - 违规记录
   - 参数敏感性分析
```

---

## 6. 错误处理

```rust
#[derive(Debug, thiserror::Error)]
pub enum RiskError {
    #[error("Order rejected: {0}")]
    OrderRejected(String),
    
    #[error("Position limit exceeded: {0}")]
    PositionLimit(String),
    
    #[error("Portfolio risk exceeded: VaR={current:.2f} > Limit={limit:.2f}")]
    PortfolioRisk { current: f64, limit: f64 },
    
    #[error("Dynamic adjustment failed: {0}")]
    AdjustmentFailed(String),
    
    #[error("Rule evaluation error: {0}")]
    RuleError(String),
}
```

---

## 7. 配置管理

```yaml
# risk_config.yaml
order_risk:
  max_qty: 100
  max_notional: 1000000
  price_deviation_limit: 0.05  # 5%
  
dynamic_adjustment:
  enabled: true
  half_life: "20m"
  target_volatility: 0.10
  
position_risk:
  max_position_percent: 0.10  # 10% of portfolio
  daily_pnl_limit: 0.05  # 5% daily limit
  stop_loss: 0.02  # 2% stop loss
  
portfolio_risk:
  var_limit_95: 0.03  # 3% VaR limit
  max_correlation: 0.8  # 最大相关性阈值
  
alerts:
  enabled: true
  channels:
    - slack
    - email
```

---

## 8. 测试策略

### 8.1 单元测试

```rust
#[test]
fn test_order_rejected_when_price_too_high() {
    let risk = OrderRisk::new();
    let order = OrderIntent { limit_price: 100.0, ..Default::default() };
    let result = risk.check(&order);
    assert!(!result.allowed);
    assert!(result.reasons.contains(&"price exceeds limit".to_string()));
}

#[test]
fn test_dynamic_adjustment() {
    let mut adjuster = VolatilityAdjuster::new();
    adjuster.update_params(0.50); // 高波动率
    let factor = adjuster.adjust_limit(0.50, 1.0);
    assert!(factor < 1.0); // 收紧限制
}
```

### 8.2 集成测试

```rust
#[test]
fn test_full_risk_pipeline() {
    // 模拟完整风控流程
    let portfolio = Portfolio::new();
    let risk = PortfolioRisk::new();
    let result = risk.check(&portfolio);
    
    // 验证 VaR 计算
    assert!(result.total_var > 0.0);
    assert!(result.total_var < 0.05); // 低于限制
}
```

---

## 9. 使用示例

```rust
// 1. 配置风控
let risk_config = RiskConfig {
    order_limits: OrderLimits { max_qty: 100, ..Default::default() },
    dynamic_adjustment: VolatilityAdjuster { half_life: 20.min, ..Default::default() },
    ..Default::default()
};

// 2. 创建风控器
let portfolio_risk = PortfolioRisk::new(risk_config);

// 3. 实时检查
let order = OrderIntent {
    instrument: InstrumentId::new(Venue::Crypto, "BTC-USD"),
    qty: 50.0,
    limit_price: 45000.0,
    ..Default::default()
};

let result = portfolio_risk.check(&order);
if result.allowed {
    execute_order(order);
} else {
    log_error(&result.reasons);
}

// 4. 回测
let backtest = BacktestEngine::new(
    strategy,
    historical_data,
    RiskConfig {
        // 回测配置
    },
);
let report = backtest.run_with_risk();
```

---

## 10. 实施计划

### 阶段 1：核心框架（1 周）
- [ ] 核心 Trait 定义和基础实现
- [ ] 订单层风控（价格、数量）
- [ ] 动态调整算法
- [ ] 规则引擎框架

### 阶段 2：仓位与组合（1 周）
- [ ] 仓位层风控（PnL、止损）
- [ ] 组合层风控（VaR、相关性）
- [ ] 实时指标计算

### 阶段 3：高级功能（1 周）
- [ ] 压力测试
- [ ] 完整报告系统
- [ ] 监控和告警

---

## 11. 依赖

```toml
[dependencies]
# 数学计算
num-traits = "0.2"
rand = "0.8"

# 矩阵运算（压力测试）
ndarray = "0.15"

# 时间序列
arrow = "5.0"

# 日志和监控
tracing = "0.1"
tracing-appender = "0.2"
```

---

**审批问题**：
1. 三层架构设计是否符合预期？
2. 动态调整算法是否合理？
3. 是否需要调整功能范围？

请确认是否批准此设计。