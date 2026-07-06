# algorithm 技术文档

## 职责

`crates/algorithm` 是共享交易决策引擎，Backtest、Paper、Live 都应收敛到这里执行同一条业务链路。它负责把行情切片转换为可审计的决策、订单、事件和账户状态变化。

## 关键实现

- `AlgorithmEngineSettings` 汇总运行 ID、模式、账户、主 symbol、下单数量、风控阈值、short 权限、初始现金和交易时段。
- `AlgorithmEngine` 装配 `UniverseSelector`、`AlphaModel`、`Portfolio`、`market_rules`、`risk`、`execution`、`oms` 和 `accounting`。
- 决策顺序是 universe selection -> alpha -> target -> market rule validation -> risk -> execution order -> OMS。
- `ExecutionReport` 回填成交后更新账户、订单事件、持仓和 reconciliation 状态。
- `EngineEvent` / `EngineEventKind` 记录风控、订单、账户、reconciliation 等运行事件。

## 输入输出与持久化

输入是 `data::MarketSlice` 和 broker/runtime 回报；输出是 `AlgorithmDecision`、`EngineEvent`、账户/持仓快照和执行结果。模块不直接写库，调用方负责把事件、订单、成交和快照写入 `storage`。

## 边界与约束

- 策略只产出 signal，不能绕过 engine 直接下单。
- engine 可以生成订单意图，但真实送单由 runtime/executor/broker adapter 承担。
- 风控和市场规则必须在 broker 前执行。
- engine 内部金额和数量必须使用 `Decimal`，事件 payload 序列化时要保留可审计值。

## 测试与验证

重点覆盖共享决策链、short permission、risk reject、market rule reject、成交回填、订单事件和账户快照。修改此模块通常需要跑相关 crate 的单测和集成 smoke。

