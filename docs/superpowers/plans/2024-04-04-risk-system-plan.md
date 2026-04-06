# Risk System Implementation Plan

**Version**: 1.0.0  
**Priority**: P0  
**Estimated Duration**: 4 weeks  
**Dependencies**: Strategy System (Phase 1)

---

## 1. Implementation Phases

### Phase 1: Core Framework & Order Layer (Week 1)
**Goal**: Establish risk checker traits and order-level risk controls

| Task | Duration | Dependencies | Deliverables |
|------|----------|--------------|--------------|
| 1.1 RiskChecker Trait Definition | 2 days | None | `src/core/risk/mod.rs` |
| 1.2 OrderRisk Implementation | 3 days | 1.1 | `src/risk/order.rs` |
| 1.3 PriceGuard & QuantityLimits | 2 days | 1.2 | `src/risk/order.rs` |
| 1.4 VolatilityAdjuster | 2 days | 1.3 | `src/risk/dynamic.rs` |
| 1.5 RiskRule Engine | 3 days | 1.1 | `src/risk/rules.rs` |
| 1.6 OrderRiskResult & Scoring | 2 days | 1.2 | `src/risk/order.rs` |
| 1.7 Integration Tests (Unit) | 2 days | 1.1-1.6 | `tests/unit/risk/*.rs` |
| 1.8 Performance Benchmarks | 2 days | 1.2-1.6 | `benches/risk/*.rs` |

**Rollback Plan**: If volatility adjustment causes instability, revert to static limits and implement gradual rollout.

---

### Phase 2: Position & Portfolio Layer (Week 2)
**Goal**: Implement position tracking and portfolio-level risk

| Task | Duration | Dependencies | Deliverables |
|------|----------|--------------|--------------|
| 2.1 PositionManager Core | 3 days | 1.1 | `src/risk/position.rs` |
| 2.2 PnLLimits & StopLoss | 2 days | 2.1 | `src/risk/position.rs` |
| 2.3 PortfolioRisk Core | 3 days | 1.1 | `src/risk/portfolio.rs` |
| 2.4 VaRCalculator | 3 days | 2.3 | `src/risk/portfolio.rs` |
| 2.5 CorrelationMatrix | 2 days | 2.4 | `src/risk/portfolio.rs` |
| 2.6 Real-time Metrics | 2 days | 2.1-2.3 | `src/risk/metrics.rs` |
| 2.7 Integration Tests | 2 days | 2.1-2.6 | `tests/integration/risk/*.rs` |
| 2.8 Chaos Testing | 2 days | 2.1-2.6 | `tests/chaos/risk/*.rs` |

**Rollback Plan**: If VaR calculation performance degrades, switch to simplified parametric method.

---

### Phase 3: Advanced Analytics & Stress Testing (Week 3)
**Goal**: Build Monte Carlo, sensitivity analysis, and alert system

| Task | Duration | Dependencies | Deliverables |
|------|----------|--------------|--------------|
| 3.1 MonteCarloSimulator | 4 days | 2.4 | `src/analysis/monte_carlo.rs` |
| 3.2 SensitivityAnalyzer | 3 days | 2.3 | `src/analysis/sensitivity.rs` |
| 3.3 StressTestEngine | 3 days | 2.4 | `src/analysis/stress.rs` |
| 3.4 AlertManager | 2 days | 2.6 | `src/alert/mod.rs` |
| 3.5 RiskReport Generator | 2 days | 3.1-3.3 | `src/report/mod.rs` |
| 3.6 Data Quality Checks | 2 days | 1.1 | `src/data/quality.rs` |
| 3.7 Integration Tests | 2 days | 3.1-3.6 | `tests/integration/risk/*.rs` |
| 3.8 Performance Optimization | 2 days | 3.1-3.6 | `benches/risk/*.rs` |

**Rollback Plan**: If Monte Carlo is too slow, fall back to historical simulation only.

---

### Phase 4: Execution Modes & Integration (Week 4)
**Goal**: Connect risk to live/paper/backtest modes

| Task | Duration | Dependencies | Deliverables |
|------|----------|--------------|--------------|
| 4.1 Live Execution Mode | 3 days | 2.3-2.6 | `src/execution/live.rs` |
| 4.2 Paper Execution Mode | 2 days | 2.1-2.2 | `src/execution/paper.rs` |
| 4.3 Backtest Mode | 3 days | 1.1, 2.3 | `src/execution/backtest.rs` |
| 4.4 Database Persistence | 3 days | 2.6 | `src/persistence/mod.rs` |
| 4.5 gRPC Service | 3 days | 2.3-2.6 | `src/api/grpc.rs` |
| 4.6 REST API | 2 days | 4.5 | `src/api/http.rs` |
| 4.7 Configuration Schema | 2 days | 1.1 | `src/config/mod.rs` |
| 4.8 System Integration | 2 days | All | `src/main.rs` |

**Rollback Plan**: If live mode causes latency spikes, implement async risk checks with fallback.

---

## 2. Technical Architecture

### 2.1 Core Design Decisions

| Decision | Rationale | Trade-offs |
|----------|-----------|------------|
| **Pure Functional RiskChecker** | Ensures deterministic decisions for backtesting | Requires explicit context passing |
| **Three-Layer Architecture** | Order < Position < Portfolio hierarchy | Complexity in cross-layer dependencies |
| **EWMA Volatility Adjustment** | Adapts to market conditions | Computational overhead (negligible) |
| **Hybrid VaR Methods** | Historical + Parametric + Monte Carlo accuracy | Implementation complexity |
| **Arc<Mutex<>> for State** | Thread-safe concurrent access | Potential contention hotspots |
| **Real-time vs Batch** | Real-time for trading, batch for analysis | Code duplication concerns |

### 2.2 Architecture Diagram

```
┌─────────────────────────────────────────────────────────────┐
│                    Application Layer                         │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐          │
│  │ Monte Carlo │  │ Sensitivity │  │ Alert       │          │
│  │ Simulator   │  │ Analyzer    │  │ Manager     │          │
│  └─────────────┘  └─────────────┘  └─────────────┘          │
└─────────────────────────────────────────────────────────────┘
                              │
┌─────────────────────────────────────────────────────────────┐
│                     Risk Engine Layer                        │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐          │
│  │ Portfolio   │  │ Position    │  │ Order       │          │
│  │ Risk        │  │ Risk        │  │ Risk        │          │
│  └─────────────┘  └─────────────┘  └─────────────┘          │
│  ┌─────────────┐  ┌─────────────┐                          │
│  │ Dynamic     │  │ Rules       │                          │
│  │ Adjuster    │  │ Engine      │                          │
│  └─────────────┘  └─────────────┘                          │
└─────────────────────────────────────────────────────────────┘
                              │
┌─────────────────────────────────────────────────────────────┐
│                      Execution Layer                         │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐          │
│  │ Live        │  │ Paper       │  │ Backtest    │          │
│  │ Adapter     │  │ Adapter     │  │ Adapter     │          │
│  └─────────────┘  └─────────────┘  └─────────────┘          │
└─────────────────────────────────────────────────────────────┘
```

### 2.3 Key Implementation Details

#### 2.3.1 RiskChecker Trait
```rust
pub trait RiskChecker: Send + Sync {
    fn check(&self, input: &RiskInput) -> Result<RiskDecision, RiskError>;
    fn update_params(&mut self, volatility: f64) -> Result<(), ConfigError>;
}

pub struct RiskInput {
    pub order: OrderIntent,
    pub portfolio: PortfolioState,
    pub market_data: MarketData,
}

pub struct RiskDecision {
    pub allowed: bool,
    pub risk_score: f64,  // 0-100
    pub reasons: Vec<String>,
    pub adjusted_limit: Option<OrderIntent>,
}
```

#### 2.3.2 Dynamic Volatility Adjustment
```rust
pub struct VolatilityAdjuster {
    pub half_life: Duration,
    pub target_volatility: f64,
    pub adjustment_factor: f64,  // Default 0.1
}

impl VolatilityAdjuster {
    pub fn current_volatility(&self, returns: &[f64]) -> f64;
    pub fn adjust_limit(&self, current: f64, target: f64) -> f64;
}
```

#### 2.3.3 VaR Calculation
```rust
pub enum VaRMethod {
    Historical { window: usize, confidence: f64 },
    Parametric { distribution: DistributionType, confidence: f64 },
    MonteCarlo { iterations: usize, confidence: f64 },
}

pub struct VaRCalculator {
    pub method: VaRMethod,
    pub correlation_matrix: CorrelationMatrix,
}
```

---

## 3. Database Schema

### 3.1 Migration Files

#### 001_risk_core.sql
```sql
-- Risk rules configuration
CREATE TABLE risk_rules (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    rule_type TEXT NOT NULL CHECK (rule_type IN ('order', 'position', 'portfolio')),
    condition_json JSONB,
    action TEXT NOT NULL CHECK (action IN ('reject', 'approve', 'modify', 'delay')),
    priority INTEGER DEFAULT 0,
    is_active BOOLEAN DEFAULT true,
    created_at TIMESTAMP,
    updated_at TIMESTAMP
);

CREATE INDEX idx_risk_rules_active ON risk_rules(is_active);
CREATE INDEX idx_risk_rules_type ON risk_rules(rule_type);

-- Risk events (audit trail)
CREATE TABLE risk_events (
    id TEXT PRIMARY KEY,
    event_type TEXT NOT NULL CHECK (event_type IN (
        'order_check',
        'position_check',
        'portfolio_check',
        'pnl_alert',
        'var_exceeded',
        'rule_triggered'
    )),
    instrument TEXT,
    current_value REAL,
    threshold_value REAL,
    severity TEXT,
    decision TEXT,
    context_json JSONB,
    ts_ms INTEGER NOT NULL
);

CREATE INDEX idx_risk_events_ts ON risk_events(ts_ms);
CREATE INDEX idx_risk_events_type ON risk_events(event_type);

-- Risk metrics history
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

CREATE INDEX idx_risk_metrics_ts ON risk_metrics_history(timestamp);

-- Stress test results
CREATE TABLE stress_test_results (
    id TEXT PRIMARY KEY,
    scenario TEXT NOT NULL,
    baseline_value REAL,
    stressed_value REAL,
    loss_percent REAL,
    violations JSONB,
    ts_ms INTEGER NOT NULL
);

CREATE INDEX idx_stress_test_ts ON stress_test_results(ts_ms);
```

#### 002_risk_optimization.sql
```sql
-- Optimization runs
CREATE TABLE optimization_runs (
    id TEXT PRIMARY KEY,
    strategy_id TEXT,
    optimization_type TEXT CHECK (optimization_type IN ('grid_search', 'bayesian')),
    param_grid JSONB,
    objective TEXT,
    validation_split INTEGER,
    status TEXT CHECK (status IN ('completed', 'failed', 'running')),
    best_params JSONB,
    best_score REAL,
    results JSONB,
    created_at TIMESTAMP,
    completed_at TIMESTAMP
);

-- Parameter sweep
CREATE TABLE parameter_sweep (
    id TEXT PRIMARY KEY,
    optimization_id TEXT REFERENCES optimization_runs(id),
    param_set JSONB,
    score REAL,
    backtest_id TEXT,
    created_at TIMESTAMP
);

CREATE INDEX idx_parameter_sweep_score ON parameter_sweep(score DESC);
```

---

## 4. Test Strategy

### 4.1 Unit Tests
```rust
#[test]
fn test_order_rejected_when_price_too_high() {
    let risk = OrderRisk::new();
    let order = OrderIntent { limit_price: 100.0, ..Default::default() };
    let result = risk.check(&order);
    assert!(!result.allowed);
}

#[test]
fn test_dynamic_adjustment() {
    let mut adjuster = VolatilityAdjuster::new();
    adjuster.update_params(0.50);
    let factor = adjuster.adjust_limit(0.50, 1.0);
    assert!(factor < 1.0);
}
```

### 4.2 Integration Tests
```rust
#[test]
fn test_full_risk_pipeline() {
    let portfolio = Portfolio::new();
    let risk = PortfolioRisk::new();
    let result = risk.check(&portfolio);
    assert!(result.total_var > 0.0);
}
```

---

## 5. API Contracts

### 5.1 gRPC
```proto
service RiskService {
  rpc CheckOrder(CheckOrderRequest) returns (CheckOrderResponse);
  rpc GetPortfolioRisk(PortfolioRequest) returns (PortfolioRiskResponse);
  rpc RunStressTest(StressTestRequest) returns (StressTestResult);
}

message CheckOrderRequest {
  OrderRequest order = 1;
  PortfolioState portfolio = 2;
}

message CheckOrderResponse {
  bool allowed = 1;
  RiskScore risk_score = 2;
  string reason = 3;
  float adjusted_limit = 4;
}
```

---

## 6. Configuration Schema

```yaml
risk:
  order:
    max_qty: 100
    max_notional: 1000000
    price_deviation_limit: 0.05
    
  dynamic_adjustment:
    enabled: true
    half_life: "20m"
    target_volatility: 0.10
    
  position:
    max_position_percent: 0.10
    daily_pnl_limit: 0.05
    stop_loss: 0.02
    
  portfolio:
    var_limit_95: 0.03
    max_correlation: 0.8
    
  alerts:
    enabled: true
    channels: [slack, email]
```

---

## 7. Rollback Plan

- **Phase 1**: Disable dynamic adjustment, revert to static limits
- **Phase 2**: Switch to simplified VaR calculation
- **Phase 3**: Fall back to Monte Carlo only
- **Phase 4**: Implement async risk checks with circuit breakers

---

## 8. Dependencies

```toml
num-traits = "0.2"
rand = "0.8"
ndarray = "0.15"
arrow = "5.0"
```

---

This plan provides a comprehensive roadmap for building the Risk System with layered architecture, dynamic adjustment, and complete risk analytics.
