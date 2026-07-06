# alpha 技术文档

## 职责

`crates/alpha` 定义 alpha 模型接口和组合规则。它只处理行情到 `SignalEvent` 的信号生成/聚合，不处理目标仓位、风控、订单或持久化。

## 关键实现

- `AlphaModel` trait 提供 `on_bar` 和可选的 `on_bar_for_symbol`。
- `CompositeAlphaModel` 选择置信度最高的有效信号。
- `NetSignalAlphaModel` 按多空方向累加置信度，净值为零时不出信号。
- `MajorityVoteAlphaModel` 按方向投票，平票不出信号。
- `WeightedAlphaModel` 对子模型信号置信度乘权重。
- category majority 由 `strategies` 装配层按 category 先聚合再投票；未配置或未知冲突策略必须显式失败或回到已定义默认。

## 输入输出与持久化

输入是 `data::Bar` 和可选 symbol；输出是 `events::SignalEvent`。模块不访问数据库、feature 文件、broker 或 API。

## 边界与约束

- 权重和置信度使用 `f64` 是信号评分，不是金额或数量；资金、价格、仓位仍必须使用 `Decimal`。
- 聚合规则必须确定性执行，不能依赖隐式全局状态。
- 无效或非正置信度在净信号/投票中会被忽略。
- Alpha 只表达方向和信心，不表达最终数量、订单类型、账户路由或 broker 参数。

## 测试与验证

重点覆盖最高置信度选择、净信号冲突、投票平票、非有限置信度过滤和 weighted confidence。
