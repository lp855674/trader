# Trader MVP Core Rules Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking. Execute inline; do not dispatch subagents.

**Goal:** Turn the current local paper/backtest vertical slice into a stronger MVP trading core by implementing market rules, risk checks, OMS lifecycle, execution delta logic, and paper runtime integration.

**Architecture:** Keep the code already written, but stop advancing peripheral API features until the core order path is materially correct. The MVP order path must become: `Strategy -> Portfolio target -> Execution delta -> MarketRules -> Risk -> OMS -> Broker simulation -> Accounting -> Storage`. SQL remains in `storage`; strategy remains independent from storage/broker/API.

**Tech Stack:** Rust 2024, rust_decimal, Tokio, SQLx SQLite, Axum, serde.

---

## Current Baseline

- Current branch: `mvp-core-rules`, branched from `main` after merging runtime-manager work.
- `paper` can run a local MA cross loop, persist orders/fills/account/snapshots, and be started asynchronously through REST.
- `market_rules` is still a placeholder.
- `risk` only checks max absolute target position.
- `oms` only supports a minimal submit/accept/fill path and does not track quantities.
- `execution` only converts target quantity directly into a market order, without current-position delta semantics.
- `rules.md` has stale wording: it mentions `crates/db`, but this repo uses `crates/storage`; it also says cancellation must use `CancellationToken`, while the current code has `runtime::CancellationFlag`.

## Execution Rules

- Use inline execution only; no subagents.
- Use TDD: add failing tests first, verify red, implement, verify green.
- Keep small commits after each task passes.
- Keep SQL inside `crates/storage`.
- Keep financial values as `rust_decimal::Decimal` in Rust and decimal strings in SQLite.
- Keep production tests in `tests/`; do not add inline `#[cfg(test)] mod tests`.
- Keep explicit library entries: every library crate uses `[lib] path = "src/<crate_name>.rs"`.
- Use workspace dependencies for internal crates.

## File Structure

Modify:

- `rules.md`: align repository-specific wording with current `storage` and runtime cancellation implementation.
- `tech.md`: clarify current project status as MVP core rules work, not roadmap Phase 6/7 completion.
- `docs/superpowers/plans/2026-06-02-trader-mvp-core-rules-plan.md`: track execution.
- `crates/market_rules/src/market_rules.rs`: implement rule model and order validation.
- `crates/risk/src/risk.rs`: implement order-level risk policy.
- `crates/oms/src/oms.rs`: implement quantity-aware order lifecycle.
- `crates/execution/src/execution.rs`: implement target-vs-current position delta order generation.
- `crates/paper/src/paper.rs`: use execution delta, market rules, risk, and OMS before broker simulation.

Create:

- `crates/market_rules/tests/market_rules_tests.rs`
- `crates/risk/tests/risk_tests.rs`
- `crates/execution/tests/execution_tests.rs`

Existing tests to modify:

- `crates/oms/tests/oms_tests.rs`
- `crates/paper/tests/paper_tests.rs`
- `crates/paper/tests/persistent_paper_tests.rs`

---

### Task 1: Project Rule and Status Alignment

**Files:**
- Modify: `rules.md`
- Modify: `tech.md`

- [x] **Step 1: Update stale storage boundary wording**

In `rules.md`, replace:

```markdown
- 仅 `crates/db` 允许依赖/使用 `sqlx` 与内联 SQL。
- 其它 crate 访问数据必须走 `db::Db` 暴露的接口，禁止透传 `SqlitePool`。
```

with:

```markdown
- 仅 `crates/storage` 允许依赖/使用 `sqlx` 与内联 SQL。
- 其它 crate 访问数据必须走 `storage::Db` 暴露的接口，禁止透传 `SqlitePool`。
```

- [x] **Step 2: Align cancellation wording with current implementation**

In `rules.md`, replace:

```markdown
- 可取消流程必须沿用 `CancellationToken` 传递，不要私有取消协议。
```

with:

```markdown
- 可取消流程必须沿用仓库统一取消类型传递。当前统一类型是 `runtime::CancellationFlag`；若未来引入 `tokio_util::sync::CancellationToken`，必须一次性迁移并更新本规则。
```

- [x] **Step 3: Clarify current status in tech.md**

In `tech.md`, add under Phase 6 Runtime Manager:

```markdown
当前状态仍是 MVP vertical slice，不代表 roadmap 中的分布式 Phase 6 已完成。下一步重点是补齐 MVP 核心交易规则：Market Rules、Risk、OMS、Execution delta，以及 PaperRuntime 对这些规则的串联。
```

- [x] **Step 4: Verify and commit**

Run:

```powershell
cargo fmt --all -- --check
cargo check --workspace --locked
```

Commit:

```powershell
git add rules.md tech.md docs/superpowers/plans/2026-06-02-trader-mvp-core-rules-plan.md
git commit -m "docs: align mvp core rules direction"
```

---

### Task 2: Market Rules Validation

**Files:**
- Modify: `crates/market_rules/src/market_rules.rs`
- Create: `crates/market_rules/tests/market_rules_tests.rs`

- [x] **Step 1: Add failing market rules tests**

Create `crates/market_rules/tests/market_rules_tests.rs`:

```rust
use market_rules::{MarketRuleError, MarketRuleSet};
use rust_decimal::Decimal;
use trader_core::{OrderRequest, OrderSide, OrderType};

#[test]
fn rejects_quantity_below_lot_size() {
    let rules = MarketRuleSet::us_equity();
    let order = market_order(Decimal::new(5, 1));

    assert_eq!(
        rules.validate_order(&order, Decimal::from(100)).unwrap_err(),
        MarketRuleError::InvalidLotSize
    );
}

#[test]
fn rejects_limit_price_off_tick_size() {
    let rules = MarketRuleSet::us_equity();
    let mut order = market_order(Decimal::ONE);
    order.order_type = OrderType::Limit;
    order.price = Some(Decimal::new(100_001, 3));

    assert_eq!(
        rules.validate_order(&order, Decimal::from(100)).unwrap_err(),
        MarketRuleError::InvalidTickSize
    );
}

#[test]
fn rejects_notional_below_minimum() {
    let rules = MarketRuleSet {
        lot_size: Decimal::ONE,
        tick_size: Decimal::new(1, 2),
        min_qty: Decimal::ONE,
        min_notional: Decimal::from(100),
        allow_market_orders: true,
    };
    let order = market_order(Decimal::ONE);

    assert_eq!(
        rules.validate_order(&order, Decimal::from(50)).unwrap_err(),
        MarketRuleError::MinNotional
    );
}

#[test]
fn accepts_valid_us_equity_market_order() {
    let rules = MarketRuleSet::us_equity();
    rules
        .validate_order(&market_order(Decimal::ONE), Decimal::from(100))
        .unwrap();
}

fn market_order(qty: Decimal) -> OrderRequest {
    OrderRequest {
        symbol: "US:NASDAQ:AAPL:EQUITY".to_string(),
        side: OrderSide::Buy,
        order_type: OrderType::Market,
        qty,
        price: None,
        account_id: "paper".to_string(),
    }
}
```

- [x] **Step 2: Run tests and verify RED**

Run:

```powershell
cargo test -p market_rules
```

Expected: FAIL because `MarketRuleSet`, `MarketRuleError`, and `validate_order` do not exist.

- [x] **Step 3: Implement market rule model**

Replace `crates/market_rules/src/market_rules.rs` with:

```rust
#![forbid(unsafe_code)]

use rust_decimal::Decimal;
use thiserror::Error;
use trader_core::{OrderRequest, OrderType};

#[derive(Debug, Error, PartialEq, Eq)]
pub enum MarketRuleError {
    #[error("quantity is below minimum quantity")]
    MinQuantity,
    #[error("quantity is not a multiple of lot size")]
    InvalidLotSize,
    #[error("price is not a multiple of tick size")]
    InvalidTickSize,
    #[error("order notional is below minimum notional")]
    MinNotional,
    #[error("market orders are not allowed")]
    MarketOrdersDisabled,
    #[error("reference price must be positive")]
    InvalidReferencePrice,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarketRuleSet {
    pub lot_size: Decimal,
    pub tick_size: Decimal,
    pub min_qty: Decimal,
    pub min_notional: Decimal,
    pub allow_market_orders: bool,
}

impl MarketRuleSet {
    pub fn us_equity() -> Self {
        Self {
            lot_size: Decimal::ONE,
            tick_size: Decimal::new(1, 2),
            min_qty: Decimal::ONE,
            min_notional: Decimal::ZERO,
            allow_market_orders: true,
        }
    }

    pub fn validate_order(
        &self,
        order: &OrderRequest,
        reference_price: Decimal,
    ) -> Result<(), MarketRuleError> {
        if reference_price <= Decimal::ZERO {
            return Err(MarketRuleError::InvalidReferencePrice);
        }
        if order.order_type == OrderType::Market && !self.allow_market_orders {
            return Err(MarketRuleError::MarketOrdersDisabled);
        }
        if order.qty < self.min_qty {
            return Err(MarketRuleError::MinQuantity);
        }
        if !is_multiple(order.qty, self.lot_size) {
            return Err(MarketRuleError::InvalidLotSize);
        }
        if let Some(price) = order.price
            && !is_multiple(price, self.tick_size)
        {
            return Err(MarketRuleError::InvalidTickSize);
        }

        let price = order.price.unwrap_or(reference_price);
        if price * order.qty < self.min_notional {
            return Err(MarketRuleError::MinNotional);
        }
        Ok(())
    }
}

fn is_multiple(value: Decimal, step: Decimal) -> bool {
    if step <= Decimal::ZERO {
        return false;
    }
    value % step == Decimal::ZERO
}
```

- [x] **Step 4: Verify and commit**

Run:

```powershell
cargo fmt --all -- --check
cargo test -p market_rules
cargo check --workspace --locked
```

Commit:

```powershell
git add crates/market_rules docs/superpowers/plans/2026-06-02-trader-mvp-core-rules-plan.md
git commit -m "feat: add market order rules"
```

---

### Task 3: Order-Level Risk Policy

**Files:**
- Modify: `crates/risk/src/risk.rs`
- Create: `crates/risk/tests/risk_tests.rs`

- [x] **Step 1: Add failing risk policy tests**

Create `crates/risk/tests/risk_tests.rs`:

```rust
use risk::{RiskError, RiskPolicy};
use rust_decimal::Decimal;
use trader_core::{OrderRequest, OrderSide, OrderType};

#[test]
fn rejects_order_quantity_above_limit() {
    let policy = RiskPolicy::new(Decimal::from(10), Decimal::from(1_000), Decimal::from(500));
    let order = buy_order(Decimal::from(11));

    assert_eq!(
        policy
            .check_order(&order, Decimal::from(100), Decimal::from(10_000), false)
            .unwrap_err(),
        RiskError::MaxOrderQuantity
    );
}

#[test]
fn rejects_order_notional_above_limit() {
    let policy = RiskPolicy::new(Decimal::from(100), Decimal::from(1_000), Decimal::from(500));
    let order = buy_order(Decimal::from(11));

    assert_eq!(
        policy
            .check_order(&order, Decimal::from(100), Decimal::from(10_000), false)
            .unwrap_err(),
        RiskError::MaxOrderNotional
    );
}

#[test]
fn rejects_buy_when_cash_is_insufficient() {
    let policy = RiskPolicy::new(Decimal::from(100), Decimal::from(10_000), Decimal::from(500));
    let order = buy_order(Decimal::from(6));

    assert_eq!(
        policy
            .check_order(&order, Decimal::from(100), Decimal::from(500), false)
            .unwrap_err(),
        RiskError::InsufficientCash
    );
}

#[test]
fn rejects_when_trading_halted() {
    let policy = RiskPolicy::new(Decimal::from(100), Decimal::from(10_000), Decimal::from(500));

    assert_eq!(
        policy
            .check_order(&buy_order(Decimal::ONE), Decimal::from(100), Decimal::from(500), true)
            .unwrap_err(),
        RiskError::TradingHalted
    );
}

fn buy_order(qty: Decimal) -> OrderRequest {
    OrderRequest {
        symbol: "US:NASDAQ:AAPL:EQUITY".to_string(),
        side: OrderSide::Buy,
        order_type: OrderType::Market,
        qty,
        price: None,
        account_id: "paper".to_string(),
    }
}
```

- [x] **Step 2: Run tests and verify RED**

Run:

```powershell
cargo test -p risk
```

Expected: FAIL because `RiskPolicy`, order-level errors, and `check_order` do not exist.

- [x] **Step 3: Implement risk policy**

Update `crates/risk/src/risk.rs` while keeping `check_max_position` for compatibility:

```rust
use trader_core::{OrderRequest, OrderSide};

#[derive(Debug, Error, PartialEq, Eq)]
pub enum RiskError {
    #[error("target quantity exceeds max position")]
    MaxPosition,
    #[error("order quantity exceeds max order quantity")]
    MaxOrderQuantity,
    #[error("order notional exceeds max order notional")]
    MaxOrderNotional,
    #[error("buy order requires more cash than available")]
    InsufficientCash,
    #[error("trading is halted")]
    TradingHalted,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RiskPolicy {
    pub max_order_qty: Decimal,
    pub max_order_notional: Decimal,
    pub min_cash_after_order: Decimal,
}

impl RiskPolicy {
    pub fn new(
        max_order_qty: Decimal,
        max_order_notional: Decimal,
        min_cash_after_order: Decimal,
    ) -> Self {
        Self {
            max_order_qty,
            max_order_notional,
            min_cash_after_order,
        }
    }

    pub fn check_order(
        &self,
        order: &OrderRequest,
        reference_price: Decimal,
        available_cash: Decimal,
        trading_halted: bool,
    ) -> Result<(), RiskError> {
        if trading_halted {
            return Err(RiskError::TradingHalted);
        }
        if order.qty > self.max_order_qty {
            return Err(RiskError::MaxOrderQuantity);
        }
        let notional = order.qty * order.price.unwrap_or(reference_price);
        if notional > self.max_order_notional {
            return Err(RiskError::MaxOrderNotional);
        }
        if order.side == OrderSide::Buy && available_cash - notional < self.min_cash_after_order {
            return Err(RiskError::InsufficientCash);
        }
        Ok(())
    }
}
```

- [x] **Step 4: Verify and commit**

Run:

```powershell
cargo fmt --all -- --check
cargo test -p risk
cargo check --workspace --locked
```

Commit:

```powershell
git add crates/risk docs/superpowers/plans/2026-06-02-trader-mvp-core-rules-plan.md
git commit -m "feat: add order risk policy"
```

---

### Task 4: Quantity-Aware OMS

**Files:**
- Modify: `crates/oms/src/oms.rs`
- Modify: `crates/oms/tests/oms_tests.rs`

- [x] **Step 1: Add failing OMS tests**

Append to `crates/oms/tests/oms_tests.rs`:

```rust
use rust_decimal::Decimal;

#[test]
fn partial_fill_tracks_cumulative_and_remaining_quantity() {
    let mut machine = OrderStateMachine::with_order_qty(Decimal::from(10));
    machine.submit().unwrap();
    machine.accept().unwrap();

    machine.record_fill(Decimal::from(4)).unwrap();
    assert_eq!(machine.status(), OrderStatus::PartiallyFilled);
    assert_eq!(machine.filled_qty(), Decimal::from(4));
    assert_eq!(machine.remaining_qty(), Decimal::from(6));

    machine.record_fill(Decimal::from(6)).unwrap();
    assert_eq!(machine.status(), OrderStatus::Filled);
    assert_eq!(machine.remaining_qty(), Decimal::ZERO);
}

#[test]
fn rejects_overfill() {
    let mut machine = OrderStateMachine::with_order_qty(Decimal::from(10));
    machine.submit().unwrap();
    machine.accept().unwrap();

    assert_eq!(
        machine.record_fill(Decimal::from(11)).unwrap_err(),
        oms::OmsError::Overfill
    );
}

#[test]
fn cancel_requested_order_can_cancel_before_fill() {
    let mut machine = OrderStateMachine::with_order_qty(Decimal::from(10));
    machine.submit().unwrap();
    machine.request_cancel().unwrap();
    machine.cancel().unwrap();

    assert_eq!(machine.status(), OrderStatus::Canceled);
}
```

- [x] **Step 2: Run tests and verify RED**

Run:

```powershell
cargo test -p oms
```

Expected: FAIL because `with_order_qty`, `record_fill`, quantity accessors, `Overfill`, and cancel methods do not exist.

- [x] **Step 3: Implement quantity-aware OMS**

Update `crates/oms/src/oms.rs`:

```rust
use rust_decimal::Decimal;
```

Extend `OmsError`:

```rust
#[error("fill quantity must be positive")]
InvalidFillQuantity,
#[error("fill quantity exceeds remaining quantity")]
Overfill,
```

Extend `OrderStateMachine`:

```rust
order_qty: Decimal,
filled_qty: Decimal,
```

Add methods:

```rust
pub fn with_order_qty(order_qty: Decimal) -> Self {
    Self {
        status: OrderStatus::New,
        order_qty,
        filled_qty: Decimal::ZERO,
    }
}

pub fn filled_qty(&self) -> Decimal {
    self.filled_qty
}

pub fn remaining_qty(&self) -> Decimal {
    self.order_qty - self.filled_qty
}

pub fn request_cancel(&mut self) -> Result<(), OmsError> {
    self.transition(
        OrderStatus::PendingCancel,
        &[OrderStatus::New, OrderStatus::PendingSubmit, OrderStatus::Submitted],
    )
}

pub fn cancel(&mut self) -> Result<(), OmsError> {
    self.transition(OrderStatus::Canceled, &[OrderStatus::PendingCancel])
}

pub fn reject(&mut self) -> Result<(), OmsError> {
    self.transition(
        OrderStatus::Rejected,
        &[OrderStatus::New, OrderStatus::PendingSubmit, OrderStatus::Submitted],
    )
}

pub fn record_fill(&mut self, fill_qty: Decimal) -> Result<(), OmsError> {
    if fill_qty <= Decimal::ZERO {
        return Err(OmsError::InvalidFillQuantity);
    }
    if fill_qty > self.remaining_qty() {
        return Err(OmsError::Overfill);
    }
    if !matches!(self.status, OrderStatus::Submitted | OrderStatus::PartiallyFilled) {
        return Err(OmsError::InvalidTransition(self.status));
    }
    self.filled_qty += fill_qty;
    self.status = if self.remaining_qty() == Decimal::ZERO {
        OrderStatus::Filled
    } else {
        OrderStatus::PartiallyFilled
    };
    Ok(())
}
```

Make `new()` call `with_order_qty(Decimal::ONE)` for existing tests.

- [x] **Step 4: Verify and commit**

Run:

```powershell
cargo fmt --all -- --check
cargo test -p oms
cargo check --workspace --locked
```

Commit:

```powershell
git add crates/oms docs/superpowers/plans/2026-06-02-trader-mvp-core-rules-plan.md
git commit -m "feat: track oms fill quantities"
```

---

### Task 5: Execution Delta Orders

**Files:**
- Modify: `crates/execution/src/execution.rs`
- Create: `crates/execution/tests/execution_tests.rs`

- [ ] **Step 1: Add failing execution tests**

Create `crates/execution/tests/execution_tests.rs`:

```rust
use execution::order_for_target_delta;
use portfolio::TargetPosition;
use rust_decimal::Decimal;
use trader_core::OrderSide;

#[test]
fn creates_buy_order_for_positive_delta() {
    let target = target(Decimal::from(10));
    let order = order_for_target_delta(&target, Decimal::from(4), "paper").unwrap();

    assert_eq!(order.side, OrderSide::Buy);
    assert_eq!(order.qty, Decimal::from(6));
}

#[test]
fn creates_sell_order_for_negative_delta() {
    let target = target(Decimal::from(3));
    let order = order_for_target_delta(&target, Decimal::from(10), "paper").unwrap();

    assert_eq!(order.side, OrderSide::Sell);
    assert_eq!(order.qty, Decimal::from(7));
}

#[test]
fn returns_none_when_target_already_met() {
    let target = target(Decimal::from(10));

    assert!(order_for_target_delta(&target, Decimal::from(10), "paper").is_none());
}

fn target(target_qty: Decimal) -> TargetPosition {
    TargetPosition {
        symbol: "US:NASDAQ:AAPL:EQUITY".to_string(),
        target_qty,
    }
}
```

- [ ] **Step 2: Run tests and verify RED**

Run:

```powershell
cargo test -p execution
```

Expected: FAIL because `order_for_target_delta` does not exist.

- [ ] **Step 3: Implement execution delta**

In `crates/execution/src/execution.rs`, add:

```rust
pub fn order_for_target_delta(
    target: &TargetPosition,
    current_qty: Decimal,
    account_id: impl Into<String>,
) -> Option<OrderRequest> {
    let delta = target.target_qty - current_qty;
    if delta == Decimal::ZERO {
        return None;
    }
    let side = if delta > Decimal::ZERO {
        OrderSide::Buy
    } else {
        OrderSide::Sell
    };
    Some(OrderRequest {
        symbol: target.symbol.clone(),
        side,
        order_type: OrderType::Market,
        qty: delta.abs(),
        price: None,
        account_id: account_id.into(),
    })
}
```

Keep `immediate_order` for compatibility.

- [ ] **Step 4: Verify and commit**

Run:

```powershell
cargo fmt --all -- --check
cargo test -p execution
cargo check --workspace --locked
```

Commit:

```powershell
git add crates/execution docs/superpowers/plans/2026-06-02-trader-mvp-core-rules-plan.md
git commit -m "feat: create orders from target deltas"
```

---

### Task 6: Paper Runtime Core Rule Integration

**Files:**
- Modify: `crates/paper/src/paper.rs`
- Modify: `crates/paper/tests/paper_tests.rs`
- Modify: `crates/paper/tests/persistent_paper_tests.rs`

- [ ] **Step 1: Add failing paper test for max order risk**

In `crates/paper/tests/paper_tests.rs`, add:

```rust
#[tokio::test]
async fn paper_runtime_rejects_order_above_max_order_qty() {
    let db = storage::Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    let mut settings = paper::PaperSettings::sample();
    settings.order_qty = rust_decimal::Decimal::from(2);
    settings.max_order_qty = rust_decimal::Decimal::ONE;

    let result = paper::PaperRuntime::new(db, settings)
        .run_bars(vec![
            data::Bar {
                ts_ms: 1,
                open: rust_decimal::Decimal::from(100),
                high: rust_decimal::Decimal::from(100),
                low: rust_decimal::Decimal::from(100),
                close: rust_decimal::Decimal::from(100),
                volume: rust_decimal::Decimal::from(10),
            },
            data::Bar {
                ts_ms: 2,
                open: rust_decimal::Decimal::from(101),
                high: rust_decimal::Decimal::from(101),
                low: rust_decimal::Decimal::from(101),
                close: rust_decimal::Decimal::from(101),
                volume: rust_decimal::Decimal::from(10),
            },
            data::Bar {
                ts_ms: 3,
                open: rust_decimal::Decimal::from(102),
                high: rust_decimal::Decimal::from(102),
                low: rust_decimal::Decimal::from(102),
                close: rust_decimal::Decimal::from(102),
                volume: rust_decimal::Decimal::from(10),
            },
        ])
        .await;

    assert!(result.unwrap_err().to_string().contains("max order quantity"));
}
```

- [ ] **Step 2: Run paper test and verify RED**

Run:

```powershell
cargo test -p paper paper_runtime_rejects_order_above_max_order_qty
```

Expected: FAIL because `PaperSettings.max_order_qty` does not exist and paper does not use order risk policy.

- [ ] **Step 3: Extend PaperSettings**

In `crates/paper/src/paper.rs`, add:

```rust
pub max_order_qty: Decimal,
pub max_order_notional: Decimal,
pub min_cash_after_order: Decimal,
```

In `PaperSettings::sample()`, set:

```rust
max_order_qty: Decimal::from(100),
max_order_notional: Decimal::from(1_000_000),
min_cash_after_order: Decimal::ZERO,
```

Update `apps/trader-cli/src/main.rs` and `crates/api/src/api.rs` `paper_settings()`:

```rust
max_order_qty: Decimal::from_str(&app_config.portfolio.max_abs_qty)?,
max_order_notional: Decimal::from(1_000_000),
min_cash_after_order: Decimal::ZERO,
```

- [ ] **Step 4: Integrate execution delta, market rules, risk, and OMS**

In `crates/paper/src/paper.rs`:

- Replace `use execution::immediate_order;` with `use execution::order_for_target_delta;`.
- Add `use market_rules::MarketRuleSet;`.
- Add `use oms::OrderStateMachine;`.
- Add `use risk::{RiskPolicy, check_max_position};`.

Inside the loop, after `let target = equal_weight_target(...)`, compute current qty:

```rust
let current_qty = account_book
    .position(&self.settings.symbol)
    .map_or(Decimal::ZERO, |position| position.qty);
let Some(order) =
    order_for_target_delta(&target, current_qty, self.settings.account_id.clone())
else {
    continue;
};
```

Then validate:

```rust
let market_rules = MarketRuleSet::us_equity();
market_rules.validate_order(&order, bar.close)?;
let risk_policy = RiskPolicy::new(
    self.settings.max_order_qty,
    self.settings.max_order_notional,
    self.settings.min_cash_after_order,
);
risk_policy.check_order(&order, bar.close, account_book.cash(), false)?;
let mut order_state = OrderStateMachine::with_order_qty(order.qty);
order_state.submit()?;
order_state.accept()?;
```

After simulated fill:

```rust
order_state.record_fill(fill.qty)?;
```

Persist order status from `order_state.status()` instead of hard-coded `FILLED`.

- [ ] **Step 5: Verify full paper path**

Run:

```powershell
cargo fmt --all -- --check
cargo test -p paper
cargo test -p api
cargo check --workspace --locked
```

Expected: PASS.

- [ ] **Step 6: Commit**

Commit:

```powershell
git add apps/trader-cli/src/main.rs crates/api/src/api.rs crates/paper docs/superpowers/plans/2026-06-02-trader-mvp-core-rules-plan.md
git commit -m "feat: enforce core rules in paper runtime"
```

---

### Task 7: Final Verification and Documentation

**Files:**
- Modify: `tech.md`
- Modify: `README.md`
- Modify: `docs/superpowers/plans/2026-06-02-trader-mvp-core-rules-plan.md`

- [ ] **Step 1: Update docs**

In `tech.md`, add an `MVP Core Rules` section:

```markdown
## MVP Core Rules

当前 MVP 订单链路按 `Strategy -> Portfolio -> Execution delta -> MarketRules -> Risk -> OMS -> Broker -> Accounting -> Storage` 执行。MarketRules 校验 lot size、tick size、min qty、min notional；Risk 校验 max order qty、max order notional、cash buffer 和 trading halt；OMS 跟踪订单状态、累计成交和剩余数量。
```

In `README.md`, add under Paper MVP:

```markdown
Paper runtime now enforces MVP core order rules before simulated broker fills: market rules, order-level risk, execution delta, and OMS lifecycle.
```

- [ ] **Step 2: Final verification**

Run:

```powershell
cargo fmt --all -- --check
cargo check --workspace --locked
cargo test --workspace
cargo run -p trader-cli -- paper-run --config configs/backtest/ma_cross.toml
powershell -ExecutionPolicy Bypass -File .\scripts\server-smoke.ps1
Get-ChildItem crates -Directory | ForEach-Object { Join-Path $_.FullName 'src\lib.rs' } | Where-Object { Test-Path $_ }
rg "= \{ path =" apps crates -g Cargo.toml
```

Expected:

- fmt/check/test pass.
- CLI output includes `paper completed: signals=1 orders=1`.
- server smoke returns `status = completed`, `fills = 1`, and non-empty snapshots.
- naming check prints no files.
- direct member dependency check prints no matches.

- [ ] **Step 3: Commit**

Commit:

```powershell
git add README.md tech.md docs/superpowers/plans/2026-06-02-trader-mvp-core-rules-plan.md
git commit -m "docs: document mvp core rules"
```

---

## Acceptance Criteria

This plan is complete when:

- Market rules reject invalid lot size, tick size, min quantity, min notional, disabled market orders, and invalid reference price.
- Risk rejects oversized order quantity, oversized notional, insufficient cash, and halted trading.
- OMS tracks order quantity, filled quantity, remaining quantity, partial fill, full fill, cancel, and overfill rejection.
- Execution can generate no order, buy order, or sell order from target/current position delta.
- Paper runtime uses execution delta, market rules, risk policy, OMS lifecycle, simulated broker fill, accounting, and storage in order.
- Existing CLI paper run still prints `paper completed: signals=1 orders=1`.
- Existing REST server smoke still passes.
- `cargo fmt --all -- --check`, `cargo check --workspace --locked`, and `cargo test --workspace` pass.
- Crate root naming convention remains satisfied.
- Member crates do not use direct internal `{ path = ... }` dependencies.

## Self-Review

Spec coverage:

- Core business rules first: Tasks 2 through 6.
- Existing code retained: all tasks evolve current crates, no rollback of runtime manager.
- Skeleton gap handling: Task 1 aligns stale rules; core crates receive real behavior before more API work.
- Paper architecture path: Task 6 wires the order path to match architecture.md.

Placeholder scan:

- No `TBD`, `TODO`, “implement later”, or unbounded “add tests” steps remain.
- Each code task includes concrete test code, expected red failure, implementation shape, verification, and commit command.

Type consistency:

- `MarketRuleSet`, `RiskPolicy`, `OrderStateMachine::with_order_qty`, and `order_for_target_delta` are introduced before paper runtime uses them.
- `PaperSettings.max_order_qty`, `max_order_notional`, and `min_cash_after_order` are introduced before API/CLI constructors set them.
