# Trader  项目编码规则（Rust）

> 本文件用于约束 `crates/*` 与项目文档的一致性。内容以本仓库现状为准。

## 1) Rust 基础规范

- 优先保证正确性与可读性；性能优化必须有明确场景。
- 注释只解释“为什么”，不写复述代码的注释。
- 非必要不拆新文件；优先在现有模块演进。
- 禁止 `unwrap()` / `expect()`（测试代码除外）；统一使用 `?` 或显式错误分支。
- 谨慎使用可能 panic 的索引访问；优先 `get()` 等安全访问方式。
- 禁止缩写变量名（如 `q`/`tx2`/`rt2` 这类不含语义命名）。
- 禁止 `mod.rs` 目录式模块；统一使用 `src/<name>.rs`。

## 2) 错误处理规范

- 默认禁止对可失败操作使用 `let _ = ...` 静默丢错。
- 允许“best-effort 忽略错误”的唯一场景：
  - 审计日志追加失败不应阻断主流程；
  - websocket/oneshot 回包时连接已断开；
  - 退出阶段的 `join/abort` 清理动作；
  - telemetry/observer 发射失败。
- 对上述允许场景，必须满足两条：
  1. 行内加简短原因注释（如 `// best-effort: client may disconnect`）；
  2. 若是可观测链路，尽量带 `debug!/warn!`，并严格遵循 **3.1** 的结构化字段写法。

## 3) 异步与并发规范

- async 失败必须能传播到调用方或 UI 层，返回可读错误。
- 新增后台任务必须明确生命周期（await / detach / 持有句柄）。
- 可取消流程必须沿用 `CancellationToken` 传递，不要私有取消协议。

## 3.1) 日志写法规范（结构化 tracing）

- 必须使用结构化字段写法，遵循以下形态：
  - `info!(channel = "ws_nodes", node = %name, "node registered");`
  - `warn!(channel = "http", error_code = "rate_limited", "chat request rejected");`
- 推荐字段顺序：
  1. 上下文字段（`channel`、`session_id`、`trace_id`、`tool`、`node`）
  2. 结果字段（`error_code`、`error_category`、`status`）
  3. 错误对象（`err = %e`）
  4. 最后是日志消息字符串
- message 文本必须简短，关键语义放在字段里，不要把结构化信息拼到字符串中。
- 日志级别约定：
  - `debug`：过程细节、best-effort 失败、诊断信息
  - `info`：状态变更、生命周期事件（启动/结束/注册/注销）
  - `warn`：可恢复错误、降级路径
  - `error`：请求失败或影响主链路的错误

## 4) 架构边界（强约束）

- 仅 `crates/db` 允许依赖/使用 `sqlx` 与内联 SQL。
- 其它 crate 访问数据必须走 `db::Db` 暴露的接口，禁止透传 `SqlitePool`。
- tools 文件能力必须受 workspace 边界约束（禁止绝对路径、`..`、symlink escape）。
- gateway/channels/app 的请求处理必须复用统一契约：`ChannelRequest/ChannelResponse`。

## 5) 依赖与版本策略

- 小需求禁止引入重依赖；优先复用现有 crate。
- 若与外部“skill 示例”版本冲突，以仓库当前 `Cargo.toml` 为准。
  - 例如本项目当前 `crossterm = 0.27`，不要按通用示例强行升到 `0.28`。

## 6) 文档规范

- 模块文档单一来源：`crates/<module>/tech.md`。
- `docs/runbook.md` 保留作为启动手册；其它历史拆分文档不再作为真源。
- 任何行为/配置变更，必须同步更新对应模块 `tech.md` 与必要的 `runbook.md`。
- 禁止在文档中保留失效路径引用（删除文件前必须改完反向链接）。

## 7) 验证规范

- 代码改动后至少执行：
  - Windows：`powershell -ExecutionPolicy Bypass -File .\script\verify.ps1`
  - Linux/macOS：`bash ./script/verify`
  - 至少包含 `cargo check`
  - `./script/clippy`（不要直接 `cargo clippy`）
- 触及 DB 边界时额外执行：
  - `./script/check-db-boundary` 或 `./script/check-db-boundary.ps1`
- 高风险改动（gateway/tools/daemon/db）需要补充风险说明与回滚思路。

## 8) 风险分级

- Low: docs/chore/test-only
- Medium: 常规 `src/`** 行为改动
- High: `gateway/tools/daemon/db`、安全边界、访问控制、工作流/发布相关

不确定时按更高风险处理。

## 9) Secrets 与配置卫生（保留）

- 永远不要提交 secrets、个人隐私数据或真实身份信息。
- API keys 等敏感信息仅允许通过环境变量或本地 `.env` 使用。
- `.env` 必须在 `.gitignore` 中被忽略。
- 文档、测试数据、示例代码中同样禁止出现真实密钥或个人隐私信息。

## 10) 分支/提交/PR 流程（保留）

- 不直接向 `master` 推送；使用功能分支并通过 PR 合并。
- 提交信息建议遵循 conventional commit，优先小 PR（`XS/S/M`）。
- 变更说明需覆盖：行为影响、风险级别、副作用、回滚方式。
- 若采用 stacked PR：
  - 依赖链需标注 `Depends on #...`
  - 替换旧 PR 需标注 `Supersedes #...`

## 11) Anti-Patterns（保留）

- 不为小需求引入重依赖。
- 不静默弱化安全边界或访问约束。
- 不预埋“也许以后会用”的配置开关。
- 不把大规模格式化改动与功能改动混在同一个 patch。
- 不“顺手”修改无关模块。
- 不在没有说明的情况下绕过失败检查。
- 不隐藏行为变化（例如把行为变更伪装为纯重构）。

