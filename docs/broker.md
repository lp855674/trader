# broker.md

## 1. Overview

Broker 模块负责：

```text
OMS
  ↓
Broker
  ↓
Exchange / Broker API
```

职责：

* 订单发送
* 撤单
* 查询订单
* 查询成交
* 查询持仓
* 查询账户
* 接收回报
* 统一不同券商接口

Broker 不负责：

* 风控
* 仓位管理
* 策略逻辑
* 订单拆分
* PnL计算

这些职责属于：

```text
Risk
Portfolio
Execution
Accounting
```

---

## 2. Architecture

```text
OMS
 │
 ▼
Broker Router
 │
 ├── CTP Broker
 ├── Futu Broker
 ├── IB Broker
 ├── Binance Broker
 ├── OKX Broker
 └── Mock Broker
```

Broker Router 根据：

```text
Account
Market
AssetType
```

路由到具体实现。

---

## 3. Supported Markets

### CN A股

```text
Futu
QMT
XtQuant
```

---

### HK 港股

```text
Futu
Interactive Brokers
```

---

### US 美股

```text
Interactive Brokers
Futu
Alpaca
```

---

### CRYPTO

```text
Binance
OKX
Bybit
```

---

## 4. Broker Trait

统一抽象。

```rust
pub trait Broker: Send + Sync {

    async fn connect(&self) -> Result<()>;

    async fn disconnect(&self) -> Result<()>;

    async fn place_order(
        &self,
        req: PlaceOrderRequest,
    ) -> Result<PlaceOrderResponse>;

    async fn cancel_order(
        &self,
        order_id: &str,
    ) -> Result<()>;

    async fn get_order(
        &self,
        order_id: &str,
    ) -> Result<Order>;

    async fn get_positions(
        &self,
    ) -> Result<Vec<Position>>;

    async fn get_account(
        &self,
    ) -> Result<AccountSnapshot>;
}
```

---

## 5. Broker Capability

不同券商能力不同。

```rust
pub struct BrokerCapability {
    pub market_order: bool,

    pub limit_order: bool,

    pub stop_order: bool,

    pub trailing_stop: bool,

    pub short_sell: bool,

    pub margin: bool,

    pub option: bool,

    pub future: bool,
}
```

---

## 6. Account Model

```rust
pub struct BrokerAccount {
    pub account_id: String,

    pub broker: String,

    pub currency: String,

    pub market: String,
}
```

示例：

```text
HK_STOCK
US_STOCK
CN_STOCK
CRYPTO
```

---

## 7. Place Order Request

```rust
pub struct PlaceOrderRequest {
    pub symbol: String,

    pub side: OrderSide,

    pub order_type: OrderType,

    pub qty: Decimal,

    pub price: Option<Decimal>,

    pub account_id: String,
}
```

---

## 8. Place Order Response

```rust
pub struct PlaceOrderResponse {
    pub broker_order_id: String,

    pub accepted: bool,

    pub reason: Option<String>,
}
```

---

## 9. Order Status Mapping

内部统一状态。

```rust
pub enum OrderStatus {
    Pending,
    Submitted,
    Accepted,
    PartiallyFilled,
    Filled,
    CancelPending,
    Canceled,
    Rejected,
}
```

---

券商状态映射：

```text
Broker Status
      ↓
OrderStatus
```

例如：

```text
NEW
ACCEPTED

PARTIALLY_FILLED
PARTIALLY_FILLED

FILLED
FILLED

CANCELLED
CANCELED

REJECTED
REJECTED
```

---

## 10. Fill Mapping

所有成交统一转换。

```rust
pub struct BrokerFill {
    pub trade_id: String,

    pub order_id: String,

    pub symbol: String,

    pub qty: Decimal,

    pub price: Decimal,

    pub fee: Decimal,

    pub ts: DateTime<Utc>,
}
```

转换后发布：

```text
FillEvent
```

---

## 11. Position Mapping

```rust
pub struct BrokerPosition {
    pub symbol: String,

    pub qty: Decimal,

    pub avg_price: Decimal,

    pub market_value: Decimal,

    pub unrealized_pnl: Decimal,
}
```

---

## 12. Account Snapshot

```rust
pub struct AccountSnapshot {
    pub cash: Decimal,

    pub equity: Decimal,

    pub buying_power: Decimal,

    pub margin_used: Decimal,

    pub unrealized_pnl: Decimal,

    pub realized_pnl: Decimal,
}
```

---

## 13. Market Specific Rules

Broker 不处理交易规则。

例如：

```text
A股 T+1
港股 Lot
美股 Fractional
币圈 Funding
```

全部由：

```text
MarketRule
```

处理。

Broker 只负责发送。

---

## 14. WebSocket Callback

Broker 必须支持回报订阅。

```rust
pub trait BrokerStream {

    async fn subscribe_orders(
        &self,
    );

    async fn subscribe_fills(
        &self,
    );

    async fn subscribe_positions(
        &self,
    );

    async fn subscribe_account(
        &self,
    );
}
```

---

## 15. Broker Events

Broker 接收到回报后发布订单、成交、持仓、账户类事件。事件名称、字段和持久化规则统一维护在 `events.md`，本文只描述 Broker 产生事件的职责边界。

---

## 16. Broker Router

统一路由。

```rust
pub struct BrokerRouter {
    brokers: HashMap<
        String,
        Arc<dyn Broker>
    >,
}
```

---

根据：

```text
account_id
```

选择 Broker。

```rust
router.place_order(
    account_id,
    request
)
```

---

## 17. Reconnect Strategy

支持自动重连。

```text
1s
2s
4s
8s
16s
30s
60s
```

指数退避。

---

## 18. Idempotency

避免重复下单。

OMS 创建：

```text
client_order_id
```

Broker 必须保证：

```text
same client_order_id

→ same order
```

---

## 19. Rate Limit

每个 Broker 维护限流器。

```rust
pub struct RateLimiter {
    permits_per_sec: usize,
}
```

避免：

```text
429
Too Many Requests
```

---

## 20. Mock Broker

用于：

```text
BACKTEST
REPLAY
PAPER
```

支持：

```text
订单撮合

部分成交

滑点

手续费

延迟模拟
```

当前 V1 fake broker adapter 已实现本地 deterministic paper surface：

```text
place_order
query_order
cancel_order
account_snapshot
status
```

该 surface 只保存在进程内存，用于 paper 测试和 API smoke；不连接真实券商网络，也不作为 SQLite 交易状态真源。

Binance testnet 已开始接入 read-only adapter：

```text
base_url: https://testnet.binance.vision/api
read-only: ping, signed account snapshot
manual testnet order: limit order, query order, cancel order
manual sync: order status, executed quantity, myTrades fills, account balance, position, portfolio snapshot into SQLite
disabled: strategy auto-submit, full OMS recovery, full broker account/position reconciliation
credentials: environment variables only
strategy auto-submit gate: broker.order_submit_enabled, default false
```

当前 CLI 入口：

```powershell
$env:BINANCE_TESTNET_API_KEY = "..."
$env:BINANCE_TESTNET_SECRET_KEY = "..."
trader paper-preflight --config configs/paper/binance_testnet.toml
trader binance-paper-readonly --config configs/paper/binance_testnet.toml
```

`paper-preflight` 会在不访问网络的情况下校验 Binance paper config、Spot Testnet base_url 和凭证环境变量是否存在，并输出 `real_broker_connection=true`。`binance-paper-readonly` 用于实际验证 Spot Testnet 连接与账户读取，不会发送订单。

只读 smoke：

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\binance-paper-smoke.ps1
```

该脚本会复制临时 config 并使用临时 SQLite，执行 `check-config`、`paper-preflight`、`migrate` 和 `binance-paper-readonly`，不会发送订单。无网络环境可追加 `-SkipNetwork` 只验证配置、凭证环境变量和 SQLite migration。

手动 tiny order/cancel 入口：

```powershell
trader binance-paper-tiny-order `
  --config configs/paper/binance_testnet.toml `
  --symbol BTCUSDT `
  --side buy `
  --qty 0.001 `
  --price 100000 `
  --confirm-testnet-order
```

该命令会在 Binance Spot Testnet 发送一笔 limit order，随后 query 并 cancel。没有 `--confirm-testnet-order` 时会拒绝执行。价格必须落在 Binance 当前价格保护范围内；如果订单立即成交导致 cancel 返回 `Unknown order sent`，流程会重新 query、同步 `myTrades`，并把 cancel 错误写入审计事件。

执行成功后会写入 SQLite：

```text
strategy_runs: run status completed
orders: broker_order_id、最终 cancel status 与 filled_qty
fills: Binance myTrades 成交明细；没有成交时为空
account_balances: Binance account snapshot 中的 USDT cash
positions / portfolio_snapshots: 基于当前 run 已持久化 fills 的本地累计状态
event_store: binance.testnet_order.started / completed
```

当前已把 manual tiny order 的 Binance `myTrades` 同步为 `fills`，并把当前 run 的已持久化 fills 聚合为本地 position 与 portfolio snapshot。策略自动订单已提供 Binance Spot Testnet executor，但仍未做完整 broker account/position reconciliation。

`[broker] order_submit_enabled` 是策略自动送单闸门，默认必须为 `false`。当该字段为 `true` 时，`paper-run` 只允许 `broker.kind = "binance"`、`broker.mode = "paper"`、Spot Testnet `base_url` 与 `BINANCE_TESTNET_API_KEY` / `BINANCE_TESTNET_SECRET_KEY` 同时满足；否则拒绝启动。自动 Binance paper run 启动时会读取 Binance account snapshot，并用 USDT cash 覆盖本次 `PaperSettings.initial_cash`。runtime 会在调用 broker executor 前先持久化一条 `SUBMITTED` pending order，写入稳定的 `client_order_id = trader-paper-{run_id_prefix}-{order_number}`。executor 每次执行前会先通过 Binance `origClientOrderId` 查询已存在订单，查到则同步该订单 `myTrades`，查不到才提交新 testnet order。executor 只根据 Binance `myTrades` 聚合成交价格、数量与 fee；没有真实 trades 时 run 会失败，不写入伪造成交。

注意：自动策略送单使用 bar close 作为 limit price。执行前必须确认数据源价格与 Binance 当前价格保护范围一致，否则 Binance 会因价格过滤拒单。当前 `configs/paper/binance_testnet.toml` 仍指向本地样例 CSV，不应直接开闸作为 BTCUSDT 实际行情源。

自动策略送单 smoke 可用：

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\binance-paper-auto-smoke.ps1 -ConfirmTestnetOrder
```

该脚本读取 Binance Spot Testnet 当前 BTCUSDT ticker，生成临时 BTCUSDT bars、临时配置和临时 SQLite，然后打开 `order_submit_enabled = true` 执行 `paper-run`。没有 `-ConfirmTestnetOrder` 时会拒绝执行。

pending order 恢复命令：

```powershell
trader binance-paper-recover --config configs/paper/binance_testnet.toml
```

该命令不会提交新订单。它扫描当前 run 的 `SUBMITTED` / `PARTIALLY_FILLED` 本地订单，用 `client_order_id` 调 Binance `origClientOrderId` 查询订单；查到后同步 `myTrades`，更新本地订单执行状态，并刷新 account balance、position 和 portfolio snapshot。

---

## 21. Broker Configuration

```yaml
brokers:

  ib:
    enabled: true

    host: 127.0.0.1

    port: 7497

    client_id: 1

  futu:
    enabled: true

    host: 127.0.0.1

    port: 11111

  binance:
    enabled: true

    api_key: xxx

    secret: xxx
```

---

## 22. Fault Tolerance

Broker 故障不会影响 OMS。

```text
OMS
 ↓
Broker Queue
 ↓
Broker
```

发送失败：

```text
Retry
Dead Letter Queue
Alert
```

---

## 23. Broker Metrics

采集：

```text
order_submit_latency

cancel_latency

fill_latency

ws_disconnect_count

reconnect_count

rejected_order_count

fill_count

broker_error_count
```

---

## 24. Order Lifecycle

```text
OMS

 ↓

OrderCreated

 ↓

Broker.place_order

 ↓

Submitted

 ↓

Accepted

 ↓

PartiallyFilled

 ↓

Filled

 ↓

Accounting

 ↓

PositionUpdated
```

---

## 25. Mandatory Rules

```text
Broker

只负责连接交易通道

只负责发送订单

只负责接收回报

不负责风控

不负责仓位管理

不负责PnL计算
```

禁止：

```text
Broker → Risk

Broker → Portfolio

Broker → Strategy
```

唯一合法路径：

```text
OMS
 ↓
Broker
 ↓
Exchange

Exchange
 ↓
Broker
 ↓
Events
 ↓
Accounting
```
