# paper 技术文档

## 职责

`crates/paper` 负责 paper runtime 和 paper order executor。它在不使用真实资金的前提下复用共享算法链路，支持 simulated、Binance testnet 和 IBKR paper 路径。

## 关键实现

- `PaperRuntime` 读取行情、装配 `AlgorithmEngine`、执行订单、写入运行状态和快照。
- `PaperSettings` 保存 run、account、slippage、fee、bar delay、risk 和 broker 装配参数。
- `PaperOrderExecutor` trait 抽象 simulated/Binance/IBKR paper 执行器。
- `ExecutedPaperOrder` 表达 paper 成交结果。
- 运行中会写 submitted/failed/executed orders、engine events、portfolio snapshots、contract positions 和 final state。

## 输入输出与持久化

输入是 paper config、行情 bars/slices、executor 和 cancellation flag；输出是 paper run 状态、订单、成交、事件和快照。所有审计状态写入 `storage`。

## 边界与约束

- paper 可以接 testnet/paper broker，但必须受 `order_submit_enabled` 和 broker mode/kind 保护。
- paper executor 只能写入真实 testnet/paper 回报确认的成交；simulated executor 必须明确标识。
- 不能绕过 AlgorithmEngine 直接生成交易结果。

## 测试与验证

重点覆盖 simulated paper、取消、订单失败、事件落库、快照落库、broker preflight 和无网络 smoke。

