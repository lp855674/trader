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

## 实施计划

完整执行计划见：

- `docs/superpowers/plans/2026-05-31-trader-v1-implementation.md`
- `docs/superpowers/plans/2026-06-01-trader-paper-mvp-plan.md`
- `docs/superpowers/plans/2026-06-02-trader-paper-runtime-plan.md`
- `docs/superpowers/plans/2026-06-02-trader-paper-production-plan.md`
- `docs/superpowers/plans/2026-06-02-trader-runtime-control-plan.md`
