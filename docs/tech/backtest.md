# backtest 技术文档

## 职责

`crates/backtest` 负责历史行情回测 runtime。它把 bars/market slices 输入共享 `AlgorithmEngine`，使用历史 bar close 生成模拟成交，并记录回测运行、成交、事件和最终持仓。

## 关键实现

- `BacktestSettings` 保存 run、strategy、universe、alpha、risk、portfolio、交易时段、日志等配置快照。
- `BacktestRuntime::run` 将单标的 bars 转为 `MarketSlice`；`run_market_slices` 支持多标的切片。
- 通过 `StrategyRegistry::assemble_alpha` 装配 universe 和 alpha。
- 回测成交用 bar close 作为 fill price，fee 由 `FeeRuleEngine` 按时间和账户成交量计算。
- 可选 `LogWriter` 将 tracing 日志写入 storage。

## 输入输出与持久化

输入是 `Vec<Bar>` 或 `Vec<MarketSlice>`；输出是 `BacktestSummary { signals, orders }`。有 `Db` 时会写 backtest completed run、filled execution、runtime events 和 final positions。

## 边界与约束

- 回测必须走共享 `AlgorithmEngine`，不能实现一条独立交易链。
- 成交模型当前是 bar close 立即成交，不能被误描述为真实撮合。
- 回测策略运行时只接收已装配好的内存 feature records。

## 测试与验证

重点覆盖回测产生订单/成交、fee rule 应用、事件持久化、最终持仓记录和多标的 `MarketSlice`。

