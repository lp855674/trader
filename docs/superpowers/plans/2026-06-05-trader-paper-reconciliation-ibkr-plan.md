# Trader Paper Reconciliation + IBKR Stock Paper 执行计划

## 目标

把当前已经能跑通的 Binance Spot Testnet paper 和本地股票 paper 推进到可长期验证状态。数字货币继续走 Binance；股票 paper 统一改为 IBKR，不再按 Longbridge 规划。

## 边界

- Inline Execution，不使用子代理。
- 不提交 secrets，不把生成的 Parquet、SQLite、report 产物纳入 git。
- Binance 仅用于 crypto；IBKR 用于股票 paper。
- Paper 数据优先 Parquet；CSV 只作为导入兼容输入。
- 真实 IBKR 下单 adapter 未完成前，`order_submit_enabled` 必须保持 `false`。

## 阶段 1：Binance Paper 可审计增强

1. [x] 为 `binance-paper-run.ps1` 增加 `summary.json`，记录 run id、config、database、Parquet 路径、ticker price、order_submit、report 路径、recover/open orders 结果。
2. [x] 增加 Binance reconciliation CLI：读取 SQLite orders/fills/positions/account_balances 与 Binance account/open orders 做只读对账。
3. 对账输出必须区分 `matched`、`local_only`、`remote_open`、`cash_delta`、`position_delta`。
4. [x] 增加 Binance soak 脚本，多轮执行固定 runner 并汇总每轮 transcript、summary、open-orders 和 reconciliation。
5. [x] 验证：`cargo test -p trader-cli -p paper -p broker`，一次 `binance-paper-run.ps1 -Limit 100 -ConfirmTestnetOrder`，以及一次 `binance-paper-soak.ps1 -Iterations 2 -Limit 100 -ConfirmTestnetOrder`。

## 阶段 2：IBKR 股票 Paper 本地闭环

1. 支持 `[broker] kind = "ibkr"` 作为 `interactive_brokers` 的配置别名。
2. 新增 `configs/paper/ibkr_aapl_1d_parquet.toml`，固定使用 Parquet 股票行情。
3. 新增 `scripts/ibkr-paper-run.ps1`：把样例 AAPL CSV 转 Parquet，创建 per-run config/SQLite/report，执行 preflight、paper-run、report。
4. 验证：`cargo test -p config`、`powershell -ExecutionPolicy Bypass -File .\scripts\ibkr-paper-run.ps1`。

## 阶段 3：IBKR Read-only Preflight

1. [x] 增加 IBKR 连接配置边界：host、port、client id。
2. [x] 新增 `trader ibkr-paper-readonly`，先只做本地 TWS / Gateway TCP 连接探测。
3. [x] `paper-preflight` 对 IBKR 在 `order_submit_enabled = true` 时拒绝启动；真实 IBKR order adapter 完成前不允许开闸。
4. [x] 文档写清 TWS paper account、Gateway、端口、client id、只读限制。

## 阶段 4：IBKR Paper Order Adapter

1. [x] 抽象 IBKR order client，先用测试 client 覆盖 place/query/cancel/fills。
2. [x] 增加 `broker::IbkrPaperGatewayAdapter` 作为真实 TWS / Gateway TCP readiness 边界。
3. [x] 增加 IBKR TWS API wire codec：V100+ client version handshake、长度前缀 frame、server version 解析。
4. [x] 接入真实 socket session，完成 TWS / Gateway server version 握手。
5. [x] 读取并校验 IBKR paper account id，`[paper] account_id` 必须匹配 TWS / Gateway 返回账号。
6. 接入 PaperRuntime executor：只写真实 IBKR paper fills；未成交不伪造成交。
7. [~] 增加 IBKR recover/open-orders 等价命令：open-orders / executions 只读命令已完成，recover 尚未完成。
8. 在 runner 中加入 `-ConfirmIbkrPaperOrder` 闸门，默认仍不提交订单。

## 当前状态

Binance summary、只读 reconciliation、自动订单生命周期事件和 soak 脚本已经完成。`binance-paper-soak.ps1 -Iterations 2 -Limit 100 -ConfirmTestnetOrder` 已通过，两轮均 completed 且 `open_orders=0`。

IBKR stock paper 本地 Parquet runner、read-only preflight、`broker::IbkrPaperGatewayAdapter`、IBKR TWS API wire codec、真实 socket server version 握手、managed accounts 读取与 `[paper] account_id` 校验、open orders / executions 只读读取、IBKR paper order client trait 和测试 executor 已完成。下一步实现真实 IBKR order submit / query / cancel adapter，并在 runner 中加显式确认闸门；在真实 adapter 完成并验证前，`order_submit_enabled` 必须保持 `false`。
