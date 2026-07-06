# core 技术文档

## 职责

`crates/core` 的包名是 `trader-core`，只放领域基础类型和错误，避免与 Rust 标准库 `core` 混淆。它是其他 crate 可以共享的轻量领域层。

## 关键实现

- `trader_core.rs` re-export `account`、`market`、`order`、`symbol`。
- 领域类型覆盖账户、市场、订单状态和 symbol 表达。
- crate 级别启用 `#![forbid(unsafe_code)]`。

## 输入输出与持久化

该模块只提供类型，不直接处理 IO、数据库、网络或配置。

## 边界与约束

- 不能依赖应用层、storage、api、broker、strategy 等高层模块。
- 只能放稳定、通用、跨模块共享的领域类型。
- 新增类型前要确认不会把运行逻辑或外部协议 DTO 下沉进 core。

## 测试与验证

重点覆盖领域枚举/值对象的序列化、状态判断和基础不变量。

