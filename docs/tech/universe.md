# universe 技术文档

## 职责

`crates/universe` 负责从候选 symbol 中选择当前可参与策略计算的标的池。它不负责风控、下单、持久化或 feature 文件读取。

## 关键实现

- `UniverseSelector` trait 接收 `UniverseContext` 并输出 symbol 列表。
- `StaticUniverseSelector` 返回固定列表。
- `FilteredUniverseSelector` 支持 include/exclude、symbol prefix、require_current_data 和 max_symbols。
- `RankedUniverseSelector` 在给定排序基础上应用 filter。

## 输入输出与持久化

输入是候选 symbols、filter 和当前 `MarketSlice` 上下文；输出是有序 symbol 列表。模块不访问数据库或文件。

## 边界与约束

- `require_current_data` 只能根据当前 market slice 中实际存在的数据收缩 universe。
- 排序来源必须由调用方明确提供，feature ranked 读取逻辑在 `strategies` 装配层。
- universe 不做 risk 和 broker 可交易性判断。

## 测试与验证

重点覆盖 include/exclude 优先级、prefix、max_symbols、require_current_data 和 ranked 稳定顺序。

