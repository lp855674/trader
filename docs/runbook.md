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

## 自动化测试入口

`runbook` 负责 operator 手工联调，但当前仓库已经有一组可直接回归的自动化测试。建议先跑自动化测试，再做后面的手工联调。

### Rust: terminal / runtime / execution guard

仓库根目录执行：

```powershell
cargo test -p api --test terminal_trading_smoke
cargo test -p api --test runtime_cycle_smoke
cargo test -p pipeline --test execution_guard_smoke
```

这些测试分别覆盖：

- `terminal_trading_smoke`
  - submit / amend / cancel 主链
  - `observe_only` 下 submit 拒绝
  - `degraded` 下 amend 拒绝
  - `observe_only` 下 cancel 允许
  - `terminal overview` / `quote` / `orders list`
- `runtime_cycle_smoke`
  - cycle round trip
  - `latest cycle` / `history`
  - `execution-state`
  - `reconciliation/latest`
- `execution_guard_smoke`
  - duplicate idempotency
  - same-direction position
  - open order

### Python: model service

在 `services/model` 目录执行：

```powershell
uv run pytest tests
```

当前测试覆盖：

- `test_health.py`
  - 服务健康检查与模型发现
- `test_data_update.py`
  - `/data/update` 路由与返回结构
- `test_predict.py`
  - artifact 加载后的 `/predict`
- `test_predict_live.py`
  - 预测主链与请求校验
- `test_models_features.py`
  - 支持模型列表与 feature 端点
- `test_train.py`
  - 训练产物写出与训练路由

备注：

- `test_train.py` 中有 1 个 integration case 依赖 Qlib Yahoo provider，默认可能 `skip`
- 这是预期行为，不影响本地 paper / service 主链验证

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

### 场景 E：runtime mode 拒绝矩阵

这一组是最小权限回归，用来确认手工单路由和 runtime mode 契约一致。

Case 1: `observe_only` 下 submit 被拒绝

```powershell
$mode = @{ mode = 'observe_only' } | ConvertTo-Json
Invoke-WebRequest -Method Put -Uri http://127.0.0.1:18081/v1/runtime/mode -ContentType 'application/json' -Body $mode

$body = @{
  account_id = 'acc_mvp_paper'
  symbol = 'AAPL.US'
  side = 'buy'
  qty = 10
  order_type = 'limit'
  limit_price = 123.45
} | ConvertTo-Json

try {
  Invoke-WebRequest -Method Post -Uri http://127.0.0.1:18081/v1/orders -ContentType 'application/json' -Body $body
} catch {
  $_.Exception.Response.StatusCode.value__
}
```

预期：

- HTTP `403`
- `error_code = runtime_mode_rejected`

Case 2: `degraded` 下 amend 被拒绝

操作：

1. 先在 `enabled` 或 `paper_only` 下创建一笔订单
2. 再切到 `degraded`
3. 对该订单执行 amend

预期：

- HTTP `403`
- `error_code = runtime_mode_rejected`

Case 3: `observe_only` 下 cancel 仍允许

操作：

1. 先在 `enabled` 或 `paper_only` 下创建一笔订单
2. 切回 `observe_only`
3. 调用 cancel

预期：

- HTTP `200`
- 订单状态变为 `CANCELLED`

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

如果需要显式指定本地 Qlib 数据目录，可同时设置：

```powershell
$env:QLIB_DATA_DIR = 'C:\Users\Hi\.qlib\qlib_data\us_data'
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

可选：检查当前 torch 是否已启用 CUDA。

```powershell
services\model\.venv\Scripts\python.exe -c "import torch; print(torch.__version__); print(torch.version.cuda); print(torch.cuda.is_available()); print(torch.cuda.get_device_name(0) if torch.cuda.is_available() else 'cpu')"
```

预期：

- 若希望训练走 GPU，则输出类似：
  - `2.10.0+cu128`
  - `12.8`
  - `True`
  - `NVIDIA GeForce ...`
- 若输出 `+cpu` 或 `False`，则当前训练仍会回退 CPU

### 步骤 2：训练一个最小可预测 artifact

如果本地没有现成模型，建议优先走服务暴露的 `/train` workflow，而不是依赖未承诺稳定的脚本入口。

先在窗口 A 另开一个终端进入 `services/model`：

```powershell
Set-Location E:\code\trader\services\model
$env:MODEL_ARTIFACTS_DIR = '.\models'
```

最小训练请求：

```powershell
$train = @{
  symbol = 'AAPL.US'
  model_type = 'alstm'
  start = '2020-01-01'
  end = '2025-12-31'
} | ConvertTo-Json

Invoke-RestMethod `
  -Method Post `
  -Uri http://127.0.0.1:8000/train `
  -ContentType 'application/json' `
  -Body $train
```

当前 operator 视角下，训练环节的最低要求不是“必须是 LSTM”，而是：

- 能产出标准 artifact 目录
- `/health` 能发现
- `/predict` 能加载并返回分数

如果只是要先跑通开源 paper 主链，可以使用当前支持的任意模型类型先打通训练与预测；后续再把 LSTM/ALSTM 作为正式模型接回同一 workflow 边界。

训练成功后，响应体除了 `model_id` 和 `metrics`，还应包含：

- `requested_start`
- `requested_end`
- `effective_start`
- `effective_end`
- `sample_count`

关键校验点：

- `requested_*` 应与请求一致
- `effective_*` 是本地 Qlib 数据实际生效区间
- 若本地数据没有更新到请求结束日期，`effective_end` 会早于 `requested_end`
- 这是预期行为，说明本地 provider 过期，不是 `/train` 忽略了你的参数

可直接查看落盘 metadata：

```powershell
Get-Content .\models\AAPL_US_alstm\metadata.json
```

预期 metadata 至少包含：

- `requested_start`
- `requested_end`
- `effective_start`
- `effective_end`
- `training_window`
- `metrics`

如果 `metrics` 中某些字段之前显示为空白，当前实现会把 `NaN/inf` 清洗为 `0.0`，不再返回空值。

### 步骤 2.1：更新本地 Qlib 数据

当前 model service 新增了 `POST /data/update`，用于把 Yahoo 日线写回本地 Qlib provider。

示例：

```powershell
$update = @{
  symbols = @('AAPL.US')
  start = '2020-01-01'
  end = '2025-12-31'
} | ConvertTo-Json

Invoke-RestMethod `
  -Method Post `
  -Uri http://127.0.0.1:8000/data/update `
  -ContentType 'application/json' `
  -Body $update
```

预期：

- 返回 `provider_uri`
- 返回 `calendar_start` / `calendar_end`
- `updated[0]` 中包含：
  - `symbol`
  - `requested_start`
  - `requested_end`
  - `effective_start`
  - `effective_end`
  - `rows_written`

推荐操作顺序：

1. 先调用 `/data/update`
2. 观察 `calendar_end` 是否已经推进到期望日期
3. 再调用 `/train`
4. 核对训练响应中的 `effective_end` 是否与更新后的数据范围一致

说明：

- `/data/update` 当前是同步接口，调用期间会阻塞请求
- 它当前按请求的 `symbols` 更新，不是全市场重建
- 若 Yahoo 没有返回数据，应返回 `404`

### 步骤 3：直接验证预测端点

当 `models_loaded > 0` 后，可直接做一次 `/predict` 烟测。

示例：

```powershell
$bars = 0..59 | ForEach-Object {
  @{
    ts_ms = 1700000000000 + $_ * 86400000
    open = 180.0 + $_ * 0.1
    high = 182.0
    low = 179.0
    close = 181.0 + $_ * 0.05
    volume = 50000000.0
  }
}

$predict = @{
  symbol = 'AAPL.US'
  model_type = 'alstm'
  bars = $bars
} | ConvertTo-Json -Depth 4

Invoke-RestMethod `
  -Method Post `
  -Uri http://127.0.0.1:8000/predict `
  -ContentType 'application/json' `
  -Body $predict
```

预期：

- 返回预测结果，而不是 5xx
- 若模型不存在，应返回 `404`，且 `detail.error_code = model_not_found`
- 若 bars 少于 `60`，请求会返回 `422`

### 步骤 4：训练/预测相关最小回归

建议在 `services/model` 跑下面这一条：

```powershell
uv run pytest tests/test_health.py tests/test_data_update.py tests/test_predict.py tests/test_train.py
```

这些测试足够覆盖：

- artifact 发现
- 数据更新路由
- 预测主链
- 训练输出格式

如果要做一组更贴近当前联调问题的手工测试，建议按下面 4 个 case：

- Case 1: `/data/update` 更新单标的
  - 目标：确认本地 Qlib provider 的 `calendar_end` 会推进
  - 通过标准：响应 `updated[0].rows_written > 0`
- Case 2: `/train` 返回实际生效日期
  - 目标：确认请求区间与实际训练区间都可见
  - 通过标准：响应里同时出现 `requested_end` 和 `effective_end`
- Case 3: metadata 落盘日期校验
  - 目标：确认 artifact 自描述完整
  - 通过标准：`metadata.json` 顶层包含 `effective_start/effective_end`
- Case 4: GPU 训练链路校验
  - 目标：确认 torch 已识别 CUDA
  - 通过标准：`torch.cuda.is_available() == True`

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

## Semi-Auto Rehearsal

这一节把本地 paper rehearsal 的关键测试场景并入主 runbook，目标是验证：

- 第一轮满足条件时允许下单
- 后续重复轮次会被 execution guard 正确拦截
- `latest_cycle`、`execution-state`、`orders/fills` 与日志能互相对上

### 演练建议

- 使用独立数据库，例如 `quantd_rehearsal.db`
- 保留两份日志：
  - `quantd_rehearsal.out.log`
  - `quantd_rehearsal_model.out.log`
- 固定标的：
  - `venue = US_EQUITY`
  - `symbol = AAPL.US`
  - `account_id = acc_mvp_paper`

### 推荐启动方式

窗口 A：

```powershell
Set-Location E:\code\trader\services\model
$env:MODEL_ARTIFACTS_DIR = '.\models'
uv run uvicorn main:app --host 127.0.0.1 --port 8000 *> ..\..\quantd_rehearsal_model.out.log
```

窗口 B：

```powershell
Set-Location E:\code\trader
$env:QUANTD_DATABASE_URL = 'sqlite:quantd_rehearsal.db'
$env:QUANTD_HTTP_BIND = '127.0.0.1:18081'
$env:QUANTD_ACCOUNT_ID = 'acc_mvp_paper'
$env:QUANTD_DATA_SOURCE_ID = 'paper_bars'
$env:QUANTD_UNIVERSE_LOOP_ENABLED = '0'
$env:QUANTD_UNIVERSE_LOOP_VENUE = 'US_EQUITY'
$env:QUANTD_UNIVERSE_LOOP_ACCOUNT_ID = 'acc_mvp_paper'
$env:QUANTD_EXEC_SYMBOL_COOLDOWN_SECS = '300'
$env:RUST_LOG = 'info'
cargo run -p quantd *> quantd_rehearsal.out.log
```

### Rehearsal Case 1: First Fill

目的：

- 验证 `paper_only` 下第一轮 cycle 确实会下单

操作：

1. 写入 allowlist，只保留 `AAPL.US`
2. 切 `runtime mode = paper_only`
3. 写入 `model.service_url` 与 `strategy.acc_mvp_paper`
4. 触发：

```powershell
$cycle = '{"venue":"US_EQUITY","account_id":"acc_mvp_paper"}'
Invoke-WebRequest -Method Post -Uri http://127.0.0.1:18081/v1/runtime/cycle -ContentType 'application/json' -Body $cycle
```

核对：

```powershell
Invoke-RestMethod http://127.0.0.1:18081/v1/runtime/cycle/latest
Invoke-RestMethod "http://127.0.0.1:18081/v1/runtime/execution-state?account_id=acc_mvp_paper"
Invoke-RestMethod "http://127.0.0.1:18081/v1/orders?account_id=acc_mvp_paper"
```

预期：

- `latest_cycle.accepted` 包含 `AAPL.US`
- `latest_cycle.placed` 至少 1 条
- `execution-state.positions` 中出现 `AAPL.US`
- `orders` 中最新订单状态为 `FILLED`

### Rehearsal Case 2: Same-Direction Position Guard

目的：

- 验证已有同向仓位后不会继续加仓

操作：

- 在 Case 1 成功后，不做清理，立刻再次触发同一轮 cycle

预期：

- `accepted` 仍可能包含 `AAPL.US`
- `placed` 为空
- `skipped.reason = guard_same_direction_position_open`
- `positions.net_qty` 不增加
- `orders` 总数不增加

### Rehearsal Case 3: Open Order Guard

目的：

- 验证本地存在未完成订单时，会直接拒绝新的执行尝试

说明：

- `PaperAdapter` 默认直接写 `FILLED`
- 这里需要手工插一条 `SUBMITTED` 订单

示例 SQL：

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

然后再次触发 cycle。

预期：

- `placed` 为空
- `skipped.reason = guard_open_order_exists`
- `execution-state.open_orders` 中能看到 `AAPL.US`
- 不会新增新的下单记录

### Rehearsal Case 4: Cooldown / Duplicate Guard

目的：

- 验证短时间重复执行不会连续下同方向单

操作：

1. 删除 Case 3 中手工插入的 `SUBMITTED` 订单
2. 删除已有持仓，或者重建干净 DB
3. 在 `paper_only` 下快速连续触发两轮相同 cycle

预期：

- 第一轮允许下单
- 第二轮不会新增订单
- `latest_cycle.skipped` 出现以下之一：
  - `guard_duplicate_idempotency`
  - `guard_cooldown_active`

### 每轮固定检查顺序

1. `GET /v1/runtime/cycle/latest`
2. `GET /v1/runtime/execution-state?account_id=acc_mvp_paper`
3. `GET /v1/orders?account_id=acc_mvp_paper`
4. 看 `quantd_rehearsal.out.log`

建议按“决策 -> 状态 -> 台账 -> 日志”四层对账。

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
8. Rehearsal Case 1 首轮允许下单
9. Rehearsal Case 2 同向持仓被拦截
10. Rehearsal Case 3 open order 被拦截
11. Rehearsal Case 4 短时间重复执行不会重复下单

## 结束清理

结束后可手工清理：

- 停止模型服务
- 停止 `quantd`
- 删除 `quantd_tui_manual.db`
- 若做了 rehearsal，额外删除：
  - `quantd_rehearsal.db`
  - `quantd_rehearsal.out.log`
  - `quantd_rehearsal_model.out.log`

如果你需要复盘现场，就保留 DB 和终端日志。
