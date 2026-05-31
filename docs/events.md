# events.md

## 1. Overview

Trader 采用 Event-Driven Architecture（EDA）。

所有模块之间通过统一事件总线通信：

```text
Market Data
    │
    ▼
Event Bus
    │
    ├── Alpha
    ├── Portfolio
    ├── Risk
    ├── Execution
    ├── OMS
    ├── Broker
    ├── Accounting
    ├── Metrics
    └── API
```

设计目标：

* 模块解耦
* 可回放
* 可审计
* 可持久化
* 支持实时交易
* 支持历史回测

---

## 2. Event Categories

```rust
pub enum EventCategory {
    Market,
    Signal,
    Portfolio,
    Risk,
    Execution,
    Order,
    Trade,
    Position,
    Account,
    System,
}
```

---

## 3. Event Envelope

所有事件统一封装。

```rust
pub struct EventEnvelope<T> {
    pub event_id: Uuid,
    pub ts: DateTime<Utc>,
    pub source: String,
    pub category: EventCategory,
    pub payload: T,
}
```

字段：

| Field    | Description |
| -------- | ----------- |
| event_id | 全局唯一ID      |
| ts       | 事件时间        |
| source   | 来源模块        |
| category | 分类          |
| payload  | 实际数据        |

---

## 4. Market Events

### TickEvent

```rust
pub struct TickEvent {
    pub symbol: String,
    pub exchange: String,

    pub bid_price: Decimal,
    pub ask_price: Decimal,

    pub bid_size: Decimal,
    pub ask_size: Decimal,

    pub last_price: Decimal,
    pub volume: Decimal,

    pub ts: DateTime<Utc>,
}
```

---

### BarEvent

```rust
pub struct BarEvent {
    pub symbol: String,
    pub timeframe: String,

    pub open: Decimal,
    pub high: Decimal,
    pub low: Decimal,
    pub close: Decimal,

    pub volume: Decimal,
    pub turnover: Decimal,

    pub ts: DateTime<Utc>,
}
```

---

### OrderBookEvent

```rust
pub struct OrderBookEvent {
    pub symbol: String,

    pub bids: Vec<Level>,
    pub asks: Vec<Level>,

    pub ts: DateTime<Utc>,
}
```

```rust
pub struct Level {
    pub price: Decimal,
    pub qty: Decimal,
}
```

---

## 5. Signal Events

策略只能产生 Signal。

禁止直接产生订单。

---

### SignalEvent

```rust
pub struct SignalEvent {
    pub strategy_id: String,

    pub symbol: String,

    pub side: SignalSide,

    pub confidence: f64,

    pub ts: DateTime<Utc>,
}
```

---

### SignalSide

```rust
pub enum SignalSide {
    Buy,
    Sell,
    CloseLong,
    CloseShort,
}
```

---

## 6. Portfolio Events

### TargetPositionEvent

Portfolio 根据 Signal 生成目标仓位。

```rust
pub struct TargetPositionEvent {
    pub strategy_id: String,

    pub symbol: String,

    pub target_qty: Decimal,

    pub ts: DateTime<Utc>,
}
```

---

### PositionChangedEvent

```rust
pub struct PositionChangedEvent {
    pub symbol: String,

    pub old_qty: Decimal,
    pub new_qty: Decimal,

    pub ts: DateTime<Utc>,
}
```

---

## 7. Risk Events

### RiskApprovedEvent

```rust
pub struct RiskApprovedEvent {
    pub request_id: Uuid,
    pub ts: DateTime<Utc>,
}
```

---

### RiskRejectedEvent

```rust
pub struct RiskRejectedEvent {
    pub request_id: Uuid,
    pub reason: String,
    pub ts: DateTime<Utc>,
}
```

---

### RiskLimitTriggeredEvent

```rust
pub struct RiskLimitTriggeredEvent {
    pub limit_name: String,
    pub reason: String,
    pub ts: DateTime<Utc>,
}
```

---

## 8. Execution Events

Execution 负责把目标仓位转换为订单请求。

---

### ExecutionRequestEvent

```rust
pub struct ExecutionRequestEvent {
    pub symbol: String,

    pub target_qty: Decimal,
    pub current_qty: Decimal,

    pub ts: DateTime<Utc>,
}
```

---

### OrderIntentEvent

OMS 接收的统一订单意图。

```rust
pub struct OrderIntentEvent {
    pub symbol: String,

    pub side: OrderSide,

    pub qty: Decimal,

    pub order_type: OrderType,

    pub ts: DateTime<Utc>,
}
```

---

## 9. OMS Events

### OrderCreatedEvent

```rust
pub struct OrderCreatedEvent {
    pub order_id: String,

    pub symbol: String,

    pub side: OrderSide,

    pub qty: Decimal,

    pub ts: DateTime<Utc>,
}
```

---

### OrderSubmittedEvent

```rust
pub struct OrderSubmittedEvent {
    pub order_id: String,

    pub broker_order_id: String,

    pub ts: DateTime<Utc>,
}
```

---

### OrderAcceptedEvent

```rust
pub struct OrderAcceptedEvent {
    pub order_id: String,
    pub ts: DateTime<Utc>,
}
```

---

### OrderRejectedEvent

```rust
pub struct OrderRejectedEvent {
    pub order_id: String,
    pub reason: String,
    pub ts: DateTime<Utc>,
}
```

---

### OrderCanceledEvent

```rust
pub struct OrderCanceledEvent {
    pub order_id: String,
    pub ts: DateTime<Utc>,
}
```

---

## 10. Fill Events

成交事件是系统核心事件。

---

### FillEvent

```rust
pub struct FillEvent {
    pub trade_id: String,

    pub order_id: String,

    pub symbol: String,

    pub side: OrderSide,

    pub qty: Decimal,

    pub price: Decimal,

    pub fee: Decimal,

    pub ts: DateTime<Utc>,
}
```

---

### PartialFillEvent

```rust
pub struct PartialFillEvent {
    pub trade_id: String,

    pub order_id: String,

    pub filled_qty: Decimal,

    pub remain_qty: Decimal,

    pub price: Decimal,

    pub ts: DateTime<Utc>,
}
```

---

## 11. Position Events

### PositionOpenedEvent

```rust
pub struct PositionOpenedEvent {
    pub symbol: String,

    pub qty: Decimal,

    pub avg_price: Decimal,

    pub ts: DateTime<Utc>,
}
```

---

### PositionClosedEvent

```rust
pub struct PositionClosedEvent {
    pub symbol: String,

    pub realized_pnl: Decimal,

    pub ts: DateTime<Utc>,
}
```

---

### PositionUpdatedEvent

```rust
pub struct PositionUpdatedEvent {
    pub symbol: String,

    pub qty: Decimal,

    pub avg_price: Decimal,

    pub unrealized_pnl: Decimal,

    pub ts: DateTime<Utc>,
}
```

---

## 12. Account Events

### BalanceUpdatedEvent

```rust
pub struct BalanceUpdatedEvent {
    pub cash: Decimal,
    pub equity: Decimal,

    pub ts: DateTime<Utc>,
}
```

---

### MarginUpdatedEvent

```rust
pub struct MarginUpdatedEvent {
    pub used_margin: Decimal,
    pub available_margin: Decimal,

    pub ts: DateTime<Utc>,
}
```

---

### EquityUpdatedEvent

```rust
pub struct EquityUpdatedEvent {
    pub equity: Decimal,
    pub drawdown: Decimal,

    pub ts: DateTime<Utc>,
}
```

---

## 13. System Events

### StrategyStartedEvent

```rust
pub struct StrategyStartedEvent {
    pub strategy_id: String,
    pub ts: DateTime<Utc>,
}
```

---

### StrategyStoppedEvent

```rust
pub struct StrategyStoppedEvent {
    pub strategy_id: String,
    pub ts: DateTime<Utc>,
}
```

---

### BacktestStartedEvent

```rust
pub struct BacktestStartedEvent {
    pub run_id: String,
    pub ts: DateTime<Utc>,
}
```

---

### BacktestFinishedEvent

```rust
pub struct BacktestFinishedEvent {
    pub run_id: String,
    pub ts: DateTime<Utc>,
}
```

---

### ReplayStartedEvent

```rust
pub struct ReplayStartedEvent {
    pub run_id: String,
    pub ts: DateTime<Utc>,
}
```

---

### ReplayFinishedEvent

```rust
pub struct ReplayFinishedEvent {
    pub run_id: String,
    pub ts: DateTime<Utc>,
}
```

---

## 14. Event Bus Interface

```rust
pub trait EventBus {
    fn publish<E>(&self, event: E);

    fn subscribe<E>(
        &self,
        handler: impl Fn(E) + Send + Sync + 'static,
    );
}
```

---

## 15. Event Persistence

支持 Event Sourcing。

所有关键事件可写入 SQLite。

记录：

```text
SignalEvent
TargetPositionEvent
OrderCreatedEvent
OrderSubmittedEvent
OrderAcceptedEvent
OrderRejectedEvent
FillEvent
PositionUpdatedEvent
BalanceUpdatedEvent
RiskLimitTriggeredEvent
```

用途：

* 审计
* 调试
* 回放
* 事故恢复

---

## 16. Event Flow

```text
MarketData

    ↓

SignalEvent

    ↓

TargetPositionEvent

    ↓

RiskApprovedEvent

    ↓

ExecutionRequestEvent

    ↓

OrderIntentEvent

    ↓

OrderCreatedEvent

    ↓

OrderSubmittedEvent

    ↓

OrderAcceptedEvent

    ↓

FillEvent

    ↓

PositionUpdatedEvent

    ↓

BalanceUpdatedEvent

    ↓

Metrics
```

---

## 17. Mandatory Rules

```text
Strategy
    └── ONLY SignalEvent

Portfolio
    └── ONLY TargetPositionEvent

Risk
    └── Approve / Reject

Execution
    └── Generate OrderIntent

OMS
    └── Manage Order Lifecycle

Broker
    └── Send Order

Accounting
    └── Update Position & PnL
```

禁止：

```text
Strategy → Broker

Strategy → OMS

Strategy → SQLite

Strategy → Order
```

唯一合法路径：

```text
Strategy
 → Portfolio
 → Risk
 → Execution
 → OMS
 → Broker
```
