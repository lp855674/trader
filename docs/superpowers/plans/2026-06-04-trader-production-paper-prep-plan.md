# Trader Production Paper Prep 执行计划

## 目标

把 V1 本地闭环推进到可以开始实际 paper 验证的状态。这里的 paper 是生产前验证环境：使用真实配置、真实运行控制、真实持久化和可审计报告，但仍不连接真实券商网络、不发送真实资金订单。

## 边界

- 保留 V1 已完成代码，不重写架构。
- Inline Execution：所有任务在当前分支顺序执行，不使用子代理。
- 先完成 MVP 核心交易链路可验证性，再补更大的 broker/live 能力。
- SQL 仍只允许在 `crates/storage`。
- 新测试放在各 crate 的 `tests/` 目录。
- 新增配置必须同步 `tech.md`、`docs/architecture.md` 或对应专题文档。

## 完成标准

- Paper run 不再依赖隐藏硬编码风控默认值；风控、broker、live/paper 关键开关来自配置。
- CLI 与 REST 启动 paper/live surface 时使用同一套配置映射。
- Paper runtime 可以用 paced bar stream 进行更接近在线 paper 的本地验证，并支持取消。
- Fake broker 提供 paper 测试需要的订单查询、撤单、账户状态接口。
- 提供 `scripts/paper-smoke.ps1`，一条命令完成 migrate、paper start、status/control、report、broker/account 查询验证。
- `cargo fmt --all -- --check`、`cargo check --workspace --locked`、相关 crate 测试和 smoke 通过。

## 阶段 1：配置真源

1. 在 `crates/config/tests` 先写失败测试，覆盖 `[risk]`、`[broker]`、`[live]` 配置解析。
2. 在 `crates/config` 增加 `RiskConfig`、`BrokerConfig`、`LiveConfig`。
3. 将 CLI/API 的 `backtest_settings`、`paper_settings`、`LiveRuntimeSettings` 从配置构造，移除风控和 Futu hard-code。
4. 更新样例配置和文档。
5. 验证：`cargo test -p config`、`cargo check --workspace --locked`。

## 阶段 2：Paper Streaming Runtime

1. 在 `crates/data` 或 `crates/paper` 增加 deterministic bar stream 测试：按输入顺序输出、支持 pacing、支持取消。
2. 拆分 `PaperRuntime` 内部单 bar 处理逻辑，`run_bars` 与 stream 入口复用同一核心路径。
3. 增加 `run_bar_stream` 或等价入口，用于后续 server paper 测试。
4. 验证：`cargo test -p paper -p data`。

## 阶段 3：Broker Paper Surface

1. 在 `crates/broker/tests` 先定义 fake broker 期望行为：place、query、cancel、account snapshot/status。
2. 扩展 broker trait 与 fake adapters；状态必须确定、可重复。
3. REST 增加必要查询端点，避免 UI/脚本只能看持久化结果而看不到 broker surface。
4. 更新 `docs/broker.md`。
5. 验证：`cargo test -p broker -p api`。

## 阶段 4：Paper Smoke

1. 新增 `configs/paper/local.toml` 或更新现有样例，覆盖风险、broker、paper pacing。
2. 新增 `scripts/paper-smoke.ps1`：迁移、启动 server、发起 paper run、查询 status/events/orders/fills/account/report、验证 broker surface。
3. 文档写清 paper 测试启动步骤、失败排查、当前不包含真实券商网络。
4. 验证：`powershell -ExecutionPolicy Bypass -File .\scripts\paper-smoke.ps1`。

## 阶段 5：进入真实 Paper 前检查

1. 增加 preflight：配置合法性、数据库可写、行情源可读、risk 阈值非空、broker mode 为 paper。
2. 增加 dry-run 报告，列出将使用的 run id、symbols、risk limits、broker kind/mode。
3. 给真实券商 adapter 留接口边界，但不在本阶段接入网络凭证。

## 当前下一步

从阶段 1 开始：先写配置解析失败测试，再实现配置结构和 CLI/API 映射。
