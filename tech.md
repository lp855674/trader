# Trader 技术规则

本文是 Trader 项目的技术规则入口，用来约束架构边界、运行链路、数据真源和交付门禁。

本文不维护阶段计划、实施日志、脚本流水账或“当前已完成列表”。计划只放在 `docs/roadmap.md` 和 `docs/superpowers/plans/`；模块技术事实只放在 `docs/tech/*.md`。

## 文档真源

- 架构总览、核心原则、分层边界和 crate 职责以本文件的跨模块规则和 `docs/tech/*.md` 的模块文档为准。
- SQLite 表、Parquet schema、repository、migration 和状态恢复只维护在 `docs/tech/storage.md`、`docs/tech/data.md` 和 `docs/tech/feature_store.md`。
- REST / WebSocket endpoint、payload、错误码和安全设计只维护在 `docs/tech/api.md`。
- Event envelope、事件分类、事件流和事件持久化只维护在 `docs/tech/events.md`。
- Strategy / Alpha / Broker 的当前实现边界只维护在 `docs/tech/strategies.md`、`docs/tech/alpha.md` 和 `docs/tech/broker.md`。
- 已归档的 `docs/archive/legacy/*.md` 只作为历史资料，不作为当前实现真源。
- 阶段目标、MVP 范围和发布计划只维护在 `docs/roadmap.md`。
- 本文件只保留跨文档必须共同遵守的规则；不得复制专题文档中的表结构、API payload、完整 trait 或执行计划。

## 产品边界

- Trader 是 Rust 量化交易系统，目标是用统一运行模型支持 Backtest、Replay、Paper 和 Live。
- 系统必须面向多市场和多资产设计，包括 A 股、港股、美股、数字货币现货、永续合约和交割合约。
- 本地可验证交易闭环优先级高于完整实盘券商矩阵。
- 真实资金实盘交易必须处于显式开关、独立凭证、可审计事件和人工确认保护之后。
- 默认交付不得假装接入真实券商；未接真实外部通道时必须标注为 simulated、fake、testnet 或 paper。

## Workspace 规则

目标 workspace 结构：

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

- `apps/trader-cli` 只负责本地运维、数据导入、迁移、回测、Replay、Paper、报告和配置检查。
- `apps/trader-server` 只负责 HTTP / WebSocket 服务、配置加载、storage、event bus、runtime、broker 和 market data adapter 的装配。
- 业务规则必须落在 crate 中，不得沉入 CLI、REST handler 或脚本。
- 新 crate 必须先明确职责、依赖方向和测试边界，再接入 workspace。

## 模块边界

| 模块 | 规则 |
| --- | --- |
| `core` (`trader_core`) | 只放领域类型和领域错误；crate 名避开 Rust 标准库 `core` 冲突。 |
| `events` | 只定义事件 envelope、事件枚举、发布订阅接口和事件持久化边界。 |
| `config` | 只负责 TOML / 环境变量配置解析和有效配置派生。 |
| `storage` | 只负责 SQLite、Parquet、repository、migration 和持久化转换。 |
| `data` | 只负责历史/实时行情模型、bars、ticks、order book 和 `MarketSlice`。 |
| `market_rules` | 只负责市场规则、交易单位、tick、min notional、保证金率等校验。 |
| `universe` | 只负责标的池选择。 |
| `alpha` / `strategies` | 只负责信号生成，不负责下单、持仓、风控或持久化。 |
| `portfolio` | 只负责把信号转换为目标仓位。 |
| `risk` | 只负责风险校验和拒单理由。 |
| `execution` | 只负责把目标仓位转换为订单意图。 |
| `oms` | 只负责订单状态机、client order id、broker order id 映射、恢复和同步。 |
| `broker` | 只负责券商/交易所通道和外部回报映射。 |
| `accounting` | 只负责现金、持仓、PnL、费用、保证金和组合账本。 |
| `metrics` | 只负责收益、回撤、Sharpe、胜率、换手和成交质量指标。 |
| `backtest` | 只负责历史回测 runtime、模拟时钟、成交模型和报告输入。 |
| `replay` | 只负责历史行情回放、暂停、恢复、跳转和倍速。 |
| `api` | 只负责 router、command handler、query handler 和 event broadcast。 |

## 模块 tech.md 汇总与映射

本节只维护模块技术文档索引和短摘要；模块细节以 `docs/tech/*.md` 为准。

| 模块 | crate/app 路径 | tech.md | 文档聚焦点 |
| --- | --- | --- | --- |
| `trader-cli` | `apps/trader-cli` | `docs/tech/trader-cli.md` | 本地运维、迁移、数据导入、feature、回测、paper、broker smoke、报告。 |
| `trader-server` | `apps/trader-server` | `docs/tech/trader-server.md` | 服务进程启动、server config、SQLite migration、API state、Axum 监听。 |
| `core` (`trader_core`) | `crates/core` | `docs/tech/core.md` | 领域基础类型、订单/账户/市场/symbol、低层依赖边界。 |
| `events` | `crates/events` | `docs/tech/events.md` | Event envelope、event bus、typed events、结构化日志写入边界。 |
| `config` | `crates/config` | `docs/tech/config.md` | TOML 模型、server/run 配置分离、broker/risk/live/paper 配置派生。 |
| `storage` | `crates/storage` | `docs/tech/storage.md` | SQLite、migration、repository、command/read model、审计真源。 |
| `data` | `crates/data` | `docs/tech/data.md` | Bar/MarketSlice、CSV/Parquet、外部元数据/资金费率/公司行为采集。 |
| `market_rules` | `crates/market_rules` | `docs/tech/market_rules.md` | lot/tick/min notional、合约规则、fee rule 和 tier engine。 |
| `universe` | `crates/universe` | `docs/tech/universe.md` | static/filtered/ranked 标的池选择和当前行情过滤。 |
| `alpha` | `crates/alpha` | `docs/tech/alpha.md` | AlphaModel、最高置信度、净信号、投票和权重聚合。 |
| `strategies` | `crates/strategies` | `docs/tech/strategies.md` | 策略 registry、技术指标策略、多 alpha 装配、feature gate/rank。 |
| `portfolio` | `crates/portfolio` | `docs/tech/portfolio.md` | Signal 到 TargetPosition 的目标仓位映射。 |
| `risk` | `crates/risk` | `docs/tech/risk.md` | 组合风控、short 权限、live guards、broker 前业务闸门。 |
| `execution` | `crates/execution` | `docs/tech/execution.md` | 目标仓位差额到订单意图、分片、reduce-only/post-only。 |
| `oms` | `crates/oms` | `docs/tech/oms.md` | 订单状态机、部分成交、overfill 防护、report 幂等。 |
| `broker` | `crates/broker` | `docs/tech/broker.md` | Broker trait、fake/simulated、Binance testnet、IBKR paper adapter。 |
| `algorithm` | `crates/algorithm` | `docs/tech/algorithm.md` | 共享决策链、AlgorithmEngine、事件、账户和执行回填。 |
| `backtest` | `crates/backtest` | `docs/tech/backtest.md` | 历史回测 runtime、MarketSlice、fee rule、回测成交和持久化。 |
| `replay` | `crates/replay` | `docs/tech/replay.md` | 行情回放、暂停/恢复/seek、倍速和 market.bar 事件。 |
| `runtime` | `crates/runtime` | `docs/tech/runtime.md` | RunSpec、RuntimeManager、LiveRuntime、worker protocol、取消和恢复。 |
| `paper` | `crates/paper` | `docs/tech/paper.md` | PaperRuntime、simulated/Binance/IBKR paper executor、快照和审计写入。 |
| `accounting` | `crates/accounting` | `docs/tech/accounting.md` | 现金、持仓均价、已实现/未实现 PnL、权益和敞口。 |
| `metrics` | `crates/metrics` | `docs/tech/metrics.md` | 收益、回撤、胜率、Sharpe/Sortino 和 paper summary。 |
| `api` | `crates/api` | `docs/tech/api.md` | Axum router、AppState、REST/WebSocket、运行控制和 response 映射。 |
| `indicators` | `crates/indicators` | `docs/tech/indicators.md` | SMA、EMA、RSI 和指标状态边界。 |
| `feature_store` | `crates/feature_store` | `docs/tech/feature_store.md` | Feature record、manifest、Parquet store、研究特征契约。 |

### 模块简短总结

- `trader-cli` 是本地操作面，负责把命令行、配置和文件装配到 crate 能力上。
- `trader-server` 是服务进程入口，负责配置、数据库、migration、API state 和 HTTP/WebSocket 服务装配。
- `core` 提供跨模块共享领域类型，保持低层、稳定、无业务流程。
- `events` 统一事件 envelope、运行时广播和结构化日志写入边界。
- `config` 是行为配置入口，区分 run template 与 server deployment。
- `storage` 是 SQLite、repository、migration 和审计 read/write model 真源。
- `data` 管理行情模型、历史文件、market slice 和外部数据采集。
- `market_rules` 在 broker 前校验市场交易规则并计算费用。
- `universe` 只选择标的池，不处理风控、订单或持久化。
- `alpha` 和 `strategies` 只生成/组合信号，并保持策略运行可复现。
- `portfolio` 将信号变成目标仓位，short 合法性留给 risk。
- `risk` 是 broker 前最后业务闸门，负责组合风险和 live guard。
- `execution` 将目标仓位差额转换为订单意图和订单片段。
- `oms` 维护订单状态机和 broker report 幂等。
- `broker` 封装外部交易通道，不承担策略、风控或 PnL。
- `algorithm` 执行共享决策链，把 universe、alpha、portfolio、rules、risk、execution、OMS 和 accounting 串起来。
- `backtest`、`replay`、`paper`、`runtime` 分别承担历史回测、行情回放、paper 交易和运行编排差异。
- `accounting` 管理账户现金、持仓、均价和 PnL。
- `metrics` 只做绩效指标和报告输入计算。
- `api` 负责控制面/查询面/事件广播，不直接暴露 storage record。
- `indicators` 提供策略可复用指标状态。
- `feature_store` 提供研究特征、manifest 和 Parquet 契约。

## 依赖方向

- 应用层可以装配 crate，但不得承载领域规则。
- `storage` 不得反向依赖 `api`、`backtest`、`paper`、`broker` 或 `strategies`。
- `strategies` / `alpha` 不得依赖 `broker`、`oms`、`storage`、`api` 或外部交易 SDK。
- `broker` 不得依赖策略、组合、风控或 API response 类型。
- `api` response 必须是 API-owned struct，不得直接暴露 storage record。
- 跨边界数据必须通过明确 read model、command、domain type 或 adapter DTO 转换。

## 运行链路

所有运行模式必须收敛到同一条决策链：

```text
User / Operator
  -> CLI / REST API / WebSocket API
  -> Runtime Manager
  -> BacktestRuntime / ReplayRuntime / PaperRuntime / LiveRuntime
  -> Event Bus
  -> Algorithm Engine
  -> OMS
  -> Broker Adapter
  -> SQLite / Parquet
```

Algorithm Engine 内部顺序必须保持：

```text
Universe Selection
  -> Alpha / Strategy
  -> Portfolio Construction
  -> Market Rule Validation
  -> Risk Management
  -> Execution Model
  -> OMS
```

- Backtest、Replay、Paper、Live 的差异只能由 runtime、adapter 和配置承担。
- 同一个策略必须能在 Backtest、Replay、Paper 和 Live 中复用；策略代码不得根据运行模式分叉访问外部系统。
- 所有订单必须经过 Market Rules、Risk、Execution 和 OMS 后才能进入 Broker。
- Runtime 可以管理取消、pacing、状态持久化和事件发布，但不得绕过 Algorithm Engine 直接产生交易结果。

## 策略与 Alpha 规则

- Strategy 只产生 Signal / Insight，不得直接访问 Broker、OMS、SQLite、WebSocket、REST client 或 Exchange API。
- Alpha model 可以组合，但组合规则必须显式配置，并在代码中有确定性实现。
- 信号置信度、权重、冲突处理和 feature gate 必须可复现，不得依赖隐式全局状态。
- 多标的策略必须按 symbol 维护独立指标状态，避免状态串扰。
- `Sell` 可以表达负目标仓位；是否允许 short 由 Risk 和配置决定，不由策略私自决定。
- `CloseLong` / `CloseShort` 只能表达归零目标，不得被实现成新的方向性开仓。

## Universe 规则

- Universe selector 只负责选标的，不负责风控、下单或持久化。
- `static`、`filtered`、`ranked`、`feature_ranked` 等选择方式必须在配置中显式表达。
- `ranked` 的排序来源必须确定；配置顺序、feature value 或外部排名都必须可审计。
- `feature_ranked` 只能读取只读 feature records；策略运行时不得访问 SQLite 或 Parquet 文件。
- `require_current_data` 只能根据当前 `MarketSlice` 中实际存在的数据收缩 universe。

## 风控与订单规则

- Risk 是订单进入 Broker 前的最后业务闸门。
- Risk 必须校验 max order qty、max order notional、cash buffer、trading halt、short permission、gross exposure、leverage 和 margin limit。
- `allow_short` 必须支持显式全局覆盖；未配置时按 symbol 资产类型保守派生 shortable 集合。
- 股票和 crypto spot 默认不得 short；`CRYPTO_PERP` 和 `CRYPTO_FUTURE` 可以按规则派生为 shortable。
- Gross exposure、leverage 和 margin 校验必须基于目标仓位投影，而不是只看本次订单方向。
- Market Rules 必须校验 lot size、tick size、min qty、min notional 和初始保证金率。
- OMS 是订单状态真源；client order id 必须稳定、可恢复、可用于远端幂等查询。

## Broker 与实盘安全

- Broker 只负责交易通道，不负责风控、仓位管理、策略逻辑、订单拆分或 PnL。
- Broker adapter 必须把外部订单、成交、余额和持仓映射为项目内领域类型。
- 真实 broker executor 只能写入真实 broker 回报确认的成交；不得为了让流程“跑通”伪造成交。
- Paper order submission 必须有 `order_submit_enabled` 一类显式闸门，默认关闭。
- Testnet / paper 自动送单必须校验 broker kind、mode、base URL、账号、凭证和行情来源。
- 凭证只能来自环境变量或受控 secret provider；不得写入配置文件、日志、报告或事件 payload。
- 手动 tiny order、撤单、open order 清理等真实外部操作必须有显式确认参数。
- Live surface 不得提供绕过 Runtime、Risk、Execution、OMS 的手动下单 API。

## 存储规则

- SQLite 是交易状态、运行状态、订单、成交、账户、持仓、组合快照和审计事件的真源。
- Parquet 是历史行情、研究数据和 feature store 文件的真源。
- SQLite / SQL / `sqlx` 只属于 `storage` 边界。
- 边界外生产路径不得构造 storage 写入 DTO，不得透传 `SqlitePool`，不得拼 SQL。
- 写入必须走 `storage` 暴露的语义 command / repository 方法。
- 对外读取必须返回明确 read model，不复用写入 DTO。
- REST 查询路由必须再映射为 API-owned response struct。
- 金额、数量和 feature value 在 Rust 内优先使用 Decimal；进入 SQLite 或 Parquet 时必须有明确精度策略。

## 数据与研究规则

- 多标的行情统一用 `MarketSlice` / `SymbolBar` 表达同一时间点的数据。
- 单文件 `[data] source/path` 只作为单标的兼容入口；新增多标的流程优先使用 `[[data.inputs]]`。
- Feature Parquet schema 必须稳定，至少包含 `run_id`、`symbol`、`name`、`ts_ms`、`value`、`version`。
- Feature `value` 必须保留 Decimal 精度；不得用隐式浮点转换破坏研究可复现性。
- Feature manifest 必须能校验 parquet path、schema、run id、symbols、feature name、version 和可选 build contract。
- Backtest / Paper 装配边界负责读取 Parquet 和校验 manifest；策略运行时只接收内存 feature records。
- 研究特征和回测行情源必须防止漂移；有输入快照时必须校验 content hash、bar count、首尾时间戳。

## API 与事件规则

- API 不直接暴露数据库，不绕过 OMS 下单，不绕过 Risk 控制。
- Command API 负责触发运行或控制运行；Query API 负责读取已持久化 read model。
- WebSocket 只能发布 runtime events 和可回放事件，不得成为独立状态真源。
- Event payload 必须由 typed payload struct 构造后序列化，避免各 runtime 手写 JSON 漂移。
- `event_store` 是审计事件真源；内存 event bus 只负责运行时广播。
- Replay 必须发布 `market.bar` 等行情事件，并写入必要 lifecycle events。

## 配置规则

- 配置必须是行为真源；不得在 CLI、REST handler 或 runtime 中隐藏硬编码风控默认值。
- `[risk]` 负责风控阈值和 short 权限。
- `[broker]` 负责 broker kind、mode 和送单闸门。
- `[live]` 负责 live 是否启用和运行心跳等 live-only 参数。
- `[paper]` 负责 paper account、费用、滑点和 paper-only 参数。
- 配置解析后必须生成有效配置；非法组合应在 preflight 或启动阶段失败。
- 配置别名可以支持兼容，例如 `ibkr` 映射到 `interactive_brokers`，但内部枚举必须规范化。

## 技术栈规则

- 语言和 workspace：Rust，Cargo resolver 2。
- Async runtime：Tokio。
- HTTP / WebSocket：Axum、tower、tower-http。
- 序列化：serde、serde_json、toml。
- 错误处理：thiserror 用于领域错误，anyhow 用于应用边界。
- 时间与 ID：chrono / time、uuid。
- 金额与数量：rust_decimal。
- 结构化日志：tracing、tracing-subscriber。
- SQLite：sqlx，只能在 `storage` 边界使用。
- 历史与研究数据：Apache Arrow / Parquet、Polars。
- CLI：clap。
- HTTP / WS client：reqwest、tokio-tungstenite。
- 引入新基础依赖前必须说明它替代什么、边界在哪里、如何测试。

## 验证门禁

- 修改领域规则、订单链路、风控、持久化或 broker adapter 后，必须补充或更新相应测试。
- 修改 API contract 前必须同步 `docs/tech/api.md`，并验证调用方和 response shape。
- 修改数据库 schema 前必须同步 migration、repository、`docs/tech/storage.md` 和恢复路径。
- 修改事件 schema 前必须同步 `docs/tech/events.md`，并验证 event replay / WebSocket 消费路径。
- 修改 feature / research 路径前必须验证 Parquet round-trip、manifest 校验和 Backtest / Paper 装配路径。
- Paper / broker 相关改动必须至少覆盖无网络 smoke；真实外部连接测试必须受显式确认参数保护。
- 本地完整验证命令以 `scripts/` 中的 smoke/readiness 脚本为准；本文只记录门禁原则，不复制脚本清单。

## 禁止事项

- 不得把阶段计划、TODO 清单、实施日志写入本文。
- 不得在本文复制长命令清单、临时 runner 输出或账号环境说明。
- 不得让策略直接读写 storage 或调用 broker。
- 不得让 API handler 直接拼 SQL、构造 storage DTO 或返回 storage record。
- 不得让 Broker 承担风控、PnL、策略判断或订单拆分。
- 不得在没有真实 broker execution / trade 回报时写入真实 fill。
- 不得把凭证写入仓库、配置、日志、报告、事件 payload 或测试快照。
- 不得为了单个运行模式破坏共享 Algorithm Engine 链路。

## 参考

- 设计文档入口：`docs/README.md`
- 阶段计划入口：`docs/roadmap.md`
- 历史执行计划：`docs/superpowers/plans/`
- 代码参考：`D:\code-refer\trader`
