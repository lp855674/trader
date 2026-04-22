# Quantd Paper 闭环与 Model 边界设计

> 历史说明：本文中提到的 `services/lstm-service` 代表迁移前路径；当前有效路径是 `services/model`。

日期：2026-04-21
状态：Draft
范围：`quantd` / `trader` 最小 paper 下单闭环、`runtime cycle` 半自动闭环、`services/model` 与 qlib workflow 边界

## 1. 目标

当前仓库的主要问题不是完全没有下单代码，而是：

- 显式手工下单链和半自动下单链都已部分存在
- 但入口语义、模块职责、运行文档、LSTM 训练方式混在一起
- 导致系统难以回答“现在到底怎么下单”“为什么这次下不了单”“LSTM 应该如何接入”

本设计的目标是收敛出一套清晰的运行边界：

1. 明确当前真正参与交易闭环的模块关系
2. 定义最小可复现的 paper 闭环
3. 明确正式有业务意义的半自动路径必须由 LSTM 驱动
4. 将当前 `services/lstm-service` 收敛为 `services/model`
5. 将模型训练正式切换到 qlib workflow/config，而不是长期依赖手写训练脚本

## 2. 非目标

本设计明确不做以下事情：

- 不直接照搬 `vn.py` 做一次性大重构
- 不同时收敛 `risk`、`infra`、`marketdata` 的全部历史扩张
- 不把没有 LSTM 的 deterministic strategy 当正式交易策略
- 不继续把 `POST /v1/tick` 作为 operator 标准运行入口
- 不让 `model` 在线服务承担正式训练职责

## 3. 当前系统的真实交易主链

当前真正影响“能不能下单”的主链，不是整个 workspace 的所有 crate，而是以下模块：

- `crates/trader`
  - 统一 CLI / TUI 入口
  - 只作为 `quantd` 客户端
- `crates/terminal_client`
  - 负责 HTTP / WebSocket 调用
- `crates/api`
  - `quantd` 的入口层
  - 暴露 `/v1/orders`、`/v1/runtime/cycle`、`/v1/tick`
- `crates/pipeline`
  - 半自动调度编排层
  - 负责 ingest、score、execution guard、risk、execute
- `crates/exec`
  - 执行路由与 adapter 层
  - `ExecutionRouter` 依据 `account_id` 解析到 paper/live adapter
- `crates/db`
  - 本地账本、运行控制、cycle history、reconciliation 持久化

### 3.1 当前存在的两条下单路径

当前系统并不是一条统一下单链，而是两条：

1. 显式手工路径

`trader CLI/TUI -> terminal_client -> POST /v1/orders -> api::post_order -> execution_router.submit_manual_order -> adapter -> db`

2. 半自动路径

`POST /v1/runtime/cycle -> api::post_runtime_cycle -> pipeline::run_universe_cycle -> strategy.evaluate_candidate -> accepted -> evaluate_signal_for_tick -> execution_guard -> execute_signal -> execution_router.place_order -> adapter -> db`

此外还存在一个单标的调试入口：

`POST /v1/tick -> pipeline::run_one_tick_for_venue -> strategy -> risk -> execute`

该入口可以保留，但不应继续作为 operator 对“最小闭环”的主要心智模型。

## 4. 当前问题的根因

当前“看起来有下单代码，但系统依然不好用”的根因主要有四点：

### 4.1 手工路径与半自动路径的语义没有被正式声明

- `/v1/orders` 是显式 OMS 式委托入口
- `/v1/runtime/cycle` 是策略编排入口
- `/v1/tick` 是单标的调试入口

这三个入口当前都存在，但缺少明确分层，导致 operator 容易混淆。

另一个关键事实是，半自动路径当前不是“评分一次后直接执行”。

真实实现中会发生两次策略调用：

- 第一次 `evaluate_candidate()` 用于 ranking / accepted
- 第二次 `evaluate_signal_for_tick()` 用于把 accepted symbol 转成真正的执行信号

因此排障时必须明确区分：

- ranking 阶段为什么进入 `accepted`
- execute 阶段为什么最终没有下单，或者下单方向/数量与预期不一致

### 4.2 最小闭环没有被拆成基础闭环与正式闭环

当前仓库在叙事上容易把“能不能下单”和“LSTM 是否接通”混为一谈。

这会导致：

- 基础执行链问题和模型问题混在一起
- 任何下单失败都同时怀疑 `quantd`、paper adapter、LSTM、qlib、模型文件

### 4.3 文档没有单一真源

README、`docs/runbook.md`、`docs/execution/2026-04-13-semi-auto-paper-rehearsal-runbook.md` 分别覆盖不同视角，但没有围绕“显式手工下单”和“半自动 cycle”拆成清晰分层文档。

### 4.4 当前 LSTM 训练方式不是 qlib-native workflow

`services/lstm-service/qlib_pipeline/train.py` 目前的方式是：

- 手动 `qlib.init`
- 手动构造 `Alpha158`
- 手动 rolling window
- 手动 PyTorch 训练 loop
- 手动保存 `.pt`

这适合最初打通，但不适合长期作为正式训练体系。

## 5. 参考 vn.py 后的目标方向

本项目不直接复制 `vn.py` 的代码结构，但可以借用它的职责心智模型。

推荐映射如下：

- `trader` / `terminal_client` / `api`
  - 对应 UI / 应用入口层
- `pipeline`
  - 对应 strategy orchestration 层
- `exec`
  - 对应 gateway + OMS 的组合层
- `db`
  - 对应本地 ledger / state store
- `services/model`
  - 对应独立研究/模型子系统

设计原则是：

- 先让当前主链边界稳定、闭环清楚
- 再在后续阶段评估是否进一步靠拢 `vn.py` 的 event bus / app-engine 分层

## 6. 目标闭环定义

### 6.1 基础闭环：paper smoke

基础闭环用于：

- 开源仓库初次联调
- 排障基线
- 验证交易系统本身可运行

这一层不要求 LSTM，也不要求 qlib。

其目标不是有业务价值，而是回答：

“执行链、账本链、终端链是否通了？”

验收对象包括：

- `trader order submit`
- `trader order amend`
- `trader order cancel`
- `GET /v1/orders`
- `GET /v1/terminal/overview`
- `GET /v1/runtime/execution-state`
- WebSocket order events
- DB 中 `orders` / `fills` / `positions`

### 6.2 正式闭环：LSTM 驱动的 runtime cycle

正式有业务意义的半自动交易路径必须是：

`allowlist -> ingest bars -> LSTM/ALSTM score -> accepted -> execution_guard -> place_order -> paper/live adapter`

这一层回答：

“模型驱动的半自动调度为什么会下这笔单？”

因此：

- 无 LSTM 的 deterministic strategy 只能作为 smoke / 降级基线
- 正式策略路径必须以 LSTM 或其后续模型演进为核心

## 7. 最小运行边界设计

### 7.1 显式手工路径

显式手工下单是 operator 的一等公民路径。

标准入口：

- `POST /v1/orders`
- `POST /v1/orders/:order_id/amend`
- `POST /v1/orders/:order_id/cancel`

其职责是：

- 显式提交、改价改量、撤销订单
- 不经过 `pipeline`
- 不依赖 `model`
- 仅依赖 `ExecutionRouter` 与对应 adapter

### 7.2 半自动路径

半自动路径是 `runtime cycle`。

标准入口：

- `POST /v1/runtime/cycle`

其职责是：

- 拉 allowlist
- ingest 最新 bars
- 评分与筛选
- 生成 accepted / rejected / skipped / placed
- 通过 execution guard 后再执行
- 记录 cycle history

### 7.3 `/v1/tick` 的角色

`POST /v1/tick` 保留为：

- 单标的调试入口
- 诊断 ingest / strategy / execute 单链是否工作

但它不再作为 operator 的标准运行路径，不进入“最小闭环”的主 runbook。

## 8. runtime mode 规则

当前最容易产生歧义的问题之一是：

“手工单是否受 `observe_only` 影响？”

当前实现现状：

- `POST /v1/orders` / `amend` / `cancel` 还没有读取 runtime mode
- 因此当前代码层面并不存在“手工单被 runtime mode 拒绝”的契约

目标设计选择：

- `submit` 与 `amend` 受 runtime mode 约束
- `observe_only` / `degraded` 下禁止 `submit`
- `observe_only` / `degraded` 下禁止 `amend`
- `cancel` 在所有 mode 下都允许，避免 operator 失去撤单能力
- `paper_only` / `enabled` 允许 `submit` / `amend` / `cancel`

拦截层选择：

- 在 `crates/api` 的手工订单入口层拦截
- 不把该约束下沉到 `ExecutionRouter`

错误契约要求：

- 新增手工订单被 mode 拒绝的稳定 `error_code`
- `submit` / `amend` 可使用统一错误码，例如 `runtime_mode_rejected`
- 返回消息需包含当前 mode 与操作名
- `cancel` 不走该拒绝分支

理由：

- 系统语义统一
- operator 不会误以为 `observe_only` 还可以继续加仓或改价
- 同时保留撤单/止损能力

## 9. Model 子系统边界

当前 `services/lstm-service` 应重命名为：

- `services/model`

原因：

- 该子系统不应再被单一模型架构命名绑定
- 后续可能支持 `lstm`、`alstm` 或其它 qlib 模型
- 其核心职责是“模型训练与推理边界”，不是某一个模型名

### 9.1 `services/model` 的职责

该子系统只承担两类职责：

1. 离线训练与评估
   - 使用 qlib workflow/config 运行实验
   - 导出标准 serving artifact
2. 在线推理
   - 加载导出的模型产物
   - 对外提供 `/health` 与 `/predict`

它不应长期承担：

- 在线正式训练
- 临时实验脚本堆积
- 训练逻辑与推理逻辑混写

### 9.2 推荐目录结构

```text
services/model/
  main.py
  readme.md
  pyproject.toml
  requirements.txt
  requirements-dev.txt

  workflow/
    workflow_by_code.py
    train.py
    export.py
    configs/
      lstm_alpha360.yaml
      alstm_alpha360.yaml

  runtime/
    loader.py
    predict.py
    schemas.py

  models/
    .gitkeep

  tests/
    test_health.py
    test_predict.py
    test_train_workflow.py
    test_export.py
```

其中关键约束是：

- `workflow/` 是训练真源
- `main.py` 不再承担正式训练
- `runtime/` 只负责 serving 侧加载与预测适配
- `models/` 只保存可供 serving 加载的导出产物

## 10. qlib workflow 设计

### 10.1 设计选择

训练正式切换到 qlib workflow/config。

不再把当前手写 `qlib_pipeline/train.py` 当作长期方案。

### 10.2 原因

当前手写训练方式存在这些不足：

- 训练参数、数据集、模型、评估边界混在一个文件
- 缺少 workflow 级实验管理
- 缺少标准化训练产物
- 服务化训练与研究训练职责混在一起

切到 qlib workflow 后，训练侧职责将变为：

- dataset 配置
- model 配置
- train / valid / test 切分
- record / benchmark
- export serving artifact

### 10.3 真源约束

推荐真源是 config：

- yaml config 负责实验模板
- `workflow_by_code.py` 负责仓库内脚本调用与薄封装

换句话说：

- 训练配置以 config 为真源
- 代码脚本只是调用入口

## 11. 训练产物契约

训练与在线推理之间必须通过“导出产物契约”解耦。

不允许 `quantd` 或 `main.py` 依赖训练代码内部细节推断模型结构。

### 11.1 推荐产物结构

```text
services/model/models/
  alstm_alpha360_us_v1/
    model.pt
    metadata.json
```

### 11.2 `metadata.json` 最低要求

至少包含：

- `model_id`
- `model_type`
- `feature_set`
- `lookback`
- `train_start`
- `train_end`
- `trained_at`
- `qlib_region`
- `symbol_universe` 或适用范围
- `prediction_semantics`

### 11.3 serving 侧行为

`services/model/main.py` 启动时应：

- 扫描 `models/`
- 读取 `metadata.json`
- 按 metadata 初始化 loader
- 暴露统一 `/predict`

## 12. `quantd` 对 `model` 的依赖规则

`quantd` 对 `model` 的依赖仅发生在半自动路径中。

### 12.1 显式手工路径

- 不依赖 `model`
- 不依赖 qlib

### 12.2 半自动路径

- `strategy.evaluate_candidate()` 或等效评分层调用 `model /predict`，生成 ranked / accepted / rejected
- 对 accepted symbol 再调用 `evaluate_signal_for_tick()`，把 symbol 级候选转成真正执行信号

这意味着正式设计中必须明确接受“双阶段调用”：

1. candidate 阶段回答“这个 symbol 是否值得进入 accepted”
2. signal 阶段回答“这个 accepted symbol 是否形成可执行订单”

后续若要减少重复调用，可以作为实现优化，但在当前 spec 中不能假装这一步不存在

### 12.3 故障语义

当前实现现状：

- `strategy::lstm` 返回的是字符串错误
- `pipeline` 当前会把这些错误进一步包装成：
  - `strategy_error:...`
  - `execution_error:...`
- `runtime cycle history` 当前持久化的是 `reason` 字符串，而不是结构化错误对象

目标设计要求：

- `strategy::lstm` 或其后继 `model` strategy 层必须先把服务错误归一化为稳定错误码
- `pipeline` 不负责理解底层 HTTP 文本，只负责传播标准化结果
- `runtime cycle history` 至少继续持久化一个稳定字符串 `reason_code`
- 如需兼容现有 schema，可先将 `reason` 规范成：
  - `model_unreachable`
  - `model_not_found`
  - `insufficient_bars`
  - `response_parse_failed`
  - `model_service_error`
- 若未来扩表，可再引入 `reason_message`

归一化责任边界：

- `services/model` 定义服务错误返回格式
- `strategy::lstm` 负责把服务错误映射成统一 `ModelErrorCode`
- `pipeline` 负责把该 code 写入 `skipped.reason`

强约束：

- 不允许因为模型失败退化到默认策略继续下单
- cycle history 中必须保留稳定的错误码，而不是仅保留不可比较的自由文本

## 13. 测试分层设计

LSTM/Model 与交易系统测试必须拆成 4 层。

### 13.1 Model 子系统单测

放在 `services/model/tests/`，至少覆盖：

- `test_health.py`
- `test_predict.py`
- `test_train_workflow.py`
- `test_export.py`

其目的是验证：

- 训练
- 导出
- 加载
- 预测

这些行为在不启动 `quantd` 时也能独立通过。

### 13.2 Model 服务独立 runbook

建议文档：

- `docs/runbook-model.md`

只说明：

- qlib 数据准备
- workflow 训练
- export 模型
- 启动 `services/model/main.py`
- 验证 `/health`、`/predict`

### 13.3 `quantd` + model 联调 runbook

建议文档：

- `docs/runbook-lstm-cycle-paper.md`

只说明：

- `model` 服务启动
- `quantd` 配置 `model.service_url`
- `strategy.<account_id>` 写入 LSTM / ALSTM 配置
- `runtime mode`、allowlist、cycle、history、execution-state 的联调

### 13.4 基础执行链降级 runbook

建议文档：

- `docs/runbook-paper-smoke.md`

该文档刻意不依赖 LSTM，仅用于回答：

- 执行链是否工作
- 账本链是否工作
- TUI / CLI / WS 是否工作

## 14. 文档重构建议

仓库规则要求 `docs/runbook.md` 保持启动手册真源，因此文档不能再把多个 `docs/runbook-*.md` 作为新的真源。

本设计调整为：

- `docs/runbook.md`
  - 保持为 operator 启动与联调真源
  - 其中明确拆分三个章节：
    - paper smoke
    - model workflow / service
    - LSTM cycle paper
- `services/model/readme.md`
  - 作为 model 子系统自己的模块文档与开发手册
- 必要时允许新增辅助手册，但它们不是行为真源，真源仍需回写到 `docs/runbook.md` 与对应模块文档

README 只保留总览与文档导航，不再承担完整运行手册角色。

## 15. `services/lstm-service` -> `services/model` 迁移兼容期

重命名与产物结构切换不能一步切断现有 `.pt` 发现逻辑，需要定义兼容期。

### 15.1 现状

当前 serving 逻辑按根目录下的 `<symbol>_<model_type>.pt` 直接发现模型。

### 15.2 目标

目标产物切换为目录化 artifact：

```text
services/model/models/
  <model_id>/
    model.pt
    metadata.json
```

### 15.3 兼容期策略

迁移期内 runtime loader 应同时支持：

- 新格式：`<model_id>/model.pt + metadata.json`
- 旧格式：根目录 `<symbol>_<model_type>.pt`

兼容期行为：

- 优先加载新格式 artifact
- 若未命中新格式，再回退旧格式
- `/health` 应暴露新旧格式各自加载数量，便于迁移排障

兼容期结束条件：

- workflow/export 已稳定产出新格式
- runbook 与测试全部切到新格式
- 仓库内不再依赖旧 `.pt` 发现逻辑

## 16. 分阶段实施路线

### Phase 1：交易边界收敛

- 明确 `/v1/orders` 与 `/v1/runtime/cycle` 的角色
- 将 `/v1/tick` 降级为调试入口
- 固定 runtime mode 对手工 `submit` / `amend` / `cancel` 的约束
- 为 mode 拒绝补稳定 `error_code`
- 在 spec 与实现里明确半自动路径的双阶段策略调用

### Phase 2：paper 基础闭环

- 跑通 submit / amend / cancel
- 跑通 overview / execution-state / WS / DB 一致性
- 将 paper smoke 真源写回 `docs/runbook.md`

### Phase 3：Model 子系统重构

- `services/lstm-service` -> `services/model`
- 拆分 workflow 与 runtime
- 将训练切换到 qlib workflow/config
- 导出 serving artifact
- 增加新旧 artifact 兼容加载期

### Phase 4：LSTM 正式闭环

- `quantd` 稳定调用 `model /predict`
- 跑通 `runtime cycle` 的 LSTM/ALSTM paper 闭环
- 将 LSTM cycle 真源写回 `docs/runbook.md`
- 统一 `model` 相关错误码与 cycle history 的 `reason`

### Phase 5：后续再评估向 vn.py 靠拢

待闭环稳定后，再评估：

- event bus
- app-engine 化
- 更清晰的 gateway 分层

而不是在此之前提前大重构。

## 17. 验收标准

本设计完成后，系统需要能明确回答以下问题：

1. 手工单到底走哪条链
2. 半自动单到底走哪条链
3. 为什么没有 LSTM 时仍然需要基础 paper 闭环
4. 为什么正式半自动路径必须以 LSTM 驱动
5. `services/model`、qlib workflow、在线推理、`quantd` 之间的边界是什么

当这五个问题都能被清楚回答时，后续实现才不会继续陷入“代码有了，但系统解释不清”的状态。
