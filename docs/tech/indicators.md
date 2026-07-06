# indicators 技术文档

## 职责

`crates/indicators` 提供策略可复用的技术指标计算。当前实现包括简单移动平均、指数移动平均和 RSI。

## 关键实现

- `SimpleMovingAverage` 维护窗口内 close 并输出均值。
- `ExponentialMovingAverage` 维护 EMA 状态。
- `RelativeStrengthIndex` 根据涨跌变化输出 RSI。
- `IndicatorError::ZeroPeriod` 防止零周期指标。

## 输入输出与持久化

输入是 `Decimal` 价格序列；输出是可选 `Decimal` 指标值。模块不持久化、不访问行情源。

## 边界与约束

- 指标只计算数值，不产生交易信号；信号由 `strategies` / `alpha` 处理。
- 价格和指标值使用 `Decimal`。
- 指标状态按实例隔离，多标的策略必须为每个 symbol 使用独立实例。

## 测试与验证

重点覆盖 warm-up 阶段、零周期错误、窗口滚动、EMA 初始值和 RSI 边界。

