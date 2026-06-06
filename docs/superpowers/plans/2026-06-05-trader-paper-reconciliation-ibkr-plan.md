# Trader Paper Reconciliation + IBKR Stock Paper 执行计划

## 目标

把当前已经能跑通的 Binance Spot Testnet paper 和本地股票 paper 推进到可长期验证状态。数字货币继续走 Binance；股票 paper 统一改为 IBKR，不再按 Longbridge 规划。

## 边界

- Inline Execution，不使用子代理。
- 不提交 secrets，不把生成的 Parquet、SQLite、report 产物纳入 git。
- Binance 仅用于 crypto；IBKR 用于股票 paper。
- Paper 数据优先 Parquet；CSV 只作为导入兼容输入。
- 真实 IBKR 下单必须通过 `order_submit_enabled` 与脚本确认参数双闸门；默认 runner 必须保持不下单。

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
3. [x] `paper-preflight` 对 IBKR 在 `order_submit_enabled = true` 时实际连接 Gateway 并校验 paper account。
4. [x] 文档写清 TWS paper account、Gateway、端口、client id、只读限制。

## 阶段 4：IBKR Paper Order Adapter

1. [x] 抽象 IBKR order client，先用测试 client 覆盖 place/query/cancel/fills。
2. [x] 增加 `broker::IbkrPaperGatewayAdapter` 作为真实 TWS / Gateway TCP readiness 边界。
3. [x] 迁移到 Rust 开源 crate `ibapi`，不再维护项目内手写 TWS API wire codec。
4. [x] 接入真实 socket session，完成 TWS / Gateway server version 握手。
5. [x] 读取并校验 IBKR paper account id，`[paper] account_id` 必须匹配 TWS / Gateway 返回账号。
6. [x] 接入 PaperRuntime executor：CLI 与 REST `paper-run` 已在 `order_submit_enabled = true` 时注入 `IbkrPaperOrderExecutor`。
7. [x] 增加 IBKR recover/reconciliation/open-orders 等价命令：open-orders / executions / reconciliation / recover 已完成。
8. [x] 在 runner 中加入 `-ConfirmIbkrPaperOrder` 闸门：默认本地 dry-run；开闸时要求真实 `-AccountId DU...` 并打开临时 config 的 `order_submit_enabled = true`。
9. [x] 增加 IBKR soak 脚本：默认本地多轮 dry-run；开闸后多轮执行真实 Gateway paper runner 并汇总 summary / reconcile / recover。
10. [x] 增加本地 paper readiness 门禁：账号未就绪时跑 cargo 检查、Binance 无网络 smoke 和 IBKR 本地 dry-run soak。

## 当前状态

Binance summary、只读 reconciliation、自动订单生命周期事件和 soak 脚本已经完成。`binance-paper-soak.ps1 -Iterations 2 -Limit 100 -ConfirmTestnetOrder` 已通过，两轮均 completed 且 `open_orders=0`。

IBKR stock paper 本地 Parquet runner、read-only preflight、`broker::IbkrPaperGatewayAdapter`、`ibapi` 真实 Gateway client、managed accounts 读取与 `[paper] account_id` 校验、open orders / executions / reconciliation / recover / next valid order id、受确认保护的 paper cancel、受确认保护的 tiny stock LMT paper order、IBKR paper order client trait、测试 executor、CLI/REST 自动 `paper-run` executor 注入、runner 的 `-ConfirmIbkrPaperOrder` 闸门、IBKR soak 脚本，以及本地 paper readiness 门禁已完成。下一步用真实 TWS / IB Gateway 执行 `ibkr-paper-readonly`、`ibkr-paper-tiny-order` 和 `ibkr-paper-run.ps1 -AccountId DU... -ConfirmIbkrPaperOrder`，验证真实 IBKR paper 生命周期。
