# Risk System Implementation Plan

**Project**: Layered Risk Management System  
**Date**: 2024-04-04  
**Duration**: 4 weeks  
**Priority**: P0

---

## 1. Executive Summary

This plan implements a **three-layer risk engine** (Order → Position → Portfolio) with dynamic volatility adjustment, real-time VaR/CVaR calculation, and Monte Carlo stress testing. The system supports three execution modes: Live, Paper, and Backtest.

**Key Technical Decisions**:
- **Language**: Rust with zero-cost abstractions for high-frequency risk calculations
- **Architecture**: Trait-based plugin system for extensibility
- **Data Flow**: Event-driven with gRPC for real-time updates
- **Observability**: OpenTelemetry for distributed tracing

---

## 2. Implementation Phases

### Phase 1: Core Framework & Order Layer (Week 1)

| Task | Duration | Dependencies | Rollback Plan |
|------|----------|--------------|---------------|
| **1.1 Core Trait System** | 2 days | None | Revert to previous risk module |
| | | | |
| | Define `RiskChecker` trait with generics for Input/Output/Error types | |
| | Implement `Result` wrapper with risk_score (0-100) and reasons vector | |
| | Create `RiskContext` struct for rule evaluation state | |
| 1.2 Order Layer Risk | 3 days | 1.1 |
| | | | |
| | Implement `PriceGuard`: deviation checks, price bands, max slippage | |
| | Implement `QuantityLimits`: base limits with soft/hard boundaries | |
| | Implement `VolatilityAdjuster`: EWMA volatility calculation with half-life decay |
| | Create `OrderRiskResult` with adjusted_limit field for soft rejections |
| 1.3 Dynamic Adjustment | 2 days | 1.1, 1.2 |
| | | | |
| | Implement EWMA volatility estimator (half-life = 20min default) | |
| | Implement adjustment factor calculation: `factor = exp(-ln(target/current)/half_life)` | |
| | Create `DynamicLimits` trait for pluggable strategies |
| | Add config schema for adjustment parameters |
| **Phase 1 Deliverables**:
- Core trait library with zero-cost abstractions
- Order risk engine with dynamic limits
- Configuration schema v1.0

**Rollback Strategy**:
- All components are pure functions with no shared state
- Feature flags enable instant rollback via Cargo feature toggles
- Database schema uses backward-compatible JSONB columns

---

### Phase 2: Position & Portfolio Layer (Week 2)

| Task | Duration | Dependencies | Rollback Plan |
|------|----------|--------------|---------------|
| **2.1 Position Layer** | 3 days | 1.1 |
| | | | |
| | Implement `PositionRisk`: margin calculation, exposure tracking | |
| | Implement `PnLLimits`: daily/total limits with reset logic | |
| | Implement `StopLoss`: trailing stop, hard stop, time-based exits | |
| | Create `PositionRiskResult` with margin_used/free breakdown |
| 2.2 Portfolio Layer | 4 days | 2.1 |
| | | | |
| | Implement `CorrelationMatrix`: rolling window correlation (21-day default) | |
| | Implement `VaRCalculator`: Historical, Parametric, Monte Carlo methods | |
| | Implement `CVACalculator`: Expected shortfall calculation | |
| | Create `PortfolioRiskResult` with sector exposure breakdown |
| 2.3 Liquidity Risk | 2 days | 2.1 |
| | | | |
| | Implement `LiquidityRisk`: market depth analysis, spread impact | |
| | Implement `ImpactCost` calculation: VWAP vs TWAP deviation | |
| | Add order book depth integration hooks |
| **Phase 2 Deliverables**:
- Position and portfolio risk engines
- VaR/CVaR calculation library
- Liquidity risk module

**Rollback Strategy**:
- Portfolio calculations are idempotent and re-entrant
- All math operations use `f64` with explicit precision controls
- Matrix operations cached with TTL to prevent OOM

---

### Phase 3: Advanced Analytics & Stress Testing (Week 3)

| Task | Duration | Dependencies | Rollback Plan |
|------|----------|--------------|---------------|
| **3.1 Stress Testing** | 3 days | 2.2 |
| | | | |
| | Implement `StressTester`: scenario generation and execution | |
| | Implement scenarios: MarketCrash, LiquidityDryUp, CorrelationSpike, BlackSwan | |
| | Create `StressResult` with violation tracking per scenario | |
| | Add Monte Carlo simulation engine (10,000 iterations default) | |
| 3.2 Real-time Metrics | 2 days | 2.2 |
| | | | |
| | Implement `MetricsCollector`: sliding window aggregations | |
| | Implement histograms for PnL distribution analysis | |
| | Add correlation change rate detection | |
| 3.3 Alert System | 2 days | 2.2 |
| | | | |
| | Implement `RiskAlert`: threshold, rule-triggered, system alerts | |
| | Create `AlertManager` with severity levels (Info, Warning, Critical, Emergency) |
| | Implement alert deduplication and rate limiting |
| **Phase 3 Deliverables**:
- Stress testing framework with 4+ scenario types
- Real-time metrics with histogram support
- Alert system with severity escalation

**Rollback Strategy**:
- Monte Carlo uses seed for reproducibility in backtest mode
- Alert system is append-only (no state mutation)
- Metrics use ring buffer with configurable size

---

### Phase 4: Execution Modes & Integration (Week 4)

| Task | Duration | Dependencies | Rollback Plan |
|------|----------|--------------|---------------|
| **4.1 Three Execution Modes** | 3 days | All previous |
| | | | |
| | Implement `ExecutionMode` enum: Live, Paper, Backtest |
| | Live: Real-time price feeds, immediate execution |
| | Paper: Simulated execution with real risk checks |
| | Backtest: Historical replay with deterministic RNG |
| | Mode-specific config overrides |
| 4.2 API Integration | 2 days | 4.1 |
| | | | |
| | gRPC service for real-time risk queries |
| | REST API for rule management and reports |
| | WebSocket streaming for risk metrics |
| 4.3 Database & Persistence | 2 days | 4.2 |
| | | | |
| | PostgreSQL schema with migration files |
| | TimescaleDB for metric history (retention: 1 year) |
| | Audit logging for all risk decisions |
| **Phase 4 Deliverables**:
- Full execution mode support
- Production-ready APIs
- Database schema and migrations

**Rollback Strategy**:
- Mode switching via feature flags (no code changes)
- API versioning for backward compatibility
- Database migrations are reversible with migration scripts

---

## 3. Technical Architecture

### 3.1 Architecture Overview

```
┌─────────────────────────────────────────────────────────────────┐
│                         API Layer                                │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌─────────────────┐  │
│  │  gRPC    │  │  REST    │  │  WS      │  │  Batch API      │  │
│  │  Client  │  │  Admin   │  │  Stream  │  │  (Nightly jobs) │  │
│  └──────────┘  └──────────┘  └──────────┘  └─────────────────┘  │
└─────────────────────────────────────────────────────────────────┘
                              │
┌─────────────────────────────────────────────────────────────────┐
│                         Core Engine                              │
│  ┌──────────────────────────────────────────────────────────┐   │
│  │                Risk Rule Combinator                       │   │
│  │  (Pluggable conditions, short-circuit evaluation)        │   │
│  └──────────────────────────────────────────────────────────┘   │
│                           │                                      │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌─────────────────┐   │
│  │ Order    │  │Position  │  │Portfolio │  │  Liquidity      │   │
│  │ Risk     │  │ Risk     │  │ Risk     │  │ Risk            │   │
│  │ Engine   │  │ Engine   │  │ Engine   │  │ Engine          │   │
│  └──────────┘  └──────────┘  └──────────┘  └─────────────────┘   │
└─────────────────────────────────────────────────────────────────┘
                              │
┌─────────────────────────────────────────────────────────────────┐
│                         Analytics                                │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌─────────────────┐   │
│  │ VaR/CVaR │  │ Stress   │  │ Metrics  │  │ Correlation    │   │
│  │ Calc     │  │ Testing  │  │ Collector│  │ Matrix          │   │
│  └──────────┘  └──────────┘  └──────────┘  └─────────────────┘   │
└─────────────────────────────────────────────────────────────────┘
                              │
┌─────────────────────────────────────────────────────────────────┐
│                         Infrastructure                           │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌─────────────────┐   │
│  │  Event   │  │  Cache   │  │  Config  │  │  Audit Log      │   │
│  │  Bus     │  │  Redis   │  │  Manager │  │  (Immutable)    │   │
│  └──────────┘  └──────────┘  └──────────┘  └─────────────────┘   │
└─────────────────────────────────────────────────────────────────┘
```

### 3.2 Design Decisions & Trade-offs

#### Decision 1: Pure Functions vs. State Machines
**Choice**: Pure functions for all risk checks
- **Pros**: Deterministic, testable, thread-safe, no race conditions
- **Cons**: Requires explicit state passing (context struct)
- **Trade-off**: Complexity in state management vs. Safety and correctness

#### Decision 2: f64 vs. Decimal for Financial Calculations
**Choice**: f64 with explicit precision control
- **Pros**: Fast SIMD operations, hardware acceleration
- **Cons**: Binary floating point representation errors
- **Mitigation**: All monetary values use `round_to_cents()` after calculation
- **Trade-off**: Performance vs. Decimal library overhead

#### Decision 3: EWMA vs. Simple Moving Average for Volatility
**Choice**: EWMA with configurable half-life
- **Formula**: `vol²_new = (1-λ)vol²_old + λ*ret²` where `λ = 2^(2/τ)`
- **Pros**: Exponential weight to recent data, constant memory
- **Cons**: Sensitive to outliers
- **Trade-off**: Responsiveness vs. Stability

#### Decision 4: Monte Carlo vs. Historical Simulation for VaR
**Choice**: Hybrid approach (method selectable per instrument)
- **Pros**: Monte Carlo handles tail events better; Historical uses actual distribution
- **Cons**: Monte Carlo requires covariance matrix (computationally expensive)
- **Trade-off**: Accuracy vs. Latency

#### Decision 5: Synchronous vs. Asynchronous Execution
**Choice**: Synchronous blocking for risk checks
- **Pros**: Deterministic execution order, easier debugging
- **Cons**: Potential latency in high-frequency scenarios
- **Trade-off**: Complexity reduction vs. Latency

---

## 4. Database Schema

### 4.1 Core Tables

```sql
-- 4.1.1 Risk Rules Configuration
CREATE TABLE risk_rules (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    rule_type TEXT NOT NULL,  -- 'order', 'position', 'portfolio', 'liquidity'
    version INTEGER NOT NULL DEFAULT 1,
    
    -- JSONB for flexible rule definitions
    condition_json JSONB NOT NULL,
    action_json JSONB NOT NULL,
    
    -- Metadata
    priority INTEGER NOT NULL DEFAULT 0,
    is_active BOOLEAN DEFAULT true,
    is_enabled BOOLEAN DEFAULT true,  -- Runtime toggle
    effective_from TIMESTAMP,
    effective_until TIMESTAMP,
    
    -- Audit
    created_by TEXT,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    updated_by TEXT,
    updated_at TIMESTAMP,
    
    -- Indexes for performance
    INDEX idx_rule_type_active (rule_type, is_active),
    INDEX idx_effective (effective_from, effective_until)
);

-- 4.1.2 Risk Events (Audit Trail)
CREATE TABLE risk_events (
    id TEXT PRIMARY KEY,
    event_type TEXT NOT NULL,  -- 'order_check', 'limit_exceeded', 'alert_triggered'
    
    -- Event details
    instrument TEXT,
    symbol TEXT,
    venue TEXT,
    
    -- Values
    current_value REAL,
    threshold_value REAL,
    risk_score REAL,
    
    -- Decision
    decision TEXT NOT NULL,  -- 'ALLOW', 'DENY', 'MODIFY', 'DELAY'
    decision_reason TEXT,
    decision_id TEXT,  -- Reference for audit chain
    
    -- Context snapshot
    context_json JSONB,
    
    -- Timing
    ts_ms BIGINT NOT NULL,
    
    -- Indexes
    INDEX idx_event_type_time (event_type, ts_ms),
    INDEX idx_instrument_time (instrument, ts_ms),
    INDEX idx_decision (decision, ts_ms)
);

-- 4.1.3 Risk Metrics History (Time-series)
CREATE TABLE risk_metrics_history (
    id BIGSERIAL PRIMARY KEY,
    
    -- Timestamp (wall clock)
    timestamp BIGINT NOT NULL,  -- Unix ms
    
    -- Portfolio metrics
    var_95 REAL CHECK(var_95 >= 0),
    var_99 REAL CHECK(var_99 >= 0),
    cvar_95 REAL CHECK(cvar_95 >= 0),
    max_drawdown REAL CHECK(max_drawdown >= 0),
    daily_pnl REAL,
    
    -- Exposure
    total_exposure REAL,
    margin_used REAL,
    margin_free REAL,
    utilization REAL CHECK(utilization >= 0 AND utilization <= 1),
    
    -- Volatility state
    current_volatility REAL,
    target_volatility REAL,
    adjustment_factor REAL,
    volatility_adjusted BOOLEAN,
    
    -- Sector breakdown
    sector_exposure JSONB,
    
    -- Rule triggers
    rules_triggered JSONB,
    alerts_triggered INTEGER DEFAULT 0,
    
    -- Constraints
    ts_ms BIGINT NOT NULL,  -- Unique index
    UNIQUE(ts_ms)
);

-- 4.1.4 Stress Test Results
CREATE TABLE stress_test_results (
    id TEXT PRIMARY KEY,
    
    -- Scenario info
    scenario_type TEXT NOT NULL,
    scenario_params JSONB NOT NULL,
    scenario_name TEXT,
    
    -- Baseline vs. Stressed
    baseline_value REAL NOT NULL,
    stressed_value REAL NOT NULL,
    loss_percent REAL,
    loss_absolute REAL,
    
    -- Violations
    violations_count INTEGER DEFAULT 0,
    violations_json JSONB,
    
    -- Execution
    execution_mode TEXT,  -- 'LIVE', 'PAPER', 'BACKTEST'
    
    ts_ms BIGINT NOT NULL,
    
    INDEX idx_scenario_time (scenario_type, ts_ms)
);

-- 4.1.5 Alert Log
CREATE TABLE risk_alerts (
    id TEXT PRIMARY KEY,
    
    -- Alert type
    alert_type TEXT NOT NULL,  -- 'THRESHOLD', 'RULE_TRIGGERED', 'SYSTEM'
    severity TEXT NOT NULL,  -- 'INFO', 'WARNING', 'CRITICAL', 'EMERGENCY'
    
    -- Content
    metric_name TEXT,
    current_value REAL,
    threshold_value REAL,
    message TEXT NOT NULL,
    
    -- Handling
    acknowledged BOOLEAN DEFAULT false,
    acknowledged_by TEXT,
    acknowledged_at TIMESTAMP,
    resolved BOOLEAN DEFAULT false,
    resolved_at TIMESTAMP,
    
    ts_ms BIGINT NOT NULL,
    
    INDEX idx_severity_time (severity, ts_ms)
);
```

### 4.2 Migration Files

**Migration 001: Initial Schema**
```sql
-- Create core tables as defined above
-- Add foreign keys to instruments table
-- Create initial risk_rules with default values
```

**Migration 002: Add Volatility Tracking**
```sql
ALTER TABLE risk_metrics_history 
ADD COLUMN current_volatility REAL;

ALTER TABLE risk_metrics_history 
ADD COLUMN volatility_adjusted BOOLEAN;
```

**Migration 003: Liquidity Risk Support**
```sql
CREATE TABLE liquidity_metrics (
    id BIGSERIAL PRIMARY KEY,
    timestamp BIGINT NOT NULL,
    instrument TEXT NOT NULL,
    spread REAL,
    market_depth REAL,
    avg_impact_cost REAL,
    ts_ms BIGINT NOT NULL,
    UNIQUE(ts_ms)
);
```

**Migration 004: Audit Compliance**
```sql
CREATE TABLE risk_audit_log (
    id TEXT PRIMARY KEY,
    check_type TEXT NOT NULL,
    result_hash TEXT NOT NULL,  -- SHA256 of input for reproducibility
    config_hash TEXT,  -- Hash of active rules
    ts_ms BIGINT NOT NULL
);
```

---

## 5. Test Strategy

### 5.1 Unit Testing (Coverage: >95%)

```rust
// 5.1.1 Core Logic Tests
#[test]
fn test_price_guard_rejects_high_price() {
    let guard = PriceGuard::new(100.0, 0.05); // 5% deviation
    let order = OrderIntent { limit_price: 120.0, ..Default::default() };
    assert!(!guard.check(&order).allowed);
}

#[test]
fn test_volatility_adjuster_half_life() {
    let mut adjuster = VolatilityAdjuster::new(20.minutes());
    adjuster.update_params(0.50);
    
    // After 20 min, volatility should halve
    let factor = adjuster.adjust_limit(0.50, 1.0);
    assert!((factor - 0.5).abs() < 0.001);
}

#[test]
fn test_var_calculation_methods() {
    // Test all three VaR methods with same portfolio
    let parametric = VaRCalculator::parametric(0.95);
    let historical = VaRCalculator::historical(1000, 0.95);
    let monte_carlo = VaRCalculator::monte_carlo(10000, 0.95);
    
    let v1 = parametric.calculate(&portfolio);
    let v2 = historical.calculate(&portfolio);
    let v3 = monte_carlo.calculate(&portfolio);
    
    // All should be within 5% of each other
    assert!((v1 - v2).abs() / v1 < 0.05);
}

// 5.1.2 Edge Cases
#[test]
fn test_zero_position_risk() {
    // Empty portfolio should return zero VaR
    let risk = PortfolioRisk::new();
    let result = risk.check(&PortfolioState::empty());
    assert_eq!(result.total_var, 0.0);
}

#[test]
fn test_divergent_correlation_matrix() {
    // Handle NaN/Inf in correlation calculations
    let matrix = CorrelationMatrix::empty();
    let _ = matrix.get_sector_correlation("A", "A"); // Should be 1.0
}
```

### 5.2 Integration Testing

```rust
// 5.2.1 End-to-End Pipeline
#[tokio::test]
async fn test_live_execution_mode() {
    // Setup: Mock price feeds
    let price_feed = MockPriceFeed::new();
    
    // Execute: Real-time risk check
    let risk = PortfolioRisk::new(RiskConfig::live());
    let order = create_test_order();
    
    let result = risk.check(&order, &price_feed);
    assert!(result.allowed);
}

// 5.2.2 Backtest Reproducibility
#[test]
fn test_backtest_deterministic() {
    // Same seed should produce identical results
    const SEED: u64 = 42;
    
    let backtest1 = run_backtest(&strategy, &data, SEED);
    let backtest2 = run_backtest(&strategy, &data, SEED);
    
    assert_eq!(backtest1.risk_metrics, backtest2.risk_metrics);
}

// 5.2.3 Database Integration
#[test]
fn test_event_persistence() {
    // Insert risk event
    let event = RiskEvent {
        event_type: "order_check",
        decision: "DENY",
        ..Default::default()
    };
    
    // Verify persistence and retrieval
    let stored = db.insert(&event);
    let retrieved = db.get_by_id(&stored.id);
    assert_eq!(retrieved.decision, "DENY");
}
```

### 5.3 Performance Testing

| Metric | Target | Test Method |
|--------|--------|-------------|
| **Order Check Latency** | < 100μs | Benchmark suite |
| **VaR Calculation** | < 5ms | Stress test with 1000 instruments |
| **Correlation Update** | < 1ms | Rolling window of 1000 data points |
| **Monte Carlo** | < 100ms | 10,000 iterations, 50 instruments |
| **Full Pipeline** | < 1ms | Order → Position → Portfolio |

**Benchmark Setup**:
```rust
#[bench]
fn bench_order_risk_pipeline(bencher: &mut Bencher) {
    let risk = PortfolioRisk::new();
    let order = OrderIntent::default();
    
    bencher.iter(|| {
        let _ = risk.check(&order);
    });
}
```

### 5.4 Chaos Testing

- **Database failures**: Test with connection pool exhaustion
- **Network partitions**: Simulate price feed latency spikes
- **Clock skew**: Test with NTP desynchronization
- **Memory pressure**: Run with 80% RAM utilization

---

## 6. API Contracts

### 6.1 gRPC Protobuf Definitions

```protobuf
// risk.proto
syntax = "proto3";

package risk;

message OrderIntent {
  string id = 1;
  string instrument = 2;
  string symbol = 3;
  string venue = 4;
  int64 side = 5;  // 1=BUY, -1=SELL
  double quantity = 6;
  double limit_price = 7;
  double notional_value = 8;
  map<string, string> metadata = 9;
}

message RiskResult {
  bool allowed = 1;
  double risk_score = 2;  // 0-100
  repeated string reasons = 3;
  OrderIntent adjusted_order = 4;  // Soft rejection with modification
  map<string, double> metrics = 5;
}

service RiskEngine {
  // Synchronous checks
  rpc CheckOrder(OrderIntent) returns (RiskResult);
  rpc CheckPosition(PositionCheckRequest) returns (RiskResult);
  rpc CheckPortfolio(PortfolioCheckRequest) returns (PortfolioRiskResult);
  
  // Asynchronous analysis
  rpc CalculateVaR(VaRRequest) returns (VaRResponse);
  rpc RunStressTest(StressTestRequest) returns (StressTestResponse);
  
  // Configuration
  rpc UpdateRules(RuleUpdate) returns (RuleUpdateResponse);
  rpc GetRules(RuleQuery) returns (RuleResponse);
}

message PortfolioRiskResult {
  bool allowed = 1;
  double total_var_95 = 2;
  double total_cvar_95 = 3;
  double total_exposure = 4;
  map<string, double> sector_exposure = 5;
  double max_correlation = 6;
}

message StressTestRequest {
  repeated StressScenario scenarios = 1;
  string execution_mode = 2;  // LIVE, PAPER, BACKTEST
  bool include_baseline = 3;
}

message StressScenario {
  string type = 1;
  map<string, double> parameters = 2;
}
```

### 6.2 REST API (OpenAPI/Swagger)

```yaml
# /api/v1/rules
POST /api/v1/rules
Content-Type: application/json

{
  "rules": [
    {
      "id": "limit-001",
      "rule_type": "position",
      "condition": {
        "field": "notional_value",
        "operator": "GT",
        "threshold": 100000.0
      },
      "action": "DENY",
      "priority": 10
    }
  ]
}

# /api/v1/metrics
GET /api/v1/metrics/portfolio?from=2024-04-01&to=2024-04-04

# /api/v1/stress-test
POST /api/v1/stress-test
{
  "scenarios": [
    {"type": "MARKET_CRASH", "parameters": {"drop_percent": 20.0}}
  ],
  "execution_mode": "LIVE"
}
```

### 6.3 WebSocket Streaming

```json
// Connection: wss://risk-system.internal/stream
{
  "type": "RISK_METRIC_UPDATE",
  "payload": {
    "instrument": "BTC-USD",
    "var_95": 15000.50,
    "exposure": 250000.00,
    "timestamp": 1712208000000
  }
}

{
  "type": "ALERT",
  "payload": {
    "severity": "CRITICAL",
    "message": "VaR exceeded 95% threshold",
    "metric": "var_95",
    "current": 250000.00,
    "threshold": 200000.00
  }
}
```

---

## 7. Configuration Schema

### 7.1 YAML Configuration

```yaml
# risk_config.yaml
execution_mode: LIVE  # LIVE, PAPER, BACKTEST

# 7.1.1 Order Layer
order_risk:
  price_guard:
    max_deviation_percent: 5.0
    max_slippage_bps: 10.0
    price_bands:
      upper: 1.05
      lower: 0.95
  
  quantity_limits:
    max_qty_per_order: 1000
    max_notional_per_order: 1000000.0
    max_notional_per_day: 10000000.0
    
  volatility_adjustment:
    enabled: true
    half_life_minutes: 20
    target_volatility_percent: 10.0
    adjustment_factor: 0.1  # 10% of deviation
    
    # Strategies
    strategy: EWMA  # EWMA, HISTORICAL, HYBRID
    historical_window_days: 30

# 7.1.2 Position Layer
position_risk:
  limits:
    max_position_percent: 10.0  # 10% of portfolio
    max_concentration: 5.0      # Top 5 positions < 50%
    
  pnl_limits:
    daily_limit_percent: 5.0
    total_limit_percent: 15.0
    reset_at_mkt_close: true
    
  stop_loss:
    hard_stop_percent: 2.0
    trailing_stop_percent: 1.5
    trailing_lookback_minutes: 30
    time_based_stop_hours: 24

# 7.1.3 Portfolio Layer
portfolio_risk:
  var:
    confidence_level: 95.0  # 95% VaR
    method: HISTORICAL  # HISTORICAL, PARAMETRIC, MONTE_CARLO
    window_size: 252  # 1 year
    
  correlation:
    window_size_days: 21
    min_correlation_threshold: 0.7
    max_sector_correlation: 0.8
    
  stress_testing:
    enabled: true
    default_scenarios:
      - MARKET_CRASH: 20.0
      - LIQUIDITY_DRYUP: 3.0
      - CORRELATION_SPIKE: 0.9
    
    monte_carlo:
      iterations: 10000
      seed: 42  # For reproducibility in backtest

# 7.1.4 Liquidity Risk
liquidity_risk:
  enabled: true
  impact_cost_threshold: 0.02  # 2% impact = reject
  market_depth_window: 100  # Last 100 ticks

# 7.1.5 Alert System
alerts:
  enabled: true
  deduplication_window_seconds: 300
  rate_limit_per_minute: 100
  
  channels:
    - type: SLACK
      webhook: ${SLACK_WEBHOOK}
      severity_filter: [WARNING, CRITICAL]
    - type: EMAIL
      recipients: risk-team@company.com
      severity_filter: [CRITICAL]
    - type: PAGERDUTY
      service_key: ${PAGERDUTY_KEY}
      severity_filter: [EMERGENCY]

# 7.1.6 Execution Modes
modes:
  live:
    price_source: REALTIME  # WebSocket price feed
    latency_budget_ms: 10
    
  paper:
    price_source: SIMULATED
    simulate_latencies_ms: 5
    
  backtest:
    price_source: HISTORICAL
    deterministic_seed: 42
    replay_speed: 1x  # 1x, 10x, 100x
```

### 7.2 JSON Runtime Configuration

```json
{
  "runtime": {
    "mode": "LIVE",
    "debug": false,
    "metrics_interval_ms": 1000
  },
  "dynamic": {
    "volatility": {
      "current": 0.18,
      "target": 0.10,
      "factor": 0.65
    },
    "active_rules": 47,
    "last_update_ms": 1712208000000
  }
}
```

---

## 8. Rollback Plans

### 8.1 Phase 1 Rollback
**Scenario**: Dynamic adjustment algorithm causes unexpected limit tightening

**Immediate Actions**:
1. Set `dynamic_adjustment.enabled: false` in config (instant)
2. Revert to static limits via config hot-reload
3. Database query: `UPDATE risk_rules SET is_enabled = false WHERE rule_type = 'position'`

**Verification**:
- Check `risk_events` table for continued ALLOW decisions
- Verify latency remains < 100μs

**Recovery Time**: < 2 minutes

### 8.2 Phase 2 Rollback
**Scenario**: VaR calculation causes OOM or slow response

**Immediate Actions**:
1. Reduce Monte Carlo iterations: `ALTER TABLE risk_metrics_history ADD COLUMN var_method VARCHAR` (track method)
2. Switch to HISTORICAL method via config
3. Kill and restart with reduced matrix size: `MAX_INSTRUMENTS=100`

**Verification**:
- Memory usage < 2GB
- Response time < 500ms

**Recovery Time**: < 5 minutes

### 8.3 Phase 3 Rollback
**Scenario**: Alert system floods channels

**Immediate Actions**:
1. Set `alerts.rate_limit_per_minute: 1` (blocks new alerts)
2. Enable deduplication: `alerts.deduplication_window_seconds: 60`
3. Pause critical channels: `alerts.channels[0].enabled: false`

**Verification**:
- Channel traffic < 10/sec
- No message loss (check `risk_alerts` table)

**Recovery Time**: < 1 minute

### 8.4 Phase 4 Rollback
**Scenario**: Production execution mode crashes

**Immediate Actions**:
1. Switch mode: `execution_mode: PAPER` (safe fallback)
2. Database checkpoint: `CHECKPOINT` to ensure data consistency
3. Revert to previous working config via config management

**Verification**:
- System stable
- No data corruption
- Risk checks still functional

**Recovery Time**: < 30 seconds

---

## 9. Deployment & Operations

### 9.1 Infrastructure Requirements

| Component | Specification | Quantity |
|-----------|---------------|----------|
| **Compute** | 4 vCPU, 8GB RAM | 3 instances (active-active-standby) |
| **Database** | PostgreSQL 15+ | 1 primary, 1 replica |
| **Cache** | Redis 7+ | 1 cluster (3 nodes) |
| **Storage** | TimescaleDB hypertable | 1TB retention |
| **Monitoring** | Prometheus + Grafana | 1 instance |

### 9.2 Health Checks

```yaml
# Health endpoints
- /health/live: Checks if process is running
- /health/ready: Checks if DB connection pool is healthy
- /health/risk: Checks if risk engine is processing
- /health/metrics: Checks if metrics are being collected
```

### 9.3 Runbooks

**Runbook: High Latency**
1. Check `current_volatility` metric
2. If > 5x target, verify volatility_adjustment config
3. Restart with `half_life_minutes: 60` (slower adjustment)

**Runbook: False Positives**
1. Review `risk_rules` with `priority > 50`
2. Check `context_json` in `risk_events` for edge cases
3. Temporarily increase `price_guard.max_deviation_percent`

---

## 10. Risk & Compliance

### 10.1 Model Validation
- **Backtesting**: 1-year historical data, 95% VaR must be < 3% daily
- **Sensitivity Analysis**: Test with ±20% parameter changes
- **Stress Testing**: Monthly full portfolio stress test

### 10.2 Audit Requirements
- All risk decisions logged with `result_hash` for reproducibility
- Config changes tracked with `effective_from`/`effective_until`
- Immutable audit trail in `risk_audit_log` table

### 10.3 Change Management
- Rule changes require approval workflow
- Emergency changes (via `is_enabled` toggle) documented within 1 hour
- Weekly rule review meeting

---

## 11. Success Metrics

| Metric | Target | Measurement |
|--------|--------|-------------|
| **Risk Check Latency** | P99 < 100μs | Distributed tracing |
| **False Positive Rate** | < 0.1% | `DENY` events / total orders |
| **VaR Accuracy** | ±5% | Backtested vs. realized losses |
| **Uptime** | 99.99% | Health check endpoint |
| **Config Reload Time** | < 10s | Hot-reload testing |
| **Recovery Time** | < 30s | Chaos engineering tests |

---

## 12. Appendix

### A. Glossary
- **VaR**: Value at Risk - Maximum expected loss at confidence level
- **CVaR**: Conditional VaR - Expected loss beyond VaR threshold
- **EWMA**: Exponentially Weighted Moving Average
- **Half-life**: Time for volatility to decay by 50%

### B. References
- Basel III Market Risk Framework
- J.P. Morgan RiskMetrics (1996)
- Hull, "Options, Futures, and Other Derivatives" (VaR chapters)

### C. Version History
| Version | Date | Changes |
|---------|------|---------|
| 1.0 | 2024-04-04 | Initial implementation plan |

---

**Approval Required**: Architecture Review Board (ARB)  
**Next Review**: 2024-04-11 (Phase 1 completion)
