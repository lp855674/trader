# Execution Enhancement System Implementation Plan

**Date**: 2024-04-04  
**Version**: 1.0  
**Status**: Draft  
**Estimated Duration**: 12 weeks (3 phases)

---

## 1. Implementation Phases

### Phase 1: Core Foundation (Weeks 1-3)
**Goal**: Establish core domain models, state machine, and basic execution flow

| Task | Duration | Dependencies | Rollback Plan |
|------|----------|--------------|---------------|
| 1.1 Core Trait Definitions | 2 days | None | Revert to previous trait definitions |
| 1.2 Order State Machine | 3 days | 1.1 | Remove state transitions, use simple status flag |
| 1.3 Position Manager MVP | 3 days | 1.2 | Disable auto-update, manual position management |
| 1.4 Paper Adapter Enhancement | 3 days | 1.3 | Disable new order types, revert to v1.0 |
| 1.4.1 Multi-order type support | 2 days | 1.4 | Limit to Market/Limit only |
| 1.4.2 Stop/Trailing implementation | 1 day | 1.4.1 | Disable stop orders |
| 1.4.3 Iceberg/TWAP/VWAP stubs | 1 day | 1.4.2 | Return error for advanced types |

**Deliverables**:
- `src/domain/order.rs` - Order types and state machine
- `src/domain/position.rs` - Position manager
- `src/adapters/paper_v2.rs` - Enhanced Paper adapter
- Unit test coverage: 80%

**Rollback Plan**: 
- Database: No schema changes yet
- Code: `git revert HEAD~2` then cherry-pick fixes
- Data: No persistent storage changes

---

### Phase 2: Execution Quality & Optimization (Weeks 4-6)
**Goal**: Implement slippage models, execution optimizer, and batch processing

| Task | Duration | Dependencies | Rollback Plan |
|------|----------|--------------|---------------|
| 2.1 Slippage & Commission Models | 3 days | 1.4 | Use hardcoded values |
| 2.2 Execution Optimizer | 3 days | 2.1 | Skip optimization, direct execution |
| 2.3 Batch Execution Queue | 3 days | 2.2 | Single-threaded execution |
| 2.3.1 Priority queue implementation | 2 days | 2.3 | FIFO queue only |
| 2.3.2 Concurrent execution | 1 day | 2.3.1 | Max concurrency = 1 |
| 2.4 Advanced Order Types | 4 days | 2.3 | Disable in production |
| 2.4.1 Iceberg Order Manager | 2 days | 2.4 | Single display order |
| 2.4.2 TWAP/VWAP Engine | 2 days | 2.4.1 | Single execution only |

**Deliverables**:
- `src/optimizer/slippage.rs` - Slippage models
- `src/executor/batch.rs` - Batch execution queue
- `src/domain/iceberg.rs` - Iceberg order logic
- Integration tests: 90%

**Rollback Plan**:
- Database: Add `execution_config` table with defaults
- Code: Disable concurrent execution, single-threaded fallback
- Data: Snapshot positions before batch execution

---

### Phase 3: Production Readiness (Weeks 7-9)
**Goal**: Production monitoring, persistence, and multi-adapter support

| Task | Duration | Dependencies | Rollback Plan |
|------|----------|--------------|---------------|
| 3.1 Database Schema & Migrations | 3 days | None | Use SQLite in-memory |
| 3.2 Execution Persistence | 3 days | 3.1 | Disable persistence, in-memory only |
| 3.3 Multi-Adapter Support | 3 days | 3.2 | Single adapter only |
| 3.3.1 Adapter factory pattern | 2 days | 3.3 | Hardcoded adapter selection |
| 3.3.2 Adapter health checks | 1 day | 3.3.1 | No health checks |
| 3.4 Monitoring & Alerting | 3 days | 3.2 | Basic logging only |
| 3.4.1 Metrics collection | 2 days | 3.4 | No metrics export |
| 3.4.2 Alert system | 1 day | 3.4.1 | No alerts |

**Deliverables**:
- Database schema (PostgreSQL/SQLite)
- Migration files
- Adapter registry
- Monitoring dashboard stub

**Rollback Plan**:
- Database: Add `orders` and `fills` tables with minimal constraints
- Code: Single adapter mode, no persistence
- Data: Export current state before migration

---

### Phase 4: Performance & Optimization (Weeks 10-12)
**Goal**: Performance tuning, stress testing, and optimization

| Task | Duration | Dependencies | Rollback Plan |
|------|----------|--------------|---------------|
| 4.1 Performance Benchmarks | 2 days | 3.4 | Disable benchmarking |
| 4.2 Memory Optimization | 3 days | 4.1 | Use conservative memory limits |
| 4.3 Network I/O Optimization | 3 days | 4.2 | Sync I/O only |
| 4.4 Load Testing | 2 days | 4.3 | Disable load testing in prod |
| 4.5 Production Configuration | 2 days | 4.4 | Development defaults only |

**Deliverables**:
- Performance benchmarks
- Optimization reports
- Production config templates

**Rollback Plan**:
- Performance: Revert to synchronous I/O
- Config: Development config only
- Monitoring: Basic logging

---

## 2. Technical Architecture

### 2.1 Design Decisions

#### Decision 1: State Machine vs. Event Sourcing
**Chosen**: State Machine with Event Logging
- **Rationale**: Simpler debugging, easier rollback, sufficient for trading domain
- **Trade-off**: Lost audit trail compared to full event sourcing
- **Mitigation**: Log all state transitions to `execution_quality` table

#### Decision 2: Memory-First with Async Persistence
**Chosen**: In-memory state with async database writes
- **Rationale**: Sub-millisecond execution latency critical for trading
- **Trade-off**: Risk of data loss on crash
- **Mitigation**: Async WAL (Write-Ahead Logging) to disk every 100ms

#### Decision 3: Adapter Pattern for Exchange Integration
**Chosen**: Trait-based adapter with central registry
- **Rationale**: Easy to add new exchanges without modifying core
- **Trade-off**: Adapter complexity increases with new features
- **Mitigation**: Strict interface contracts, comprehensive tests

#### Decision 4: Batch Execution with Priority Queue
**Chosen**: PriorityFIFO queue with bounded concurrency
- **Rationale**: Reduce network latency through batching while respecting urgency
- **Trade-off**: Potential delay for low-priority orders
- **Mitigation**: Configurable max delay, emergency override

### 2.2 Component Diagram

```
┌─────────────────────────────────────────────────────────────┐
│                    Strategy Layer                            │
│  Strategy → Signal → OrderIntent → OrderRequest              │
└────────────────────────┬────────────────────────────────────┘
                         │
                         ▼
┌─────────────────────────────────────────────────────────────┐
│                  Execution Engine                            │
│  ┌──────────────┐  ┌──────────────┐  ┌────────────────┐    │
│  │ OrderManager │  │PositionManager│ │ExecutionOptimizer│   │
│  │(State Machine)│ │(PnL Calc)    │ │(Slippage Model) │    │
│  └──────────────┘  └──────────────┘  └────────────────┘    │
│  ┌──────────────┐  ┌──────────────┐                         │
│  │BatchExecutor │  │  Metrics     │                         │
│  │(PriorityQueue)│ │Collector     │                         │
│  └──────────────┘  └──────────────┘                         │
└────────────────────────┬────────────────────────────────────┘
                         │
                         ▼
┌─────────────────────────────────────────────────────────────┐
│                    Adapter Layer                             │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌────────────┐  │
│  │Paper     │  │Binance   │  │Longbridge│  │Custom     │  │
│  │Adapter   │  │Adapter   │  │Adapter   │  │Adapter    │  │
│  └──────────┘  └──────────┘  └──────────┘  └────────────┘  │
└─────────────────────────────────────────────────────────────┘
                         │
                         ▼
┌─────────────────────────────────────────────────────────────┐
│                    Persistence Layer                          │
│  PostgreSQL (Primary) + Redis (Cache) + S3 (Archives)        │
└─────────────────────────────────────────────────────────────┘
```

### 2.3 Technology Stack

**Language & Runtime**:
- Rust 1.75+ (async/await, tokio runtime)
- Target: x86_64-unknown-linux-gnu (production), wasm32-unknown-unknown (browser strategies)

**Key Dependencies**:
```toml
[dependencies]
tokio = { version = "1.35", features = ["full", "rt-multi-thread"] }
async-trait = "0.1"
thiserror = "1.0"
serde = { version = "1.0", features = ["derive"] }
chrono = "0.4"
bigdecimal = "0.4"  # Exact decimal arithmetic for PnL
concurrent-queue = "2.0"
priority-queue = "2.0"
sqlx = { version = "0.7", features = ["runtime-tokio-rustls", "postgres"] }
redis = "0.23"
metrics = "0.22"
prometheus = "0.13"
```

### 2.4 Concurrency Model

**Approach**: Actor-like model using `Arc<Mutex<>>` for shared state
- **OrderManager**: Single instance, lock-free reads, lock for writes
- **PositionManager**: Lock-free reads (RwLock), writes serialized
- **BatchExecutor**: Worker pool (default 4), bounded channel to queue

**Thread Safety**:
- Traits implement `Send + Sync`
- All state in `Arc<Mutex<>>` or `Arc<RwLock<>>`
- Critical sections < 1ms (measured in benchmarks)

---

## 3. Database Schema

### 3.1 Core Tables

```sql
-- orders: Main order table with state machine support
CREATE TABLE orders (
    order_id UUID PRIMARY KEY,              -- Client order ID (immutable)
    exchange_order_id VARCHAR(100),         -- Exchange-assigned ID
    account_id UUID NOT NULL,               -- Account reference
    instrument_id VARCHAR(50) NOT NULL,     -- Trading pair
    side VARCHAR(10) NOT NULL CHECK (side IN ('BUY', 'SELL')),
    qty DECIMAL(18,6) NOT NULL,             -- Order quantity
    limit_price DECIMAL(18,6),              -- Limit price
    stop_price DECIMAL(18,6),               -- Stop trigger price
    trigger_price DECIMAL(18,6),            -- For stop orders
    order_type VARCHAR(20) NOT NULL,
    time_in_force VARCHAR(10) NOT NULL DEFAULT 'DAY',
    
    -- State machine
    status VARCHAR(20) NOT NULL DEFAULT 'PENDING',
    status_updated_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    
    -- Execution metadata
    created_at_ms BIGINT NOT NULL,
    submitted_at_ms BIGINT,
    filled_at_ms BIGINT,
    cancelled_at_ms BIGINT,
    completed_at_ms BIGINT,                  -- Filled or Cancelled
    
    -- Financials
    commission_paid DECIMAL(18,6) DEFAULT 0,
    commission_currency VARCHAR(10) DEFAULT 'USD',
    commission_type VARCHAR(20),             -- maker/taker/percentage
    
    -- Optimization data
    slippage_tolerance DECIMAL(10,6),
    estimated_slippage_bps DECIMAL(10,2),
    
    -- Advanced order types
    iceberg_total_qty DECIMAL(18,6),
    iceberg_display_qty DECIMAL(18,6),
    twap_interval_ms BIGINT,
    twap_duration_ms BIGINT,
    vwap_target_volume DECIMAL(18,6),
    
    -- Audit
    reject_reason TEXT,
    execution_strategy VARCHAR(50),          -- Used execution strategy
    
    INDEX idx_account_status (account_id, status),
    INDEX idx_created_at (created_at_ms),
    INDEX idx_instrument_status (instrument_id, status),
    UNIQUE UNIQUE_account_exchange (account_id, exchange_order_id)
);

-- fills: Individual execution legs
CREATE TABLE fills (
    fill_id UUID PRIMARY KEY,
    order_id UUID NOT NULL REFERENCES orders(order_id) ON DELETE CASCADE,
    exchange_fill_id VARCHAR(100),
    
    qty DECIMAL(18,6) NOT NULL,
    price DECIMAL(18,6) NOT NULL,
    side VARCHAR(10) NOT NULL,
    commission DECIMAL(18,6) NOT NULL DEFAULT 0,
    exchange_fee DECIMAL(18,6),              -- Exchange fees
    
    ts_ms BIGINT NOT NULL,
    
    -- VWAP calculation
    running_vwap DECIMAL(18,6),              -- VWAP up to this fill
    
    INDEX idx_order_id (order_id),
    INDEX idx_ts_ms (ts_ms),
    INDEX idx_instrument_time (instrument_id, ts_ms)
);

-- positions: Current positions (materialized view candidate)
CREATE TABLE positions (
    instrument_id VARCHAR(50) PRIMARY KEY,
    qty DECIMAL(18,6) NOT NULL DEFAULT 0,
    avg_price DECIMAL(18,6) NOT NULL,
    side VARCHAR(10) NOT NULL,
    open_time BIGINT NOT NULL,
    commission_paid DECIMAL(18,6) NOT NULL DEFAULT 0,
    
    UNIQUE UNIQUE_instrument_side (instrument_id, side)
);

-- positions_history: Audit trail for positions
CREATE TABLE positions_history (
    id BIGSERIAL PRIMARY KEY,
    instrument_id VARCHAR(50) NOT NULL,
    qty DECIMAL(18,6),
    avg_price DECIMAL(18,6),
    side VARCHAR(10),
    ts_ms BIGINT NOT NULL,
    
    UNIQUE UNIQUE_instrument_ts (instrument_id, ts_ms),
    INDEX idx_ts_ms (ts_ms)
);

-- execution_quality: Performance metrics
CREATE TABLE execution_quality (
    id BIGSERIAL PRIMARY KEY,
    order_id UUID REFERENCES orders(order_id),
    instrument_id VARCHAR(50),
    
    -- Execution metrics
    slippage_bps DECIMAL(10,2),
    fill_rate DECIMAL(5,4),
    vwap_deviation_bps DECIMAL(10,2),
    
    -- Market conditions
    market_volatility DECIMAL(10,4),
    spread_bps DECIMAL(10,2),
    order_book_depth_usd DECIMAL(18,2),
    
    -- Execution strategy used
    strategy VARCHAR(50),
    latency_ms DECIMAL(10,2),              -- Total execution latency
    
    ts_ms BIGINT NOT NULL,
    
    INDEX idx_order_id (order_id),
    INDEX idx_ts_ms (ts_ms)
);

-- execution_jobs: Batch processing queue
CREATE TABLE execution_jobs (
    job_id UUID PRIMARY KEY,
    job_type VARCHAR(20) NOT NULL,
    priority INTEGER NOT NULL DEFAULT 0,
    payload JSONB NOT NULL,               -- Serialized job data
    status VARCHAR(20) NOT NULL DEFAULT 'PENDING',
    
    submitted_at_ms BIGINT NOT NULL,
    completed_at_ms BIGINT,
    error_message TEXT,
    retry_count INTEGER DEFAULT 0,
    
    INDEX idx_status (status),
    INDEX idx_priority_time (priority, submitted_at_ms)
);

-- configuration: System configuration
CREATE TABLE configuration (
    key VARCHAR(100) PRIMARY KEY,
    value JSONB NOT NULL,
    updated_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);

-- indexes for performance
CREATE INDEX CONCURRENTLY idx_orders_created_at ON orders(created_at_ms);
CREATE INDEX CONCURRENTLY idx_fills_ts_ms ON fills(ts_ms);
CREATE INDEX CONCURRENTLY idx_execution_quality_ts ON execution_quality(ts_ms);
```

### 3.2 Migration Files

```bash
# 001_initial_schema.sql
CREATE TABLE orders (...)...
CREATE TABLE fills (...)
...

# 002_add_advanced_orders.sql
ALTER TABLE orders ADD COLUMN iceberg_total_qty DECIMAL(18,6);
ALTER TABLE orders ADD COLUMN twap_duration_ms BIGINT;
CREATE TABLE execution_jobs (...)

# 003_add_monitoring.sql
CREATE TABLE execution_quality (...)
CREATE TABLE configuration (...)

# 004_add_indexes.sql
CREATE INDEX CONCURRENTLY idx_orders_created_at ON orders(created_at_ms);
...

# 005_partition_positions_history.sql
-- Partitioning for large datasets
CREATE TABLE positions_history_202401 (
    LIKE positions_history
);
ALTER TABLE positions_history_202401 SET TABLESPACE tsdata;
```

---

## 4. Test Strategy

### 4.1 Unit Tests

**Coverage Target**: 85% minimum

```rust
// Core Domain
#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_order_state_transition() {
        let mut order = OrderState::new(request);
        order.transition(OrderEvent::Created).unwrap();
        order.transition(OrderEvent::Submitted).unwrap();
        // Invalid transition should fail
        assert!(order.transition(OrderEvent::Filled).is_err());
    }
    
    #[test]
    fn test_position_pnl_calculation() {
        let mut manager = PositionManager::new();
        // Simulate buy at $100, sell at $110
        let fill1 = Fill { price: 100.0, qty: 10.0, ..Default::default() };
        let fill2 = Fill { price: 110.0, qty: 10.0, ..Default::default() };
        manager.update(&fill1).unwrap();
        manager.update(&fill2).unwrap();
        
        let pnl = manager.calculate_pnl();
        assert!((pnl.realized - 100.0).abs() < 0.01);  // $1000 profit
    }
    
    #[test]
    fn test_slippage_models() {
        let fixed = SlippageModel::Fixed { base_bps: 5.0, ..Default::default() };
        assert_eq!(fixed.calculate(instrument, 100.0, 100.0), 0.0005);
        
        let volume = SlippageModel::VolumeBased {
            small_order_bps: 2.0,
            large_order_bps: 50.0,
            exponent: 0.5,
        };
        // Large order should have higher slippage
        assert!(volume.calculate(instrument, 10000.0, 100.0) > 0.0002);
    }
    
    #[test]
    fn test_iceberg_order_consumption() {
        let mut iceberg = IcebergOrder {
            total_qty: 1000.0,
            display_qty: 100.0,
            remaining_qty: 1000.0,
            active_display_orders: vec![],
        };
        
        assert_eq!(iceberg.get_next_display_qty(), 100.0);
        iceberg.consume_fill(50.0).unwrap();
        assert_eq!(iceberg.get_next_display_qty(), 100.0);  // Refresh display
        assert!(iceberg.is_complete());  // All filled
    }
    
    #[test]
    fn test_batch_queue_priority() {
        let mut queue = ExecutionQueue::new(10);
        queue.submit(ExecutionJob::Order { priority: 1, ..Default::default() });
        queue.submit(ExecutionJob::Order { priority: 5, ..Default::default() });
        queue.submit(ExecutionJob::Order { priority: 3, ..Default::default() });
        
        // Should process priority 5, then 3, then 1 (highest first)
        assert_eq!(queue.get_next_priority(), 5);
    }
}
```

### 4.2 Integration Tests

**Infrastructure**: Testcontainers (PostgreSQL, Redis)

```rust
#[tokio::test]
async fn test_paper_adapter_full_workflow() {
    // Arrange
    let adapter = PaperAdapter::new();
    let order = OrderRequest {
        order_id: "test-123".to_string(),
        instrument: InstrumentId::new(Venue::Crypto, "BTC-USD"),
        side: Side::Buy,
        qty: 0.1,
        order_type: OrderType::Limit,
        limit_price: Some(45000.0),
        ..Default::default()
    };
    
    // Act
    let result = adapter.execute_order(order).await;
    
    // Assert
    assert!(result.is_ok());
    let state = adapter.query_order("test-123").await.unwrap();
    assert_eq!(state.status, OrderStatus::Filled);
}

#[tokio::test]
async fn test_batch_execution_concurrency() {
    let mut queue = ExecutionQueue::new(10);
    
    // Submit 100 orders
    for i in 0..100 {
        queue.submit(ExecutionJob::Order {
            request: create_test_order(i),
            priority: i % 10,
        });
    }
    
    // Process with max 5 concurrent
    let start = Instant::now();
    queue.process_batch(5).await;
    let duration = start.elapsed();
    
    assert!(duration < Duration::from_secs(5));  // Should be fast
}

#[tokio::test]
async fn test_multi_adapter_failover() {
    // Create primary adapter that fails after 3 calls
    let primary = MockAdapter {
        call_count: Arc::new(AtomicUsize::new(0)),
        ..MockAdapter::default()
    };
    
    let secondary = MockAdapter::default();
    
    // Configure failover
    let manager = ExecutionEngine::builder()
        .with_adapter(primary)
        .with_fallback(secondary)
        .build();
    
    // Execute 5 orders - should use secondary after primary fails
    for _ in 0..5 {
        manager.execute(create_test_order(0)).await;
    }
    
    assert!(primary.call_count.load(Ordering::SeqCst) > 3);
    assert!(secondary.call_count.load(Ordering::SeqCst) > 0);
}
```

### 4.3 Performance Tests

**Benchmarks**:
```rust
#[bench]
fn bench_order_state_transition(b: &mut Bencher) {
    let mut order = OrderState::new(request);
    b.iter(|| {
        order.transition(OrderEvent::Created).unwrap();
        order.transition(OrderEvent::Submitted).unwrap();
    });
}

#[bench]
fn bench_position_update(b: &mut Bencher) {
    let mut manager = PositionManager::new();
    let fill = Fill { qty: 10.0, price: 100.0, ..Default::default() };
    b.iter(|| manager.update(&fill).unwrap());
}

#[bench]
fn bench_slippage_calculation(b: &mut Bencher) {
    let model = SlippageModel::Hybrid { strategies: vec![] };
    let qty = 1000.0;
    let price = 100.0;
    b.iter(|| model.calculate(instrument, qty, price));
}
```

**Load Testing**:
- **Goal**: 10,000 orders/second throughput
- **Latency**: P99 < 50ms for order submission
- **Memory**: < 500MB heap under load

---

## 5. API Contracts

### 5.1 gRPC Protobuf (Internal)

```protobuf
syntax = "proto3";

package trading;  

import "google/protobuf/timestamp.proto";

// Order Types
enum OrderType {
  ORDER_TYPE_UNSPECIFIED = 0;
  ORDER_TYPE_MARKET = 1;
  ORDER_TYPE_LIMIT = 2;
  ORDER_TYPE_STOP = 3;
  ORDER_TYPE_STOP_LIMIT = 4;
  ORDER_TYPE_TRAILING = 5;
  ORDER_TYPE_ICEBERG = 6;
  ORDER_TYPE_TWAP = 7;
  ORDER_TYPE_VWAP = 8;
}

enum OrderStatus {
  ORDER_STATUS_PENDING = 0;
  ORDER_STATUS_SUBMITTED = 1;
  ORDER_STATUS_PARTIALLY_FILLED = 2;
  ORDER_STATUS_FILLED = 3;
  ORDER_STATUS_CANCELLED = 4;
  ORDER_STATUS_REJECTED = 5;
  ORDER_STATUS_EXPIRED = 6;
}

// Request/Response
message OrderRequest {
  string order_id = 1;                    // Client ID
  InstrumentId instrument = 2;
  Side side = 3;
  double qty = 4;
  OrderType order_type = 5;
  TimeInForce time_in_force = 6;
  
  // Pricing
  double limit_price = 7;
  double stop_price = 8;
  double trigger_price = 9;
  
  // Execution
  double slippage_tolerance = 10;
  double commission_rate = 11;
  
  // Advanced
  double iceberg_qty = 12;                // Total
  double display_qty = 13;                // Show
  uint64 twap_interval_ms = 14;
  uint64 duration_ms = 15;
}

message Fill {
  string fill_id = 1;
  string order_id = 2;
  double qty = 3;
  double price = 4;
  Side side = 5;
  double commission = 6;
  google.protobuf.Timestamp timestamp = 7;
}

message OrderState {
  OrderRequest request = 1;
  OrderStatus status = 2;
  repeated Fill fills = 3;
  google.protobuf.Timestamp created_at = 4;
  google.protobuf.Timestamp updated_at = 5;
  string exchange_order_id = 6;
  string reject_reason = 7;
}

// Services
service ExecutionService {
  rpc SubmitOrder(OrderRequest) returns (OrderState);
  rpc UpdateStatus(OrderEvent) returns (Empty);
  rpc GetOrder(string) returns (OrderState);
  rpc GetOrders(OrderFilter) returns (OrderList);
  rpc CancelOrder(string) returns (Empty);
  rpc GetPortfolio(PortfolioRequest) returns (PortfolioSnapshot);
}

service AdapterHealthService {
  rpc HealthCheck(HealthRequest) returns (HealthResponse);
  rpc GetMetrics(MetricsRequest) returns (MetricsSnapshot);
}

// Filter
message OrderFilter {
  string instrument_id = 1;
  OrderStatus status = 2;
  int64 since_ms = 3;
  int64 until_ms = 4;
}

// Portfolio
message PortfolioRequest {
  string account_id = 1;
}

message PortfolioSnapshot {
  repeated Position positions = 1;
  double cash = 2;
  double total_net_value = 3;
  PnL pnl = 4;
}

message PnL {
  double unrealized = 1;
  double realized = 2;
  double total = 3;
  double daily_pnl = 4;
}
```

### 5.2 REST API (External/Management)

```yaml
# OpenAPI 3.0
openapi: 3.0.3
info:
  title: Execution Enhancement API
  version: 1.0.0
  description: Order management and execution APIs

paths:
  /api/v1/orders:
    post:
      summary: Submit order
      requestBody:
        required: true
        content:
          application/json:
            schema:
              $ref: '#/components/schemas/OrderRequest'
      responses:
        201:
          description: Order submitted
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/OrderState'

    get:
      summary: List orders
      parameters:
        - name: instrument_id
          in: query
          schema:
            type: string
        - name: status
          in: query
          schema:
            type: string
        - name: since
          in: query
          schema:
            type: integer
            format: int64
      responses:
        200:
          description: List of orders
          content:
            application/json:
              schema:
                type: array
                items:
                  $ref: '#/components/schemas/OrderState'

  /api/v1/portfolio:
    get:
      summary: Get portfolio snapshot
      responses:
        200:
          description: Portfolio data
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/PortfolioSnapshot'

  /api/v1/execution/quality:
    get:
      summary: Execution quality metrics
      parameters:
        - name: since
          in: query
          schema:
            type: integer
            format: int64
        - name: instrument
          in: query
          schema:
            type: string
      responses:
        200:
          description: Quality metrics
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/ExecutionQuality'

components:
  schemas:
    OrderRequest:
      type: object
      required:
        - order_id
        - instrument
        - side
        - qty
        - order_type
      properties:
        order_id:
          type: string
        instrument:
          type: string
        side:
          type: string
          enum: [BUY, SELL]
        qty:
          type: number
        order_type:
          type: string
          enum: [MARKET, LIMIT, STOP, ICEBERG, TWAP, VWAP]
        limit_price:
          type: number
        slippage_tolerance:
          type: number

    OrderState:
      type: object
      properties:
        order_id:
          type: string
        status:
          type: string
          enum: [PENDING, SUBMITTED, FILLED, CANCELLED]
        fills:
          type: array
          items:
            $ref: '#/components/schemas/Fill'

    ExecutionQuality:
      type: object
      properties:
        avg_slippage_bps:
          type: number
        fill_rate:
          type: number
          minimum: 0
          maximum: 1
        p99_latency_ms:
          type: number
```

---

## 6. Configuration Schema

### 6.1 YAML Configuration

```yaml
# config/execution.yaml
execution:
  # Core settings
  max_concurrent_orders: 100
  order_timeout_ms: 30000
  max_retries: 3
  retry_backoff_ms: 100
  
  # Batch execution
  batch_size: 50
  max_concurrent_batches: 4
  priority_levels:
    - name: URGENT
      weight: 100
    - name: HIGH
      weight: 50
    - name: NORMAL
      weight: 10
    - name: LOW
      weight: 1

slippage:
  # Default model: Hybrid (fixed + volume-based)
  default_model: hybrid
  models:
    fixed:
      base_bps: 5.0
      volatility_multiplier: 2.0
    volume_based:
      small_order_bps: 2.0
      large_order_bps: 50.0
      exponent: 0.5
      volume_threshold_usd: 100000
    market_depth:
      enabled: true
      depth_source: order_book
      impact_factor: 0.001

commission:
  # Exchange-specific configs
  exchanges:
    binance:
      rate: 0.001
      maker_rate: 0.0001
      min_fee: 0.01
      taker_fee: 0.001
    longbridge:
      rate: 0.0005
      tiered: true
      tiers:
        - volume_threshold: 1000
          rate: 0.0002
        - volume_threshold: 10000
          rate: 0.0001

  default:
    rate: 0.001
    min_fee: 0.01
    max_fee: 100.0

position:
  # Margin settings
  initial_margin: 0.1
  maintenance_margin: 0.05
  max_leverage: 10
  
  # Risk limits
  max_position_usd: 1000000
  max_daily_loss: 0.1  # 10% daily stop
  max_drawdown: 0.2

advanced_orders:
  iceberg:
    default_display_ratio: 0.1
    max_display_qty: 1000
    auto_refresh: true
    refresh_interval_ms: 1000
  
  twap:
    default_interval_ms: 60000
    min_duration_ms: 60000
    max_duration_ms: 86400000
    min_qty_per_execution: 0.1
  
  vwap:
    volume_profile_source: historical
    profile_lookback_days: 30
    min_qty_per_execution: 0.1

monitoring:
  # Metrics
  enabled: true
  prometheus_port: 9090
  metrics_interval_ms: 1000
  
  # Alerts
  alert_channels:
    - type: webhook
      url: https://hooks.slack.com/...
      severity_threshold: warning
    - type: email
      recipients:
        - trader@company.com
      severity_threshold: critical
  
  # Thresholds
  thresholds:
    slippage_warning_bps: 50
    slippage_critical_bps: 100
    fill_rate_warning: 0.8
    fill_rate_critical: 0.5
    latency_p99_warning_ms: 100
    latency_p99_critical_ms: 500

logging:
  level: info
  format: json
  outputs:
    - type: file
      path: /var/log/execution/
      rotation_size_mb: 100
      max_files: 10
    - type: stdout
      level: error

persistence:
  database:
    type: postgres
    url: postgresql://user:pass@localhost:5432/execution
    pool_size: 20
    max_lifetime_ms: 30000
  
  cache:
    type: redis
    url: redis://localhost:6379
    ttl_ms: 60000

adapters:
  primary:
    type: paper
    config:
      auto_execute: true
      simulate_slippage: false
  
  fallback:
    type: paper
    config:
      auto_execute: false

security:
  api_key_required: true
  rate_limit_orders_per_minute: 1000
  ip_whitelist:
    - 10.0.0.0/8
    - 172.16.0.0/12
```

### 6.2 JSON Configuration (Runtime Override)

```json
{
  "execution": {
    "max_concurrent_orders": 50,
    "order_timeout_ms": 15000,
    "max_retries": 1
  },
  "environment": "testing",
  "mock_slippage": {
    "enabled": true,
    "fixed_bps": 10
  },
  "debug": {
    "verbose_logging": true,
    "print_state_transitions": true,
    "trace_execution_path": true
  }
}
```

---

## 7. Rollback Plans (Detailed)

### Phase 1 Rollback
**Trigger**: Critical bug in state machine or position calculation

1. **Immediate** (0-5 min):
   - Kill process: `pkill -f execution-engine`
   - Revert code: `git revert HEAD~3` (keeps only critical fixes)
   - Restart with previous release: `systemctl restart execution-engine@v1.0.0`

2. **Data Integrity**:
   - No schema changes, no data loss risk
   - Position data in memory only, but `positions_history` table provides audit trail

3. **Verification**:
   ```bash
   # Verify state machine reverted
   curl http://localhost:8080/api/v1/orders?status=PENDING
   
   # Verify positions intact
   curl http://localhost:8080/api/v1/portfolio
   ```

### Phase 2 Rollback
**Trigger**: Slippage calculation produces infinite values or batch queue deadlock

1. **Immediate**:
   - Disable batch execution: `curl -X POST http://localhost:8080/api/v1/config -d '{"batch_enabled": false}'`
   - Revert optimizer: `git revert HEAD~2`

2. **Circuit Breaker**:
   - If slippage > 1000 bps for 10 seconds, auto-disable optimizer
   - Fallback to direct execution with hardcoded slippage

3. **Data Recovery**:
   - Snapshot positions before batch execution: `SELECT * INTO positions_snapshot FROM positions` (PostgreSQL)
   - Batch jobs table allows rollback of failed executions

### Phase 3 Rollback
**Trigger**: Database migration fails or adapter connectivity issues

1. **Database Rollback**:
   ```sql
   -- If migration 002 fails
   DROP TABLE execution_jobs;  -- New table, safe to drop
   DROP TABLE execution_quality;  -- New table, safe to drop
   ALTER TABLE orders DROP COLUMN iceberg_total_qty;  -- If partial migration
   ```

2. **Adapter Failover**:
   - Automatic: Switch to secondary adapter if primary health check fails
   - Manual: `curl -X POST http://localhost:8080/api/v1/adapters/switch -d '{"target": "paper"}'`

3. **Graceful Degradation**:
   - Disable advanced order types: `ALTER TABLE orders ALTER COLUMN order_type SET DEFAULT 'LIMIT'`
   - Reduce max_concurrent_orders to 10

### Phase 4 Rollback
**Trigger**: Performance degradation under load

1. **Performance Rollback**:
   - Revert to synchronous I/O: `git revert HEAD~2`
   - Disable concurrent batch execution
   - Reduce max_concurrent_orders to 10

2. **Memory Management**:
   - Set memory limits: `ulimit -v 200000` (200MB)
   - Enable aggressive GC: `RUST_MIN_STACK=1048576`

3. **Load Shedding**:
   - Reject new orders if queue > 1000: `ALTER TABLE execution_jobs ADD COLUMN max_queue_size INTEGER DEFAULT 1000`

---

## 8. Risk Mitigation

### Operational Risks
| Risk | Probability | Impact | Mitigation |
|------|------------|--------|------------|
| State machine deadlock | Low | Critical | Timeout detection, auto-recovery |
| Position calculation error | Medium | High | Dual calculation (memory + DB), reconciliation |
| Slippage model failure | Medium | High | Circuit breaker, hardcoded fallback |
| Database lock contention | Low | High | Connection pooling, async writes |

### Financial Risks
| Risk | Probability | Impact | Mitigation |
|------|------------|--------|------------|
| Margin call | Low | Critical | Pre-trade margin check, hard limits |
| Order routing error | Medium | High | Multi-adapter, failover, audit logs |
| Slippage > tolerance | Medium | Medium | Pre-trade simulation, execution quality monitoring |

---

## 9. Success Metrics

### Phase 1 Success Criteria
- [ ] 100% state transition correctness (verified by property-based testing)
- [ ] Position PnL accuracy < 0.01% vs. manual calculation
- [ ] Paper adapter supports all 8 order types

### Phase 2 Success Criteria
- [ ] Slippage model accuracy: ±5 bps vs. actual execution
- [ ] Batch execution throughput: 1000 orders/sec
- [ ] Memory usage: < 500MB under load

### Phase 3 Success Criteria
- [ ] Database query latency: P99 < 10ms
- [ ] Adapter failover time: < 100ms
- [ ] Monitoring coverage: 100% of critical paths

### Phase 4 Success Criteria
- [ ] Production load: 10,000 orders/day sustained
- [ ] No P99 latency > 50ms in production
- [ ] Zero data loss during crash simulation

---

## 10. Approval Requirements

**Technical Review**:
- [ ] Architecture review (CTO)
- [ ] Security review (Security team)
- [ ] Database schema review (DBA)

**Risk Review**:
- [ ] Risk management approval
- [ ] Compliance review (if regulated)
- [ ] Disaster recovery plan approval

**Go/No-Go Criteria**:
- All critical tests passing
- Rollback plan tested in staging
- On-call support assigned
- Monitoring dashboards operational

---

**Document Control**:
- **Author**: Trading Infrastructure Team
- **Reviewers**: [Pending]
- **Last Updated**: 2024-04-04
- **Next Review**: 2024-04-18 (Phase 1 completion)

**Version History**:
- v1.0 (2024-04-04): Initial implementation plan
- v1.1 (2024-04-11): Phase 1 completion review
- v1.2 (2024-04-18): Phase 2 completion review
- v1.3 (2024-04-25): Production readiness review

---

**End of Implementation Plan**
