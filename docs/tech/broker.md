# broker 技术文档

## 职责

`crates/broker` 定义 broker adapter 边界，负责外部交易通道和外部回报到项目领域类型的映射。它不负责策略、风控、PnL、订单拆分或持久化。

## 关键实现

- `Broker` trait 定义订单提交、账户、持仓、open orders、executions、状态等通道能力。
- `BrokerError` 表达配置、网络、认证、外部响应和不支持能力等错误。
- `BrokerKind`、`BrokerCapabilities`、`BrokerStatus` 描述 adapter 类型和能力。
- Fake/simulated adapter 支持本地测试和无网络 smoke。
- Binance Spot Testnet adapter 封装 signed request、account balances、open orders、trades、klines 和 status；当前 `place_order` 送单路径仍受未启用保护，不能把 testnet 能力误写成默认可送单。
- IBKR Paper Gateway adapter 通过 ibapi Gateway client 做 connect/handshake、open orders、executions、cancel 和 limit order 映射。
- `cancel_open_orders_for_account_symbol` 等 helper 只做 broker 通道操作，调用方负责运行上下文、审计和风险边界。

## 输入输出与持久化

输入是内部订单/查询请求；输出是 broker order id、成交、余额、持仓、open order 和 adapter status。模块不写 storage，调用方负责审计事件和状态落库。

## 边界与约束

- broker 不做业务风控，所有订单进入 adapter 前必须已通过 market rules、risk、execution、OMS。
- 凭证只能来自环境变量或受控 secret provider，不能写入配置、日志或事件 payload。
- 真实外部操作必须有 paper/testnet/live 模式和显式确认保护。
- 不得为了流程跑通伪造真实 broker fill。
- fake adapter 可以用于 deterministic smoke；真实 adapter 只能把外部确认的 order/execution/balance/position 映射回领域类型。
- 旧文档中未实现的 CTP、Futu、OKX、Bybit、Alpaca 或 broker router 能力不能作为当前能力引用。

## 测试与验证

默认测试应覆盖 fake/simulated 和请求映射。真实外部连接测试必须受显式确认参数保护。
