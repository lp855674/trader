# Trader Runbook

`docs/runbook.md` 是当前 operator 联调与排障的真源文档。

本手册按三层闭环组织：

1. `Paper Smoke`
   - 验证 `quantd` / `trader` / paper adapter / WebSocket / SQLite 台账本身可运行
   - 不依赖模型服务
2. `Model Workflow / Service`
   - 验证模型服务进程是否可启动、健康、可预测
   - 不验证交易执行
3. `Model Cycle Paper`
   - 验证 `runtime cycle` 如何通过模型评分、accepted、signal、execution guard 生成 paper 订单

## 终端窗口规划

建议至少开 4 个终端窗口：

1. 窗口 A：model 服务
2. 窗口 B：`quantd`
3. 窗口 C：`trader tui`
4. 窗口 D：手工请求与 CLI 操作

## 临时文件与端口

建议使用独立测试库，避免污染根目录默认 `quantd.db`：

- DB：`quantd_tui_manual.db`
- model 服务：`127.0.0.1:8000`
- `quantd`：`127.0.0.1:18081`

## 前置条件

### Rust

仓库根目录应能执行：

```powershell
cargo check -p trader -p terminal_tui -p terminal_client -p quantd
```

### Python

当前模型服务目录：

- `services/model`
- 进入该目录后执行 `uv sync`

---

## Paper Smoke

这一节回答的是：

- 手工 submit / amend / cancel 是否工作
- `terminal overview` / `execution-state` / TUI / WebSocket / DB 是否一致

这一节刻意不依赖模型服务。

### 步骤 1：启动 `quantd`

在窗口 B：

```powershell
Set-Location E:\code\trader
$env:QUANTD_DATABASE_URL = 'sqlite:quantd_tui_manual.db'
$env:QUANTD_HTTP_BIND = '127.0.0.1:18081'
$env:QUANTD_ACCOUNT_ID = 'acc_mvp_paper'
$env:QUANTD_DATA_SOURCE_ID = 'paper_bars'
$env:QUANTD_UNIVERSE_LOOP_ENABLED = '0'
$env:QUANTD_EXEC_SYMBOL_COOLDOWN_SECS = '300'
$env:RUST_LOG = 'info'
cargo run -p quantd
```

健康检查：

```powershell
Invoke-RestMethod http://127.0.0.1:18081/health
Invoke-RestMethod http://127.0.0.1:18081/v1/runtime/mode
```

预期：

- `/health` 返回 `status = ok`
- `/v1/runtime/mode` 初始返回 `observe_only`

### 步骤 2：初始化 allowlist

在窗口 D：

```powershell
$allowlist = @{
  symbols = @(
    @{ symbol = 'AAPL.US'; enabled = $true }
    @{ symbol = 'MSFT.US'; enabled = $true }
  )
} | ConvertTo-Json -Depth 4

Invoke-WebRequest `
  -Method Put `
  -Uri http://127.0.0.1:18081/v1/runtime/allowlist `
  -ContentType 'application/json' `
  -Body $allowlist
```

确认：

```powershell
Invoke-RestMethod http://127.0.0.1:18081/v1/runtime/allowlist
Invoke-RestMethod "http://127.0.0.1:18081/v1/terminal/overview?account_id=acc_mvp_paper"
```

### 步骤 3：启动 TUI

在窗口 C：

```powershell
Set-Location E:\code\trader
cargo run -p trader -- --base-url http://127.0.0.1:18081 tui
```

预期：

- TUI 正常进入全屏界面
- 顶部显示运行模式、账户、WebSocket 状态
- watchlist / quote / orders / positions / events 面板有内容

### TUI 当前键位

- `q`：退出
- `Tab`：下一个面板
- `Shift+Tab`：上一个面板
- `j` / `Down` / `Right`：非 Events 面板时切换 symbol；Events 面板时向下滚动事件
- `k` / `Up` / `Left`：非 Events 面板时切换 symbol；Events 面板时向上滚动事件
- `r`：手动刷新
- `e`：切换事件过滤器
- `PageUp` / `PageDown`：滚动事件面板

### 场景 A：TUI 基本联通

目的：

- 确认 `trader tui` 能从 `quantd` 拉 overview 与 quote
- 确认 WebSocket 能连上 `/v1/stream`

预期：

- 顶部出现 `WS CONNECTED`
- `j/k` 切换后 quote 面板随 symbol 变化
- Events 面板能看到 `stream connected`、`synced | account=... symbol=...` 等事件

### 场景 B：行情刷新 + quote 事件

在窗口 D：

```powershell
$tick = @{
  venue = 'US_EQUITY'
  symbol = 'AAPL.US'
  account_id = 'acc_mvp_paper'
} | ConvertTo-Json

Invoke-WebRequest `
  -Method Post `
  -Uri http://127.0.0.1:18081/v1/tick `
  -ContentType 'application/json' `
  -Body $tick
```

预期：

- HTTP 返回成功
- TUI 的 Events 面板出现 `quote event: quote_updated`
- 若当前选中 `AAPL.US`，quote 面板中的 `last/day high/day low/bars` 有刷新

备注：

- `/v1/tick` 在这里是单标的调试入口，不是 operator 标准主入口
- 如果策略未产出信号，这一步通常只验证 quote 更新

### 场景 C：CLI 手工下单 + TUI 订单事件

先切运行模式到 `paper_only`。

```powershell
$mode = @{ mode = 'paper_only' } | ConvertTo-Json

Invoke-WebRequest `
  -Method Put `
  -Uri http://127.0.0.1:18081/v1/runtime/mode `
  -ContentType 'application/json' `
  -Body $mode
```

提交订单：

```powershell
cargo run -p trader -- `
  --base-url http://127.0.0.1:18081 `
  order submit `
  --account-id acc_mvp_paper `
  --symbol AAPL.US `
  --side buy `
  --qty 10 `
  --limit-price 123.45
```

补充核对：

```powershell
cargo run -p trader -- --base-url http://127.0.0.1:18081 orders list --account-id acc_mvp_paper
cargo run -p trader -- --base-url http://127.0.0.1:18081 quote AAPL.US
Invoke-RestMethod "http://127.0.0.1:18081/v1/terminal/overview?account_id=acc_mvp_paper"
Invoke-RestMethod "http://127.0.0.1:18081/v1/runtime/execution-state?account_id=acc_mvp_paper"
```

预期：

- CLI 返回订单结果，包含 `order_id`
- TUI Events 面板出现 `order event: order_created`
- TUI Orders 面板出现新订单
- `terminal overview`、`execution-state`、CLI 列表结果一致

### 场景 D：CLI 撤单 + 改单 + TUI 增量更新

先拿到 `order_id`，再执行：

```powershell
cargo run -p trader -- `
  --base-url http://127.0.0.1:18081 `
  order amend `
  --account-id acc_mvp_paper `
  --order-id <ORDER_ID> `
  --qty 12 `
  --limit-price 124
```

```powershell
cargo run -p trader -- `
  --base-url http://127.0.0.1:18081 `
  order cancel `
  --account-id acc_mvp_paper `
  --order-id <ORDER_ID>
```

预期：

- 改单后出现 `order event: order_replaced` 或 `order_updated`
- 撤单后出现 `order event: order_cancelled`
- Orders 面板中的该订单状态同步变化

### 手工订单与 runtime mode 约束

当前设计要求：

- `submit` / `amend` 只允许在 `paper_only` / `enabled`
- `submit` / `amend` 在 `observe_only` / `degraded` 下应返回：
  - HTTP `403`
  - `error_code = runtime_mode_rejected`
- `cancel` 在所有 mode 下都允许

这也是本节建议先切到 `paper_only` 再做 submit / amend 的原因。

---

## Model Workflow / Service

这一节回答的是：

- 模型服务是否可启动
- `/health` 是否正常
- 是否有可加载模型
- `/predict` 是否有响应

这一节不验证交易执行。

### 步骤 1：启动模型服务

在窗口 A：

```powershell
Set-Location E:\code\trader\services\model
$env:MODEL_ARTIFACTS_DIR = '.\models'
uv run uvicorn main:app --host 127.0.0.1 --port 8000
```

健康检查：

```powershell
Invoke-RestMethod http://127.0.0.1:8000/health
```

预期：

- 返回 `status = ok`
- 返回 `models_loaded`

如果 `models_loaded = 0`：

- 说明服务进程活着，但没有可供 `/predict` 使用的模型
- 可以继续做 `Paper Smoke`
- 不要继续做后面的 `Model Cycle Paper`

---

## Model Cycle Paper

这一节回答的是：

- `runtime cycle` 如何从 allowlist 进入模型评分
- accepted symbol 如何再次进入 signal 阶段
- execution guard 如何决定 placed / skipped

关键事实：

- 当前半自动路径是双阶段调用
- 第一次 `evaluate_candidate()` 负责 ranking / accepted
- 对 accepted symbol，第二次再走 signal 阶段，之后才进入 `execution_guard` 与执行

因此 `accepted` 不等于一定 `placed`。

### 步骤 1：写入 model 策略配置

`quantd` 当前优先从 SQLite `system_config` 读取策略配置。

建议写两条 key：

- `model.service_url`
- `strategy.acc_mvp_paper`

兼容说明：

- 运行时仍兼容旧 key `lstm.service_url`
- 新配置不要再写 `lstm.service_url` 或 `{"type":"lstm",...}`

可用任意 SQLite 工具执行以下 SQL：

```sql
INSERT OR REPLACE INTO system_config (id, key, value, updated_at, created_at)
VALUES (
  'model.service_url',
  'model.service_url',
  'http://127.0.0.1:8000',
  strftime('%s','now'),
  strftime('%s','now')
);

INSERT OR REPLACE INTO system_config (id, key, value, updated_at, created_at)
VALUES (
  'strategy.acc_mvp_paper',
  'strategy.acc_mvp_paper',
  '{"type":"model","model_type":"alstm","lookback":60,"buy_threshold":0.6,"sell_threshold":-0.6}',
  strftime('%s','now'),
  strftime('%s','now')
);
```

写完后重启 `quantd`。

预期日志里出现：

- `loaded model strategy from system_config`

### 步骤 2：切到 `paper_only`

```powershell
$mode = @{ mode = 'paper_only' } | ConvertTo-Json

Invoke-WebRequest `
  -Method Put `
  -Uri http://127.0.0.1:18081/v1/runtime/mode `
  -ContentType 'application/json' `
  -Body $mode
```

### 步骤 3：跑一轮 runtime cycle

在窗口 D：

```powershell
$cycle = @{
  venue = 'US_EQUITY'
  account_id = 'acc_mvp_paper'
} | ConvertTo-Json

Invoke-WebRequest `
  -Method Post `
  -Uri http://127.0.0.1:18081/v1/runtime/cycle `
  -ContentType 'application/json' `
  -Body $cycle
```

核对：

```powershell
Invoke-RestMethod http://127.0.0.1:18081/v1/runtime/cycle/latest
Invoke-RestMethod "http://127.0.0.1:18081/v1/runtime/cycle/history?limit=10"
Invoke-RestMethod "http://127.0.0.1:18081/v1/runtime/execution-state?account_id=acc_mvp_paper"
Invoke-RestMethod "http://127.0.0.1:18081/v1/terminal/overview?account_id=acc_mvp_paper"
```

预期：

- `latest` / `history` 中能看到 `ranked`、`accepted`、`skipped`、`placed`
- 若分数和阈值满足，可能出现新订单
- 若已被 guard 拦截，则 `accepted` 中的 symbol 可能只出现在 `skipped`
- TUI Events 面板会继续出现 quote / order 相关事件

### Execution Guard Notes

- `accepted` 不等于一定 `placed`
- accepted symbol 在真正执行前还会重新进入 signal 阶段
- execution guard 当前会检查：
  - 幂等 key
  - open order
  - cooldown
  - 同向持仓

当前常见 `skipped.reason` 包括：

- `guard_duplicate_idempotency`
- `guard_open_order_exists`
- `guard_cooldown_active`
- `guard_same_direction_position_open`
- `model_not_found`
- `model_unreachable`
- `insufficient_bars`
- `response_parse_failed`
- `model_service_error`

### 常见失败定位

- `models_loaded = 0`
  - model 服务活着，但没有可预测模型
- `/predict` 返回 `404 model_not_found`
  - 模型目录里没有对应模型
- `insufficient_bars`
  - 本地 bars 数量不足 lookback
- `model_unreachable`
  - model 服务没启动，或 `model.service_url` 写错
- `accepted` 有值但 `placed` 为空
  - 优先看 `latest_cycle.skipped`
  - 再看 `execution-state.open_orders` / `positions`

---

## 完整通过标准

以下项目都成立，才算当前本地联调通过：

1. `trader tui` 能正常启动并显示 `WS CONNECTED`
2. watchlist / quote / orders / positions / events 都能正常展示
3. `POST /v1/tick` 后，TUI 能收到 `quote_updated`
4. `trader order submit` 后，TUI 能收到 `order_created`
5. `trader order amend` / `cancel` 后，TUI 能看到对应订单事件和状态变化
6. `terminal overview`、`execution-state`、CLI 输出、TUI 面板四者一致
7. 若模型服务已配置且有模型，`runtime cycle` 能把结果体现在 `latest cycle`、history 与终端视图中

## 结束清理

结束后可手工清理：

- 停止模型服务
- 停止 `quantd`
- 删除 `quantd_tui_manual.db`

如果你需要复盘现场，就保留 DB 和终端日志。
