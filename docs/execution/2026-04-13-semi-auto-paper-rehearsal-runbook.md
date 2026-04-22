# Semi-Auto Paper Rehearsal Runbook

## 目标

这份文档用于手工验证当前半自动交易链路是否满足以下要求：

1. `model service` 可稳定启动并对 `quantd` 提供预测服务。
2. `quantd` 在 `paper_only` 下可以完成一轮 `universe cycle`。
3. 首次满足条件时允许下单。
4. 后续重复轮次会被 execution guard 正确拦截，而不是继续下单。
5. `runtime_cycle_history`、`runtime/execution-state`、`orders/fills` 与日志能互相对上。

本文档只覆盖本地 paper rehearsal，不覆盖 Longbridge 实盘。

## 适用版本

- 当前分支的 `quantd`
- 当前仓库内的 `services/model`
- 当前 API:
  - `POST /v1/runtime/cycle`
  - `GET /v1/runtime/cycle/latest`
  - `GET /v1/runtime/cycle/history`
  - `GET /v1/runtime/execution-state`
  - `GET /v1/orders`

## 预期验证的 guard

本 runbook 重点验证以下 `skipped.reason`：

- `guard_duplicate_idempotency`
- `guard_open_order_exists`
- `guard_cooldown_active`
- `guard_same_direction_position_open`

注意：

- 在真实 paper 路径下，`PaperAdapter` 默认直接写 `FILLED`，所以 `guard_open_order_exists` 需要手工构造一条 `SUBMITTED` 订单来验证。
- `guard_duplicate_idempotency` 和 `guard_cooldown_active` 都与“重复执行”相关，但前者更偏同一个稳定 bucket 的幂等，后者更偏最近订单时间窗口。

## 测试产物

建议本次演练统一使用以下临时文件，避免污染主库：

- 数据库：`quantd_rehearsal.db`
- `quantd` stdout：`quantd_rehearsal.out.log`
- `quantd` stderr：`quantd_rehearsal.err.log`
- `model service` stdout：`quantd_rehearsal_model.out.log`
- `model service` stderr：`quantd_rehearsal_model.err.log`

## 前置条件

### 1. Python 环境

优先使用：

- `services/model`

进入该目录后执行 `uv sync`。

不要使用系统 Python 3.13。

### 2. Rust 环境

确认本地可以运行：

```powershell
cargo test -p pipeline -- --nocapture
```

### 3. 端口规划

建议固定为：

- `model service`: `127.0.0.1:8000`
- `quantd`: `127.0.0.1:18081`

### 4. 测试标的

统一使用：

- `venue = US_EQUITY`
- `symbol = AAPL.US`
- `account_id = acc_mvp_paper`

## 环境变量

### `quantd`

建议用下面这组：

```powershell
$env:QUANTD_DATABASE_URL = 'sqlite:quantd_rehearsal.db'
$env:QUANTD_HTTP_BIND = '127.0.0.1:18081'
$env:QUANTD_ACCOUNT_ID = 'acc_mvp_paper'
$env:QUANTD_DATA_SOURCE_ID = 'paper_bars'
$env:QUANTD_UNIVERSE_LOOP_ENABLED = '0'
$env:QUANTD_UNIVERSE_LOOP_VENUE = 'US_EQUITY'
$env:QUANTD_UNIVERSE_LOOP_ACCOUNT_ID = 'acc_mvp_paper'
$env:QUANTD_UNIVERSE_MIN_SCORE = '0.05'
$env:QUANTD_UNIVERSE_MIN_CONFIDENCE = '0.05'
$env:QUANTD_EXEC_SYMBOL_COOLDOWN_SECS = '300'
$env:RUST_LOG = 'info'
```

说明：

- `QUANTD_DATA_SOURCE_ID=paper_bars` 是关键前提，必须和 mock/paper bars 的数据源一致。
- 先把后台 loop 关掉，统一用手工触发 `POST /v1/runtime/cycle`，这样更容易定位问题。

### `model service`

无额外强制要求，但要确保服务启动后：

```powershell
Invoke-WebRequest -Uri http://127.0.0.1:8000/health -UseBasicParsing
```

能返回 `200`。

## 启动顺序

### 步骤 1：清理旧临时文件

手工删除以下文件（如果存在）：

- `quantd_rehearsal.db`
- `quantd_rehearsal.out.log`
- `quantd_rehearsal.err.log`
- `quantd_rehearsal_model.out.log`
- `quantd_rehearsal_model.err.log`

预期：

- 没有旧 DB 残留。
- 没有旧日志干扰。

### 步骤 2：启动 `model service`

在仓库根目录执行：

```powershell
Set-Location services/model
..\lstm-service\.venv\Scripts\python.exe main.py *> ..\..\quantd_rehearsal_model.out.log 2> ..\..\quantd_rehearsal_model.err.log
```

若你希望前台观察日志，也可以直接前台启动。

预期：

- 服务成功监听 `127.0.0.1:8000`
- `/health` 返回：
  - `status = ok`
  - `models_loaded >= 1`

失败判定：

- 启动直接退出
- `/health` 非 200
- 日志中出现模型加载失败

### 步骤 3：启动 `quantd`

回到仓库根目录执行：

```powershell
cargo run -p quantd *> quantd_rehearsal.out.log 2> quantd_rehearsal.err.log
```

预期：

- `quantd` 成功启动 HTTP 服务
- 自动迁移 `quantd_rehearsal.db`
- 初始运行模式自动是 `observe_only`

失败判定：

- 进程启动失败
- 日志中出现 DB/migration 错误
- 日志中出现 `broker_connect_failed` 以外的致命错误

### 步骤 4：健康检查

验证：

```powershell
Invoke-WebRequest -Uri http://127.0.0.1:18081/health -UseBasicParsing
Invoke-WebRequest -Uri http://127.0.0.1:18081/v1/runtime/mode -UseBasicParsing
```

预期：

- `/health` 返回 `{"status":"ok"}`
- `/v1/runtime/mode` 返回 `{"mode":"observe_only"}`

## 初始化配置

### 步骤 5：设置 allowlist

```powershell
$body = '{"symbols":[{"symbol":"AAPL.US","enabled":true}]}'
Invoke-WebRequest `
  -Method Put `
  -Uri http://127.0.0.1:18081/v1/runtime/allowlist `
  -ContentType 'application/json' `
  -Body $body `
  -UseBasicParsing
```

预期：

- 返回 `204`
- `GET /v1/runtime/allowlist` 只包含 `AAPL.US`

### 步骤 6：切到 `paper_only`

```powershell
$body = '{"mode":"paper_only"}'
Invoke-WebRequest `
  -Method Put `
  -Uri http://127.0.0.1:18081/v1/runtime/mode `
  -ContentType 'application/json' `
  -Body $body `
  -UseBasicParsing
```

预期：

- 返回 `204`
- `GET /v1/runtime/mode` 返回 `paper_only`

## 测试场景

---

## 场景 A：首轮允许下单

### 目的

验证在 `paper_only` 下，第一轮满足条件时系统确实会下单。

### 操作

触发一轮 cycle：

```powershell
$body = '{"venue":"US_EQUITY","account_id":"acc_mvp_paper"}'
Invoke-WebRequest `
  -Method Post `
  -Uri http://127.0.0.1:18081/v1/runtime/cycle `
  -ContentType 'application/json' `
  -Body $body `
  -UseBasicParsing
```

然后查询：

```powershell
Invoke-WebRequest -Uri "http://127.0.0.1:18081/v1/runtime/cycle/latest" -UseBasicParsing
Invoke-WebRequest -Uri "http://127.0.0.1:18081/v1/runtime/execution-state?account_id=acc_mvp_paper" -UseBasicParsing
Invoke-WebRequest -Uri "http://127.0.0.1:18081/v1/orders?account_id=acc_mvp_paper" -UseBasicParsing
```

### 预期

`/v1/runtime/cycle/latest`：

- `mode = paper_only`
- `accepted` 包含 `AAPL.US`
- `placed` 至少 1 条
- `placed[0].symbol = AAPL.US`

`/v1/runtime/execution-state`：

- `positions` 至少 1 条
- `positions[0].symbol = AAPL.US`
- `positions[0].net_qty > 0`
- `open_orders` 通常为空
  - 因为 `PaperAdapter` 默认直接 `FILLED`

`/v1/orders`：

- 出现新订单
- 最新订单状态是 `FILLED`

日志预期：

- `background universe cycle completed` 不一定出现
  - 因为当前是手工触发 cycle，不是后台 loop
- 但应能看到与下单相关的 `order placed`

失败判定：

- `accepted` 有值但 `placed` 为空，且 `skipped` 不是 guard 原因
- `orders` 没有新订单
- `execution-state.positions` 没有仓位

---

## 场景 B：同向持仓拦截

### 目的

验证已有同向仓位后，后续同向信号不会继续加仓。

### 操作

在场景 A 完成后，不做任何清理，立即再次触发同一轮：

```powershell
$body = '{"venue":"US_EQUITY","account_id":"acc_mvp_paper"}'
Invoke-WebRequest `
  -Method Post `
  -Uri http://127.0.0.1:18081/v1/runtime/cycle `
  -ContentType 'application/json' `
  -Body $body `
  -UseBasicParsing
```

然后查询：

```powershell
Invoke-WebRequest -Uri "http://127.0.0.1:18081/v1/runtime/cycle/latest" -UseBasicParsing
Invoke-WebRequest -Uri "http://127.0.0.1:18081/v1/runtime/execution-state?account_id=acc_mvp_paper" -UseBasicParsing
Invoke-WebRequest -Uri "http://127.0.0.1:18081/v1/orders?account_id=acc_mvp_paper" -UseBasicParsing
```

### 预期

`/v1/runtime/cycle/latest`：

- `accepted` 仍可能包含 `AAPL.US`
- `placed` 为空
- `skipped` 中包含：
  - `symbol = AAPL.US`
  - `reason = guard_same_direction_position_open`

`/v1/runtime/execution-state`：

- `positions` 中仍只有之前的持仓
- `net_qty` 不应继续增加

`/v1/orders`：

- 订单总数不应增加

失败判定：

- 第二轮又新增一笔 `FILLED` 订单
- `positions.net_qty` 增长
- `latest_cycle.skipped` 没有 `guard_same_direction_position_open`

---

## 场景 C：open order 拦截

### 目的

验证本地存在未完成订单时，system 会直接拒绝新的执行尝试。

### 说明

因为 `PaperAdapter` 默认直接 `FILLED`，所以这里需要手工在 SQLite 里插一条 `SUBMITTED` 订单。

### 操作

先停止 `quantd`，或者确保你可以安全地直接写 SQLite 文件。

用任意 SQLite 工具对 `quantd_rehearsal.db` 执行以下 SQL：

```sql
INSERT INTO orders (
  id,
  account_id,
  instrument_id,
  side,
  qty,
  status,
  idempotency_key,
  created_at_ms
)
SELECT
  'manual-submitted-order',
  'acc_mvp_paper',
  instruments.id,
  'buy',
  1.0,
  'SUBMITTED',
  'manual-submitted-key',
  strftime('%s','now') * 1000
FROM instruments
WHERE instruments.venue = 'US_EQUITY'
  AND instruments.symbol = 'AAPL.US';
```

然后重新启动 `quantd`，再次触发同一轮 cycle。

### 预期

`/v1/runtime/cycle/latest`：

- `accepted` 仍可能包含 `AAPL.US`
- `placed` 为空
- `skipped` 中包含：
  - `reason = guard_open_order_exists`

`/v1/runtime/execution-state`：

- `open_orders` 至少 1 条
- 该条订单：
  - `symbol = AAPL.US`
  - `status = SUBMITTED`

`/v1/orders`：

- 不应新增新的下单记录

失败判定：

- 插入 `SUBMITTED` 后系统仍继续下单
- `execution-state.open_orders` 为空
- `latest_cycle.skipped` 没有 `guard_open_order_exists`

---

## 场景 D：cooldown / duplicate 行为验证

### 目的

验证重复执行窗口内不会继续下同方向单。

### 说明

在当前实现中：

- `duplicate idempotency` 更偏同一个稳定 bucket
- `cooldown` 更偏最近订单时间

手工验证时，不容易稳定卡在两个边界之间，所以建议主要用现有自动化测试结果作为主验证，手工侧只做“短时间连续触发不会重复下单”的观察。

### 操作

1. 删除场景 C 中手工插入的 `SUBMITTED` 订单。
2. 删除已有持仓，或者重建干净 DB。
3. 在 `paper_only` 下连续快速触发两轮相同 cycle。

### 预期

- 第一轮允许下单
- 第二轮不会继续新增订单
- `latest_cycle.skipped` 出现以下之一：
  - `guard_duplicate_idempotency`
  - `guard_cooldown_active`

失败判定：

- 两轮都成功新增 `FILLED` 订单

## 建议检查顺序

每跑完一个场景，固定按以下顺序检查：

1. `GET /v1/runtime/cycle/latest`
2. `GET /v1/runtime/execution-state?account_id=acc_mvp_paper`
3. `GET /v1/orders?account_id=acc_mvp_paper`
4. 看 `quantd_rehearsal.out.log`

这样你能按“决策 -> 状态 -> 台账 -> 日志”四层对账。

## 常见异常与定位

### 1. `accepted` 为空

可能原因：

- allowlist 没设对
- `model service` 没跑起来
- LSTM score/confidence 没过阈值

先看：

- `/v1/runtime/allowlist`
- `model service /health`
- `quantd` 日志

### 2. `accepted` 有值但 `placed` 为空

优先看 `latest_cycle.skipped`：

- `guard_open_order_exists`
- `guard_same_direction_position_open`
- `guard_duplicate_idempotency`
- `guard_cooldown_active`

### 3. `model service` 正常但一直 `no_signal_on_execution`

可能原因：

- `QUANTD_DATA_SOURCE_ID` 错了
- bars 不足
- LSTM 预测落在 hold 区间

### 4. `execution-state.positions` 为空，但你以为已经成交

优先检查：

- `orders` 是否真的有 `FILLED`
- 是否存在对应 `fills`
- 是否用了正确的 `account_id`

## 测试完成标准

本次 rehearsal 通过，需要同时满足：

1. 场景 A 首单成功。
2. 场景 B 同向持仓被正确拦截。
3. 场景 C open order 被正确拦截。
4. 场景 D 短时间连续触发不会重复下单。
5. 每个场景的 `latest_cycle`、`execution-state`、`orders`、日志四者能够互相解释。

## 结束清理

测试完成后：

1. 停止 `model service`
2. 停止 `quantd`
3. 删除临时文件：
   - `quantd_rehearsal.db`
   - `quantd_rehearsal.out.log`
   - `quantd_rehearsal.err.log`
   - `quantd_rehearsal_model.out.log`
   - `quantd_rehearsal_model.err.log`

如果你要保留现场供分析，就不要删 DB 和日志。
