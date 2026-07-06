# oms 技术文档

## 职责

`crates/oms` 负责订单状态机和 broker report 幂等处理。它是订单生命周期的领域状态边界，不负责实际送单。

## 关键实现

- `OrderStateMachine` 保存 `status`、`order_qty`、`filled_qty` 和已处理 `report_id` 集合。
- 支持 submit、accept、fill、request_cancel、cancel、reject。
- `record_fill` 校验正成交数量、禁止 overfill，并自动切换 partially filled / filled。
- `apply_fill_report`、`apply_cancel_report`、`apply_reject_report` 使用 report id 防重复处理。

## 输入输出与持久化

输入是订单数量和 broker report；输出是订单状态、filled/remaining qty 和是否应用了 report。持久化由 runtime/storage 记录订单事件和状态快照。

## 边界与约束

- OMS 不做风控、不算 PnL、不访问 broker。
- terminal 状态下重复 cancel/reject report 应幂等忽略。
- 数量使用 `Decimal`，禁止非正成交和超过剩余数量成交。

## 测试与验证

重点覆盖非法状态转换、部分成交、overfill、重复 report、terminal report 和 cancel/reject 幂等。

