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
- SQLite / SQL / `sqlx` 只属于 storage 边界。Backtest、Paper、API、CLI 等边界外生产路径不得构造 storage 写入 DTO，不得透传 `SqlitePool`，写入必须走 storage 暴露的语义 command / repository 方法。storage 对外读取返回明确 read model，不复用写入 DTO；边界外只读/对账逻辑不直接传递 storage record，从 storage 查询结果进入业务 helper 前先映射为本 crate 的 read model。REST API 查询路由会再映射为 API-owned response struct，不直接把 storage record 作为 HTTP contract 暴露。
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

当前分支处于生产前 paper 验证准备阶段。该阶段目标是让 paper run 使用真实配置、真实运行控制、真实持久化、可审计报告和受闸门保护的真实 paper broker 连接。Binance Spot Testnet crypto paper 已完成真实 testnet 订单链路验证；IBKR stock paper 已完成真实 TWS / Gateway adapter 与 runner 边界，仍等待真实 paper 账号和本机 Gateway 环境做完整生命周期验证。该阶段仍不包含真实资金实盘交易。

配置真源已扩展为：

- `[risk]`：`max_order_notional`、`min_cash_after_order`、`max_exposure`、`max_drawdown`、`max_leverage`、`max_margin_used`、`trading_halted`、`allow_short`。`allow_short` 是可选显式覆盖项：配置为 `true` 时允许所有策略 symbol 产生负目标仓位，配置为 `false` 时全部禁止；未配置时按每个 `strategy.symbols` 的资产类型保守派生，只有 `CRYPTO_PERP` 或 `CRYPTO_FUTURE` symbol 默认允许 short，股票、crypto spot 或无法识别的 symbol 默认禁止。混合 Universe 不再整体压成单一默认值，crypto derivative 可以 short，非 shortable 标的仍会被 Risk 拒绝。`max_margin_used` 是绝对保证金占用上限；配置为 `0` 时不启用该上限，配置为正数时按 market rules 的初始保证金率校验衍生品目标仓位。
- `[broker]`：`kind` 与 `mode`。当前支持配置枚举 `simulated`、`futu`、`binance`、`okx`、`interactive_brokers`，并支持 `ibkr` 作为 `interactive_brokers` 别名；`mode` 支持 `paper`、`live`。
- `[live]`：`enabled` 与可选 `heartbeat_ms`。

CLI 与 REST 的 paper/backtest settings 从 `[risk]` 读取风控阈值，并通过 `AppConfig::effective_allow_short()` 与 `AppConfig::shortable_symbols()` 解析全局覆盖和逐标的短仓权限，不再使用隐藏硬编码风控默认值。REST live surface 的 broker kind 从 `[broker]` 读取；当前 live surface 仍使用本地 fake broker adapter。Paper order submission 不走 live surface，Binance/IBKR 通过各自受闸门保护的 paper executor 接入。

`PaperRuntime` 现在提供两类入口：

- `run_bars` / `run_bars_with_cancel`：一次性输入历史 bars，保持 V1 本地验证路径。
- `run_market_slices` / `run_market_slices_with_cancel`：一次性输入多标的 `MarketSlice`，用于 Backtest / Paper 的配置化多文件行情路径。
- `run_bar_stream_with_cancel`：从 channel-based 单标的 bar stream 顺序消费 bars，保持旧 paper stream 兼容路径。
- `run_market_slice_stream_with_cancel`：从 channel-based 多标的 `MarketSlice` stream 顺序消费行情，复用同一套 Strategy、Portfolio、MarketRules、Risk、OMS、Broker simulation、Accounting、Storage 处理逻辑，并支持 pacing 与取消。

Broker fake adapters 现在提供本地 paper 测试 surface：`place_order`、`query_order`、`cancel_order`、`account_snapshot`、`status`。REST 已提供 `GET /api/v1/brokers/account/{account_id}` 返回配置 broker kind 对应的本地 broker account surface；仍不提供绕过 Runtime/OMS 的手动下单 API。IBKR 真实 paper 下单不通过通用 `Broker::place_order`，统一通过 `IbkrPaperOrderExecutor`，避免绕过 Runtime/OMS/审计链路。

REST 也提供 `GET /api/v1/preflight/paper`，用于在 server 运行时检查当前配置是否满足本地 paper 验证条件。本地 simulated paper 返回 `real_broker_connection=false`；Binance Spot Testnet paper 在 testnet base_url 与凭证环境变量检查通过后返回 `real_broker_connection=true`。

Binance testnet adapter 已开始接入。当前支持 `ping`、signed account snapshot，以及手动 tiny limit order -> query -> cancel -> myTrades sync -> local accounting snapshot；也已提供受闸门保护的策略自动 Binance Spot Testnet executor。`[broker] order_submit_enabled` 是策略自动送单闸门，默认 `false`；打开后 `paper-run` 只在 `broker.kind = "binance"`、`broker.mode = "paper"`、Spot Testnet `base_url` 和环境变量凭证都满足时才会提交 testnet limit order。自动 Binance paper run 启动时会读取 Binance account snapshot，并用 USDT cash 覆盖本次 `PaperSettings.initial_cash`。runtime 会在调用 broker executor 前先持久化一条 `SUBMITTED` pending order，写入稳定的 `client_order_id = trader-paper-{run_id_prefix}-{order_number}`；executor 会先用 Binance `origClientOrderId` 查询已存在订单，查到则同步该订单的 `myTrades`，查不到才提交新 testnet order。该 executor 只把 Binance `myTrades` 返回的真实成交写入本地 fill；如果订单没有真实 trades，会先尝试撤销仍 open 的 testnet order，然后以 0 filled qty 更新订单状态，不写 fill、不更新本地账本，也不会伪造成交。凭证只从环境变量读取：

```powershell
$env:BINANCE_TESTNET_API_KEY = "..."
$env:BINANCE_TESTNET_SECRET_KEY = "..."
trader paper-preflight --config configs/paper/binance_testnet.toml
trader binance-paper-readonly --config configs/paper/binance_testnet.toml
```

当 `[broker] kind = "binance"` 且 `mode = "paper"` 时，`paper-preflight` 会要求 Spot Testnet base_url 和 `BINANCE_TESTNET_API_KEY` / `BINANCE_TESTNET_SECRET_KEY` 存在；通过后输出 `real_broker_connection=true`。该检查不访问网络，网络连接仍由 `binance-paper-readonly` 验证。

Binance signed endpoint 使用 Spot Testnet `/v3/time` 返回的 `serverTime` 生成 timestamp，避免本机时钟偏移导致 `code=-1021`。

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

真实 Binance Spot Testnet K 线优先落盘为 Parquet：

```powershell
trader binance-paper-klines --config configs/paper/binance_btcusdt_1m_parquet.toml --symbol BTCUSDT --interval 1m --limit 100 --format parquet --output datasets/binance/btcusdt_1m.parquet
powershell -ExecutionPolicy Bypass -File .\scripts\binance-refresh-klines.ps1 -Limit 100
powershell -ExecutionPolicy Bypass -File .\scripts\binance-paper-klines-smoke.ps1
```

正式配置 `configs/paper/binance_btcusdt_1m_parquet.toml` 固定使用 `[data] source = "parquet"` 与 `datasets/binance/btcusdt_1m.parquet`。刷新脚本只维护 Parquet 数据并执行 preflight，不运行策略、不下单。输出字段为 `ts_ms,open,high,low,close,volume`，通过现有 Polars Parquet writer 写入。CSV 仅作为兼容格式，需显式加 `--format csv`。

真实行情 runner：

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\binance-paper-real-run.ps1 -Limit 100
powershell -ExecutionPolicy Bypass -File .\scripts\binance-paper-real-run.ps1 -Limit 100 -RunPaper
powershell -ExecutionPolicy Bypass -File .\scripts\binance-paper-run.ps1 -Limit 1000
```

`binance-paper-real-run.ps1` 使用临时 config/DB，适合 smoke。`binance-paper-run.ps1` 使用正式 Parquet 配置刷新 `datasets/binance/btcusdt_1m.parquet`，并为每次运行在 `data/binance-paper-runs/{run_id}/` 生成独立 `config.toml`、`run.sqlite`、`report.txt`、`report.csv` 和 `report.html`，执行 paper-run、report、recover 和 open order 巡检。两者默认都不下单；只有 `-ConfirmTestnetOrder` 会打开 Binance Spot Testnet 策略送单。`binance-paper-run.ps1` 开闸送单时禁止同时使用 `-SkipRefresh`，并会读取一次 Spot Testnet ticker price 写入运行输出，避免用旧 Parquet 数据直接送单；如果 testnet paper-run 因 broker 错误失败，脚本会先 best-effort 执行 recover 与 open order 巡检，再保留原始失败。

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

自动策略送单 smoke：

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\binance-paper-auto-smoke.ps1 -ConfirmTestnetOrder
```

该脚本会从 Binance Spot Testnet 读取当前 BTCUSDT ticker，生成临时 BTCUSDT bars，创建临时 SQLite 与临时配置，打开 `order_submit_enabled = true` 后执行 `paper-run`、`report` 和 BTCUSDT open order 巡检。没有 `-ConfirmTestnetOrder` 时会拒绝执行。

pending order 恢复：

```powershell
trader binance-paper-recover --config configs/paper/binance_testnet.toml
```

该命令扫描当前 run 的 `SUBMITTED` / `NEW` / `PARTIALLY_FILLED` 本地订单，使用本地 `client_order_id` 调 Binance `origClientOrderId` 查询订单，查到后同步 `myTrades`、更新订单执行状态，并刷新本地 account balance、position 和 portfolio snapshot。该命令不会提交新订单；输出中的 `remaining` 表示恢复后仍需继续跟踪的订单数。如果本次扫描过订单、没有 missing、且 `remaining=0`，非 completed 的 run 会标记为 `recovered`。

恢复 smoke：

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\binance-paper-recover-smoke.ps1
```

该脚本使用临时配置与临时 SQLite，只执行配置、migration 和恢复命令验证；它不会打开 `order_submit_enabled`，不会提交新订单。无网络环境可追加 `-SkipNetwork`。

open order 巡检：

```powershell
trader binance-paper-open-orders --config configs/paper/binance_testnet.toml --symbol BTCUSDT
powershell -ExecutionPolicy Bypass -File .\scripts\binance-paper-open-orders-smoke.ps1
```

该命令和 smoke 只查询 Spot Testnet open orders，不会提交或撤销订单。清理 testnet 挂单必须显式使用：

```powershell
trader binance-paper-cancel-open-orders --config configs/paper/binance_testnet.toml --symbol BTCUSDT --confirm-testnet-cancel
```

清理命令会同步当前配置 SQLite 中匹配 `run_id + client_order_id` 的订单状态，并输出 `local_synced`。

股票 paper 方向固定为 IBKR。当前 IBKR AAPL Parquet runner 用来验证股票链路的配置、Parquet 数据、SQLite、paper runtime 和报告归档；默认不连接 IBKR TWS / Gateway，也不提交 IBKR paper 订单：

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\ibkr-paper-run.ps1
```

固定配置为 `configs/paper/ibkr_aapl_1d_parquet.toml`，使用 `[broker] kind = "ibkr"`、`mode = "paper"`、`host = "127.0.0.1"`、`port = 7497`、`client_id = 1`、`order_submit_enabled = false`，行情文件为 `datasets/ibkr/aapl_1d.parquet`。脚本会把 `datasets/sample/aapl_1d.csv` 转成 Parquet 作为本地验证输入，并为每次运行在 `data/ibkr-paper-runs/{run_id}/` 生成独立 `config.toml`、`run.sqlite`、`report.txt`、`report.csv`、`report.html` 和 `summary.json`。

真实 IBKR paper 自动下单必须显式开闸：

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\ibkr-paper-run.ps1 -AccountId DU12345 -ConfirmIbkrPaperOrder
```

`-ConfirmIbkrPaperOrder` 会把临时 run config 的 `order_submit_enabled` 改为 `true`，执行 `paper-preflight` 时连接 TWS / IB Gateway 并校验账号，然后让 `paper-run` 注入 `IbkrPaperOrderExecutor` 发送股票 LMT paper order。开闸时必须提供真实 `-AccountId DU...` 或提前修改配置中的 `[paper] account_id`；默认占位 `DU000000` 会被脚本拒绝。可用 `-GatewayHost`、`-Port`、`-ClientId` 覆盖 Gateway 连接参数。脚本成功后会运行 read-only Gateway checks，并把输出写入 `summary.json`；如果自动下单失败，也会 best-effort 执行 read-only 巡检后保留原始失败。

完整测试步骤脚本：

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\ibkr-paper-test-guide.ps1
```

该脚本默认只打印测试计划，不连接 Gateway、不下单；账号准备好后可用 `-Stage ReadOnly`、`-Stage TinyOrder`、`-Stage AutoRun` 分阶段执行。

多轮 soak 验证用于连续跑多次 runner，检查稳定性和每轮 `summary.json`。默认不连接 Gateway、不下单：

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\ibkr-paper-soak.ps1 -Iterations 3 -SkipRefresh
powershell -ExecutionPolicy Bypass -File .\scripts\ibkr-paper-soak.ps1 -Iterations 3 -AccountId DU12345 -ConfirmIbkrPaperOrder
```

本地 paper readiness 门禁用于账号未就绪时的无网络回归检查：

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\paper-readiness.ps1
```

默认会跑 cargo 格式/检查/测试、Binance 无网络 smoke，以及 IBKR 本地 test plan + dry-run soak；不会连接真实 Gateway，也不会下单。可用 `-SkipCargo`、`-SkipBinance`、`-SkipIbkr` 缩小范围。

IBKR read-only preflight 当前提供 Gateway 握手与账号校验，底层通过 Rust 开源 crate `ibapi` 连接 TWS / IB Gateway；项目内部仍保留 `Decimal` 领域类型，只在 adapter 边界和 `ibapi` 的 f64 订单字段做显式转换：

```powershell
trader ibkr-paper-readonly --config configs/paper/ibkr_aapl_1d_parquet.toml
trader ibkr-paper-open-orders --config configs/paper/ibkr_aapl_1d_parquet.toml
trader ibkr-paper-executions --config configs/paper/ibkr_aapl_1d_parquet.toml --request-id 1
trader ibkr-paper-reconcile --config configs/paper/ibkr_aapl_1d_parquet.toml --request-id 1
trader ibkr-paper-recover --config configs/paper/ibkr_aapl_1d_parquet.toml --request-id 1
trader ibkr-paper-next-order-id --config configs/paper/ibkr_aapl_1d_parquet.toml
trader ibkr-paper-cancel-order --config configs/paper/ibkr_aapl_1d_parquet.toml --order-id 42 --confirm-ibkr-paper-cancel
trader ibkr-paper-tiny-order --config configs/paper/ibkr_aapl_1d_parquet.toml --symbol AAPL --side buy --qty 1 --price 185.25 --confirm-ibkr-paper-order
```

该命令要求 TWS / IB Gateway paper 环境已启动，并能连接配置中的 `host:port`。默认 paper 端口为 `7497`，`client_id` 用于 TWS API socket session。当前命令通过 `broker::IbkrPaperGatewayAdapter` 调用 `ibapi::Client` 做 server version 握手，发送 managed accounts 只读请求，并校验 `[paper] account_id` 是否在 Gateway 返回账号列表中；`[paper] account_id` 必须改为真实 IBKR paper account id（通常是 `DU...`），当前配置中的 `DU000000` 只是结构化占位。只读命令不提交订单。`paper-preflight` 在 IBKR `order_submit_enabled = true` 时会实际连接 Gateway 并校验账号，通过后输出 `real_broker_connection=true`。

`ibkr-paper-open-orders`、`ibkr-paper-executions`、`ibkr-paper-reconcile`、`ibkr-paper-recover` 和 `ibkr-paper-next-order-id` 已接入同一 `ibapi` adapter，分别读取远端 open orders、executions、本地/远端订单成交匹配计数、按真实 Gateway 回报恢复本地 recoverable orders，以及 next valid order id。`ibkr-paper-recover` 会写 SQLite；其他只读命令不写 SQLite，也不提交、撤销订单。`ibkr-paper-cancel-order` 是受确认保护的真实 paper 撤单命令，必须显式传 `--confirm-ibkr-paper-cancel`，不提交新订单、不写 SQLite。`ibkr-paper-tiny-order` 是受确认保护的手动 tiny LMT paper order 命令，先取 next valid order id，再发送 place order，并等待 order status / open order 回报；该手动命令不写 SQLite，策略自动执行由下方 `IbkrPaperOrderExecutor` 处理。

IBKR paper order adapter 已接入真实 Gateway client wrapper：`IbkrPaperGatewayOrderClient` 通过 `IbkrPaperGatewayAdapter` 查询 open orders、提交 limit order、撤单、读取 executions；`IbkrPaperOrderExecutor` 只根据 executions 写入成交，空 executions 会撤销 open order 并返回 0 filled qty，不伪造成交。CLI 与 REST 的 `paper-run` 在 `[broker] kind = "ibkr"`、`mode = "paper"`、`order_submit_enabled = true` 且账号校验通过后，会注入该 executor 执行股票 paper order。

IBKR TWS API wire codec 不再由项目手写维护，`broker` crate 改为依赖 `ibapi`。当前 adapter 覆盖 server version / connection time、managed accounts、open orders、executions、next valid order id、cancel order、tiny stock LMT place order，并已接入真实 socket session 完成账号校验、只读订单/成交读取、受确认保护的 paper 撤单、受确认保护的 tiny paper order，以及 `paper-run` 自动股票 paper order。`IbkrGatewayClient` / `IbapiIbkrGatewayClient` 已把 Gateway 调用从 `IbkrPaperGatewayAdapter` 中隔离出来，账号校验、open orders、executions、next order id、place/cancel order 均可用 fake Gateway client 做无网络验证；真实完整生命周期仍需要本机 TWS / Gateway 与真实 paper account。

Binance 也有 Rust 开源包可选，例如 `binance`、`binance_spot_connector_rust` 和 `binance-sdk`。当前 Binance Spot Testnet adapter 仍使用项目内 `reqwest + HMAC` 实现；`BinanceHttpClient` / `ReqwestBinanceHttpClient` 已把底层 HTTP 调用从 `BinanceSpotTestnetAdapter` 中隔离出来，read-only 与下单调用均可用 fake client 验证。后续若迁移 Rust SDK，应优先保留现有 `BinancePaperOrderClient` / `BinanceSpotTestnetAdapter` 领域边界，逐步替换底层 REST client，避免和 IBKR 迁移混在同一改动里。

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
- `paper` 路径支持本地 simulated 成交；生产前 paper 验证阶段已扩展 Binance Spot Testnet crypto paper executor 与 IBKR stock paper executor，真实 broker paper 成交只从 broker trades/executions 写入，不伪造成交。
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

Phase 3 将 paper 从 backtest wrapper 拆成独立 runtime。当前 Backtest 与 Paper 共用 `AlgorithmEngine` 执行 `Universe -> Alpha / Strategy -> Portfolio -> MarketRules -> Risk -> Execution -> OMS` 决策链；`PaperRuntime` 负责 broker executor、accounting 应用结果和 storage 持久化。

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
- Backtest runtime 通过 `storage` 的 backtest repository 接口持久化 run、order、fill、position 和 runtime events，不直接构造 SQLite 表 DTO。
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

当前 MVP 订单链路已收敛到共享 `algorithm` crate，由 `AlgorithmEngine` 统一执行 `Universe -> Alpha / Strategy -> Portfolio -> MarketRules -> Risk -> Execution -> OMS` 决策链，并输出标准化 `algorithm.*` / `broker.order.*` / `accounting.*` 事件。Algorithm 事件 payload 在 Rust 内使用 typed payload struct 构造，再序列化为 `serde_json::Value` 写入 EventBus / SQLite，避免各 runtime 手写漂移的 JSON schema。

Alpha 层支持单模型、`CompositeAlphaModel` 最高置信度组合、`NetSignalAlphaModel` 净信号组合和 `MajorityVoteAlphaModel` 多数投票组合；Strategy/Alpha registry 当前支持 `moving_average_cross`、`exponential_moving_average_cross`、`price_momentum`、`price_channel_breakout`、`price_channel_reversion` 与 `relative_strength_index_reversion` 六个模型。配置也可用 `[[strategy.alpha_components]]` 装配多个 alpha component，每个 component 的 `weight` 会缩放 signal confidence。`alpha_conflict_resolution = "highest_confidence"` 会选择加权 confidence 最高的信号；`alpha_conflict_resolution = "net_signal"` 会把 Buy / CloseShort 视为正向、Sell / CloseLong 视为负向，按已加权 confidence 抵消后只输出净方向，完全抵消时不输出信号；`alpha_conflict_resolution = "majority_vote"` 会按正负方向计票，票数多的一侧胜出，输出 confidence 为胜方已加权 confidence 的平均值，票数相同时不输出信号；`alpha_conflict_resolution = "category_majority"` 会先按 component `category` 分组，每组内部执行净信号组合，再让每个 category 贡献一票做多数投票，避免同一模型族靠重复 component 放大票数，未配置 `category` 时默认按 component `name` 分组。`[strategy.alpha_gate]` 可把只读 feature store 记录作为 Alpha 信号闸门，按 `run_id + symbol + feature_name` 读取不晚于当前 bar 的最新 feature，配置了 `version` 时只使用匹配版本的 feature，缺失或不满足 `min_value` / `max_value` 区间时抑制信号。

`data` 提供 `MarketSlice` / `SymbolBar` 表达同一时间点的多标的行情；`AlgorithmEngine::on_market_slice` 会对 Universe 选出的每个有行情 symbol 独立运行 Alpha、Portfolio、MarketRules、Risk、Execution 和 OMS，并用全组合持仓价格表计算权益、敞口和未实现盈亏。Portfolio target 使用 signed quantity：`Buy` 映射为正目标仓位，`Sell` 映射为负目标仓位，`CloseLong` / `CloseShort` 映射为 0；Accounting 支持负持仓、卖空开仓、买入回补和短仓未实现盈亏。Risk 对目标仓位按 gross exposure 做投影，因此 short 不会绕过敞口限制，真实减仓卖出也不会被误判为增加风险；同时 `[risk] allow_short` 控制是否允许负目标仓位，显式 `true`/`false` 作为全局覆盖，未显式配置时按 symbol 派生 shortable 集合，混合 Universe 中 crypto derivative short 可放行而股票/spot short 仍会被拒绝。`MarketRuleSet` 还提供初始保证金率：股票与 crypto spot 为 0，`CRYPTO_PERP` / `CRYPTO_FUTURE` 为 10%；Algorithm 在目标仓位投影时计算 projected `margin_used`，`max_margin_used` 为正数时由 Risk 拒绝超过上限的衍生品目标仓位。Backtest 与 Paper 不再各自维护一套策略 loop；Backtest 使用同一 engine 加模拟成交，Paper 使用同一 engine 加 simulated / Binance / IBKR paper executor。Backtest / Paper runtime 会消费配置中的 `strategy.universe`、`strategy.alpha` 与 `strategy.symbols`，通过 `StrategyRegistry::assemble_alpha` 装配 universe selector、alpha model 和可选 feature gate；多标的 moving average alpha 会为每个 symbol 维护独立指标状态，Backtest/Paper 也提供 `run_market_slices` 入口持久化多标的订单、成交和持仓。

Universe 当前支持 `static`、`filtered`、`ranked` 和 `feature_ranked` 四类 selector：`static` 保持原有配置 symbol 全量返回；`filtered` 在配置候选 symbols 上应用 `include_symbols`、`exclude_symbols`、`symbol_prefixes` 和 `require_current_data` 通用规则，其中 `require_current_data` 会基于当前 `MarketSlice` 的可用 symbols 动态收缩 universe；`ranked` 使用 `symbols` 的配置顺序作为排名，并可通过 `[strategy.universe_filter].max_symbols` 截取前 N 个通过过滤条件的 symbol；`feature_ranked` 从只读 Feature Parquet 中读取不晚于当前 bar 的最新 feature value 排名，再复用同一套 universe filter 与 `max_symbols` 截断。CLI / REST 的 Backtest 与 Paper 入口支持 `[[data.inputs]]`，可直接把多个 symbol 映射到各自 CSV / Parquet 文件并合并为 `MarketSlice`；旧 `[data] source/path` 单文件配置继续作为单标的兼容包装。REST 启动的 Backtest/Paper 会把同一批 algorithm runtime events 发布到 `AppState.event_bus`，同时继续写入 SQLite `event_store` 作为审计真源。Replay 会为每根历史 K 线生成 `market.bar` runtime event；REST 启动的 Replay 同样发布到 `AppState.event_bus`，并继续写入 replay lifecycle events。Replay runtime 会读取共享 `ReplayController`，正在运行的 replay loop 会响应 pause、resume、seek 和 speed。MarketRules 校验 lot size、tick size、min qty、min notional 和初始保证金率；Risk 校验 max order qty、max order notional、cash buffer、trading halt、short permission、gross exposure、leverage 和 max margin；OMS 跟踪订单状态、累计成交和剩余数量。

多标的行情配置使用 `[[data.inputs]]`：

```toml
[data]
[[data.inputs]]
symbol = "US:NASDAQ:AAPL:EQUITY"
source = "csv"
path = "datasets/sample/aapl_1d.csv"

[[data.inputs]]
symbol = "US:NASDAQ:MSFT:EQUITY"
source = "csv"
path = "datasets/sample/msft_1d.csv"
```

过滤型 Universe 配置使用 `[strategy.universe_filter]`：

```toml
[strategy]
name = "moving_average_cross"
universe = "filtered"
symbols = ["US:NASDAQ:AAPL:EQUITY", "US:NASDAQ:MSFT:EQUITY"]
fast_window = 2
slow_window = 3

[strategy.universe_filter]
exclude_symbols = ["US:NASDAQ:MSFT:EQUITY"]
symbol_prefixes = ["US:NASDAQ:"]
require_current_data = true
```

排序型 Universe 配置使用 `universe = "ranked"`，`symbols` 顺序就是 rank：

```toml
[strategy]
name = "moving_average_cross"
universe = "ranked"
symbols = ["US:NASDAQ:AAPL:EQUITY", "US:NASDAQ:MSFT:EQUITY"]
fast_window = 2
slow_window = 3

[strategy.universe_filter]
max_symbols = 1
require_current_data = true
```

Feature 排序型 Universe 配置使用 `universe = "feature_ranked"` 和 `[strategy.universe_rank]`。当前只支持 Parquet feature source；`descending` 默认 `true`：

```toml
[strategy]
name = "moving_average_cross"
universe = "feature_ranked"
symbols = ["US:NASDAQ:AAPL:EQUITY", "US:NASDAQ:MSFT:EQUITY"]
fast_window = 2
slow_window = 3

[strategy.universe_filter]
max_symbols = 1
require_current_data = true

[strategy.universe_rank]
source = "parquet"
path = "datasets/features/multi_symbol_sma_1.parquet"
manifest_path = "datasets/features/multi_symbol_sma_1.manifest.json"
run_id = "research-2026-06-11"
feature_name = "sma_close_1"
version = "v1"
build_indicator = "sma"
build_period = 1
build_value_column = "close"
```

`feature_ranked` 与 Alpha feature gate 一样只在 CLI / REST settings 装配边界读取 Parquet 和校验 manifest；策略运行时只接收内存 feature records，不访问 SQLite，不透传 SQL 连接。

EMA 交叉 Alpha 使用 `exponential_moving_average_cross`：

```toml
[strategy]
name = "exponential_moving_average_cross"
alpha = "exponential_moving_average_cross"
symbols = ["US:NASDAQ:AAPL:EQUITY"]
fast_window = 2
slow_window = 3
```

价格动量 Alpha 使用 `price_momentum`，比较短周期和长周期的单位时间价格动量：

```toml
[strategy]
name = "price_momentum"
alpha = "price_momentum"
symbols = ["US:NASDAQ:AAPL:EQUITY"]
fast_window = 1
slow_window = 2
```

价格通道突破 Alpha 使用 `price_channel_breakout`，比较最近 `fast_window` 根确认 K 线是否全部突破前 `slow_window` 根 close 价格通道：

```toml
[strategy]
name = "price_channel_breakout"
alpha = "price_channel_breakout"
symbols = ["US:NASDAQ:AAPL:EQUITY"]
fast_window = 1
slow_window = 2
```

价格通道均值回归 Alpha 使用 `price_channel_reversion`，复用同一价格通道判断，但在向上突破时发 Sell、向下突破时发 Buy。Sell 会通过 signed portfolio target 形成负目标仓位，而不是只把已有多头打平：

```toml
[strategy]
name = "price_channel_reversion"
alpha = "price_channel_reversion"
symbols = ["US:NASDAQ:AAPL:EQUITY"]
fast_window = 1
slow_window = 2
```

RSI 均值回归 Alpha 使用 `relative_strength_index_reversion`。`fast_window` 是 RSI period，`slow_window` 是 overbought 阈值；oversold 阈值按 `100 - slow_window` 派生。RSI 低于 oversold 时发 Buy，高于 overbought 时发 Sell：

```toml
[strategy]
name = "relative_strength_index_reversion"
alpha = "relative_strength_index_reversion"
symbols = ["US:NASDAQ:AAPL:EQUITY"]
fast_window = 3
slow_window = 70
```

加权 Alpha 组合配置使用 `[[strategy.alpha_components]]`：

```toml
[strategy]
name = "moving_average_cross"
alpha = "moving_average_cross"
alpha_conflict_resolution = "highest_confidence"
symbols = ["US:NASDAQ:AAPL:EQUITY"]
fast_window = 2
slow_window = 3

[[strategy.alpha_components]]
name = "moving_average_cross"
fast_window = 2
slow_window = 3
weight = 0.25

[[strategy.alpha_components]]
name = "moving_average_cross"
fast_window = 2
slow_window = 3
weight = 0.5
```

冲突处理可改为净信号组合：

```toml
[strategy]
name = "moving_average_cross"
alpha = "moving_average_cross"
alpha_conflict_resolution = "net_signal"
symbols = ["US:NASDAQ:AAPL:EQUITY"]
fast_window = 2
slow_window = 3

[[strategy.alpha_components]]
name = "moving_average_cross"
fast_window = 1
slow_window = 2
weight = 1.0

[[strategy.alpha_components]]
name = "moving_average_cross"
fast_window = 2
slow_window = 1
weight = 0.25
```

也可改为多数投票组合：

```toml
[strategy]
name = "moving_average_cross"
alpha = "moving_average_cross"
alpha_conflict_resolution = "majority_vote"
symbols = ["US:NASDAQ:AAPL:EQUITY"]
fast_window = 2
slow_window = 3

[[strategy.alpha_components]]
name = "moving_average_cross"
fast_window = 1
slow_window = 2
weight = 0.25

[[strategy.alpha_components]]
name = "moving_average_cross"
fast_window = 1
slow_window = 2
weight = 0.5

[[strategy.alpha_components]]
name = "moving_average_cross"
fast_window = 2
slow_window = 1
weight = 1.0
```

也可改为按模型类别分层投票：

```toml
[strategy]
name = "moving_average_cross"
alpha = "moving_average_cross"
alpha_conflict_resolution = "category_majority"
symbols = ["US:NASDAQ:AAPL:EQUITY"]
fast_window = 2
slow_window = 3

[[strategy.alpha_components]]
name = "moving_average_cross"
category = "trend"
fast_window = 2
slow_window = 1
weight = 0.25

[[strategy.alpha_components]]
name = "moving_average_cross"
category = "trend"
fast_window = 2
slow_window = 1
weight = 0.5

[[strategy.alpha_components]]
name = "moving_average_cross"
category = "mean_reversion"
fast_window = 1
slow_window = 2
weight = 1.0

[[strategy.alpha_components]]
name = "moving_average_cross"
category = "quality"
fast_window = 1
slow_window = 2
weight = 0.5
```

Alpha feature gate 使用 `[strategy.alpha_gate]`，当前只支持 Parquet feature source：

```toml
[strategy.alpha_gate]
source = "parquet"
path = "datasets/features/quality.parquet"
manifest_path = "datasets/features/quality.manifest.json"
run_id = "research-2026-06-11"
feature_name = "quality_score"
version = "v2"
build_indicator = "sma"
build_period = 20
build_value_column = "close"
min_value = "0.7"
max_value = "1.0"
```

该闸门是通用 Alpha 包装器，不限定具体策略或市场。Feature Parquet 记录使用 `feature_store` schema：`run_id`、`symbol`、`name`、`ts_ms`、`value`、`version`；`value` 以 Decimal 字符串保存。`version` 是可选约束；省略时允许任意版本，配置后只在匹配版本中取最新记录。`manifest_path` 是可选校验文件；配置后 CLI / REST 会在装配 Backtest / Paper settings 时先校验 manifest 的 `parquet_path`、schema、`run_id`、策略 symbols、`feature_name` 和 `version` 是否覆盖当前 alpha gate 或 universe rank；若配置了 `build_indicator`、`build_period` 或 `build_value_column`，且 manifest 带 `build_contract`，还会校验这些构建参数一致，再读取 Parquet feature records 并传入 StrategyRegistry。策略运行时不访问 SQLite，也不透传 SQL 连接。

研究特征 Parquet 可生成配套 manifest，用于记录可复现实验元数据：

```powershell
trader feature-manifest --parquet datasets/features/quality.parquet --output datasets/features/quality.manifest.json
```

Manifest JSON 由 `feature_store` 生成，包含 `schema_version`、`parquet_path`、`record_count`、`run_ids`、`symbols`、`feature_names` 和 `versions`。由 `feature-build-indicator` / `feature-build-sma` 生成的 manifest 还会携带可选 `build_contract`，记录 builder、indicator、value_column、period、run_id、feature_name、version 以及生成 feature 时使用的 bars inputs。bars input 除 source/path/symbol 外，可包含当前 bars 文件的 `content_hash`、`bar_count`、`first_ts_ms` 和 `last_ts_ms` 快照。该 manifest 只描述 Parquet 研究特征与构建输入元数据，不引入 SQL 持久化；当它被 `[strategy.alpha_gate].manifest_path` 或 `[strategy.universe_rank].manifest_path` 引用时，CLI / REST 会提前校验 Parquet 文件、研究批次、标的、feature、version，并在 manifest 带 `build_contract` 时校验当前 Backtest / Paper 的 data inputs 与生成 feature 的 bars inputs 一致；若 manifest 带输入快照，还会重新加载当前 bars 并复算内容 hash、bar 数和首尾时间戳，拒绝同一路径文件内容或时间范围漂移。配置可选 `build_indicator`、`build_period`、`build_value_column` 后，还会严格校验 manifest 构建参数，防止训练/研究特征和回测行情源或特征生成方式漂移。

CLI 也提供最小本地 feature 生成入口，可从 CSV / Parquet bars 的 close 价格生成 SMA / EMA / RSI feature Parquet，并同步写 manifest：

```powershell
trader feature-build-indicator --indicator sma --source parquet --input datasets/sample/aapl_1d.parquet --symbol US:NASDAQ:AAPL:EQUITY --run-id research-2026-06-11 --feature-name sma_close_20 --period 20 --version v1 --output datasets/features/aapl_sma_20.parquet --manifest-output datasets/features/aapl_sma_20.manifest.json
trader feature-build-indicator --indicator ema --source parquet --input datasets/sample/aapl_1d.parquet --symbol US:NASDAQ:AAPL:EQUITY --run-id research-2026-06-11 --feature-name ema_close_20 --period 20 --version v1 --output datasets/features/aapl_ema_20.parquet --manifest-output datasets/features/aapl_ema_20.manifest.json
trader feature-build-indicator --indicator rsi --source parquet --input datasets/sample/aapl_1d.parquet --symbol US:NASDAQ:AAPL:EQUITY --run-id research-2026-06-11 --feature-name rsi_close_14 --period 14 --version v1 --output datasets/features/aapl_rsi_14.parquet --manifest-output datasets/features/aapl_rsi_14.manifest.json
trader feature-build-indicator --indicator sma --inputs-config configs/backtest/multi_symbol_ma_cross.toml --run-id research-2026-06-11 --feature-name sma_close_20 --period 20 --version v1 --output datasets/features/multi_sma_20.parquet --manifest-output datasets/features/multi_sma_20.manifest.json
trader feature-build-sma --source parquet --input datasets/sample/aapl_1d.parquet --symbol US:NASDAQ:AAPL:EQUITY --run-id research-2026-06-11 --feature-name sma_close_20 --period 20 --version v1 --output datasets/features/aapl_sma_20.parquet --manifest-output datasets/features/aapl_sma_20.manifest.json
```

`feature-build-indicator` 的输入模式二选一：单标的使用 `--source/--input/--symbol`，多标的使用 `--inputs-config <config.toml>` 读取现有 `[[data.inputs]]`，并要求配置中的 `strategy.symbols` 都有对应行情输入。`feature-build-sma` 是兼容入口；新增研究流水线应优先使用 `feature-build-indicator --indicator sma|ema|rsi`。两者输出仍是通用 `feature_store` Parquet schema，并在 manifest 的 `build_contract` 中固化 indicator、period、value_column、source / path / symbol 与可选 bars 输入快照，供后续 Backtest / Paper 装配边界做一致性校验。

仓库内提供了一个完整本地样例：

```powershell
trader backtest --config configs/backtest/ema_cross.toml
trader backtest --config configs/backtest/ranked_universe_ma_cross.toml
trader backtest --config configs/backtest/feature_ranked_universe_ma_cross.toml
trader backtest --config configs/backtest/price_momentum.toml
trader backtest --config configs/backtest/price_channel_breakout.toml
trader backtest --config configs/backtest/price_channel_reversion.toml
trader backtest --config configs/backtest/rsi_reversion.toml
trader backtest --config configs/backtest/sma_feature_gate.toml
trader backtest --config configs/backtest/rsi_feature_gate.toml
trader backtest --config configs/backtest/sma_feature_gate_suppressed.toml
trader backtest --config configs/backtest/multi_symbol_sma_feature_gate.toml
trader backtest --config configs/backtest/net_signal_alpha_ma_cross.toml
trader backtest --config configs/backtest/majority_vote_alpha_ma_cross.toml
trader backtest --config configs/backtest/category_majority_alpha_ma_cross.toml
```

单标的样例读取 `datasets/sample/aapl_1d.csv`，`ema_cross.toml` 验证 EMA 交叉 Alpha 可通过同一 StrategyRegistry / Backtest / CLI / REST 主链路运行；`price_momentum.toml` 验证非均线价格动量 Alpha 可通过同一主链路运行；`price_channel_breakout.toml` 验证价格通道突破 Alpha 可通过同一主链路运行；`price_channel_reversion.toml` 使用 `datasets/sample/aapl_reversion_1d.csv` 验证价格通道均值回归 Alpha 可在向下延伸后产生 Buy 订单，测试链路也覆盖向上突破 Sell 形成短仓并持久化负持仓；`rsi_reversion.toml` 使用 `datasets/sample/aapl_rsi_reversion_1d.csv` 验证 RSI 均值回归 Alpha 可在 oversold 后产生 Buy 订单。股票短仓示例配置必须在 `[risk]` 中显式设置 `allow_short = true`，crypto derivative 示例可依赖资产类型派生默认值。`sma_feature_gate.toml` 通过 `datasets/features/aapl_sma_2.parquet` 和 `datasets/features/aapl_sma_2.manifest.json` 校验并应用 `sma_close_2` alpha gate。`rsi_feature_gate.toml` 通过 `datasets/features/aapl_rsi_3.parquet` 和 `datasets/features/aapl_rsi_3.manifest.json` 校验并应用 `rsi_close_3` alpha gate。`sma_feature_gate_suppressed.toml` 使用同一份合法 feature/manifest，但把 `min_value` 提高到样例 feature 无法满足，用于验证 gate 会明确抑制信号并跑出 `signals=0 orders=0`。多标的样例读取 AAPL / MSFT 的 `[[data.inputs]]`，`ranked_universe_ma_cross.toml` 验证可按配置 rank 与 `max_symbols` 收缩 universe；`feature_ranked_universe_ma_cross.toml` 通过 `datasets/features/multi_symbol_sma_1.parquet` 和 manifest 按 feature value 选择当前排名最高的 symbol；`multi_symbol_sma_feature_gate.toml` 通过 `datasets/features/multi_symbol_sma_2.parquet` 和 `datasets/features/multi_symbol_sma_2.manifest.json` 覆盖两个策略 symbols。`net_signal_alpha_ma_cross.toml` 验证多个 Alpha component 方向冲突时可按加权 confidence 做净额抵消；`majority_vote_alpha_ma_cross.toml` 验证方向由 component 多数票决定；`category_majority_alpha_ma_cross.toml` 验证同一 category 内先净信号聚合、跨 category 再多数投票。feature-ranked universe 与四个 feature gate 样例共同形成 `bars -> feature-build-indicator -> feature manifest -> feature-ranked universe / alpha gate -> backtest` 的本地研究闭环。

## Local Verifiable MVP

当前分支的 MVP 完成标准是“本地可实际验证的交易闭环”，不是完整实盘交易平台。可验证闭环包括：

- CLI：`check-config`、`migrate`、`backtest`、`paper-run`、`replay`、`report`。
- REST：`health`、`backtests`、`paper-runs`、`replays`、`orders`、`fills`、`positions`、`account-balances`、`portfolio/snapshots`、`metrics`、`runs`、`events`。
- REST run/event 查询返回 API-owned response，`config` / `payload` 是结构化 JSON 值；SQLite 内部的 `config_json` / `payload_json` 只属于 storage 持久化表示。
- WebSocket：`subscribe` 会先回放 SQLite run events，再转发 `AppState.event_bus` 中 run_id 匹配的 runtime events；`replay_control` 支持 pause/resume/seek/speed。
- Storage：SQLite 持久化 run、order、fill、position、account balance、portfolio snapshot、event store；storage crate 负责 decimal string、状态字符串、event id、SQLite record shape 等持久化转换。
- Core path：共享 `AlgorithmEngine` 串联 Universe、Alpha / Strategy、Portfolio、Execution delta、MarketRules、Risk、OMS；Universe 支持静态、过滤、配置顺序排名和只读 feature value 排名；Alpha 支持最高置信度组合、净信号组合、多数投票组合、多标的独立状态和只读 feature gate；Portfolio target 支持 signed quantity，Sell 可形成短仓，Accounting 支持负持仓估值和买入回补，Risk 使用 gross exposure、leverage 和 projected margin 校验目标仓位，并用 `[risk] allow_short` 的显式覆盖值或按 symbol 派生的 shortable 集合控制是否允许负目标仓位；`[[data.inputs]]` 会加载多文件行情并合并为 `MarketSlice`，生成 per-symbol order decisions，并按全组合持仓估值；Backtest 通过 storage backtest repository 写入审计结果，Paper runtime 负责 Broker executor、Accounting 应用结果和 Storage 持久化。
- Research support：`indicators` 提供 Decimal SMA/EMA/RSI 基础指标，`moving_average_cross` 策略复用 `SimpleMovingAverage`，`exponential_moving_average_cross` 策略复用 `ExponentialMovingAverage`，`relative_strength_index_reversion` 策略复用 `RelativeStrengthIndex`，`price_momentum` 使用 Decimal close 价格斜率比较，`price_channel_breakout` 使用 Decimal close 价格通道突破判断，`price_channel_reversion` 复用同一通道判断做均值回归反向信号；`feature_store` 提供 Decimal feature record、key、in-memory range/latest repository，以及 Parquet-backed repository / round-trip 读写。Feature Parquet schema 使用 `run_id`、`symbol`、`name`、`ts_ms`、`value`、`version`，其中 Decimal `value` 以字符串保存以保留精度；Backtest / Paper 可通过 `[strategy.universe_rank]` 用 Parquet feature 记录做 Universe 排名，也可通过 `[strategy.alpha_gate]` 过滤 Alpha 信号，并可用 `version` 约束研究特征批次；CLI 可通过 `feature-build-indicator --indicator sma|ema|rsi` 从单标的 bars 或 `--inputs-config` 多标的配置生成 SMA / EMA / RSI feature Parquet 和 manifest，`feature-build-sma` 保持兼容，也可通过 `feature-manifest` 为已有 Feature Parquet 生成 JSON manifest。manifest 记录 schema version、Parquet path、记录数、run ids、symbols、feature names 和 versions；由 feature 生成命令写出的 manifest 还会记录 `build_contract`，使 `manifest_path` 可让 CLI / REST 在运行前校验 universe rank 或 alpha gate 引用的 Parquet 文件、研究批次、标的、feature、version、source bars 输入一致性、可选 bars 输入快照和可选的构建参数期望；SQLite adapter 不在 feature_store 内实现，若需要 SQL 持久化必须走 storage 边界。
- Replay：从 CSV/Parquet 加载历史 K 线，返回 replay bar summary，并向 runtime bus 发布 `market.bar` events。
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
- `docs/superpowers/plans/2026-06-05-trader-paper-reconciliation-ibkr-plan.md`


# 代码参考
D:\code-refer\trader
