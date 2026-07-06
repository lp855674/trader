# portfolio 技术文档

## 职责

`crates/portfolio` 负责把策略信号转换成目标仓位。当前实现保持很薄，只提供等权/固定数量目标仓位函数。

## 关键实现

- `TargetPosition { symbol, target_qty }` 表达目标仓位。
- `equal_weight_target(signal, qty)` 将 Buy 映射为正目标数量、Sell 映射为负目标数量。
- `CloseLong` / `CloseShort` 映射为零目标数量。

## 输入输出与持久化

输入是 `SignalEvent` 和目标数量；输出是 `TargetPosition`。模块不持久化、不访问账户、不调用 broker。

## 边界与约束

- 是否允许 short 不由 portfolio 决定，必须交给 `risk`。
- 组合构造不生成订单，订单转换属于 `execution`。
- 数量使用 `Decimal`。

## 测试与验证

重点覆盖四类 signal side 到目标数量的映射，尤其 close 信号必须归零而不是反向开仓。

