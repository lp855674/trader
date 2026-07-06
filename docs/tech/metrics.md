# metrics 技术文档

## 职责

`crates/metrics` 负责回测、paper 和报告使用的绩效指标计算，不参与交易决策链。

## 关键实现

- 计算 total return、equity returns、max drawdown、win rate、Sharpe、Sortino 等指标。
- `paper_summary` 汇总 paper run 的成交、权益和风险指标。
- 指标输入通常来自 storage read model 或运行快照。

## 输入输出与持久化

输入是权益曲线、成交/订单统计和快照数据；输出是指标结构或报告数据。模块不写库。

## 边界与约束

- 指标仅用于分析和报告，不应回写影响运行状态。
- 金额和收益计算涉及资金值时优先使用 `Decimal`；统计比率如 Sharpe 可使用浮点但不得回用于资金账本。
- 输入缺失时应返回可解释的空指标或错误。

## 测试与验证

重点覆盖空序列、单点序列、回撤峰谷、胜率分母、收益率和 paper summary。

