# accounting 技术文档

## 职责

`crates/accounting` 负责账户账本、现金、持仓均价、已实现/未实现 PnL 和敞口计算。该模块是内存账本模型，不直接访问数据库、broker 或 API。

## 关键实现

- `AccountBook` 保存 `account_id`、`cash`、`realized_pnl` 和内部 `PositionBook`。
- `PositionBook` 按 symbol 管理 `Position { symbol, qty, avg_price }`，并按 symbol 排序输出持仓快照。
- `buy` / `sell` 同时支持多头加仓、平空、开多、平多和开空；卖出路径校验正数量。
- 所有金额、价格、数量和 PnL 使用 `rust_decimal::Decimal`，避免浮点精度误差。

## 输入输出与持久化

输入是 symbol、qty、price、fee 和外部 mark price；输出是现金、持仓、权益、gross exposure、realized/unrealized PnL。模块自身不持久化，持久化由 runtime/algorithm 调用 `storage` 的语义 command 完成。

## 边界与约束

- 不处理订单状态和成交幂等，订单生命周期属于 `oms`。
- 不处理市场规则、风控阈值或保证金规则，那些属于 `market_rules` / `risk`。
- `sell` 会拒绝非正数量；`buy` 目前假定调用方已经完成正数量校验。
- 账本记录可变，但对外持久化和审计必须通过事件/存储快照保留历史。

## 测试与验证

重点覆盖多头/空头切换、手续费计入 realized PnL、均价计算、权益和敞口计算。涉及金额的断言必须使用 `Decimal`。

