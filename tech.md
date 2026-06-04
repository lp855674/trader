# Trader 技术说明

本文是根目录技术摘要，和 `docs/` 下的新设计稿保持一致。详细设计以 `docs/README.md` 中列出的专题文档为准。

## 产品目标

Trader 是一个 Rust 量化交易系统，目标是构建支持多市场、多运行模式、可扩展、可回测、可回放、可模拟交易、可实盘交易的统一交易平台。

第一阶段覆盖：

| 维度 | 范围 |
| --- | --- |
| 市场 | A股、港股、美股、数字货币 |
| 资产 | 股票、数字货币现货、永续合约、交割合约 |
| 运行模式 | Backtest、Replay、Paper、Live |
| 存储 | SQLite 运行状态与交易台账；Parquet 历史行情与研究数据 |
| 控制方式 | CLI、REST API、WebSocket API |

## 设计文档入口

- `docs/architecture.md`：总体目标、核心原则、分层架构、跨模块边界、V1 范围。
- `docs/crates.md`：Rust workspace、crate 职责、依赖方向、feature flags、测试策略。
- `docs/database.md`：SQLite 表、Parquet schema、repository、migration、状态恢复。
- `docs/api.md`：REST / WebSocket 端点、消息格式、错误码、安全设计。
- `docs/events.md`：Event envelope、事件分类、事件流、事件持久化。
- `docs/strategy.md`：Strategy trait、StrategyContext、信号模型、策略边界。
- `docs/broker.md`：Broker trait、路由、订单/成交/持仓映射、回报、重连、限流。
- `docs/roadmap.md`：阶段目标、MVP 范围、发布计划。

## Workspace 结构

目标 workspace：

```text
Trader/
├── apps/
│   ├── trader-cli/
│   └── trader-server/
├── crates/
│   ├── core/
│   ├── events/
│   ├── config/
│   ├── storage/
│   ├── data/
│   ├── market_rules/
│   ├── universe/
│   ├── alpha/
│   ├── portfolio/
│   ├── risk/
│   ├── execution/
│   ├── oms/
│   ├── broker/
│   ├── backtest/
│   ├── replay/
│   ├── accounting/
│   ├── metrics/
│   ├── api/
│   ├── indicators/
│   ├── feature_store/
│   └── strategies/
├── configs/
├── migrations/
├── datasets/
├── docs/
└── scripts/
```

应用层只包含：

- `apps/trader-cli`：本地运维、数据导入、数据库迁移、回测、Replay、报告、配置检查。
- `apps/trader-server`：HTTP / WebSocket 服务，加载配置，初始化 storage、event bus、runtime、broker、market data adapter。

## 核心模块边界

| 模块 | 职责 |
| --- | --- |
| `crates/core` (`trader_core`) | 领域类型：Market、AssetClass、Symbol、Security、Order、Fill、Position、Money、Error。目录沿用设计稿，crate 名避开 Rust 标准库 `core`。 |
| `events` | Event envelope、事件枚举、发布订阅接口、事件持久化边界。 |
| `config` | TOML / 环境变量配置、runtime config、strategy config、broker config。 |
| `storage` | SQLite、Parquet、repository、migration、数据读写。 |
| `data` | 历史数据、实时数据、MarketSlice、K线、tick、order book。 |
| `market_rules` | A股、港股、美股、数字货币交易规则与校验。 |
| `universe` | 标的池选择模型。 |
| `alpha` / `strategies` | 信号生成与示例策略。 |
| `portfolio` | 信号到目标仓位。 |
| `risk` | 目标仓位和订单意图的最终风险检查。 |
| `execution` | 目标仓位到订单意图，支持立即执行、TWAP、VWAP 等模型。 |
| `oms` | 订单状态机、client order id、broker order id 映射、恢复与同步。 |
| `broker` | 券商/交易所通道，发送订单、撤单、查询、接收回报。 |
| `accounting` | 现金、持仓、PnL、费用、保证金、组合账本。 |
| `metrics` | 收益、回撤、Sharpe、胜率、换手、成交质量。 |
| `backtest` | 历史回测 runtime、模拟时钟、成交模型、报告。 |
| `replay` | 历史行情实时回放、暂停、恢复、跳转、倍速。 |
| `api` | REST / WebSocket router、command handler、query handler、event broadcast。 |

## 核心运行链路

```text
User / Operator
  -> CLI / REST API / WebSocket API
  -> Runtime Manager
  -> BacktestRuntime / ReplayRuntime / PaperRuntime / LiveRuntime
  -> Event Bus
  -> Algorithm Framework
  -> OMS
  -> Broker Adapter
  -> SQLite / Parquet
```

Algorithm Framework 顺序：

```text
Universe Selection
  -> Alpha / Strategy
  -> Portfolio Construction
  -> Market Rule Validation
  -> Risk Management
  -> Execution Model
  -> OMS
```

## 强约束

- Strategy 只产生 Signal / Insight，不直接访问 Broker、OMS、SQLite、WebSocket 或 Exchange API。
- 同一个策略必须能运行在 Backtest、Replay、Paper、Live 中；运行模式差异由 runtime 和 adapter 承担。
- 所有订单必须经过 Market Rule、Risk、Execution、OMS，再到 Broker。
- Broker 只负责交易通道，不负责风控、仓位管理、策略逻辑、订单拆分或 PnL。
- SQLite 是交易状态和运行状态真源；Parquet 是历史行情和研究数据真源。
- API 不直接暴露数据库，不绕过 OMS 下单，不绕过 Risk 控制。
- 文档单一来源按 `docs/README.md` 执行：API、DB、事件、crate 职责不要在多个文件重复维护。

## 技术栈

- Rust workspace，resolver 2。
- async runtime：Tokio。
- HTTP / WebSocket：Axum、tower、tower-http。
- 序列化：serde、serde_json、toml。
- 错误处理：thiserror、anyhow。
- 时间与 ID：chrono / time、uuid。
- 金额与数量：rust_decimal。
- 结构化日志：tracing、tracing-subscriber。
- SQLite：sqlx。
- 历史数据：Apache Arrow / Parquet、Polars。
- CLI：clap。
- HTTP / WS 客户端：reqwest、tokio-tungstenite。

## V1 当前状态

当前分支已实现 `docs/architecture.md` 中的 V1 本地可验证功能集：

- CLI：`check-config`、`migrate`、`import-bars`、`backtest`、`paper-run`、`replay`、`report --format text|csv|html`。
- Storage：SQLite 交易状态与事件持久化；Parquet K 线读写边界。
- Runtime：Backtest、Replay、Paper、Live surface；Live 使用本地 fake broker，不连接真实券商。
- API：REST 查询与运行控制、Replay 控制、Broker status、Live start/status/stop；WebSocket 事件 replay 与 Replay 控制。
- Core chain：Strategy Registry/Context、MarketRules、Risk、Execution intents、OMS、Broker fake adapters、Accounting、Metrics。
- Verification：`scripts/v1-smoke.ps1` 覆盖 CLI、REST、WebSocket、SQLite、Parquet、Replay control、CSV/HTML report、fake broker/live surface。

V1 当前仍明确不包含：

- 真实资金实盘交易。
- 真实 Futu/Binance/OKX/IB 网络连接、凭证、下单。
- 分布式部署、Kafka/NATS、SOR、完整机构级执行算法。
- 多用户权限、生产级鉴权、生产监控告警。

完整本地验证命令：

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\v1-smoke.ps1
```

## Production Paper Prep

当前分支开始进入生产前 paper 验证准备阶段。该阶段目标是让 paper run 使用真实配置、真实运行控制、真实持久化和可审计报告，但仍不连接真实券商网络、不发送真实资金订单。

配置真源已扩展为：

- `[risk]`：`max_order_notional`、`min_cash_after_order`、`max_exposure`、`max_drawdown`、`max_leverage`、`max_margin_used`、`trading_halted`。
- `[broker]`：`kind` 与 `mode`。当前支持配置枚举 `simulated`、`futu`、`binance`、`okx`、`interactive_brokers`；`mode` 支持 `paper`、`live`。
- `[live]`：`enabled` 与可选 `heartbeat_ms`。

CLI 与 REST 的 paper/backtest settings 从 `[risk]` 读取风控阈值，不再使用隐藏硬编码风控默认值。REST live surface 的 broker kind 从 `[broker]` 读取；当前仍使用本地 fake broker adapter。

`PaperRuntime` 现在提供两类入口：

- `run_bars` / `run_bars_with_cancel`：一次性输入历史 bars，保持 V1 本地验证路径。
- `run_bar_stream_with_cancel`：从 channel-based bar stream 顺序消费 bars，复用同一套 Strategy、Portfolio、MarketRules、Risk、OMS、Broker simulation、Accounting、Storage 处理逻辑，并支持 pacing 与取消。

Broker fake adapters 现在提供 paper 测试 surface：`place_order`、`query_order`、`cancel_order`、`account_snapshot`、`status`。REST 已提供 `GET /api/v1/brokers/account/{account_id}` 返回配置 broker kind 对应的 fake account snapshot；仍不提供绕过 Runtime/OMS 的手动下单 API。

REST 也提供 `GET /api/v1/preflight/paper`，用于在 server 运行时检查当前配置是否满足本地 paper 验证条件。本地 simulated paper 返回 `real_broker_connection=false`；Binance Spot Testnet paper 在 testnet base_url 与凭证环境变量检查通过后返回 `real_broker_connection=true`。

Binance testnet adapter 已开始接入。当前支持 `ping`、signed account snapshot，以及手动 tiny limit order -> query -> cancel -> myTrades sync -> local accounting snapshot；也已提供受闸门保护的策略自动 Binance Spot Testnet executor。`[broker] order_submit_enabled` 是策略自动送单闸门，默认 `false`；打开后 `paper-run` 只在 `broker.kind = "binance"`、`broker.mode = "paper"`、Spot Testnet `base_url` 和环境变量凭证都满足时才会提交 testnet limit order。该 executor 只把 Binance `myTrades` 返回的真实成交写入本地 fill；如果订单未成交或没有 trades，run 会失败，不会伪造成交。凭证只从环境变量读取：

```powershell
$env:BINANCE_TESTNET_API_KEY = "..."
$env:BINANCE_TESTNET_SECRET_KEY = "..."
trader paper-preflight --config configs/paper/binance_testnet.toml
trader binance-paper-readonly --config configs/paper/binance_testnet.toml
```

当 `[broker] kind = "binance"` 且 `mode = "paper"` 时，`paper-preflight` 会要求 Spot Testnet base_url 和 `BINANCE_TESTNET_API_KEY` / `BINANCE_TESTNET_SECRET_KEY` 存在；通过后输出 `real_broker_connection=true`。该检查不访问网络，网络连接仍由 `binance-paper-readonly` 验证。

只读 smoke：

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\binance-paper-smoke.ps1
```

该脚本会使用临时 SQLite 且不会下单；如当前环境不能访问外网，可追加 `-SkipNetwork` 只跑配置、凭证和 SQLite migration 检查。

手动 tiny order/cancel：

```powershell
trader binance-paper-tiny-order --config configs/paper/binance_testnet.toml --symbol BTCUSDT --side buy --qty 0.001 --price 100000 --confirm-testnet-order
```

该命令会把 testnet order 写入 SQLite 的 `strategy_runs`、`orders` 和 `event_store`，并把 Binance `myTrades` 返回的成交明细写入 `fills`，再基于当前 run 已持久化 fills 更新 `account_balances`、`positions` 和 `portfolio_snapshots`。如果订单立即成交导致 cancel 返回 `Unknown order sent`，流程会保留最终订单状态并把 cancel 错误写入事件。

策略自动 testnet order 当前复用 `trader paper-run --config ...`，但必须显式把目标配置中的 `[broker] order_submit_enabled = true`。执行前必须确认行情数据价格与 Binance 当前价格保护范围一致；例如 `configs/paper/binance_testnet.toml` 目前仍使用本地样例 CSV 路径，不应直接开闸作为真实 BTCUSDT 行情源。

当前 paper 验证命令：

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\paper-smoke.ps1
```

该脚本会创建临时配置和 SQLite，启动真实 `trader-server`，执行 paper run，并验证 run status、orders、fills、account balances、portfolio snapshots、metrics、events 和 broker account snapshot。

CLI 也提供独立 preflight：

```powershell
trader paper-preflight --config configs/backtest/slow-paper.toml
```

该命令会校验 runtime mode、broker mode、risk decimal 参数、SQLite 可连接性和行情源可读取性，并输出 run id、strategy、symbol、bars、database、broker、account 与关键 risk limit。

## V1 交付范围

V1 优先完成：

- workspace 与基础 crate 骨架。
- `core` 领域类型。
- `events` 事件总线与事件类型。
- `storage` SQLite migration、repository、Parquet 读取边界。
- `data` 历史数据加载。
- `strategy` / `alpha` / `portfolio` / `risk` / `execution` / `oms` / `broker` 的最小闭环。
- `backtest` 可运行单策略历史回测。
- `replay` 可按倍速播放历史行情。
- `paper` 路径通过 mock broker 模拟成交。
- `api` 提供运行控制、订单、成交、持仓、账户、绩效查询和 WebSocket 推送。
- `trader-cli` 提供 init、migrate、import、backtest、replay、report、check-config。

V1 不做：

- Qlib 在线集成。
- 高频 order book 完整撮合。
- 分布式部署。
- 多用户权限系统。
- 期权、传统期货、外汇。
- 复杂实盘券商矩阵。

## Phase 2 Paper MVP

Phase 2 turns the V1 skeleton into a local paper/backtest workflow: config loading, SQLite persistence, CSV bar loading, persistent backtest output, CLI commands, and REST query routes.

当前可执行链路：

- `trader check-config --config configs/backtest/ma_cross.toml` 校验配置文件。
- `trader migrate --config configs/backtest/ma_cross.toml` 创建 SQLite schema。
- `trader backtest --config configs/backtest/ma_cross.toml` 加载样例 CSV，运行 MA cross，持久化 run、order、fill、position。
- `trader-server` 提供 `POST /api/v1/backtests`、`GET /api/v1/orders`、`GET /api/v1/positions`。

仍然保持的边界：

- Strategy 只产生信号，不访问 Broker、OMS、Storage 或 API。
- SQL 只在 `storage` crate 内部。

## Phase 3 Paper Runtime

Phase 3 将 paper 从 backtest wrapper 拆成独立 runtime。当前 `PaperRuntime` 自己执行 strategy loop，并串联 portfolio、risk、execution、simulated broker、accounting、storage。

当前可执行链路：

- `trader paper-run --config configs/backtest/ma_cross.toml` 加载样例 CSV，运行 MA cross paper loop，持久化 run、order、fill、position、account balance、portfolio snapshot。
- `POST /api/v1/backtests` 触发 backtest 流程。
- `POST /api/v1/paper-runs` 触发本地 paper 持久化流程，用于后续查询路由 smoke。
- `GET /api/v1/fills` 查询成交。
- `GET /api/v1/account-balances` 查询账户现金余额。
- `GET /api/v1/portfolio/snapshots` 查询组合权益快照。
- `GET /api/v1/metrics` 基于订单、成交和首尾权益快照返回 metrics summary。
- `GET /api/v1/runs` 和 `GET /api/v1/runs/{run_id}` 查询运行记录。

仍然保持的边界：

- Strategy 只产生信号，不访问 Broker、OMS、Storage 或 API。
- SQL 只在 `storage` crate 内部。
- Paper runtime 使用 `broker::simulate_market_fill` 生成本地模拟成交，账户现金与权益由 `accounting::AccountBook` 维护。
- 金额/数量在 Rust 内使用 `rust_decimal::Decimal`，写入 SQLite 时使用 decimal string。

## Phase 4 Paper Production

Phase 4 将本地 paper workflow 进一步生产化：

- `PaperRuntime` 使用 `paper::PaperSettings`，不再借用 `BacktestSettings` 作为配置载体。
- `PaperSettings` 从 `AppConfig` 构造，使用配置文件中的 initial cash、base currency、slippage bps、fee bps、strategy windows、order qty 和 max position。
- `[paper]` 配置提供 `account_id`、`slippage_bps`、`fee_bps`。
- `accounting::AccountBook` 明确区分 `buy` 与 `sell`，卖出会更新 cash、position 和 realized PnL。
- paper portfolio snapshot 持久化 realized PnL 与 unrealized PnL。
- REST 使用明确的 `POST /api/v1/paper-runs` 触发 paper，`POST /api/v1/backtests` 保留给 backtest。
- REST 增加 `GET /api/v1/runs` 与 `GET /api/v1/runs/{run_id}` 查询运行记录。
- `scripts/rest-smoke.ps1` 用于验证运行中的 server：health、paper-runs、fills、account-balances、portfolio snapshots、metrics。

## Phase 5 Runtime Control

Phase 5 增加最小运行控制面和 server smoke：

- `strategy_runs` 持久化 `status`、`ended_at_ms` 和 `error`；旧 SQLite 库在 `migrate()` 时会幂等补齐 `error` 列。
- 当前 run lifecycle 状态为 `running`、`completed`、`failed`、`cancelled`。
- `POST /api/v1/paper-runs` 会先写入 `running`；执行成功由 `PaperRuntime` 写入 `completed`；配置已解析后发生的数据加载或 runtime 错误会更新为 `failed` 并保存错误文本。
- `GET /api/v1/runs/{run_id}/status` 返回 `{ run_id, status, error }`，run 不存在时返回 `404`。
- `POST /api/v1/runs/{run_id}/cancel` 将已存在 run 标记为 `cancelled` 并设置 `ended_at_ms`；当前 runtime 仍是同步短任务，cancel 是持久化状态控制，不中断已经在同一请求内执行完成的计算。
- `scripts/server-smoke.ps1` 使用临时 Cargo target directory 和临时 SQLite 数据库，启动真实 `trader-server` 后执行 REST smoke。

## Phase 6 Runtime Manager

Phase 6 introduces `crates/runtime` as the in-memory active run registry. API starts paper runs in background tasks, persists `running`, and returns immediately with `{ run_id, status }`. `RuntimeManager` owns cancellation flags for active tasks; `PaperRuntime` checks the flag between bars and after optional pacing delay. Cancellation is now best-effort active cancellation for running paper jobs, not just a database status override.

当前状态仍是本地 MVP vertical slice，不代表 roadmap 中的分布式 Phase 6 已完成。

## MVP Core Rules

当前 MVP 订单链路按 `Strategy -> Portfolio -> Execution delta -> MarketRules -> Risk -> OMS -> Broker -> Accounting -> Storage` 执行。MarketRules 校验 lot size、tick size、min qty、min notional；Risk 校验 max order qty、max order notional、cash buffer 和 trading halt；OMS 跟踪订单状态、累计成交和剩余数量。

## Local Verifiable MVP

当前分支的 MVP 完成标准是“本地可实际验证的交易闭环”，不是完整实盘交易平台。可验证闭环包括：

- CLI：`check-config`、`migrate`、`backtest`、`paper-run`、`replay`、`report`。
- REST：`health`、`backtests`、`paper-runs`、`replays`、`orders`、`fills`、`positions`、`account-balances`、`portfolio/snapshots`、`metrics`、`runs`、`events`。
- Storage：SQLite 持久化 run、order、fill、position、account balance、portfolio snapshot、event store。
- Core path：paper runtime 串联 Strategy、Portfolio、Execution delta、MarketRules、Risk、OMS、Simulated Broker、Accounting、Storage。
- Replay：从 CSV 加载历史 K 线并返回 replay bar summary。
- Report：从 SQLite 读取真实持久化结果，输出 run status、orders、fills、balances、snapshots、total return。
- Audit events：backtest、paper、replay lifecycle 写入 `event_store`，并可通过 REST 查询。

本地完整验证命令：

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\mvp-smoke.ps1
```

该脚本会创建临时 config 与 SQLite，依次执行 CLI 全链路，然后启动真实 `trader-server` 并执行 REST smoke。通过时会输出 run id、status、fills、balances、snapshots、total_return、replay_bars、events、run_events。

当前 MVP 明确不包含：

- 真实 broker/live adapter。
- 完整 WebSocket streaming。
- 多市场完整规则矩阵。
- Parquet 研究流水线。
- 分布式 runtime、多用户权限、生产级鉴权。

## 实施计划

完整执行计划见：

- `docs/superpowers/plans/2026-05-31-trader-v1-implementation.md`
- `docs/superpowers/plans/2026-06-01-trader-paper-mvp-plan.md`
- `docs/superpowers/plans/2026-06-02-trader-paper-runtime-plan.md`
- `docs/superpowers/plans/2026-06-02-trader-paper-production-plan.md`
- `docs/superpowers/plans/2026-06-02-trader-runtime-control-plan.md`
- `docs/superpowers/plans/2026-06-02-trader-runtime-manager-plan.md`
- `docs/superpowers/plans/2026-06-02-trader-local-mvp-completion-plan.md`
- `docs/superpowers/plans/2026-06-04-trader-production-paper-prep-plan.md`
