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

## 核心问题清单

## P1：存储边界仍过厚，上层直接依赖 storage DTO

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

## P2：AlgorithmEngine 是统一编排器，但事件还不是主链路契约

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

## P3：Paper runtime 持久化和执行副作用混在同一流程中

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

## P4：Replay 控制状态和事件回放仍未形成完整闭环

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

## P5：Universe / Alpha 已有最小实现，但还不是动态研究框架

### 现状

`StaticUniverseSelector`、`AlphaModel`、`CompositeAlphaModel` 和 moving average cross 策略已经接入主链路。当前足够支撑本地 MVP 和 paper smoke。

### 真正缺口

Universe 仍以静态 symbol 为主，Alpha 组合也偏最小策略示例。距离多市场、多标的、配置化策略装配和研究流水线还有差距。

### 风险

- 过早扩展复杂策略会放大 P1/P2/P3 的边界问题。
- 多标的、多 Alpha、多订单会暴露当前事件 payload、订单 ID、portfolio target 结构的限制。

### 建议修改方向

此项不建议作为第一优先级。应在存储边界、事件契约、Paper 生命周期和 Replay 控制闭环稳定后推进。

后续可以逐步增加：

- 配置化 Universe selector。
- 多标的 `MarketSlice` 输入。
- Alpha 权重/冲突处理策略。
- Strategy/Alpha registry 的配置化装配。
- feature store 与 Parquet 研究数据接入。

### 验收标准

- 一个配置可以选择 universe selector 和 alpha model。
- 多标的输入不会破坏当前单标的 smoke。
- 事件 payload 能表达 symbol、portfolio target、risk decision 和 order intent 的多标的上下文。

## 修正后的优先级

### 第一优先级：存储边界收敛

先把 Backtest/Paper 对 `storage::Db` 和具体 record 的直接依赖降下来。原因是它影响审计一致性、恢复路径、paper broker 稳定性，也会降低后续事件契约改造的风险。

### 第二优先级：事件契约稳定

不要急着改成完全事件驱动架构。先让事件 source/category/payload schema 稳定，并让 EventBus、SQLite replay、WebSocket 过滤都基于一致契约。

### 第三优先级：Paper 订单生命周期拆分

Paper 是最接近真实 broker 的路径，应尽快把 pending order、broker result、fill/accounting、unfilled/cancel/recover 的持久化语义统一。

### 第四优先级：Replay 控制闭环

Replay 已有控制 API，但执行 loop 未完全受 controller 驱动。应补齐“状态改变能影响正在运行任务”的闭环。

### 第五优先级：Universe / Alpha 动态能力

等前四项稳定后再推进，避免在边界未收敛时引入多标的和多策略复杂度。

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
