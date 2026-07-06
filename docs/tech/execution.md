# execution 技术文档

## 职责

`crates/execution` 负责把目标仓位差额转换为订单意图和可提交订单。它位于 portfolio/risk 之后、OMS/broker 之前。

## 关键实现

- 支持 immediate order、target delta order、time sliced、weighted、reduce-only、post-only 等执行意图。
- `order_for_target_delta` 根据当前仓位和目标仓位生成买卖方向及数量。
- `expand_execution_intent` 将复杂意图展开成可执行订单片段。

## 输入输出与持久化

输入是目标仓位、当前仓位、订单参数和执行意图；输出是内部 order。模块不访问数据库和 broker。

## 边界与约束

- execution 不做最终风控批准，risk 是 broker 前业务闸门。
- execution 不维护订单状态，状态机属于 `oms`。
- 数量使用 `Decimal`，不得通过浮点拆单。

## 测试与验证

重点覆盖目标差额、reduce-only、post-only 标志、分片数量和零差额不下单。

