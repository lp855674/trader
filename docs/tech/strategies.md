# strategies 技术文档

## 职责

`crates/strategies` 实现策略 registry、策略上下文、可配置 alpha 装配、universe 装配、feature gate 和具体技术指标策略。它只产生信号，不负责下单、风控、持久化或 broker。

## 关键实现

- `Strategy` trait 定义 `on_bar -> Option<SignalEvent>`。
- `StrategyRegistry` 静态注册 `moving_average_cross`、`exponential_moving_average_cross`、`price_momentum`、`price_channel_breakout`、`price_channel_reversion`、`relative_strength_index_reversion`。
- `assemble_alpha` 根据 config 装配 static/filtered/ranked/feature_ranked universe 和单/多标的 alpha。
- 多 alpha 组件支持 highest confidence、net signal、majority vote、category majority。
- `FeatureGatedAlphaModel` 根据内存 feature records 对信号做 min/max gate。
- `FeatureRankedUniverseSelector` 根据 feature value 排序后再应用 universe filter。

## 输入输出与持久化

输入是配置、bar、内存 feature records；输出是 `SignalEvent` 或 universe symbol list。模块不访问 SQLite、Parquet、broker、API。

## 边界与约束

- 多标的 alpha 必须按 symbol 独立维护指标状态。
- feature gate/rank 的 records 必须由装配层预先读取并传入。
- `Sell` 只表达负目标意图，是否允许 short 由 `risk` 决定。
- `CloseLong` / `CloseShort` 只表达归零意图，不得在策略层被实现成方向性开仓。
- 策略不得访问 Broker、OMS、SQLite、API client 或 exchange SDK；运行模式差异由 runtime/config/adapter 承担。
- 指标和价格使用 `Decimal`；信号置信度是评分，可使用 `f64`。

## 测试与验证

重点覆盖 registry unknown 错误、窗口校验、多 alpha 冲突处理、feature gate、feature ranked universe 和多标的状态隔离。
