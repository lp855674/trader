# Trader Runbook

## 目标

这份 runbook 用于本地手工联调以下链路：

1. 启动 `lstm-service`
2. 启动 `quantd`
3. 启动 `trader tui`
4. 用 CLI / HTTP 触发行情、订单、WebSocket 事件
5. 人工确认 TUI、CLI、HTTP、SQLite 台账之间的数据一致

当前实现里：

- `trader tui` 已可查看 watchlist、quote、orders、positions、events，并通过 HTTP + WebSocket 跟 `quantd` 同步
- 订单提交、撤单、改单目前走 `trader` CLI 或 HTTP API
- TUI 当前不直接在界面里录入订单

## 测试范围

本手册覆盖两层验证：

1. 终端联调
   - `quantd` + `trader tui` + `trader` CLI
   - 验证 overview、quote、order、cancel、amend、WebSocket 增量更新
2. LSTM 联调
   - `lstm-service` + `quantd runtime cycle`
   - 验证 `quantd` 能从 `lstm-service` 拉预测，并把结果反映到运行态与终端视图

## 终端窗口规划

建议至少开 4 个终端窗口：

1. 窗口 A: `lstm-service`
2. 窗口 B: `quantd`
3. 窗口 C: `trader tui`
4. 窗口 D: 手工请求与 CLI 操作

## 临时文件与端口

建议使用独立测试库，避免污染根目录默认 `quantd.db`：

- DB: `quantd_tui_manual.db`
- `lstm-service`: `127.0.0.1:8000`
- `quantd`: `127.0.0.1:18081`

## 前置条件

### 1. Rust

仓库根目录能正常执行：

```powershell
cargo check -p trader -p terminal_tui -p terminal_client -p quantd
```

### 2. Python

使用仓库内虚拟环境：

- `services/lstm-service/.venv`

### 3. LSTM 模型文件

`lstm-service` 的 `/health` 只表示服务进程活着，不代表可预测。

- 若 `models_loaded = 0`，`/predict` 很可能返回 `404 model_not_found`
- 要做完整 LSTM 联调，请确保 `services/lstm-service/models` 里已有可用 `.pt` 模型文件

## 启动步骤

### 步骤 1：启动 `lstm-service`

在窗口 A：

```powershell
Set-Location E:\code\trader\services\lstm-service
$env:LSTM_MODELS_DIR = '.\models'
uv run uvicorn main:app --host 127.0.0.1 --port 8000
```

健康检查：

```powershell
Invoke-RestMethod http://127.0.0.1:8000/health
```

预期：

- 返回 `status = ok`
- 记录 `models_loaded`

如果 `models_loaded = 0`，先不要继续做 LSTM 预测链路验证，只做终端联调即可。

### 步骤 2：启动 `quantd`

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
- `/v1/runtime/mode` 初始是 `observe_only`

### 步骤 3：初始化 allowlist

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

预期：

- allowlist 含 `AAPL.US` 和 `MSFT.US`
- terminal overview 的 watchlist 不为空

### 步骤 4：启动 TUI

在窗口 C：

```powershell
Set-Location E:\code\trader
cargo run -p trader -- --base-url http://127.0.0.1:18081 tui
```

预期：

- TUI 正常进入全屏界面
- 顶部显示运行模式、账户、WebSocket 状态
- watchlist / quote / orders / positions / events 面板有内容

## TUI 当前键位

- `q`: 退出
- `Tab`: 下一个面板
- `Shift+Tab`: 上一个面板
- `j` / `Down` / `Right`: 非 Events 面板时切换 symbol；Events 面板时向下滚动事件
- `k` / `Up` / `Left`: 非 Events 面板时切换 symbol；Events 面板时向上滚动事件
- `r`: 手动刷新
- `e`: 切换事件过滤器
- `PageUp` / `PageDown`: 滚动事件面板

## 手工测试清单

### 场景 A：TUI 基本联通

目的：

- 确认 `trader tui` 能从 `quantd` 拉 overview 与 quote
- 确认 WebSocket 能连上 `/v1/stream`

操作：

1. 进入 TUI
2. 在 watchlist 和 quote 面板之间切换
3. 用 `j/k` 切换 symbol
4. 用 `Tab` 切到 Events 面板
5. 用 `e` 切换过滤器

预期：

- 顶部出现 `WS CONNECTED`
- `j/k` 切换后 quote 面板随 symbol 变化
- Events 面板能看到 `stream connected`、`synced | account=... symbol=...` 等事件

### 场景 B：行情刷新 + quote 事件

目的：

- 验证 `POST /v1/tick` 后，quote 与事件流同步更新

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

- 如果 `QUANTD_STRATEGY` 仍是默认 `noop`，这里通常只验证 quote 更新，不会产生订单

### 场景 C：CLI 下单 + TUI 订单事件

目的：

- 验证显式下单 API、open orders 面板、WebSocket 订单事件

先切运行模式到 `paper_only`：

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
```

预期：

- CLI 返回订单结果，包含 `order_id`
- TUI Events 面板出现 `order event: order_created`
- TUI Orders 面板出现新订单
- `GET /v1/terminal/overview` 与 CLI 列表结果一致

### 场景 D：CLI 撤单 + 改单 + TUI 增量更新

目的：

- 验证 cancel / amend API 和 TUI 增量刷新

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

- 改单后，TUI Events 面板出现 `order event: order_replaced` 或 `order_updated`
- 撤单后，TUI Events 面板出现 `order event: order_cancelled`
- Orders 面板中的该订单状态同步变化，或被移出 open orders

## LSTM 链路联调

这一段只在你已经准备好模型文件时执行。

### 步骤 1：向 `system_config` 写入 LSTM 策略

`quantd` 当前会优先从 SQLite `system_config` 读取策略配置。

需要写两条 key：

- `lstm.service_url`
- `strategy.acc_mvp_paper`

可用任意 SQLite 工具执行以下 SQL：

```sql
INSERT OR REPLACE INTO system_config (id, key, value, updated_at, created_at)
VALUES (
  'lstm.service_url',
  'lstm.service_url',
  'http://127.0.0.1:8000',
  strftime('%s','now'),
  strftime('%s','now')
);

INSERT OR REPLACE INTO system_config (id, key, value, updated_at, created_at)
VALUES (
  'strategy.acc_mvp_paper',
  'strategy.acc_mvp_paper',
  '{"type":"lstm","model_type":"alstm","lookback":60,"buy_threshold":0.6,"sell_threshold":-0.6}',
  strftime('%s','now'),
  strftime('%s','now')
);
```

写完后重启 `quantd`。

预期日志里出现：

- `loaded lstm strategy from system_config`

### 步骤 2：跑一轮 runtime cycle

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
Invoke-RestMethod "http://127.0.0.1:18081/v1/runtime/execution-state?account_id=acc_mvp_paper"
Invoke-RestMethod "http://127.0.0.1:18081/v1/terminal/overview?account_id=acc_mvp_paper"
```

预期：

- `latest` 里能看到 `accepted` / `placed` / `skipped`
- 若分数和阈值满足，可能出现新订单
- TUI Events 面板会继续出现 quote / order 相关事件

### LSTM 常见失败

- `models_loaded = 0`
  - 服务活着，但没有可预测模型
- `/predict` 返回 `404 model_not_found`
  - 模型目录里没有对应模型
- `insufficient bars for LSTM lookback; skipping`
  - 本地 bars 数量不足 `lookback`
- `service unreachable`
  - `lstm-service` 没启动，或 `lstm.service_url` 写错

## 完整通过标准

以下项目都成立，才算这轮人工联调通过：

1. `trader tui` 能正常启动并显示 `WS CONNECTED`
2. watchlist / quote / orders / positions / events 都能正常展示
3. `POST /v1/tick` 后，TUI 能收到 `quote_updated`
4. `trader order submit` 后，TUI 能收到 `order_created`
5. `trader order amend` / `cancel` 后，TUI 能看到对应订单事件和状态变化
6. `terminal overview`、CLI 输出、TUI 面板三者一致
7. 若开启 LSTM 配置，`runtime cycle` 能调用 `lstm-service` 并把结果体现在 `latest cycle` 与终端视图中

## 结束清理

结束后可手工清理：

- 停止 `lstm-service`
- 停止 `quantd`
- 删除 `quantd_tui_manual.db`

如果你需要复盘现场，就保留 DB 和终端日志。
