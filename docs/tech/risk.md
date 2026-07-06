# risk 技术文档

## 职责

`crates/risk` 负责订单进入 broker 前的风险校验和 live guard。它是 broker 前最后业务闸门。

## 关键实现

- `RiskPolicy` / `PortfolioRiskPolicy` 校验最大仓位、订单 notional、cash buffer、gross exposure、leverage、margin、trading halt 和 short permission。
- `PortfolioRiskState` 提供当前账户、持仓、价格、权益等校验上下文。
- `check_max_position` 提供基础仓位限制。
- `live_guards` 覆盖 daily loss、下单节流、行情新鲜度、价格偏离、熔断和交易时段。

## 输入输出与持久化

输入是订单、账户/组合状态、行情时间和运行 guard 状态；输出是通过/拒绝和拒绝原因。风险事件由 algorithm/runtime 写入 `storage`。

## 边界与约束

- risk 不提交订单、不更新账户、不写 broker。
- short 权限来自配置派生，股票和 crypto spot 默认不得 short。
- 校验应基于目标仓位投影，而不是只看订单方向。
- 金额、notional、margin 使用 `Decimal`。

## 测试与验证

重点覆盖 cash buffer、gross exposure、leverage、margin、short permission、kill switch、daily loss 和 market data age。

