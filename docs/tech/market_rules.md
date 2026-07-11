# market_rules 技术文档

## 职责

`crates/market_rules` 负责市场交易规则校验、合约风险参数、fee rule 和手续费分层引擎。它在订单进入 broker 前提供市场层合法性检查。

## 关键实现

- `MarketRuleSet` 表达 lot size、tick size、min qty、min notional、保证金率等规则。
- provider trait/static/configured provider 提供不同来源的规则。
- contract risk limits 描述合约杠杆、保证金和 notional 限制。
- `FeeRuleEngine` 根据 symbol、订单类型、价格、数量、时间和账户成交量计算费用。
- liquidity role 支持 maker/taker 等费用角色。
- `storage` 按 market/exchange/asset_class/symbol/effective time 选择 lot-size、price-limit、fee、calendar 和 trading-session 记录。
- `/api/v1/market-rules/effective` 和 `trader market-rules effective` 提供本地只读 effective-state readback，包含匹配的 market-rule audit events。
- `trader market-rules audits` 可按 rule type、rule id 和时间窗口查询 `market_rule.*` 变更审计。

## 输入输出与持久化

输入是订单、symbol 规则、fee rule 和账户成交量；输出是校验结果或 fee breakdown。规则读取/保存由 `storage` 承担。

`lot_size_rules`、`price_limit_rules` 和 `fee_rules` 的配置写入会记录到 `event_store`，并通过本地 API/CLI readback 形成 operator evidence。当前治理证据是本地显式 actor/config/audit 语义，不是外部 SSO/IdP、生产 RBAC 或托管审批系统。

## 边界与约束

- market rules 不决定账户是否可承受风险，组合风险属于 `risk`。
- 所有价格、数量、notional、fee 使用 `Decimal`。
- tick/lot/min notional 必须在 broker 前校验。
- 无凭证 smoke 只证明 deterministic SQLite setup、runtime enforcement、effective readback 和 audit readback；不证明真实 broker 市场规则、实盘下单或生产级审批。

## 测试与验证

重点覆盖 tick/lot 边界、min notional、合约保证金、fee tier、账户成交量窗口和未知规则 fallback。

本地运维证据由 `scripts/ops-smoke.ps1` 串联 focused gates：storage market-rule audit、paper market-rule/trading-session runtime enforcement、API effective readback、CLI effective/audit readback。
