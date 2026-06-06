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

Binance signed 请求不依赖本机时钟直接签名；adapter 会先读取 Spot Testnet `/v3/time` 的 `serverTime`，再用于 account、order、cancel、myTrades 等 signed endpoint，避免本机时间漂移触发 Binance `code=-1021`。

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

`[broker] order_submit_enabled` 是策略自动送单闸门，默认必须为 `false`。当该字段为 `true` 时，`paper-run` 只允许 `broker.kind = "binance"`、`broker.mode = "paper"`、Spot Testnet `base_url` 与 `BINANCE_TESTNET_API_KEY` / `BINANCE_TESTNET_SECRET_KEY` 同时满足；否则拒绝启动。自动 Binance paper run 启动时会读取 Binance account snapshot，并用 USDT cash 覆盖本次 `PaperSettings.initial_cash`。runtime 会在调用 broker executor 前先持久化一条 `SUBMITTED` pending order，写入稳定的 `client_order_id = trader-paper-{run_id_prefix}-{order_number}`。executor 每次执行前会先通过 Binance `origClientOrderId` 查询已存在订单，查到则同步该订单 `myTrades`，查不到才提交新 testnet order。executor 只根据 Binance `myTrades` 聚合成交价格、数量与 fee；没有真实 trades 时会先尝试撤销仍 open 的 testnet order，然后以 0 filled qty 更新订单状态，不写入 fill、不更新本地账本，也不会伪造成交。

注意：自动策略送单使用 bar close 作为 limit price。执行前必须确认数据源价格与 Binance 当前价格保护范围一致，否则 Binance 会因价格过滤拒单。当前 `configs/paper/binance_testnet.toml` 仍指向本地样例 CSV，不应直接开闸作为 BTCUSDT 实际行情源。

真实 BTCUSDT K 线可通过 Binance Spot Testnet 公共 REST 拉取，默认写成 Parquet：

```powershell
trader binance-paper-klines --config configs/paper/binance_btcusdt_1m_parquet.toml --symbol BTCUSDT --interval 1m --limit 100 --format parquet --output datasets/binance/btcusdt_1m.parquet
powershell -ExecutionPolicy Bypass -File .\scripts\binance-refresh-klines.ps1 -Limit 100
```

正式配置 `configs/paper/binance_btcusdt_1m_parquet.toml` 固定使用 `[data] source = "parquet"` 与 `datasets/binance/btcusdt_1m.parquet`；`scripts/binance-refresh-klines.ps1` 只刷新数据并执行 preflight，不运行策略、不下单。Parquet 使用现有 `data::write_bars_to_parquet` / Polars 写入，字段为 `ts_ms,open,high,low,close,volume`。CSV 仅作为兼容格式，需显式加 `--format csv`。对应 smoke 默认走 Parquet：

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\binance-paper-klines-smoke.ps1
```

真实行情 runner：

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\binance-paper-real-run.ps1 -Limit 100
powershell -ExecutionPolicy Bypass -File .\scripts\binance-paper-real-run.ps1 -Limit 100 -RunPaper
powershell -ExecutionPolicy Bypass -File .\scripts\binance-paper-run.ps1 -Limit 1000
```

`binance-paper-real-run.ps1` 使用临时 config/DB，适合 smoke。`binance-paper-run.ps1` 使用正式 Parquet 配置刷新 `datasets/binance/btcusdt_1m.parquet`，并为每次运行在 `data/binance-paper-runs/{run_id}/` 生成独立 `config.toml`、`run.sqlite`、`report.txt`、`report.csv` 和 `report.html`，执行 paper-run、report、recover 和 open order 巡检。两者默认都不下单；只有追加 `-ConfirmTestnetOrder` 时才会打开 Binance Spot Testnet 策略送单。`binance-paper-run.ps1` 开闸送单时禁止同时使用 `-SkipRefresh`，并会读取一次 Spot Testnet ticker price 写入运行输出，避免用旧 Parquet 数据直接送单；如果 testnet paper-run 因 broker 错误失败，脚本会先 best-effort 执行 recover 与 open order 巡检，再保留原始失败。

`binance-paper-run.ps1` 成功完成后还会运行只读对账命令，并写入 `summary.json`。该文件记录 run id、配置、SQLite、Parquet、report 路径、ticker price、order_submit 状态、recover/open-orders 输出和 reconciliation 输出。只读对账命令：

```powershell
trader binance-paper-reconcile --config configs/paper/binance_btcusdt_1m_parquet.toml --symbol BTCUSDT
```

该命令读取 Binance Spot Testnet account balances 与 open orders，并和当前 run 的本地 SQLite orders、fills、account_balances、positions 对比；不会下单、撤单或修改本地状态。

Paper runtime 会为自动订单写入订单生命周期事件：`paper.order.submitted`、`paper.order.filled`、`paper.order.unfilled`。事件 source 为 run id，payload 包含本地 order id、client order id、broker order id、symbol、side、qty、filled_qty 和最终 status，用于后续 WebSocket replay 与审计排查。

Binance soak 验证脚本用于多轮执行固定 runner，并汇总每轮 transcript、summary.json、open order 巡检和 reconciliation：

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\binance-paper-soak.ps1 -Iterations 3 -Limit 100
powershell -ExecutionPolicy Bypass -File .\scripts\binance-paper-soak.ps1 -Iterations 3 -Limit 100 -ConfirmTestnetOrder
```

该脚本默认不下单；只有 `-ConfirmTestnetOrder` 会打开 Binance Spot Testnet 策略送单。任一轮失败或 `open_orders != 0` 都会让 soak 失败，并在 `data/binance-paper-soak/{soak_id}/summary.json` 保留证据。

自动策略送单 smoke 可用：

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\binance-paper-auto-smoke.ps1 -ConfirmTestnetOrder
```

该脚本读取 Binance Spot Testnet 当前 BTCUSDT ticker，生成临时 BTCUSDT bars、临时配置和临时 SQLite，然后打开 `order_submit_enabled = true` 执行 `paper-run`。完成 report 后会查询 BTCUSDT open orders，确认没有遗留挂单。没有 `-ConfirmTestnetOrder` 时会拒绝执行。

pending order 恢复命令：

```powershell
trader binance-paper-recover --config configs/paper/binance_testnet.toml
```

该命令不会提交新订单。它扫描当前 run 的 `SUBMITTED` / `NEW` / `PARTIALLY_FILLED` 本地订单，用 `client_order_id` 调 Binance `origClientOrderId` 查询订单；查到后同步 `myTrades`，更新本地订单执行状态，并刷新 account balance、position 和 portfolio snapshot。输出中的 `remaining` 是恢复后仍需继续跟踪的本地订单数；如果本次扫描过订单、没有 missing、且 `remaining=0`，非 completed 的 strategy run 会被标记为 `recovered`。

恢复 smoke 可用：

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\binance-paper-recover-smoke.ps1
```

该脚本使用临时配置和临时 SQLite，执行 `check-config`、`paper-preflight`、`migrate` 与 `binance-paper-recover`。它不会打开 `order_submit_enabled`，也不会提交新订单；无网络环境可追加 `-SkipNetwork` 只验证配置和 migration。

open order 巡检命令：

```powershell
trader binance-paper-open-orders --config configs/paper/binance_testnet.toml --symbol BTCUSDT
```

该命令只查询 Binance Spot Testnet 当前 symbol 的 open orders，不会下单或撤单。对应 smoke：

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\binance-paper-open-orders-smoke.ps1
```

如确认需要清理 testnet 挂单，必须显式加确认开关：

```powershell
trader binance-paper-cancel-open-orders --config configs/paper/binance_testnet.toml --symbol BTCUSDT --confirm-testnet-cancel
```

清理命令会先查询远端 open orders，逐个撤销成功后按 `run_id + client_order_id` 同步当前配置 SQLite 中匹配订单的 `broker_order_id`、`status` 与 `updated_at_ms`，输出 `local_synced` 作为本地同步行数。

### IBKR 股票 Paper

股票 paper 方向固定为 IBKR。当前新增 IBKR AAPL Parquet 本地 paper runner，用来验证股票链路的配置、Parquet 数据、SQLite、paper runtime 和报告归档；该 runner 默认不连接 IBKR TWS / Gateway，也不提交 IBKR paper 订单：

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\ibkr-paper-run.ps1
```

固定配置为 `configs/paper/ibkr_aapl_1d_parquet.toml`，使用 `[broker] kind = "ibkr"`、`mode = "paper"`、`host = "127.0.0.1"`、`port = 7497`、`client_id = 1`、`order_submit_enabled = false`，行情文件为 `datasets/ibkr/aapl_1d.parquet`。脚本会把 `datasets/sample/aapl_1d.csv` 转成 Parquet 作为本地验证输入，并为每次运行在 `data/ibkr-paper-runs/{run_id}/` 生成独立 `config.toml`、`run.sqlite`、`report.txt`、`report.csv` 和 `report.html`。

IBKR read-only preflight：

```powershell
trader ibkr-paper-readonly --config configs/paper/ibkr_aapl_1d_parquet.toml
trader ibkr-paper-open-orders --config configs/paper/ibkr_aapl_1d_parquet.toml
trader ibkr-paper-executions --config configs/paper/ibkr_aapl_1d_parquet.toml --request-id 1
trader ibkr-paper-next-order-id --config configs/paper/ibkr_aapl_1d_parquet.toml
trader ibkr-paper-cancel-order --config configs/paper/ibkr_aapl_1d_parquet.toml --order-id 42 --confirm-ibkr-paper-cancel
```

该命令通过 `broker::IbkrPaperGatewayAdapter` 连接本机 TWS / IB Gateway，完成 server version 握手，然后发送 managed accounts 只读请求并校验 `[paper] account_id` 是否在 Gateway 返回账号列表中。默认 paper 端口为 `7497`；如果本机没有启动 TWS / Gateway，命令会以 `unable to connect to IBKR paper gateway` 失败。`client_id` 用于 TWS API socket session。`[paper] account_id` 必须改为 TWS / Gateway 返回的真实 paper account id（通常是 `DU...`）；配置中的 `DU000000` 只是结构化占位，不是可用账号。

`ibkr-paper-open-orders` 发送只读 open orders 请求并输出远端 open order 数量和首条订单关键字段。`ibkr-paper-executions` 使用 `[paper] account_id` 和策略 symbol 发送 executions 查询并输出成交数量和首条成交关键字段；可用 `--symbol AAPL` 覆盖策略 symbol。`ibkr-paper-next-order-id` 读取 Gateway 返回的 next valid order id，为后续真实下单 adapter 做前置验证。这些命令只读取 Gateway 数据，不写 SQLite，不提交或撤销订单。

`ibkr-paper-cancel-order` 会向 TWS / IB Gateway 发送真实 paper cancel 请求，必须显式传 `--confirm-ibkr-paper-cancel`，并只输出 Gateway 返回的 `orderStatus`。该命令不提交新订单，不写 SQLite。

真实 IBKR paper order adapter 完成前，`order_submit_enabled` 必须保持 `false`。如果误设为 `true`，`paper-preflight` 和 `paper-run` 都会拒绝继续，避免把本地股票 paper runner 误当成真实 IBKR paper 下单能力。

IBKR paper order adapter 当前已完成可测试接口边界：

```text
query_order_by_client_order_id
place_limit_order
query_order
cancel_order
executions
```

`IbkrPaperOrderExecutor` 只聚合 `executions` 作为真实成交来源；如果订单没有 executions 且仍处于 open 状态，会先撤单并返回 0 filled qty，不写 fill、不更新本地账本。该 executor 当前只通过测试 client 验证，尚未接真实 TWS / IB Gateway API，也未允许 CLI runner 提交 IBKR paper order。

IBKR TWS API wire codec 当前已在 `broker` crate 内实现：

```text
client version handshake: API\0 + length-prefixed v{min}..{max}
message frame: 4-byte big-endian length + NUL-separated fields
server version parse: server_version + connection_time
managed accounts: request id 17, response id 15
open orders: request id 5, open order response id 5, end id 53
executions: request id 7, execution response id 11, end id 55
next valid id: request id 8, response id 9
cancel order: request id 4, orderStatus response id 3
```

这一步已接入 socket session，完成 TWS / Gateway server version 握手、managed accounts 读取、open orders 读取、executions 读取、next valid order id 读取，以及受确认保护的 paper cancel。仍不发送真实下单消息。下一步继续实现 IBKR place order 的真实 Gateway adapter，并在 CLI runner 中加显式确认闸门。

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
