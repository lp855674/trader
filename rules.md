# trader 编码规则（Rust）

## 1) 基础

- 禁止 `unwrap()` / `expect()`（**测试代码**与 **启动阶段诊断** 除外）；库代码用 `?` 或显式分支。
- 仅 **`db` crate** 可使用 `sqlx` 与内联 SQL；其它 crate 通过 `db` 暴露的接口访问持久化。

## 2) 日志

- 使用 `tracing` 结构化字段（如 `channel = "quantd"`, `venue = ?`）；关键语义放在字段而非长字符串。

## 3) 密钥

- API key、密钥不得写入日志或对外可追溯明文。

## 4) 验证

- 提交前至少：`cargo test`、`cargo clippy`（若已配置）。
