# Quantd Paper Boundary Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make `quantd`'s paper trading boundary explicit and testable by enforcing runtime-mode rules for manual orders, documenting the two order paths accurately, normalizing cycle error reasons, and moving operator truth back into `docs/runbook.md`.

**Architecture:** Keep the existing `quantd -> api -> pipeline -> exec -> db` runtime intact. Implement the missing behavior at the API and strategy/pipeline boundaries instead of refactoring the execution core: `api` owns runtime-mode checks for manual orders, `strategy::lstm` owns model error normalization, and `pipeline` persists stable reason codes into cycle history. Documentation remains rooted in `docs/runbook.md` per repository rules.

**Tech Stack:** Rust workspace (`axum`, `tokio`, `serde`, `sqlx`-backed `db` crate), Python model service already present but not renamed in this plan, Markdown docs.

---

## File Structure

### Existing files to modify

- `E:\code\trader\crates\api\src\handlers.rs`
  - Add runtime-mode gating for manual `submit` and `amend`, while always allowing `cancel`.
- `E:\code\trader\crates\api\src\error.rs`
  - Add a stable API error constructor/code for runtime-mode rejection.
- `E:\code\trader\crates\api\tests\terminal_trading_smoke.rs`
  - Extend integration coverage for manual order mode restrictions.
- `E:\code\trader\crates\strategy\src\lstm.rs`
  - Normalize model-service failures to stable reason codes instead of free-form strings.
- `E:\code\trader\crates\pipeline\src\pipeline.rs`
  - Persist normalized model failure reason codes into `skipped.reason`; do not parse raw HTTP text here.
- `E:\code\trader\crates\pipeline\tests\universe_cycle_tests.rs`
  - Add coverage for normalized cycle skip reasons and current two-stage strategy behavior.
- `E:\code\trader\docs\runbook.md`
  - Make this the operator truth source with separate sections for paper smoke, model service, and LSTM cycle paper.
- `E:\code\trader\README.md`
  - Reduce to overview + doc navigation so it no longer acts as an alternate runbook.

### Existing files to inspect while implementing

- `E:\code\trader\crates\api\tests\http_smoke.rs`
- `E:\code\trader\crates\api\tests\runtime_cycle_smoke.rs`
- `E:\code\trader\crates\pipeline\src\execution_guard.rs`
- `E:\code\trader\docs\execution\2026-04-13-semi-auto-paper-rehearsal-runbook.md`
- `E:\code\trader\rules.md`

### No new production files in this plan

- This plan deliberately avoids renaming `services/lstm-service` or changing model artifact layout.
- Those belong in a second plan for `services/model`.

---

### Task 1: Enforce Runtime Mode for Manual Submit/Amend

**Files:**
- Modify: `E:\code\trader\crates\api\src\handlers.rs`
- Modify: `E:\code\trader\crates\api\src\error.rs`
- Test: `E:\code\trader\crates\api\tests\terminal_trading_smoke.rs`

- [ ] **Step 1: Write the failing tests for runtime-mode rejection and cancel passthrough**

Add tests to `crates/api/tests/terminal_trading_smoke.rs` that lock in the intended behavior:

```rust
#[tokio::test]
async fn manual_submit_rejected_in_observe_only() {
    let app = spawn_test_app_with_mode("observe_only").await;

    let response = app
        .post("/v1/orders")
        .json(&serde_json::json!({
            "account_id": "acc_mvp_paper",
            "symbol": "AAPL.US",
            "side": "buy",
            "qty": 1.0,
            "order_type": "limit",
            "limit_price": 123.45
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["error_code"], "runtime_mode_rejected");
}

#[tokio::test]
async fn manual_amend_rejected_in_degraded() {
    let app = spawn_test_app_with_order_and_mode("degraded").await;

    let response = app
        .post("/v1/orders/test-order/amend")
        .json(&serde_json::json!({
            "account_id": "acc_mvp_paper",
            "qty": 2.0,
            "limit_price": 124.0
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["error_code"], "runtime_mode_rejected");
}

#[tokio::test]
async fn manual_cancel_allowed_in_observe_only() {
    let app = spawn_test_app_with_open_order_and_mode("observe_only").await;

    let response = app
        .post("/v1/orders/test-order/cancel")
        .json(&serde_json::json!({
            "account_id": "acc_mvp_paper"
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}
```

- [ ] **Step 2: Run the API integration test file to verify the new cases fail**

Run:

```powershell
cargo test -p api --test terminal_trading_smoke -- --nocapture
```

Expected: FAIL because `post_order` / `post_amend_order` do not currently inspect runtime mode, and no `runtime_mode_rejected` error exists yet.

- [ ] **Step 3: Add a stable API error for runtime-mode rejection**

Update `crates/api/src/error.rs` with a dedicated constructor:

```rust
pub fn runtime_mode_rejected(action: &'static str, mode: &str) -> Self {
    Self {
        status: StatusCode::FORBIDDEN,
        code: "runtime_mode_rejected",
        message: format!("{action} is not allowed while runtime mode is {mode}"),
    }
}
```

- [ ] **Step 4: Implement runtime-mode checks in manual order handlers**

Update `crates/api/src/handlers.rs` with a shared helper and use it from `post_order` and `post_amend_order`, but not `post_cancel_order`:

```rust
async fn require_manual_write_mode(
    database: &db::Db,
    action: &'static str,
) -> Result<(), ApiError> {
    let mode = db::get_runtime_control(database.pool(), RUNTIME_MODE_KEY)
        .await
        .map_err(ApiError::internal)?
        .unwrap_or_else(|| "observe_only".to_string());

    match mode.as_str() {
        "paper_only" | "enabled" => Ok(()),
        "observe_only" | "degraded" => Err(ApiError::runtime_mode_rejected(action, &mode)),
        _ => Err(ApiError::runtime_mode_rejected(action, &mode)),
    }
}
```

Call it like:

```rust
require_manual_write_mode(&state.database, "submit").await?;
```

and:

```rust
require_manual_write_mode(&state.database, "amend").await?;
```

- [ ] **Step 5: Re-run the API integration test file**

Run:

```powershell
cargo test -p api --test terminal_trading_smoke -- --nocapture
```

Expected: PASS for the new mode-gating tests and all pre-existing terminal trading smoke tests.

- [ ] **Step 6: Commit**

```bash
git add crates/api/src/handlers.rs crates/api/src/error.rs crates/api/tests/terminal_trading_smoke.rs
git commit -m "feat: enforce runtime mode for manual submit and amend"
```

---

### Task 2: Normalize LSTM/Model Failure Reasons at the Strategy Boundary

**Files:**
- Modify: `E:\code\trader\crates\strategy\src\lstm.rs`
- Test: `E:\code\trader\crates\strategy\src\lstm.rs`

- [ ] **Step 1: Write the failing unit tests for stable model error codes**

Add focused tests to `crates/strategy/src/lstm.rs` covering normalization:

```rust
#[tokio::test]
async fn structured_model_not_found_maps_to_reason_code() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/predict"))
        .respond_with(ResponseTemplate::new(503).set_body_json(serde_json::json!({
            "detail": {
                "error_code": "model_not_found",
                "message": "please train first"
            }
        })))
        .mount(&server)
        .await;

    let strategy = make_strategy(&server.uri());
    let error = strategy.evaluate_candidate(&make_context()).await.unwrap_err();
    assert_eq!(error, "model_not_found");
}

#[tokio::test]
async fn transport_failure_maps_to_model_unreachable() {
    let strategy = make_strategy("http://127.0.0.1:19999");
    let error = strategy.evaluate_candidate(&make_context()).await.unwrap_err();
    assert_eq!(error, "model_unreachable");
}
```

- [ ] **Step 2: Run the strategy unit tests to confirm they fail**

Run:

```powershell
cargo test -p strategy lstm -- --nocapture
```

Expected: FAIL because the implementation still returns free-form strings like `service unreachable: ...` and `503 error_code=model_not_found ...`.

- [ ] **Step 3: Introduce a small normalization helper in `lstm.rs`**

Add a helper near the request/response types:

```rust
fn normalize_model_error_code(error_code: Option<&str>, fallback: &str) -> String {
    match error_code {
        Some("model_not_found") => "model_not_found".to_string(),
        Some("insufficient_bars") => "insufficient_bars".to_string(),
        Some("response_parse_failed") => "response_parse_failed".to_string(),
        Some("model_service_error") => "model_service_error".to_string(),
        Some(other) if !other.is_empty() => other.to_string(),
        _ => fallback.to_string(),
    }
}
```

- [ ] **Step 4: Make `request_prediction` return stable reason codes**

Update the transport / HTTP error branches in `request_prediction`:

```rust
.send()
.await
.map_err(|_| "model_unreachable".to_string())?;
```

and:

```rust
if let Ok(parsed) = serde_json::from_str::<ServiceError>(&body) {
    return Err(normalize_model_error_code(
        Some(parsed.detail.error_code.as_str()),
        "model_service_error",
    ));
}
return Err(normalize_model_error_code(None, "model_service_error"));
```

and:

```rust
.json()
.await
.map_err(|_| "response_parse_failed".to_string())?;
```

- [ ] **Step 5: Re-run the strategy unit tests**

Run:

```powershell
cargo test -p strategy lstm -- --nocapture
```

Expected: PASS for the new normalization tests and the existing `lstm.rs` tests.

- [ ] **Step 6: Commit**

```bash
git add crates/strategy/src/lstm.rs
git commit -m "feat: normalize model strategy failure codes"
```

---

### Task 3: Persist Stable Cycle Skip Reasons in Pipeline History

**Files:**
- Modify: `E:\code\trader\crates\pipeline\src\pipeline.rs`
- Test: `E:\code\trader\crates\pipeline\tests\universe_cycle_tests.rs`

- [ ] **Step 1: Write the failing pipeline test for normalized skip reasons**

Add a test to `crates/pipeline/tests/universe_cycle_tests.rs` using a strategy stub that returns normalized model codes:

```rust
#[tokio::test]
async fn universe_cycle_persists_model_reason_code_in_skipped() {
    let database = test_db().await;
    seed_allowlist(&database, &["AAPL.US"]).await;

    let strategy = ErrorCandidateStrategy::new("model_not_found");
    let report = run_cycle_with_strategy(&database, &strategy, "paper_only").await;

    assert!(report.skipped.iter().any(|item| {
        item.symbol == "AAPL.US" && item.reason == "model_not_found"
    }));

    let history = pipeline::load_universe_cycle_history(&database, 10)
        .await
        .unwrap();
    assert!(history[0].skipped.iter().any(|item| {
        item.symbol == "AAPL.US" && item.reason == "model_not_found"
    }));
}
```

- [ ] **Step 2: Run the pipeline test file to verify the new assertion fails**

Run:

```powershell
cargo test -p pipeline --test universe_cycle_tests -- --nocapture
```

Expected: FAIL because the test helper strategy or current pipeline path still persists prefixed free-form strings.

- [ ] **Step 3: Keep pipeline dumb; only propagate normalized codes**

Adjust `crates/pipeline/src/pipeline.rs` so it does not invent `strategy_error:` / `execution_error:` wrappers for model-service failures that already crossed the strategy boundary. Use raw normalized codes in `skipped.reason` for the candidate and signal phases:

```rust
Err(error) => skipped.push(SymbolDecision {
    symbol,
    reason: error,
}),
```

and:

```rust
Err(error) => skipped.push(SymbolDecision {
    symbol: symbol.clone(),
    reason: error,
}),
```

Only keep explicit pipeline-generated reasons for internal cases such as `no_candidate`, `no_signal_on_execution`, or execution guard decisions.

- [ ] **Step 4: Update/add the strategy stub in the pipeline test**

Make the test strategy deterministic and explicit about two-phase behavior:

```rust
struct ErrorCandidateStrategy {
    code: &'static str,
}

#[async_trait]
impl Strategy for ErrorCandidateStrategy {
    async fn evaluate(&self, _context: &StrategyContext) -> Option<Signal> {
        None
    }

    async fn evaluate_candidate(
        &self,
        _context: &StrategyContext,
    ) -> Result<Option<ScoredCandidate>, String> {
        Err(self.code.to_string())
    }
}
```

- [ ] **Step 5: Re-run the pipeline test file**

Run:

```powershell
cargo test -p pipeline --test universe_cycle_tests -- --nocapture
```

Expected: PASS, with cycle history and in-memory report both showing stable reason codes such as `model_not_found`.

- [ ] **Step 6: Commit**

```bash
git add crates/pipeline/src/pipeline.rs crates/pipeline/tests/universe_cycle_tests.rs
git commit -m "feat: persist normalized model skip reasons in cycle history"
```

---

### Task 4: Lock In the Two-Stage Cycle Behavior in Tests and Docs

**Files:**
- Modify: `E:\code\trader\crates\pipeline\tests\universe_cycle_tests.rs`
- Modify: `E:\code\trader\docs\runbook.md`
- Modify: `E:\code\trader\README.md`

- [ ] **Step 1: Add a pipeline test that makes the two-stage call sequence explicit**

Extend `crates/pipeline/tests/universe_cycle_tests.rs` with a counting strategy:

```rust
#[derive(Default)]
struct CountingStrategy {
    candidate_calls: Arc<AtomicUsize>,
    signal_calls: Arc<AtomicUsize>,
}

#[async_trait]
impl Strategy for CountingStrategy {
    async fn evaluate(&self, context: &StrategyContext) -> Option<Signal> {
        self.signal_calls.fetch_add(1, Ordering::SeqCst);
        Some(Signal {
            strategy_id: "counting".to_string(),
            instrument: context.instrument.clone(),
            instrument_db_id: context.instrument_db_id,
            side: Side::Buy,
            qty: 1.0,
            limit_price: context.last_bar_close?,
            ts_ms: context.ts_ms,
        })
    }

    async fn evaluate_candidate(
        &self,
        context: &StrategyContext,
    ) -> Result<Option<ScoredCandidate>, String> {
        self.candidate_calls.fetch_add(1, Ordering::SeqCst);
        Ok(Some(ScoredCandidate {
            symbol: context.instrument.symbol.clone(),
            score: 0.9,
            confidence: 0.9,
        }))
    }
}
```

Assert:

```rust
assert_eq!(candidate_calls.load(Ordering::SeqCst), 1);
assert_eq!(signal_calls.load(Ordering::SeqCst), 1);
```

- [ ] **Step 2: Run the pipeline test file**

Run:

```powershell
cargo test -p pipeline --test universe_cycle_tests -- --nocapture
```

Expected: PASS after Task 3; this step locks in the current behavior so future refactors do not hide it.

- [ ] **Step 3: Rewrite `docs/runbook.md` as the only operator truth source**

Replace the mixed narrative with three explicit sections:

```md
## Paper Smoke

Purpose: verify manual paper submit/amend/cancel, overview, execution-state, and WS events without any model dependency.

## Model Workflow / Service

Purpose: verify qlib-based training/export and serving health independently of quantd.

## LSTM Cycle Paper

Purpose: verify allowlist -> model-backed candidate scoring -> accepted -> signal evaluation -> execution_guard -> paper order.
```

Preserve the concrete commands already proven useful, but move the authoritative operator flow into this file rather than spreading it across multiple runbooks.

- [ ] **Step 4: Reduce `README.md` to overview and navigation**

Update the runbook section in `README.md` to point readers to `docs/runbook.md` instead of duplicating operator steps:

```md
## Operator Runbook

See `docs/runbook.md` for:

- paper smoke validation
- model workflow/service validation
- LSTM cycle paper validation
```

- [ ] **Step 5: Review the docs locally**

Run:

```powershell
Get-Content README.md
Get-Content docs\runbook.md
```

Expected: `README.md` is now overview-oriented, and `docs/runbook.md` clearly contains the three source-of-truth sections named above.

- [ ] **Step 6: Commit**

```bash
git add crates/pipeline/tests/universe_cycle_tests.rs docs/runbook.md README.md
git commit -m "docs: consolidate operator truth into runbook"
```

---

### Task 5: Verify the Whole Quantd-Side Boundary Change Set

**Files:**
- Verify only; no new files

- [ ] **Step 1: Run targeted Rust test suites**

Run:

```powershell
cargo test -p api --test terminal_trading_smoke -- --nocapture
cargo test -p pipeline --test universe_cycle_tests -- --nocapture
cargo test -p strategy lstm -- --nocapture
```

Expected:

- `terminal_trading_smoke` passes with mode-gating coverage
- `universe_cycle_tests` passes with normalized skip reasons and two-stage behavior coverage
- `strategy lstm` tests pass with stable model error codes

- [ ] **Step 2: Run the repository-required verification command**

Run:

```powershell
cargo check -p api -p pipeline -p strategy -p quantd
```

Expected: PASS with no compile errors.

- [ ] **Step 3: Sanity-check the main operator flows against the updated runbook**

Manually confirm the plan's delivered behavior matches the documentation:

```text
1. Manual submit/amend are rejected in observe_only/degraded.
2. Manual cancel remains allowed in observe_only/degraded.
3. Runtime cycle skipped reasons are stable codes, not free-form transport strings.
4. docs/runbook.md is the only operator truth source.
```

- [ ] **Step 4: Commit the final verification pass if needed**

If verification required follow-up doc/test edits:

```bash
git add crates/api crates/pipeline crates/strategy docs/runbook.md README.md
git commit -m "test: finalize quantd paper boundary verification"
```

If no changes were needed, skip this commit.

---

## Self-Review

### Spec coverage

- Runtime-mode rules for manual orders: covered by Task 1.
- Two-stage half-auto chain clarity: covered by Task 4 and reinforced by Task 3.
- Stable model error normalization: covered by Tasks 2 and 3.
- `docs/runbook.md` as truth source per `rules.md`: covered by Task 4.
- This plan intentionally excludes `services/model` rename, qlib workflow/export, and artifact compatibility loader changes; those require a second plan.

### Placeholder scan

- No `TBD`, `TODO`, or “implement later” placeholders remain.
- Each code-changing step includes concrete code or exact snippets.
- Each verification step has exact commands and expected outcomes.

### Type consistency

- Stable API error code: `runtime_mode_rejected`
- Stable model reason examples: `model_unreachable`, `model_not_found`, `insufficient_bars`, `response_parse_failed`, `model_service_error`
- Manual runtime-mode gating actions are consistently named `submit`, `amend`, `cancel`

