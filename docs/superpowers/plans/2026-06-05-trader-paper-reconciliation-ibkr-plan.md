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

1. 为 `binance-paper-run.ps1` 增加 `summary.json`，记录 run id、config、database、Parquet 路径、ticker price、order_submit、report 路径、recover/open orders 结果。
2. 增加 Binance reconciliation CLI：读取 SQLite orders/fills/positions/account_balances 与 Binance account/open orders 做只读对账。
3. 对账输出必须区分 `matched`、`local_only`、`remote_open`、`cash_delta`、`position_delta`。
4. 验证：`cargo test -p trader-cli -p paper -p broker`，以及一次 `binance-paper-run.ps1 -Limit 100 -ConfirmTestnetOrder`。

## 阶段 2：IBKR 股票 Paper 本地闭环

1. 支持 `[broker] kind = "ibkr"` 作为 `interactive_brokers` 的配置别名。
2. 新增 `configs/paper/ibkr_aapl_1d_parquet.toml`，固定使用 Parquet 股票行情。
3. 新增 `scripts/ibkr-paper-run.ps1`：把样例 AAPL CSV 转 Parquet，创建 per-run config/SQLite/report，执行 preflight、paper-run、report。
4. 验证：`cargo test -p config`、`powershell -ExecutionPolicy Bypass -File .\scripts\ibkr-paper-run.ps1`。

## 阶段 3：IBKR Read-only Preflight

1. 增加 IBKR 连接配置边界：host、port、client id 的配置或环境变量映射。
2. 新增 `trader ibkr-paper-readonly`，先只做本地 TWS / Gateway 连接探测和账号只读能力验证。
3. `paper-preflight` 对 IBKR 在 `order_submit_enabled = true` 时必须要求 read-only readiness 通过；否则拒绝启动。
4. 文档写清 TWS paper account、Gateway、端口、client id、只读限制。

## 阶段 4：IBKR Paper Order Adapter

1. 抽象 IBKR order client，先用测试 client 覆盖 place/query/cancel/fills。
2. 接入 PaperRuntime executor：只写真实 IBKR paper fills；未成交不伪造成交。
3. 增加 IBKR recover/open-orders 等价命令。
4. 在 runner 中加入 `-ConfirmIbkrPaperOrder` 闸门，默认仍不提交订单。

## 当前下一步

先完成阶段 2：把股票 paper 固定到 IBKR 本地 Parquet runner，并提交配置、脚本、测试和文档。
