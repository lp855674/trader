# Strategy System Implementation Plan

**Version**: 1.0.0  
**Priority**: P0  
**Estimated Duration**: 8-10 weeks  
**Dependencies**: None (standalone project)

---

## 1. Implementation Phases

### Phase 1: Core Framework (Weeks 1-2)
**Goal**: Establish foundational traits, types, and basic execution flow

| Task | Duration | Dependencies | Deliverables |
|------|----------|--------------|--------------|
| 1.1 Core Trait Definitions | 2 days | None | `src/core/trait.rs` |
| 1.2 Data Model & Types | 2 days | 1.1 | `src/core/types.rs` |
| 1.3 Strategy Context Implementation | 3 days | 1.1, 1.2 | `src/core/context.rs` |
| 1.4 Input Source Traits & Mocks | 3 days | 1.2 | `src/data/sources.rs` |
| 1.5 Basic Scheduler | 2 days | 1.4 | `src/scheduler/mod.rs` |
| 1.6 EventBus | 2 days | 1.5 | `src/event/mod.rs` |
| 1.7 Basic K-line Source | 3 days | 1.4 | `src/data/kline.rs` |
| 1.8 Integration Tests | 2 days | 1.1-1.8 | `tests/integration/*.rs` |

**Rollback Plan**: If Phase 1 fails, revert to last stable commit and refactor trait definitions to use associated types instead of generic parameters for better flexibility.

---

### Phase 2: Advanced Strategy Features (Weeks 3-4)
**Goal**: Implement strategy combinators and advanced evaluation patterns

| Task | Duration | Dependencies | Deliverables |
|------|----------|--------------|--------------|
| 2.1 Strategy Combinator Traits | 3 days | 1.1 | `src/core/combinator.rs` |
| 2.2 Weighted Average & Round Robin | 2 days | 2.1 | `src/core/combinators.rs` |
| 2.3 Conditional & Pipeline Strategies | 2 days | 2.1 | `src/core/combinators.rs` |
| 2.4 Strategy Registry & Factory | 2 days | 2.1 | `src/core/registry.rs` |
| 2.5 Hot Reload Mechanism | 4 days | 2.4 | `src/core/hot_reload.rs` |
| 2.6 Strategy Logger | 2 days | 1.7 | `src/core/logger.rs` |
| 2.7 Performance Metrics | 2 days | 2.6 | `src/core/metrics.rs` |
| 2.8 Benchmark Suite | 2 days | 2.1-2.7 | `benches/*.rs` |

**Rollback Plan**: If hot reload causes race conditions, implement a simpler config hot-swap mechanism with full process restart for strategy instances.

---

### Phase 3: Backtest Engine (Weeks 5-6)
**Goal**: Build deterministic backtesting infrastructure with multi-granularity support

| Task | Duration | Dependencies | Deliverables |
|------|----------|--------------|--------------|
| 3.1 Backtest Engine Core | 5 days | 1.5, 2.4 | `src/backtest/engine.rs` |
| 3.2 Execution Simulator | 3 days | 3.1 | `src/backtest/executor.rs` |
| 3.3 Slippage & Commission Models | 2 days | 3.2 | `src/backtest/models.rs` |
| 3.4 Arbitrary Granularity Support | 3 days | 3.1 | `src/backtest/granularity.rs` |
| 3.5 Performance Calculator | 2 days | 3.2 | `src/backtest/performance.rs` |
| 3.6 Result Storage & Export | 2 days | 3.3 | `src/backtest/storage.rs` |
| 3.7 Deterministic Execution Tests | 2 days | 3.1-3.6 | `tests/backtest/*.rs` |
| 3.8 Multi-instrument Backtest | 3 days | 3.1 | `src/backtest/portfolio.rs` |

**Rollback Plan**: If backtest performance degrades, switch from Vec-based storage to Polars DataFrame for O(1) lookups.

---

### Phase 4: Optimization & Analysis (Weeks 7-8)
**Goal**: Implement parameter optimization and risk analysis tools

| Task | Duration | Dependencies | Deliverables |
|------|----------|--------------|--------------|
| 4.1 Parameter Grid System | 3 days | 3.5 | `src/optimizer/grid.rs` |
| 4.2 Bayesian Optimization | 5 days | 4.1 | `src/optimizer/bayesian.rs` |
| 4.3 Monte Carlo Simulator | 4 days | 3.1 | `src/analysis/monte_carlo.rs` |
| 4.4 Sensitivity Analyzer | 3 days | 4.1 | `src/analysis/sensitivity.rs` |
| 4.5 Risk Metrics Calculator | 3 days | 3.5 | `src/analysis/risk.rs` |
| 4.6 Walk-Forward Analysis | 2 days | 4.1, 4.2 | `src/analysis/walk_forward.rs` |
| 4.7 Optimization UI/API | 2 days | 4.1-4.6 | `src/api/optimizer.rs` |
| 4.8 Cross-validation | 2 days | 4.1 | `src/analysis/cv.rs` |

**Rollback Plan**: If Bayesian optimization is too slow, fall back to grid search with parallel execution using Rayon.

---

### Phase 5: Paper Trading & Live Integration (Weeks 9-10)
**Goal**: Connect strategies to real-world execution

| Task | Duration | Dependencies | Deliverables |
|------|----------|--------------|--------------|
| 5.1 Paper Adapter Core | 3 days | 1.5 | `src/trading/paper.rs` |
| 5.2 Order Intent Processor | 2 days | 5.1 | `src/trading/intent.rs` |
| 5.3 Position Manager | 3 days | 5.1 | `src/trading/position.rs` |
| 5.4 Database Persistence | 4 days | 1.7 | `src/persistence/mod.rs` |
| 5.5 gRPC Server | 3 days | 2.4 | `src/api/grpc.rs` |
| 5.6 HTTP REST API | 2 days | 5.5 | `src/api/http.rs` |
| 5.7 Configuration Schema | 2 days | 1.2 | `src/config/schema.rs` |
| 5.8 System Integration | 3 days | All | `src/main.rs` |

**Rollback Plan**: If paper trading causes data inconsistencies, implement snapshot-based state recovery with WAL (Write-Ahead Logging).

---

## 2. Technical Architecture

### 2.1 Core Design Decisions

| Decision | Rationale | Trade-offs |
|----------|-----------|------------|
| **Pure Function Strategies** | Ensures deterministic behavior for backtesting | Requires explicit Context passing, less convenient for stateful strategies |
| **Arc<dyn Trait> for Strategies** | Enables runtime strategy swapping and hot reload | Heap allocation overhead, potential memory leaks if not managed |
| **EventBus with broadcast::Sender** | Decouples data sources from strategies | Single producer model; consider channels for multi-producer |
| **LruCache in Context** | Reduces redundant calculations | Cache invalidation complexity under hot reload |
| **Polars for DataFrames** | Fast, memory-efficient, multi-threaded | Steep learning curve, external dependency |
| **Hybrid Scheduler** | Supports both periodic and event-driven execution | Complexity in synchronization between timers and events |

### 2.2 Architecture Diagram

```
┌─────────────────────────────────────────────────────────────┐
│                      Application Layer                       │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐          │
│  │ Optimizer   │  │ Monte Carlo │  │ Sensitivity │          │
│  │ Interface   │  │ Simulator   │  │ Analyzer    │          │
│  └─────────────┘  └─────────────┘  └─────────────┘          │
└─────────────────────────────────────────────────────────────┘
                              │
┌─────────────────────────────────────────────────────────────┐
│                     Strategy Layer                           │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐          │
│  │ Combinator │  │ Registry    │  │ Hot Reload  │          │
│  │ Engine      │  │ Factory     │  │ Manager     │          │
│  └─────────────┘  └─────────────┘  └─────────────┘          │
└─────────────────────────────────────────────────────────────┘
                              │
┌─────────────────────────────────────────────────────────────┐
│                      Execution Layer                         │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐          │
│  │ Scheduler  │  │ EventBus    │  │ Logger      │          │
│  │ (Hybrid)    │  │ (Event Bus) │  │ (Tracing)   │          │
│  └─────────────┘  └─────────────┘  └─────────────┘          │
└─────────────────────────────────────────────────────────────┘
                              │
┌─────────────────────────────────────────────────────────────┐
│                     Data Layer                               │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐          │
│  │ Kline       │  │ Tick        │  │ OrderBook   │          │
│  │ Source      │  │ Source      │  │ Source      │          │
│  └─────────────┘  └─────────────┘  └─────────────┘          │
│  ┌─────────────┐  ┌─────────────┐                          │
│  │ Historical  │  │ Paper       │                          │
│  │ Data        │  │ Adapter     │                          │
│  └─────────────┘  └─────────────┘                          │
└─────────────────────────────────────────────────────────────┘
```

### 2.3 Key Implementation Details

#### 2.3.1 Strategy Context
```rust
pub struct StrategyContext {
    pub instrument: InstrumentId,
    pub ts_ms: i64,
    pub cache: LruCache<CacheKey, CacheValue>,
    pub memory: MemoryBuffer,
    pub params: HashMap<String, Value>,
    pub sources: Arc<DataSourceBundle>,
    pub logger: Arc<StrategyLogger>,
}
```

#### 2.3.2 Hybrid Scheduler
```rust
pub struct HybridScheduler {
    pub periodic: PeriodicScheduler,
    pub event_driven: Vec<EventSubscriber>,
    pub event_bus: Arc<EventBus>,
    pub last_tick: AtomicU64,
}

impl HybridScheduler {
    pub fn run(&self) -> JoinHandle<()> {
        // Combined loop: timer ticks + event processing
    }
}
```

#### 2.3.3 Backtest Engine Granularity
```rust
pub enum Granularity {
    Tick,      // ~1ms resolution
    Kline(i64), // 1m, 5m, 1h, etc.
    TickRate(f64), // Custom tick rate
}

pub struct BacktestEngine {
    pub strategy: Arc<dyn Strategy>,
    pub data: Arc<dyn HistoricalData>,
    pub config: BacktestConfig,
    pub current_ts: AtomicI64,
    pub tick_rate: u64,
}
```

---

## 3. Database Schema

### 3.1 Migration Files

#### 001_initial_schema.sql
```sql
-- Enable extensions
CREATE EXTENSION IF NOT EXISTS "uuid-ossp";
CREATE EXTENSION IF NOT EXISTS "pg_stat_statements";

-- Core tables
CREATE TABLE instruments (
    id TEXT PRIMARY KEY,
    symbol TEXT NOT NULL UNIQUE,
    exchange TEXT NOT NULL,
    base_currency TEXT NOT NULL,
    quote_currency TEXT NOT NULL,
    created_at TIMESTAMP DEFAULT NOW(),
    updated_at TIMESTAMP DEFAULT NOW()
);

CREATE TABLE strategy_configs (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    module_path TEXT NOT NULL,
    params JSONB NOT NULL,
    metadata JSONB DEFAULT '{}',
    created_at TIMESTAMP DEFAULT NOW(),
    updated_at TIMESTAMP DEFAULT NOW(),
    version INTEGER DEFAULT 1 CHECK (version > 0)
);

CREATE INDEX idx_strategy_configs_params ON strategy_configs USING GIN(params);

-- Backtest results
CREATE TABLE backtest_runs (
    id TEXT PRIMARY KEY,
    strategy_id TEXT NOT NULL REFERENCES strategy_configs(id),
    instrument_id TEXT NOT NULL REFERENCES instruments(id),
    start_ts INTEGER NOT NULL,
    end_ts INTEGER NOT NULL,
    granularity TEXT NOT NULL,
    initial_capital REAL NOT NULL,
    commission_rate REAL NOT NULL DEFAULT 0.0,
    slippage REAL NOT NULL DEFAULT 0.0,
    random_seed INTEGER,
    status TEXT CHECK (status IN ('completed', 'failed', 'running')),
    result_json JSONB,
    error_message TEXT,
    created_at TIMESTAMP DEFAULT NOW(),
    completed_at TIMESTAMP
);

CREATE INDEX idx_backtest_runs_strategy ON backtest_runs(strategy_id);
CREATE INDEX idx_backtest_runs_instrument ON backtest_runs(instrument_id);
CREATE INDEX idx_backtest_runs_date ON backtest_runs(start_ts, end_ts);

-- Strategy logs
CREATE TABLE strategy_logs (
    id BIGSERIAL PRIMARY KEY,
    strategy_id TEXT NOT NULL REFERENCES strategy_configs(id),
    event_type TEXT NOT NULL CHECK (event_type IN (
        'signal_generated',
        'signal_cancelled',
        'order_intent',
        'execution',
        'error',
        'config_changed',
        'reload',
        'performance'
    )),
    context JSONB NOT NULL,
    ts_ms INTEGER NOT NULL,
    created_at TIMESTAMP DEFAULT NOW()
);

CREATE INDEX idx_strategy_logs_strategy ON strategy_logs(strategy_id);
CREATE INDEX idx_strategy_logs_ts ON strategy_logs(ts_ms);
CREATE INDEX idx_strategy_logs_event ON strategy_logs(event_type, ts_ms);

-- Performance metrics
CREATE TABLE performance_metrics (
    id TEXT PRIMARY KEY,
    backtest_id TEXT NOT NULL REFERENCES backtest_runs(id),
    metric_name TEXT NOT NULL,
    metric_value REAL NOT NULL,
    timestamp TIMESTAMP DEFAULT NOW()
);

CREATE INDEX idx_performance_metrics_backtest ON performance_metrics(backtest_id);

-- Paper trading state
CREATE TABLE paper_positions (
    id TEXT PRIMARY KEY,
    strategy_id TEXT NOT NULL REFERENCES strategy_configs(id),
    instrument_id TEXT NOT NULL REFERENCES instruments(id),
    side TEXT NOT NULL CHECK (side IN ('long', 'short')),
    quantity REAL NOT NULL,
    avg_price REAL NOT NULL,
    unrealized_pnl REAL NOT NULL,
    last_update_ts INTEGER NOT NULL,
    created_at TIMESTAMP DEFAULT NOW()
);

CREATE INDEX idx_paper_positions_strategy ON paper_positions(strategy_id);

-- Audit trail
CREATE TABLE audit_log (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT,
    action TEXT NOT NULL,
    target_type TEXT,
    target_id TEXT,
    changes JSONB,
    ip_address TEXT,
    created_at TIMESTAMP DEFAULT NOW()
);
```

#### 002_optimization_tables.sql
```sql
-- Optimization results
CREATE TABLE optimization_runs (
    id TEXT PRIMARY KEY,
    strategy_id TEXT NOT NULL REFERENCES strategy_configs(id),
    optimization_type TEXT NOT NULL CHECK (optimization_type IN ('grid_search', 'bayesian', 'walk_forward')),
    param_grid JSONB NOT NULL,
    objective_function TEXT NOT NULL,
    validation_split INTEGER NOT NULL,
    status TEXT CHECK (status IN ('completed', 'failed', 'running')),
    best_params JSONB,
    best_score REAL,
    results JSONB,
    created_at TIMESTAMP DEFAULT NOW(),
    completed_at TIMESTAMP
);

CREATE INDEX idx_optimization_runs_strategy ON optimization_runs(strategy_id);

-- Parameter sweep results
CREATE TABLE parameter_sweep (
    id TEXT PRIMARY KEY,
    optimization_id TEXT NOT NULL REFERENCES optimization_runs(id),
    param_set JSONB NOT NULL,
    score REAL NOT NULL,
    backtest_id TEXT REFERENCES backtest_runs(id),
    created_at TIMESTAMP DEFAULT NOW()
);

CREATE INDEX idx_parameter_sweep_optimization ON parameter_sweep(optimization_id);
CREATE INDEX idx_parameter_sweep_score ON parameter_sweep(score DESC);
```

#### 003_analysis_tables.sql
```sql
-- Monte Carlo results
CREATE TABLE monte_carlo_simulations (
    id TEXT PRIMARY KEY,
    strategy_id TEXT NOT NULL REFERENCES strategy_configs(id),
    backtest_id TEXT NOT NULL REFERENCES backtest_runs(id),
    iterations INTEGER NOT NULL,
    random_seed INTEGER NOT NULL,
    results JSONB NOT NULL,
    confidence_interval_low REAL,
    confidence_interval_high REAL,
    worst_case REAL,
    best_case REAL,
    created_at TIMESTAMP DEFAULT NOW()
);

CREATE INDEX idx_mc_simulations_strategy ON monte_carlo_simulations(strategy_id);

-- Sensitivity analysis
CREATE TABLE sensitivity_analysis (
    id TEXT PRIMARY KEY,
    strategy_id TEXT NOT NULL REFERENCES strategy_configs(id),
    base_backtest_id TEXT NOT NULL REFERENCES backtest_runs(id),
    params_varying JSONB NOT NULL,
    variation_ranges JSONB NOT NULL,
    results JSONB NOT NULL,
    correlation_matrix JSONB,
    created_at TIMESTAMP DEFAULT NOW()
);

-- Data quality
CREATE TABLE data_quality_checks (
    id BIGSERIAL PRIMARY KEY,
    instrument_id TEXT NOT NULL REFERENCES instruments(id),
    data_type TEXT NOT NULL CHECK (data_type IN ('kline', 'tick', 'orderbook')),
    from_ts INTEGER NOT NULL,
    to_ts INTEGER NOT NULL,
    gaps INTEGER DEFAULT 0,
    duplicates INTEGER DEFAULT 0,
    out_of_order INTEGER DEFAULT 0,
    last_checked_at TIMESTAMP DEFAULT NOW()
);
```

---

## 4. Test Strategy

### 4.1 Unit Tests

```rust
// File: tests/unit/core/trait.rs
#[cfg(test)]
mod strategy_pure_function {
    use crate::core::{Strategy, StrategyContext, Signal};
    
    #[test]
    fn test_deterministic_output() {
        let strategy = MovingAverageCrossover::new(10, 30);
        
        let ctx1 = create_context(1000, 1.0);
        let sig1 = strategy.evaluate(&ctx1);
        
        let ctx2 = create_context(1000, 1.0);
        let sig2 = strategy.evaluate(&ctx2);
        
        assert_eq!(sig1, sig2);
    }
    
    #[test]
    fn test_no_mutability() {
        let strategy = MovingAverageCrossover::new(10, 30);
        let ctx = create_context(1000, 1.0);
        
        // Should not require &mut self or &mut ctx
        let _sig = strategy.evaluate(&ctx);
    }
}

// File: tests/unit/backtest/deterministic.rs
#[test]
fn test_backtest_deterministic() {
    let engine1 = create_engine();
    let result1 = engine1.run();
    
    let engine2 = create_engine();
    let result2 = engine2.run();
    
    assert_eq!(result1.equity_curve, result2.equity_curve);
    assert_eq!(result1.trades.len(), result2.trades.len());
}
```

### 4.2 Integration Tests

```rust
// File: tests/integration/hybrid_scheduler.rs
#[test]
fn test_hybrid_scheduler_timer_event() {
    let (event_bus, _) = EventBus::new();
    let (tx, rx) = mpsc::channel();
    
    let scheduler = HybridScheduler::new(
        PeriodicScheduler::new(100), // 100ms
        vec![EventSubscriber::new(rx)],
        Arc::new(event_bus),
    );
    
    let handle = scheduler.run();
    sleep(Duration::from_millis(300));
    handle.abort();
    
    // Should have received timer ticks and events
}

// File: tests/integration/hot_reload.rs
#[test]
fn test_strategy_hot_reload() {
    let strategy_manager = StrategyManager::new(
        PathBuf::from("tests/fixtures/strategies"),
        1000,
    );
    
    strategy_manager.load().unwrap();
    let initial = strategy_manager.get("test_strategy").unwrap();
    
    // Modify strategy config
    sleep(Duration::from_millis(500));
    strategy_manager.reload().unwrap();
    
    let updated = strategy_manager.get("test_strategy").unwrap();
    // Verify new config loaded
}
```

### 4.3 Performance Benchmarks

```rust
// File: benches/strategy_evaluation.rs
use criterion::{criterion_group, criterion_main, Criterion};
use strategy_system::core::{Strategy, StrategyContext};

fn bench_strategy_evaluate(c: &mut Criterion) {
    let strategy = MovingAverageCrossover::new(10, 30);
    let ctx = create_context_with_data(10000); // 10k bars
    
    c.bench_function("evaluate_10k_bars", |b| {
        b.iter(|| strategy.evaluate(&ctx))
    });
}

fn bench_strategy_with_cache(c: &mut Criterion) {
    let strategy = MovingAverageCrossover::new(10, 30);
    let ctx = create_context_with_cache();
    
    c.bench_function("evaluate_with_cache", |b| {
        b.iter(|| {
            let _sig = strategy.evaluate(&ctx);
            ctx.cache.hit_rate
        })
    });
}

criterion_group!(benches, bench_strategy_evaluate, bench_strategy_with_cache);
criterion_main!(benches);
```

### 4.4 Property-Based Tests

```rust
// File: tests/properties/combinators.rs
use proptest::prelude::*;
use strategy_system::core::combinator::{WeightedAverage, Signal};

proptest! {
    #[test]
    fn weighted_average_commutativity(
        w1 in 0.0..1.0,
        w2 in 0.0..1.0,
        s1 in 0.0..100.0,
        s2 in 0.0..100.0,
    ) {
        let combinator = WeightedAverage::new(vec![w1, w2]);
        let signal1 = combinator.combine(s1, s2);
        let signal2 = combinator.combine(s2, s1);
        
        // Commutativity: w1*s1 + w2*s2 should equal w1*s2 + w2*s1
        prop_assert!(
            (signal1 - signal2).abs() < 0.0001,
            "Weighted average should be commutative"
        );
    }
}
```

---

## 5. API Contracts

### 5.1 gRPC Service Definition

```proto
// src/proto/strategy_system.proto
syntax = "proto3";

package strategy.system;

import "google/protobuf/timestamp.proto";

// Core types
message InstrumentId { string id = 1; }
message StrategyId { string id = 1; }
message Signal {
  StrategyId strategy_id = 1;
  InstrumentId instrument = 2;
  Side side = 3;
  double quantity = 4;
  double limit_price = 5;
  int64 timestamp_ms = 6;
  map<string, Value> params = 7;
}

message Value {
  oneof kind {
    double number_value = 1;
    string string_value = 2;
    bool bool_value = 3;
  }
}

enum Side { BUY = 0; SELL = 1; }

// Strategy Management
service StrategyService {
  // Load and register strategies
  rpc LoadStrategies(LoadStrategiesRequest) returns (LoadStrategiesResponse);
  rpc GetStrategy(StrategyId) returns (StrategyInfo);
  rpc ReloadStrategy(StrategyId) returns (ReloadResponse);
  
  // Signal generation
  rpc Evaluate(EvaluateRequest) returns (Signal);
  rpc EvaluateBatch(BatchEvaluateRequest) returns (BatchEvaluateResponse);
  
  // Performance monitoring
  rpc GetMetrics(MetricsRequest) returns (MetricsResponse);
  rpc LogSignal(LogSignalRequest) returns (Empty);
}

message LoadStrategiesRequest { string config_path = 1; }
message LoadStrategiesResponse { repeated StrategyId strategies = 2; }

message StrategyInfo {
  StrategyId id = 1;
  string name = 2;
  map<string, Value> params = 3;
  google.protobuf.Timestamp created_at = 4;
}

message EvaluateRequest {
  StrategyId strategy_id = 1;
  InstrumentId instrument = 2;
  int64 timestamp_ms = 3;
  KlineData kline = 4;
  TickData tick = 5;
}

message KlineData {
  InstrumentId instrument = 1;
  int64 open_ts_ms = 2;
  double open = 3;
  double high = 4;
  double low = 5;
  double close = 6;
  double volume = 7;
}

message TickData {
  InstrumentId instrument = 1;
  int64 ts_ms = 2;
  double bid_price = 3;
  double ask_price = 4;
  double last_price = 5;
  double volume = 6;
}

message BatchEvaluateRequest {
  repeated EvaluateRequest requests = 1;
}

message BatchEvaluateResponse {
  repeated Signal signals = 1;
}

// Optimizer Service
service OptimizerService {
  rpc RunOptimization(OptimizationRequest) returns (OptimizationResult);
  rpc GetOptimizationHistory(OptimizationHistoryRequest) returns (OptimizationHistoryResponse);
}

message OptimizationRequest {
  StrategyId strategy_id = 1;
  InstrumentId instrument = 2;
  OptimizationType type = 3;
  map<string, repeated Value> param_grid = 4;
  int32 validation_split = 5;
  string objective = 6; // "sharpe", "max_drawdown", "total_return"
}

enum OptimizationType { GRID_SEARCH = 0; BAYESIAN = 1; WALK_FORWARD = 2; }

message OptimizationResult {
  StrategyId strategy_id = 1;
  map<string, Value> best_params = 2;
  double best_score = 3;
  repeated ParameterResult results = 4;
}

message ParameterResult {
  map<string, Value> params = 1;
  double score = 2;
  BacktestSummary summary = 3;
}

message BacktestSummary {
  double total_return = 1;
  double sharpe_ratio = 2;
  double max_drawdown = 3;
  int32 trade_count = 4;
}

// Paper Trading Service
service PaperTradingService {
  rpc SimulateOrder(SimulateOrderRequest) returns (OrderResult);
  rpc GetPositions(PositionsRequest) returns (PositionsResponse);
  rpc GetPnL(PnLRequest) returns (PnLResponse);
}

message SimulateOrderRequest {
  StrategyId strategy_id = 1;
  InstrumentId instrument = 2;
  Side side = 3;
  double quantity = 4;
  double limit_price = 5;
}

message OrderResult {
  string order_id = 1;
  ExecutionStatus status = 2;
  double filled_quantity = 3;
  double avg_price = 4;
}

enum ExecutionStatus { PENDING = 0; FILLED = 1; PARTIALLY FILLED = 2; REJECTED = 3; }

message PositionsRequest { StrategyId strategy_id = 1; }
message PositionsResponse {
  repeated Position positions = 1;
  double total_pnl = 2;
}

message Position {
  InstrumentId instrument = 1;
  Side side = 2;
  double quantity = 3;
  double avg_price = 4;
  double unrealized_pnl = 5;
}
```

### 5.2 REST API Specification

```yaml
# src/api/http/spec.yaml
openapi: 3.0.3
info:
  title: Strategy System API
  version: 1.0.0
  description: REST API for strategy management and execution

servers:
  - url: http://localhost:8080/api/v1
    description: Development server

paths:
  /strategies:
    get:
      summary: List all loaded strategies
      operationId: listStrategies
      responses:
        '200':
          description: Successful response
          content:
            application/json:
              schema:
                type: object
                properties:
                  strategies:
                    type: array
                    items:
                      $ref: '#/components/schemas/StrategyInfo'
    
    post:
      summary: Load strategy from file
      operationId: loadStrategy
      requestBody:
        required: true
        content:
          application/json:
            schema:
              $ref: '#/components/schemas/LoadStrategyRequest'
      responses:
        '200':
          description: Strategy loaded successfully
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/StrategyInfo'

  /strategies/{id}/reload:
    post:
      summary: Hot reload strategy configuration
      operationId: reloadStrategy
      parameters:
        - name: id
          in: path
          required: true
          schema:
            type: string
      responses:
        '200':
          description: Strategy reloaded
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/ReloadResponse'

  /backtest:
    post:
      summary: Run backtest
      operationId: runBacktest
      requestBody:
        required: true
        content:
          application/json:
            schema:
              $ref: '#/components/schemas/BacktestRequest'
      responses:
        '202':
          description: Backtest started
        '200':
          description: Backtest completed
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/BacktestResult'

  /optimize:
    post:
      summary: Run parameter optimization
      operationId: runOptimization
      requestBody:
        required: true
        content:
          application/json:
            schema:
              $ref: '#/components/schemas/OptimizationRequest'
      responses:
        '200':
          description: Optimization completed

components:
  schemas:
    StrategyInfo:
      type: object
      properties:
        id:
          type: string
        name:
          type: string
        params:
          type: object
          additionalProperties: true
        created_at:
          type: string
          format: date-time
    
    LoadStrategyRequest:
      type: object
      required:
        - config_path
      properties:
        config_path:
          type: string
          format: uri
    
    BacktestRequest:
      type: object
      required:
        - strategy_id
        - instrument
        - start_ts
        - end_ts
      properties:
        strategy_id:
          type: string
        instrument:
          type: string
        start_ts:
          type: integer
          format: int64
        end_ts:
          type: integer
          format: int64
        initial_capital:
          type: number
        commission_rate:
          type: number
        granularity:
          type: string
          enum: [tick, 1m, 5m, 1h, 1d]
    
    BacktestResult:
      type: object
      properties:
        id:
          type: string
        strategy_id:
          type: string
        total_return:
          type: number
        sharpe_ratio:
          type: number
        max_drawdown:
          type: number
        trades:
          type: array
          items:
            $ref: '#/components/schemas/Trade'
        equity_curve:
          type: array
          items:
            type: object
            properties:
              timestamp:
                type: integer
              value:
                type: number
    
    Trade:
      type: object
      properties:
        order_id:
          type: string
        instrument:
          type: string
        side:
          type: string
          enum: [buy, sell]
        filled_qty:
          type: number
        avg_price:
          type: number
        timestamp:
          type: integer
          format: int64
        commission:
          type: number
```

---

## 6. Configuration Schema

### 6.1 YAML Configuration Schema

```yaml
# src/config/schema.yaml
strategy_system:
  version: "1.0"
  
  # Core settings
  core:
    log_level: "info"  # debug, info, warn, error
    metrics_enabled: true
    metrics_interval_ms: 60000
    
  # Data sources
  data_sources:
    kline:
      path: "./data/klines"
      formats: ["csv", "parquet"]
      compression: "zstd"
      max_cache_size: 1000000  # 1M bars
      
    tick:
      path: "./data/ticks"
      max_tick_rate_ms: 100    # 10Hz max
      
    orderbook:
      levels: 10
      refresh_interval_ms: 100
  
  # Scheduler
  scheduler:
    default_interval_ms: 1000
    hybrid:
      enabled: true
      event_buffer_size: 10000
  
  # Backtest settings
  backtest:
    default_commission_rate: 0.001  # 0.1%
    default_slippage_pips: 5
    tick_interval_ms: 100
    max_history_days: 365
  
  # Optimization
  optimization:
    default_grid_size: 10
    bayesian:
      initial_samples: 20
      epsilon: 0.1
    walk_forward:
      step_size_days: 30
      lookback_days: 90
  
  # Paper trading
  paper_trading:
    enabled: false
    simulate_slippage: true
    simulate_commission: true
  
  # Storage
  storage:
    database:
      type: "sqlite"  # sqlite, postgres
      path: "./data/strategy_system.db"
      max_connections: 25
    cache:
      type: "memory"  # memory, disk
      max_size_mb: 1024
  
  # Hot reload
  hot_reload:
    enabled: false
    interval_ms: 5000
    auto_backup: true
  
  # Monitoring
  monitoring:
    prometheus:
      enabled: true
      port: 9090
    logging:
      format: "json"
      file: "./logs/strategy_system.log"
      max_size_mb: 100
      backup_count: 5
```

### 6.2 JSON Strategy Configuration

```json
{
  "name": "MovingAverageCrossover",
  "version": "1.0",
  "module_path": "strategies::moving_average",
  "parameters": {
    "fast_period": 10,
    "slow_period": 30,
    "threshold": 2.0,
    "lookback_bars": 100
  },
  "metadata": {
    "author": "system",
    "description": "Classic moving average crossover strategy",
    "tags": ["trend", "momentum"]
  },
  "constraints": {
    "min_capital": 10000,
    "max_position_size": 0.1,
    "max_trades_per_day": 100
  }
}
```

### 6.3 Backtest Configuration

```json
{
  "strategy_id": "macd_12_26_9",
  "instrument": "BTC/USDT:BINANCE",
  "time_range": {
    "start": "2024-01-01T00:00:00Z",
    "end": "2024-12-31T23:59:59Z"
  },
  "granularity": "1m",
  "execution": {
    "initial_capital": 100000.0,
    "commission_rate": 0.001,
    "slippage_model": "fixed",
    "slippage_bps": 5,
    "min_tick_interval_ms": 100
  },
  "portfolio": {
    "max_position_size_pct": 0.2,
    "max_total_exposure_pct": 1.0,
    "diversification": {
      "max_instruments": 10,
      "rebalance_interval": "1d"
    }
  },
  "random_seed": 42
}
```

---

## 7. Rollback Plan

### Phase-Specific Rollback Procedures

#### Phase 1 Rollback
**Trigger**: Core traits have breaking API changes or memory safety issues

**Steps**:
1. Revert to last working commit: `git revert HEAD~5..HEAD`
2. Switch to associated types: `trait Strategy: Sized { type Context; }`
3. Simplify InputSource to single concrete implementation
4. Remove Arc wrappers, use owned values temporarily

**Verification**: Compile with `cargo build --release` and run basic scheduler test.

#### Phase 2 Rollback
**Trigger**: Hot reload causes data races or memory leaks

**Steps**:
1. Disable hot reload: `hot_reload.enabled = false` in config
2. Implement config hot-swap only: read new config, validate, then restart strategy instances
3. Add memory leak detection: `cargo leak --release`
4. Implement RAII guards for strategy instances

**Verification**: Run stress test with 100 concurrent strategy reloads.

#### Phase 3 Rollback
**Trigger**: Backtest performance degradation or non-determinism

**Steps**:
1. Switch to simpler Vec-based storage: remove Polars dependency
2. Implement sequential execution: `tokio::task::spawn_blocking` instead of parallel
3. Add explicit random seed propagation: `RngState` struct
4. Implement checkpoint-based execution for long runs

**Verification**: Run 1000 iterations of same backtest, verify identical results.

#### Phase 4 Rollback
**Trigger**: Optimization algorithms too slow or unstable

**Steps**:
1. Fall back to grid search only: remove Bayesian optimization
2. Implement parallel grid search: `rayon::par_iter`
3. Add early stopping: terminate when best score doesn't improve for N iterations
4. Use surrogate models for expensive objectives

**Verification**: Complete optimization on 10k parameter grid within 1 hour.

#### Phase 5 Rollback
**Trigger**: Paper trading causes data corruption

**Steps**:
1. Implement WAL (Write-Ahead Logging): commit transactions before applying
2. Add snapshot isolation: periodic full database snapshots
3. Implement optimistic concurrency control: version numbers on positions
4. Roll back to read-only mode: disable paper trading, use simulation only

**Verification**: Run 100k simulated trades, verify database consistency.

---

## 8. Dependencies

```toml
[package]
name = "strategy-system"
version = "0.1.0"
edition = "2021"

[dependencies]
# Core
thiserror = "1.0"
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
toml = "0.8"

# Concurrency & Data Structures
tokio = { version = "1.35", features = ["full"] }
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
hashbrown = "0.14"
lru = "0.12"

# Data Processing
polars = { version = "0.40", features = ["lazy", "csv", "parquet"] }
arrow = "50"

# Optimization
rand = "0.8"
rand_distr = "0.4"
num-traits = "0.2"

# Benchmarking & Testing
criterion = "0.5"
proptest = "1.4"

[dev-dependencies]
criterion = "0.5"

[[bench]]
name = "strategy_evaluation"
harness = false

[features]
default = []
hot_reload = []
optimization = ["rand_distr"]
```

---

## 9. Key Implementation Patterns

### 9.1 Strategy Combinator Pattern
```rust
pub trait StrategyCombinator {
    type Output: Strategy;
    
    fn combine(self, other: Self::Output) -> Self::Output;
    
    fn with_condition<F>(self, cond: F) -> Conditional<F>
    where
        F: Fn(&StrategyContext) -> bool + 'static;
}

impl StrategyCombinator for MovingAverageCrossover {
    type Output = Self;
    
    fn combine(self, other: Self) -> Self::Output {
        // Weighted average of signals
    }
    
    fn with_condition<F>(self, cond: F) -> Conditional<F>
    where
        F: Fn(&StrategyContext) -> bool + 'static,
    {
        Conditional {
            condition: Box::new(cond),
            strategy: Arc::new(self),
        }
    }
}
```

### 9.2 Deterministic Backtest Execution
```rust
pub struct BacktestEngine {
    pub strategy: Arc<dyn Strategy>,
    pub data: Arc<dyn HistoricalData>,
    pub config: BacktestConfig,
    pub rng: ThreadRng,
    pub current_state: AtomicU64,
}

impl BacktestEngine {
    pub fn run(&self) -> BacktestResult {
        let mut state = BacktestState::new(&self.config);
        
        for ts in self.config.start_ts..=self.config.end_ts {
            // Deterministic time step
            self.current_state.store(ts, Ordering::SeqCst);
            
            // Build context with historical data
            let ctx = self.build_context(ts, &state);
            
            // Pure function evaluation
            if let Some(signal) = self.strategy.evaluate(&ctx) {
                // Deterministic execution with simulated slippage
                let intent = self.execute_signal(&signal, &mut state);
            }
        }
        
        state.into_result()
    }
}
```

---

This implementation plan provides a comprehensive, production-ready roadmap for building the Strategy System. Each phase includes specific deliverables, rollback procedures, and verification steps to ensure quality and maintainability.
