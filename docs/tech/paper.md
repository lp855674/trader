# paper 技术文档

## 职责

`crates/paper` 负责 paper runtime 和 paper order executor。它在不使用真实资金的前提下复用共享算法链路，支持 simulated、Binance testnet 和 IBKR paper 路径。

## 关键实现

- `PaperRuntime` 读取行情、装配 `AlgorithmEngine`、执行订单、写入运行状态和快照。
- `PaperSettings` 保存 run、account、slippage、fee、bar delay、risk 和 broker 装配参数。
- `PaperOrderExecutor` trait 抽象 simulated/Binance/IBKR paper 执行器。
- `ExecutedPaperOrder` 表达 paper 成交结果。
- 运行中会写 submitted/failed/executed orders、engine events、portfolio snapshots、contract positions 和 final state。
- IBKR paper executor 提交 limit order 后会对 open 状态做有限次数 status/execution 轮询，直到累计 execution 数量达到订单数量或订单进入 terminal 状态。轮询窗口结束后如仍有未成交数量，会取消 remainder，并再次读取 executions 以覆盖 cancel race；部分成交结果保留非零 `filled_qty`，订单状态为 `Cancelled`。
- IBKR 撤单竞态可能返回 API `10147`（订单已不可见）；该响应按 terminal cancellation 处理，但仍会刷新 executions，避免丢失竞态期间到达的成交。
- IBKR client order id 对短 run id 保留可读 prefix；超过长度预算的 run id 使用完整值的确定性 64-bit 摘要，避免仅截取共同前缀造成跨 run 幂等键碰撞。
- IBKR paper submit 默认不指定特殊路由，走 IBKR stock contract 的 SMART 路径，并设置 `outside_rth=true`。`ibkr_route_exchange = "OVERNIGHT"` 只用于诊断直接路由行为；它会把 contract exchange 显式改成 `OVERNIGHT`，可能触发 IBKR API `10329` 预防限制。

## 输入输出与持久化

输入是 paper config、行情 bars/slices、executor 和 cancellation flag；输出是 paper run 状态、订单、成交、事件和快照。所有审计状态写入 `storage`。

## 边界与约束

- paper 可以接 testnet/paper broker，但必须受 `order_submit_enabled` 和 broker mode/kind 保护。
- paper executor 只能写入真实 testnet/paper 回报确认的成交；simulated executor 必须明确标识。
- IBKR `PreSubmitted` / `Submitted` 不等于 filled evidence；filled-order acceptance 只能来自 Gateway executions 与本地 fills 对账成功。
- IBKR reconcile 只统计当前 run 中匹配配置账户和请求 symbol 的订单，并只统计属于这些订单的 fills；Gateway open orders 也按账户和 symbol 收敛。多个 broker executions 可按 broker order id 聚合后与一个本地 fill 比较。
- IBKR `outside_rth` / explicit route exchange / `override_percentage_constraints` 只表示订单参数和路由意图；如果 Gateway/TWS 返回预防设置错误或没有 execution，不能写成本地成交。
- 不能绕过 AlgorithmEngine 直接生成交易结果。

## 测试与验证

重点覆盖 simulated paper、取消、订单失败、事件落库、快照落库、broker preflight 和无网络 smoke。
