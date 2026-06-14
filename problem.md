## 2026-06-10 修复状态

本轮已按大重构计划先收敛主链路边界，暂不替换 Binance 底层 Rust client。

已修复：

- P1 存储边界：Backtest、Paper、API、CLI 生产路径不再直接构造 storage 写入 DTO；运行层改走 `storage` 暴露的语义 command / repository 方法。`scripts/check-storage-dto-boundary.ps1` 已扩大到 `crates` 与 `apps`，用于防止 storage DTO 写入形态再次泄漏到边界之外。
- P2 事件契约：Algorithm 事件 payload 改为 typed payload struct 后再序列化；EventBus / SQLite replay / WebSocket 过滤基于稳定 run source 语义。
- P3 Paper 生命周期：submitted、failed、execution result、portfolio snapshot、final state 的持久化转换集中到 `storage`。
- P4 Replay 控制闭环：正在运行的 replay loop 会响应 pause、resume、seek、speed，REST / WebSocket / replay runtime 测试已覆盖。
- P5 Universe / Alpha 动态装配：`strategy.universe`、`strategy.alpha`、`strategy.symbols` 现在进入 Backtest / Paper runtime 的实际 `StrategyRegistry::assemble_alpha` 链路，不再只是配置字段。
- 后续切片 CLI read model 隔离：CLI 只读查询与对账 helper 不再接收 `storage::NewOrder` / `storage::NewFill`，从 storage 查询结果进入 CLI 后立即映射为本地 `LocalOrder` / `LocalFill`。
- 后续切片 storage read/write DTO 命名隔离：`storage` 查询 API 已返回 `StoredOrder` / `StoredFill` / `StoredPosition` / `StoredAccountBalance` / `StoredPortfolioSnapshot` 读模型，`New*` 类型只保留为 storage 边界内写入输入；边界检查已禁止外部代码直接引用这些写入 DTO，并禁止 storage 公开读取 API 返回写入 DTO。
- 后续切片 API read model 隔离：REST 查询路由已把 run/event storage record 映射成 API-owned `RunResponse` / `EventResponse`，不再把 storage record 类型作为 HTTP response contract；`scripts/check-api-read-model-boundary.ps1` 已接入 `scripts/verify.ps1`。
- 后续切片 REST event payload 契约：`GET /api/v1/events` 与 `GET /api/v1/runs/{run_id}/events` 已返回结构化 JSON `payload`，不再把 storage 内部 `payload_json` 字符串直接暴露给客户端。
- 后续切片 REST run config 契约：`GET /api/v1/runs` 与 `GET /api/v1/runs/{run_id}` 已返回结构化 JSON `config`，不再把 storage 内部 `config_json` 字符串直接暴露给客户端。
- 后续切片边界门禁跨平台收口：DB 边界、storage DTO 边界、API read model 边界均已补齐 bash 版本，并接入 Linux/macOS `scripts/verify`；Windows `verify.ps1` 与 bash `verify` 执行同一类架构门禁。
- 后续切片 feature_store Parquet adapter：`feature_store` 已提供 Parquet-backed repository 和 feature record round-trip 读写，Decimal feature value 以字符串保存并读回为 `Decimal`；该切片不引入 SQL，不绕过 storage SQL 边界。
- 后续切片 Universe / Alpha 真实多标的能力：`data::MarketSlice` / `SymbolBar` 已表示同一时间点多标的行情；`StaticUniverseSelector` 返回配置的完整 symbol 集合；`StrategyRegistry::assemble_alpha` 会为多标的 moving average alpha 建立 per-symbol 独立状态；`AlgorithmEngine::on_market_slice`、Backtest `run_market_slices` 和 Paper `run_market_slices` 会按每个有行情 symbol 生成订单、成交和持仓，并按全组合价格表计算权益、敞口和未实现盈亏。CLI / REST 的 Backtest 与 Paper 入口已支持 `[[data.inputs]]`，可直接把多个 symbol 映射到各自 CSV / Parquet 文件并合并为 `MarketSlice`；Paper runtime 也支持 channel-based `MarketSlice` stream；旧 `[data] source/path` 单文件配置保持兼容包装。
- 后续切片配置化 Universe 选择：`universe = "filtered"`、`universe = "ranked"` 与 `universe = "feature_ranked"` 已接入 StrategyRegistry、Backtest、Paper、CLI 和 REST；`[strategy.universe_filter]` 支持通用的 `include_symbols`、`exclude_symbols`、`symbol_prefixes`、`require_current_data` 与 `max_symbols`，可在候选 symbols 和当前 MarketSlice 可用数据上动态收缩 universe；`ranked` 使用 `symbols` 配置顺序作为 rank，并用 `max_symbols` 截取前 N 个通过过滤条件的标的；`feature_ranked` 通过 `[strategy.universe_rank]` 从只读 Feature Parquet 读取不晚于当前 bar 的最新 feature value 排名，支持 manifest 与 version 校验，运行时只使用内存 feature records，不引入 SQL，不限定具体市场或业务模块。
- 后续切片配置化 Alpha 组合：`[[strategy.alpha_components]]` 已接入 config、StrategyRegistry、Backtest、Paper、CLI 和 REST；每个 component 支持独立 `fast_window`、`slow_window`、`weight` 与可选 `category`，权重会缩放 signal confidence。当前冲突策略支持 `alpha_conflict_resolution = "highest_confidence"` 选择加权 confidence 最高的信号，支持 `alpha_conflict_resolution = "net_signal"` 将正负方向按已加权 confidence 抵消后只输出净方向，支持 `alpha_conflict_resolution = "majority_vote"` 按 component 方向票数选择多数方向，也支持 `alpha_conflict_resolution = "category_majority"` 先在 category 内净信号聚合、再跨 category 多数投票，未配置 `category` 时默认按 component `name` 分组。仓库内已提供 `configs/backtest/weighted_alpha_ma_cross.toml`、`configs/backtest/net_signal_alpha_ma_cross.toml`、`configs/backtest/majority_vote_alpha_ma_cross.toml` 和 `configs/backtest/category_majority_alpha_ma_cross.toml` 四类样例。
- 后续切片 Alpha 模型注册扩展：Strategy/Alpha registry 已新增 `exponential_moving_average_cross`、`price_momentum`、`price_channel_breakout`、`price_channel_reversion` 和 `relative_strength_index_reversion`；EMA 交叉复用 Decimal `ExponentialMovingAverage` 指标，价格动量使用 Decimal close 价格斜率比较，价格通道突破使用 Decimal close 价格通道判断，价格通道均值回归复用同一通道判断生成反向信号，RSI 均值回归复用 Decimal `RelativeStrengthIndex` 指标并在 RSI 低于 `100 - slow_window` 时 Buy、高于 `slow_window` 时 Sell。它们分别接入 Backtest、CLI 与 REST 样例 `configs/backtest/ema_cross.toml`、`configs/backtest/price_momentum.toml`、`configs/backtest/price_channel_breakout.toml`、`configs/backtest/price_channel_reversion.toml`、`configs/backtest/rsi_reversion.toml`；`moving_average_cross` 继续作为 SMA 交叉模型保留。
- 后续切片 Sell / 短仓语义：Portfolio target 已从旧 MVP 的 “Sell 只打平” 改为 signed target quantity：`Buy` 目标为正仓位，`Sell` 目标为负仓位，`CloseLong` / `CloseShort` 目标为 0；Accounting 支持负持仓、卖空开仓、买入回补和短仓未实现盈亏；Risk 使用目标仓位投影 gross exposure，避免 Sell/short 绕过敞口限制，同时允许真实减仓卖出降低敞口。
- 后续切片短仓权限风控：`[risk] allow_short` 已接入 config、CLI、REST、Backtest、Paper、Algorithm 和 Risk；显式 `true` 允许所有策略 symbol 产生负目标仓位，显式 `false` 全部禁止。未配置时按 `strategy.symbols` 逐标的保守派生：`CRYPTO_PERP` / `CRYPTO_FUTURE` symbol 默认允许 short，股票、crypto spot 或无法识别的 symbol 默认禁止。混合 Universe 不再被全局 bool 一起压成禁止，crypto derivative 可以 short，非 shortable 标的仍由 Risk 拒绝。该规则是通用风控开关，不限定具体策略或模块；可能产生股票短仓的样例配置已显式设置 `allow_short = true`。
- 后续切片衍生品保证金风控：`market_rules` 已为 `CRYPTO_PERP` / `CRYPTO_FUTURE` 提供 10% 初始保证金率，股票与 crypto spot 初始保证金率为 0；`AlgorithmEngine` 在目标仓位投影时按全组合持仓价格表计算 projected `margin_used` 并交给 `Risk` 的 `max_margin_used` 校验。`max_margin_used = 0` 保持兼容语义，表示不启用绝对保证金上限；配置为正数时会拒绝超过上限的衍生品目标仓位。
- 后续切片 Alpha feature gate：`[strategy.alpha_gate]` 已接入 config、StrategyRegistry、Backtest、Paper、CLI 和 REST；当前支持只读 Parquet feature source，按 `run_id + symbol + feature_name` 读取不晚于当前 bar 的最新 feature，并支持可选 `version` 约束研究特征批次；缺失、不匹配版本或不满足 `min_value` / `max_value` 区间时抑制 Alpha 信号。该切片复用 `feature_store`，不引入 SQL，不透传 `SqlitePool`。
- 后续切片 feature manifest：`feature_store` 已提供通用 `FeatureManifest`，可从 Feature Parquet records 汇总 `schema_version`、`parquet_path`、`record_count`、`run_ids`、`symbols`、`feature_names` 和 `versions`，并支持 JSON round-trip；CLI 已提供 `trader feature-manifest --parquet <path> --output <manifest.json>` 生成 manifest；`[strategy.alpha_gate].manifest_path` 可让 CLI / REST 在装配 Backtest / Paper settings 前校验 manifest 的 `parquet_path`、schema、`run_id`、策略 symbols、`feature_name` 和 `version` 是否覆盖当前 gate。该切片只描述 Parquet 研究特征元数据，不引入 SQL，不绕过 storage 边界。
- 后续切片 feature 生成入口：CLI 已提供通用 `trader feature-build-indicator --indicator sma|ema|rsi`，可从 CSV / Parquet bars 的 close 价格生成 SMA / EMA / RSI feature Parquet，并同步写 manifest；既支持 `--source/--input/--symbol` 单标的输入，也支持 `--inputs-config <config.toml>` 复用现有 `[[data.inputs]]` 多标的配置生成合并 feature Parquet。旧 `trader feature-build-sma` 保持兼容并复用同一生成路径。仓库内已提供单标的 `configs/backtest/sma_feature_gate.toml`、负向阈值抑制样例 `configs/backtest/sma_feature_gate_suppressed.toml`、`datasets/features/aapl_sma_2.parquet`、`datasets/features/aapl_sma_2.manifest.json`，RSI gate 样例 `configs/backtest/rsi_feature_gate.toml`、`datasets/features/aapl_rsi_3.parquet`、`datasets/features/aapl_rsi_3.manifest.json`，多标的 `configs/backtest/multi_symbol_sma_feature_gate.toml`、`datasets/features/multi_symbol_sma_2.parquet`、`datasets/features/multi_symbol_sma_2.manifest.json`，以及 feature 排名 Universe 样例 `configs/backtest/feature_ranked_universe_ma_cross.toml`、`datasets/features/multi_symbol_sma_1.parquet`、`datasets/features/multi_symbol_sma_1.manifest.json`，覆盖 `bars -> feature-build-indicator -> feature manifest -> feature-ranked universe / alpha gate -> backtest` 的本地研究闭环。该命令复用 `data::load_bars`、`indicators` 和 `feature_store` schema，不引入 SQL，不绕过 storage 边界。
- 后续切片 feature manifest 构建契约治理：`FeatureManifest` 已支持可选 `build_contract`，用于记录 feature builder、indicator、value_column、period、run_id、feature_name、version 以及生成 feature 时使用的 bars inputs。`feature-build-indicator` 与兼容入口 `feature-build-sma` 会自动写入该契约；旧 manifest 不带 `build_contract` 时仍兼容加载。CLI / REST 在 `[strategy.alpha_gate].manifest_path` 或 `[strategy.universe_rank].manifest_path` 装配边界会继续校验 Parquet / run_id / symbol / feature / version，并在 manifest 带 `build_contract` 时额外校验当前 Backtest / Paper 的 data inputs 与生成 feature 的 source bars 一致。bars input 可携带 `content_hash`、`bar_count`、`first_ts_ms`、`last_ts_ms` 快照；CLI / REST 会重新加载当前 bars 并复算快照，能拒绝同一路径下文件内容或时间范围已变化但 manifest 仍旧的漂移。配置可选 `build_indicator`、`build_period`、`build_value_column` 后，装配边界也会校验 manifest 构建参数，避免研究/训练特征和回测行情源或生成方式漂移。仓库内 feature gate / feature-ranked universe 样例已写入这些期望字段，样例 manifest 也已补齐 `build_contract` 与输入快照。该治理仍是文件元数据检查，不引入 SQL 或 storage 边界外持久化。
- 后续切片 Binance client 演进：`BinanceSpotTestnetAdapter` 已抽出 `BinanceHttpClient` 边界，默认仍使用 `ReqwestBinanceHttpClient`；read-only 与下单调用已可通过 fake client 验证，后续替换 Rust SDK 时不需要改 Paper runtime 主链路。
- 后续切片 IBKR client 稳定性：`IbkrPaperGatewayAdapter` 已抽出 `IbkrGatewayClient` 边界，默认仍使用 `IbapiIbkrGatewayClient`；账号校验、open orders、executions、next order id、place/cancel order 已可通过 fake Gateway client 做无网络验证。

已验证：

- `powershell -ExecutionPolicy Bypass -File .\scripts\verify.ps1`（默认 `Jobs=1`，包含 `cargo fmt --all -- --check`、`cargo check --workspace -j 1`、`cargo test --workspace -j 1`）
- `cargo test -j 1 -p storage -p backtest -p paper -p strategies -p config -p runtime`
- `cargo test -j 1 -p trader-cli --bin trader`
- `cargo test -j 1 -p api`
- `cargo test -j 1 -p replay -p algorithm -p events`
- `cargo check -j 1 -p api -p trader-cli`
- `powershell -ExecutionPolicy Bypass -File .\scripts\check-storage-dto-boundary.ps1`
- `powershell -ExecutionPolicy Bypass -File .\scripts\check-db-boundary.ps1`
- `powershell -ExecutionPolicy Bypass -File .\scripts\mvp-smoke.ps1`
- `powershell -ExecutionPolicy Bypass -File .\scripts\paper-smoke.ps1`
- `git diff --check`

仍待后续独立切片：

- Binance 底层 SDK 仍暂不替换；HTTP client 边界 spike 已覆盖 read-only 与下单路径。后续如迁移 Rust SDK，应继续保持现有 `BinanceSpotTestnetAdapter` / `BinancePaperOrderClient` 领域边界。
- IBKR 目前保留 `ibapi` adapter 路线；Gateway client 边界已可 fake 验证，仍待有真实 paper 账号与本机 TWS / Gateway 环境后做完整生命周期验证。

我已基于 `docs/architecture.md`、`tech.md` 和当前代码重新核对项目状态。下面把原先的粗略问题分析细化为可以继续推进实现的问题清单。

## 总体结论

项目并不是“架构没做”，而是已经完成了本地 MVP 和生产前 paper 验证的大部分基础设施：策略接口、运行模式、市场规则、OMS、EventBus、SQLite/Parquet、Backtest/Paper/Replay/API/WS 都已有可运行闭环。

当前核心问题也不是“缺模块”，而是：**现有模块还没有完全收敛成 architecture.md 设想的统一事件驱动主链路。** 现在的主流程仍以 runtime 内部顺序编排为主，事件主要承担审计、广播和回放观察职责；存储 DTO、事件 payload、runtime 状态之间的边界还不够清晰。

## 已具备的基础

### 1. 策略边界已经基本成立

`crates/strategies`、`crates/alpha` 和 `crates/algorithm` 已经把策略/Alpha 限定为生成信号，不直接访问 Broker、OMS、SQLite 或 API。`AlgorithmEngine` 统一执行 Universe -> Alpha -> Portfolio -> MarketRules -> Risk -> Execution -> OMS 的决策链。

结论：这里不是“策略边界缺失”，而是后续要增强策略装配、Alpha 生态和动态 Universe。

### 2. 多运行模式已经存在

Backtest、Replay、Paper、Live surface 都已出现在配置、runtime、API 和 CLI 链路里。Backtest/Paper 已复用 `AlgorithmEngine`，Replay 已具备速度、暂停、恢复、跳转和 market event 发布能力。

结论：这里不是“运行模式没实现”，而是各 runtime 对事件、状态和持久化的统一程度还不一致。

### 3. EventBus 已接入

`AlgorithmEngine`、Backtest、Paper、Replay、API 和 WebSocket 都已经使用 EventBus。WebSocket 订阅会先回放 SQLite 中的 run events，再转发 EventBus 中匹配 run_id 的 runtime events。

结论：原问题中“事件总线未使用”的说法不准确。更准确的问题是：EventBus 目前不是业务链路的驱动机制，而是运行结果的广播和观察机制。

### 4. OMS 与 broker paper 执行边界已有基础

OMS 状态机已处理提交、接受、成交、部分成交、乱序和终态。Paper runtime 支持 simulated、Binance Spot Testnet、IBKR paper executor，真实 paper 成交只从 broker trades/executions 写入，不伪造成交。

结论：订单链路已具备本地可验证能力；后续重点是统一审计事件、恢复状态和持久化边界。

## 历史问题清单与当前状态

下面 P1-P5 是本轮重构前的问题分析。每项保留原始缺口、风险和验收标准，同时补充当前状态，便于后续判断哪些已经关闭，哪些只是进入后续切片。

## P1：存储边界（已收敛，保留历史分析）

当前状态：Backtest、Paper、API、CLI 生产写入路径已改为调用 `storage` 暴露的语义 command / repository 方法；边界外不再直接构造 storage 写入 DTO。`scripts/check-storage-dto-boundary.ps1` 已作为回归检查。

### 现状

`crates/storage/src/repositories.rs` 集中定义并暴露 `NewOrder`、`NewFill`、`NewPosition`、`NewAccountBalance`、`NewPortfolioSnapshot`、`StoredRuntimeEvent`、`BacktestExecutionRecord`、`BacktestPositionRecord` 等具体 record。Backtest 和 Paper runtime 直接构造这些 record 并调用 `Db` 方法写入 SQLite。

典型表现：

- `crates/backtest/src/backtest.rs` 直接依赖 `BacktestExecutionRecord`、`BacktestPositionRecord`、`StoredRuntimeEvent`。
- `crates/paper/src/paper.rs` 直接依赖 `NewOrder`、`NewFill`、`NewEventRecord`、`NewPortfolioSnapshot` 等 SQLite 写入 DTO。
- `storage::Db` 同时承担连接、SQL、领域写入聚合、事件回放等职责。

### 真正缺口

上层 runtime 现在知道太多 SQLite record 细节。业务层需要拼接字符串状态、decimal string、event payload JSON 和持久化 ID，这会让 Backtest/Paper/Recover/API 的行为难以统一。

### 风险

- 相同业务事件在不同 runtime 中可能落库形态不一致。
- 订单状态、成交、账户、快照的更新顺序难以集中约束。
- 后续迁移 repository、事务边界或事件溯源时，需要大范围修改 runtime。
- 金融审计要求下，业务事件和持久化记录之间缺少明确转换边界。

### 建议修改方向

优先收敛为 runtime-facing repository 接口，而不是继续让 runtime 直接拼 storage DTO：

- 增加 `RunRepository`：负责 run lifecycle。
- 增加 `ExecutionRepository` 或 `OrderRepository`：负责订单、成交、恢复状态。
- 增加 `PortfolioRepository`：负责账户余额、持仓、组合快照。
- 增加 `EventRepository`：负责 runtime event 持久化与 replay。
- 保留 SQL 只在 `storage` crate 内部；其它 crate 只传领域语义结构或专用 command。

### 验收标准

- Backtest runtime 不再直接构造 `BacktestExecutionRecord` / `BacktestPositionRecord`。
- Paper runtime 不再直接构造 SQLite 风格的 `NewOrder` / `NewFill` / `NewPortfolioSnapshot`。
- Decimal 到 string、状态枚举到数据库字符串、event_id 生成等细节集中在 `storage` 内。
- 原有 `mvp-smoke.ps1`、`paper-smoke.ps1`、Binance/IBKR paper smoke 不发生行为回退。

## P2：AlgorithmEngine 事件契约（已稳定 typed payload，保留历史分析）

当前状态：Algorithm 事件 payload 已改为 typed payload struct 后序列化；run-level 事件过滤基于稳定 source / run_id 语义，EventBus、SQLite replay、WebSocket 路径已有覆盖。

### 现状

`AlgorithmEngine` 已统一执行核心交易链路，并生成 `EngineEvent`。这些事件会发布到 EventBus，也会由 Backtest/Paper 转写进 SQLite。

当前事件更像“决策过程记录”：

- Universe selected
- Alpha generated
- Portfolio target generated
- Market rule validated
- Risk approved
- Execution order generated
- OMS submitted/accepted
- Broker filled/unfilled
- Accounting updated

### 真正缺口

architecture.md 期望的是更统一的事件驱动主链路，但现在模块之间仍是 `AlgorithmEngine` 内部同步调用。事件没有成为下游处理的强契约，payload 也是字符串 JSON，缺少 typed event schema 和版本边界。

这并不意味着要立刻把整个系统改成完全异步事件流。当前 MVP 更合理的目标是：**先把事件定义稳定为可审计、可回放、可消费的契约，再考虑是否进一步事件驱动化。**

### 风险

- EventBus 消费者只能解析 JSON 字符串，缺少编译期约束。
- 不同 runtime 对同一事件的 source、category、payload 字段可能漂移。
- Replay/WebSocket/API 对事件的过滤依赖 payload 中的 `run_id`，这比 envelope source 更脆弱。
- 后续增加策略组合、跨标的、多订单时，事件 payload 的含义可能不够稳定。

### 建议修改方向

- 在 `events` 或 `algorithm` crate 中定义 typed algorithm event payload，而不是只暴露 `payload_json: String`。
- 统一 event source 语义：run-level 事件 source 应稳定使用 run_id，模块来源放入 payload 或 category。
- 让 `AlgorithmEngine` 输出领域事件集合，由 runtime 决定持久化、广播和执行副作用。
- 明确哪些事件是审计真源，哪些只是 UI/WS runtime notification。

### 验收标准

- WebSocket run_id 过滤不再必须依赖解析 payload JSON。
- Backtest/Paper 对同一 `EngineEventKind` 的持久化字段一致。
- 事件类别、source、payload schema 在测试中有覆盖。
- `event_store` 中的事件足以重建一次 run 的关键决策和订单生命周期。

## P3：Paper runtime 持久化边界（已收敛，保留历史分析）

当前状态：Paper submitted、failed、execution result、portfolio snapshot、final state 的持久化转换已集中到 `storage` command / repository 方法；simulated、Binance、IBKR executor 继续走同一 Paper runtime 持久化路径。

### 现状

`PaperRunSession::process_bar` 同时负责：

- 调用 `AlgorithmEngine::on_bar`。
- 持久化 engine events。
- 写入 pending order。
- 调 broker executor。
- 调用 `AlgorithmEngine::apply_execution`。
- 写入 order/fill/event。
- 写入 portfolio snapshot。

### 真正缺口

Paper runtime 是当前最接近真实交易的路径，但它把“策略决策、订单提交审计、broker 执行、成交应用、账户快照、持久化”集中在一个函数中。短期能跑通，但后续做恢复、重试、部分成交、跨 broker 差异时复杂度会快速上升。

### 风险

- broker 失败时，本地 pending order、event_store、run status 的一致性不容易保证。
- 成交为 0、部分成交、撤单、恢复同步等状态在同一流程里继续扩展会变得脆弱。
- Binance 和 IBKR executor 的审计语义可能逐渐分叉。

### 建议修改方向

- 把 paper order lifecycle 拆为显式步骤：`record_submitted_order`、`execute_order`、`record_execution_result`、`apply_accounting_snapshot`。
- 让每一步都返回明确结果，失败时能记录可恢复状态。
- 统一 simulated/Binance/IBKR 的 executor result 到一个 typed `PaperExecutionResult`。
- 对 0 fill、partial fill、filled、cancelled/rejected 分别建立测试。

### 验收标准

- Paper runtime 在 broker executor 失败时保留可恢复 pending order 和审计事件。
- 0 fill 不写 fill、不更新账本，但会写 unfilled/order lifecycle event。
- partial fill 和 full fill 的本地 order/fill/accounting 结果可区分。
- Binance/IBKR/simulated executor 走同一持久化路径。

## P4：Replay 控制闭环（已补齐，保留历史分析）

当前状态：正在运行的 replay loop 已响应 pause、resume、seek、speed；REST、WebSocket 和 replay runtime 测试已覆盖控制动作对执行 loop 的影响。

### 现状

Replay 已有 `ReplayController`、`ReplayRuntime`、REST 控制路由和 WebSocket `replay_control` 消息。Replay runtime 会发布 `market.bar` 事件，控制动作会写入 event_store。

### 真正缺口

Replay runtime 本身并没有消费 `ReplayController` 的 pause/resume/seek/speed 状态。REST/WS 控制可以更新 controller，但已经启动的 replay loop 仍按创建时速度顺序 sleep 和发布 bars。也就是说，控制状态和回放执行之间还没有真正闭环。

### 风险

- API/WS 返回的 replay 状态可能和实际 replay 执行不同步。
- pause/seek/speed 看起来成功，但对已运行任务没有实际影响。
- 用户很难通过事件流判断 replay 当前偏移和控制动作是否生效。

### 建议修改方向

- 让 `ReplayRuntime` 接收共享 `ReplayController` 或 runtime manager handle。
- replay loop 每个 bar 前读取 controller 状态：paused 时等待，seek 时调整 offset，speed 变化时更新 delay。
- 每次状态变化发布 typed replay control event。
- `market.bar` payload 加入 run_id、offset、speed，方便 WS/API 过滤和调试。

### 验收标准

- REST/WS pause 后，正在运行的 replay 不继续发布新的 `market.bar`。
- resume 后从当前 offset 继续发布。
- seek 后下一条 `market.bar` 的 offset 与请求一致。
- speed 修改影响后续 bar 的 pacing。
- WebSocket 订阅能稳定收到 replay control 和 market bar 事件。

## P5：Universe / Alpha 动态装配（主链路已接入，后续扩展保留）

当前状态：`strategy.universe`、`strategy.alpha`、`strategy.symbols` 已进入 Backtest / Paper runtime 的实际 `StrategyRegistry::assemble_alpha` 链路；`[[data.inputs]]` 已把 CLI / REST 配置入口接到多标的 `MarketSlice`；Universe 已支持 static、filtered、ranked 和 feature_ranked 四类 selector；`[[strategy.alpha_components]]` 已接入最高置信度、净信号、多数投票和按 category 分层投票四类冲突处理；`[strategy.alpha_gate]` 已接入只读 feature gate；Strategy/Alpha registry 已支持 SMA 交叉、EMA 交叉、价格动量、价格通道突破、价格通道均值回归与 RSI 均值回归六个模型；Sell 信号现在会生成负目标仓位并可在 Backtest/Paper 主链路形成短仓，Risk 使用 gross exposure 与衍生品 initial margin 做目标仓位投影，并通过通用 `[risk] allow_short` 的显式覆盖值或按 symbol 派生的 shortable 集合控制是否允许 short。当前仍保留更复杂 Universe selector、更多 Alpha 模型和完整研究流水线作为后续切片。

### 现状

`StaticUniverseSelector`、`AlphaModel`、`CompositeAlphaModel` 和 moving average cross 策略已经接入主链路。当前足够支撑本地 MVP 和 paper smoke。

### 真正缺口

Universe 已支持静态、过滤、配置顺序排序与 Feature Parquet 排序四类 selector，Alpha 组合已支持最高置信度、净信号、多数投票和按 category 分层投票四类冲突处理，Alpha registry 已支持 SMA / EMA 均线交叉、价格动量、价格通道突破、价格通道均值回归和 RSI 均值回归模型。距离多市场动态筛选、更多非均线 Alpha 模型和研究流水线还有差距。

### 风险

- 过早扩展复杂策略会放大 P1/P2/P3 的边界问题。
- 多标的、多 Alpha、多订单会暴露当前事件 payload、订单 ID、portfolio target 结构的限制。

### 建议修改方向

此项已完成主链路接入，但仍不应一次性扩成复杂研究平台。后续可以在已稳定的存储边界、事件契约、Paper 生命周期和 Replay 控制闭环之上继续推进。

后续可以逐步增加：

- 配置化 Universe selector。
- 更复杂的 Universe selector。
- 更多非均线 Alpha 模型与注册能力。
- Strategy/Alpha registry 的更多模型注册。
- feature store 研究流水线继续扩展，例如 feature 生成、版本治理和训练/回测一致性检查。

### 验收标准

- 一个配置可以选择 universe selector 和 alpha model。
- 多标的输入不会破坏当前单标的 smoke。
- 事件 payload 能表达 symbol、portfolio target、risk decision 和 order intent 的多标的上下文。
- 负目标仓位必须由通用 `[risk] allow_short` 显式覆盖值或按 symbol 派生的 crypto derivative shortable 集合放行；未开启且当前 symbol 不在 shortable 集合中时 Backtest/Paper 主链路会拒绝 short 决策。
- 衍生品目标仓位必须按 market rules 的初始保证金率投影 `margin_used`；`max_margin_used` 为正数时 Backtest/Paper 主链路会拒绝超过保证金上限的目标仓位。

## 后续路线

### 已完成：存储边界收敛

Backtest/Paper 对 `storage::Db` 和具体写入 record 的直接依赖已降下来。原因是它影响审计一致性、恢复路径、paper broker 稳定性，也会降低后续事件契约改造的风险。

### 已完成：事件契约稳定

事件 source/category/payload schema 已稳定为 typed payload -> JSON wire shape；EventBus、SQLite replay、WebSocket 过滤基于一致契约。

### 已完成：Paper 订单生命周期持久化收敛

Paper 是最接近真实 broker 的路径，pending order、broker result、fill/accounting、unfilled/cancel/recover 的持久化语义已统一到 storage command / repository 边界。

### 已完成：Replay 控制闭环

Replay 控制 API 已影响正在运行的执行 loop，pause/resume/seek/speed 具备本地测试覆盖。

### 后续：Universe / Alpha 动态能力

主链路配置化装配已完成。后续继续推进多标的、多 Alpha、研究数据接入时，应保持每个切片可验证，避免一次性引入不可定位的复杂度。

## 大重构计划推进原则

这些问题不应被解释为“暂时不做”。项目仍处于新阶段，应该把统一事件驱动主链路、存储边界收敛、Replay 控制闭环、Alpha/Universe 动态能力和 broker client 演进都纳入同一个大重构计划，避免后续遗忘或因为局部设计过早定型而返工。

但实现上不要无分层地混改。每个切片都必须能单独验证，并且尽量保持现有 smoke 与 paper runner 可运行。这样做的目的不是保护线上用户，而是保护工程反馈：一旦失败，可以判断问题来自持久化边界、事件契约、runtime 调度、Replay 控制、策略装配，还是 broker adapter。

大重构计划应遵循：

- 先给出统一目标设计，明确最终主链路、事件契约、持久化边界、runtime 控制和 broker adapter 的职责。
- 同一计划内覆盖 P1-P5，不把 Alpha/Universe 或 Binance/IBKR client 迁移排除在路线之外。
- 实施顺序按可验证切片推进：每个切片只改变一类边界，并保留可运行验证命令。
- 每个切片完成后更新文档和 smoke 证据，再进入下一切片。
- 禁止在 storage 边界之外引入 SQL、`SqlitePool` 或 `sqlx::Error` 泄漏；若切片需要数据库能力，先补持久化边界接口。

## 下一步建议

建议先写一个完整实施计划：**Unified event-driven runtime refactor plan**。该计划必须覆盖 P1-P5 的最终目标，但第一批执行切片从存储边界收敛开始，以 Backtest/Paper 为入口，保持所有 smoke 命令继续通过。

推荐拆分：

1. 设计统一主链路目标：typed runtime events、event source 语义、持久化边界、runtime 控制模型、broker adapter 边界。
2. 为 runtime event 持久化建立 `EventRepository` 边界。
3. 为 Backtest 执行结果建立领域化写入接口。
4. 为 Paper pending order / execution result / snapshot 建立领域化写入接口。
5. 移除 Backtest/Paper 对 storage record DTO 的直接构造。
6. 稳定 AlgorithmEngine 事件契约，并让 WebSocket 过滤不再依赖解析 payload JSON。
7. 拆分 Paper 订单生命周期，统一 simulated/Binance/IBKR 的执行结果持久化路径。
8. 补齐 Replay 控制闭环，让 pause/resume/seek/speed 影响正在运行的 replay loop。
9. 在主链路稳定后扩展配置化 Universe/Alpha，并保留单标的 smoke。
10. 单独切片迁移或替换 Binance/IBKR 底层 client，但保持 broker adapter 领域边界不变。
11. 每个切片跑对应验证；关键切片至少跑 `scripts/mvp-smoke.ps1`、`scripts/paper-smoke.ps1`，并按触及范围补充 Binance/IBKR 无网络 smoke。
