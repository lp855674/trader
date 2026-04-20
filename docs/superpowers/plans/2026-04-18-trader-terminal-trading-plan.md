# Trader Terminal Trading Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a unified `trader` terminal product with CLI and TUI flows that can read market/runtime state from `quantd` and complete the order submit, cancel, and amend loop.

**Architecture:** Extend the backend first so `quantd` exposes explicit order-management and terminal-overview APIs backed by persistent order metadata. Then add a terminal client layer shared by a new `trader` binary and a TUI crate so both CLI and TUI consume the same HTTP and WebSocket contracts.

**Tech Stack:** Rust workspace, axum, sqlx/sqlite, tokio, reqwest, tokio-tungstenite, clap, ratatui, crossterm, serde, tracing

---

## File Structure

### Existing files to modify

- `Cargo.toml`
  - Add new workspace members and terminal dependencies.
- `README.md`
  - Document `trader` CLI/TUI usage and new API endpoints.
- `tech.md`
  - Update architecture and product surface for terminal trading.
- `crates/db/src/db.rs`
  - Re-export new order mutation and quote query helpers.
- `crates/db/src/orders.rs`
  - Add richer order rows and mutation helpers for cancel/amend.
- `crates/db/migrations/001_initial.sql`
  - Read-only reference; do not edit directly, but use it to verify new migration shape.
- `crates/exec/src/adapter.rs`
  - Extend the execution trait with manual order submit/cancel/amend operations.
- `crates/exec/src/router.rs`
  - Route manual order-management operations to the account adapter.
- `crates/exec/src/paper.rs`
  - Implement terminal-facing paper order lifecycle without bypassing the execution layer.
- `crates/exec/src/error.rs`
  - Add order-management specific error variants and stable error codes.
- `crates/longbridge_adapters/src/exec_lb.rs`
  - Map cancel/amend requests to Longbridge APIs for live accounts.
- `crates/api/src/api.rs`
  - Register new terminal routes.
- `crates/api/src/handlers.rs`
  - Add request/response bodies and handlers for orders, quotes, and terminal overview.
- `crates/api/src/ws.rs`
  - Emit new order lifecycle stream events.
- `crates/api/src/error.rs`
  - Map new execution errors to HTTP status + `error_code`.
- `crates/api/tests/runtime_cycle_smoke.rs`
  - Extend the current smoke harness for terminal routes.

### New files to create

- `crates/db/migrations/004_terminal_order_fields.sql`
  - Add order fields needed for manual order management and terminal views.
- `crates/db/tests/terminal_orders.rs`
  - DB-level tests for order persistence, mutation, and query projections.
- `crates/exec/tests/order_management_router_tests.rs`
  - Execution-layer tests for submit/cancel/amend across paper and live stubs.
- `crates/api/tests/terminal_trading_smoke.rs`
  - API smoke coverage for terminal order routes, overview, and quote views.
- `crates/terminal_core/Cargo.toml`
  - Shared terminal-facing domain and formatting helpers.
- `crates/terminal_core/src/terminal_core.rs`
  - Library root.
- `crates/terminal_core/src/models.rs`
  - Shared terminal view models and command result structs.
- `crates/terminal_core/src/errors.rs`
  - User-facing terminal error mapping.
- `crates/terminal_client/Cargo.toml`
  - HTTP/WS client crate.
- `crates/terminal_client/src/terminal_client.rs`
  - Library root.
- `crates/terminal_client/src/http.rs`
  - `quantd` HTTP client.
- `crates/terminal_client/src/stream.rs`
  - `quantd` WebSocket event client.
- `crates/terminal_client/tests/http_client_tests.rs`
  - Contract tests for response decoding and API error mapping.
- `crates/terminal_tui/Cargo.toml`
  - TUI crate dependencies.
- `crates/terminal_tui/src/terminal_tui.rs`
  - Library root.
- `crates/terminal_tui/src/app.rs`
  - App state and reducer.
- `crates/terminal_tui/src/actions.rs`
  - Submit/cancel/amend flows and confirmation rules.
- `crates/terminal_tui/src/panels.rs`
  - Multi-panel rendering helpers.
- `crates/terminal_tui/src/forms.rs`
  - Order-entry and amend forms.
- `crates/terminal_tui/tests/app_state_tests.rs`
  - Focus, confirmation, and degraded-connectivity tests.
- `crates/trader/Cargo.toml`
  - Unified binary crate.
- `crates/trader/src/main.rs`
  - CLI entrypoint.
- `crates/trader/src/cli.rs`
  - Clap subcommand definitions.
- `crates/trader/src/output.rs`
  - Table/JSON renderers.
- `crates/trader/tests/cli_smoke.rs`
  - CLI parsing and output smoke tests.

## Task 1: Add Persistent Order Fields And DB Mutation Helpers

**Files:**
- Create: `crates/db/migrations/004_terminal_order_fields.sql`
- Create: `crates/db/tests/terminal_orders.rs`
- Modify: `crates/db/src/orders.rs`
- Modify: `crates/db/src/db.rs`
- Test: `crates/db/tests/terminal_orders.rs`

- [ ] **Step 1: Write the failing DB tests for rich order rows and mutations**

```rust
#[tokio::test]
async fn order_rows_expose_limit_price_and_allow_cancel_and_amend() {
    let database = db::Db::connect("sqlite::memory:").await.expect("db");
    db::ensure_mvp_seed(database.pool()).await.expect("seed");
    let instrument_id =
        db::upsert_instrument(database.pool(), "US_EQUITY", "AAPL.US").await.expect("instrument");

    db::insert_order(
        database.pool(),
        &db::NewOrder {
            order_id: "ord-1",
            account_id: "acc_mvp_paper",
            instrument_id,
            side: "buy",
            qty: 10.0,
            status: "SUBMITTED",
            order_type: "limit",
            limit_price: Some(123.45),
            exchange_ref: Some("paper-ord-1"),
            idempotency_key: Some("client-1"),
            created_at_ms: 100,
            updated_at_ms: 100,
        },
    )
    .await
    .expect("insert order");

    db::amend_order(database.pool(), "ord-1", 12.0, Some(124.0), 120)
        .await
        .expect("amend");
    db::cancel_order(database.pool(), "ord-1", 130)
        .await
        .expect("cancel");

    let rows = db::list_orders_for_account(database.pool(), "acc_mvp_paper")
        .await
        .expect("rows");
    assert_eq!(rows[0].limit_price, Some(124.0));
    assert_eq!(rows[0].qty, 12.0);
    assert_eq!(rows[0].status, "CANCELLED");
}
```

- [ ] **Step 2: Run the DB test to verify it fails**

Run: `cargo test -p db order_rows_expose_limit_price_and_allow_cancel_and_amend -- --exact`

Expected: FAIL with missing `order_type` / `limit_price` fields on `db::NewOrder` and missing `amend_order` / `cancel_order` helpers.

- [ ] **Step 3: Add the migration and DB helpers**

```sql
ALTER TABLE orders ADD COLUMN order_type TEXT NOT NULL DEFAULT 'limit';
ALTER TABLE orders ADD COLUMN limit_price REAL;
ALTER TABLE orders ADD COLUMN exchange_ref TEXT;
ALTER TABLE orders ADD COLUMN updated_at_ms INTEGER NOT NULL DEFAULT 0;
```

```rust
pub struct NewOrder<'a> {
    pub order_id: &'a str,
    pub account_id: &'a str,
    pub instrument_id: i64,
    pub side: &'a str,
    pub qty: f64,
    pub status: &'a str,
    pub order_type: &'a str,
    pub limit_price: Option<f64>,
    pub exchange_ref: Option<&'a str>,
    pub idempotency_key: Option<&'a str>,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

pub async fn amend_order(
    pool: &SqlitePool,
    order_id: &str,
    qty: f64,
    limit_price: Option<f64>,
    updated_at_ms: i64,
) -> Result<(), DbError> {
    sqlx::query(
        "UPDATE orders
         SET qty = ?, limit_price = ?, updated_at_ms = ?
         WHERE id = ? AND UPPER(status) IN ('PENDING', 'SUBMITTED', 'PARTIALLY_FILLED')",
    )
    .bind(qty)
    .bind(limit_price)
    .bind(updated_at_ms)
    .bind(order_id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn cancel_order(
    pool: &SqlitePool,
    order_id: &str,
    updated_at_ms: i64,
) -> Result<(), DbError> {
    sqlx::query(
        "UPDATE orders
         SET status = 'CANCELLED', updated_at_ms = ?
         WHERE id = ? AND UPPER(status) IN ('PENDING', 'SUBMITTED', 'PARTIALLY_FILLED')",
    )
    .bind(updated_at_ms)
    .bind(order_id)
    .execute(pool)
    .await?;
    Ok(())
}
```

- [ ] **Step 4: Run the DB tests to verify they pass**

Run: `cargo test -p db terminal_orders -- --nocapture`

Expected: PASS for the new rich-order persistence and mutation tests.

- [ ] **Step 5: Commit**

```bash
git add crates/db/migrations/004_terminal_order_fields.sql crates/db/src/orders.rs crates/db/src/db.rs crates/db/tests/terminal_orders.rs
git commit -m "feat: persist terminal order fields"
```

## Task 2: Extend The Execution Layer For Manual Submit, Cancel, And Amend

**Files:**
- Modify: `crates/exec/src/adapter.rs`
- Modify: `crates/exec/src/router.rs`
- Modify: `crates/exec/src/error.rs`
- Modify: `crates/exec/src/paper.rs`
- Modify: `crates/longbridge_adapters/src/exec_lb.rs`
- Create: `crates/exec/tests/order_management_router_tests.rs`
- Test: `crates/exec/tests/order_management_router_tests.rs`

- [ ] **Step 1: Write the failing execution tests**

```rust
#[tokio::test]
async fn router_supports_manual_submit_cancel_and_amend_for_paper_accounts() {
    let database = db::Db::connect("sqlite::memory:").await.expect("db");
    db::ensure_mvp_seed(database.pool()).await.expect("seed");
    let paper = Arc::new(exec::PaperAdapter::new(database.clone()));
    let mut routes = HashMap::new();
    routes.insert("acc_mvp_paper".to_string(), paper as Arc<dyn exec::ExecutionAdapter>);
    let router = exec::ExecutionRouter::new(routes);

    let intent = domain::OrderIntent {
        strategy_id: "manual_terminal".to_string(),
        instrument: domain::InstrumentId::new(domain::Venue::UsEquity, "AAPL.US"),
        instrument_db_id: db::upsert_instrument(database.pool(), "US_EQUITY", "AAPL.US")
            .await
            .expect("instrument"),
        side: domain::Side::Buy,
        qty: 10.0,
        limit_price: 123.45,
    };

    let submit = router
        .submit_manual_order("acc_mvp_paper", &intent, Some("client-1"))
        .await
        .expect("submit");
    router.cancel_order("acc_mvp_paper", &submit.order_id).await.expect("cancel");
    let amended = router
        .submit_manual_order("acc_mvp_paper", &intent, Some("client-2"))
        .await
        .expect("submit-2");
    router
        .amend_order("acc_mvp_paper", &amended.order_id, 12.0, Some(124.0))
        .await
        .expect("amend");
}
```

- [ ] **Step 2: Run the execution test to verify it fails**

Run: `cargo test -p exec router_supports_manual_submit_cancel_and_amend_for_paper_accounts -- --exact`

Expected: FAIL because `ExecutionAdapter` and `ExecutionRouter` do not yet expose manual order-management methods.

- [ ] **Step 3: Add the execution trait surface and implementations**

```rust
#[derive(Debug, Clone)]
pub struct ManualOrderAck {
    pub order_id: String,
    pub exchange_ref: Option<String>,
    pub status: String,
}

#[async_trait]
pub trait ExecutionAdapter: Send + Sync {
    async fn place_order(
        &self,
        account_id: &str,
        intent: &domain::OrderIntent,
        idempotency_key: Option<&str>,
    ) -> Result<OrderAck, ExecError>;

    async fn submit_manual_order(
        &self,
        account_id: &str,
        intent: &domain::OrderIntent,
        idempotency_key: Option<&str>,
    ) -> Result<ManualOrderAck, ExecError>;

    async fn cancel_order(&self, account_id: &str, order_id: &str) -> Result<(), ExecError>;

    async fn amend_order(
        &self,
        account_id: &str,
        order_id: &str,
        qty: f64,
        limit_price: Option<f64>,
    ) -> Result<ManualOrderAck, ExecError>;
}
```

```rust
#[async_trait]
impl ExecutionAdapter for PaperAdapter {
    async fn submit_manual_order(
        &self,
        account_id: &str,
        intent: &domain::OrderIntent,
        idempotency_key: Option<&str>,
    ) -> Result<ManualOrderAck, ExecError> {
        let order_id = uuid::Uuid::new_v4().to_string();
        let ts = now_ms();
        db::insert_order(
            self.db.pool(),
            &db::NewOrder {
                order_id: &order_id,
                account_id,
                instrument_id: intent.instrument_db_id,
                side: side_str(intent.side),
                qty: intent.qty,
                status: "SUBMITTED",
                order_type: "limit",
                limit_price: Some(intent.limit_price),
                exchange_ref: Some("paper"),
                idempotency_key,
                created_at_ms: ts,
                updated_at_ms: ts,
            },
        )
        .await?;
        Ok(ManualOrderAck {
            order_id,
            exchange_ref: Some("paper".to_string()),
            status: "SUBMITTED".to_string(),
        })
    }
}
```

```rust
impl ExecutionRouter {
    pub async fn cancel_order(&self, account_id: &str, order_id: &str) -> Result<(), ExecError> {
        let adapter = self.resolve(account_id)?;
        adapter.cancel_order(account_id, order_id).await
    }
}
```

- [ ] **Step 4: Run the execution tests to verify they pass**

Run: `cargo test -p exec order_management_router_tests -- --nocapture`

Expected: PASS for paper submit/cancel/amend and route-resolution failures.

- [ ] **Step 5: Commit**

```bash
git add crates/exec/src/adapter.rs crates/exec/src/router.rs crates/exec/src/error.rs crates/exec/src/paper.rs crates/longbridge_adapters/src/exec_lb.rs crates/exec/tests/order_management_router_tests.rs
git commit -m "feat: add execution order management flows"
```

## Task 3: Add Terminal Trading HTTP Routes And Stream Events

**Files:**
- Modify: `crates/api/src/api.rs`
- Modify: `crates/api/src/handlers.rs`
- Modify: `crates/api/src/ws.rs`
- Modify: `crates/api/src/error.rs`
- Create: `crates/api/tests/terminal_trading_smoke.rs`
- Test: `crates/api/tests/terminal_trading_smoke.rs`

- [ ] **Step 1: Write the failing API smoke tests for submit/cancel/amend**

```rust
#[tokio::test]
async fn terminal_order_routes_submit_cancel_and_amend() {
    let (app, _database) = test_app().await;

    let submit = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/orders")
                .header("content-type", "application/json")
                .body(Body::from(
                    r#"{"account_id":"acc_mvp_paper","symbol":"AAPL.US","side":"buy","qty":10.0,"order_type":"limit","limit_price":123.45}"#,
                ))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(submit.status(), StatusCode::CREATED);

    let body = submit.into_body().collect().await.expect("body").to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).expect("json");
    let order_id = json["order_id"].as_str().expect("order id");

    let amend = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/v1/orders/{order_id}/amend"))
                .header("content-type", "application/json")
                .body(Body::from(r#"{"qty":12.0,"limit_price":124.0}"#))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(amend.status(), StatusCode::OK);

    let cancel = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/v1/orders/{order_id}/cancel"))
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(cancel.status(), StatusCode::OK);
}
```

- [ ] **Step 2: Run the API smoke test to verify it fails**

Run: `cargo test -p api terminal_order_routes_submit_cancel_and_amend -- --exact`

Expected: FAIL because `/v1/orders`, `/v1/orders/:id/cancel`, and `/v1/orders/:id/amend` do not exist.

- [ ] **Step 3: Add the route registrations, handlers, and new stream events**

```rust
let v1 = Router::new()
    .route("/orders", get(handlers::list_orders).post(handlers::post_order))
    .route("/orders/:order_id/cancel", post(handlers::post_cancel_order))
    .route("/orders/:order_id/amend", post(handlers::post_amend_order))
    .route("/terminal/overview", get(handlers::get_terminal_overview))
    .route("/quotes/:symbol", get(handlers::get_quote));
```

```rust
#[derive(Deserialize)]
pub struct CreateOrderBody {
    pub account_id: String,
    pub symbol: String,
    pub side: String,
    pub qty: f64,
    pub order_type: String,
    pub limit_price: Option<f64>,
}

#[derive(Serialize)]
pub struct OrderActionResponse {
    pub order_id: String,
    pub status: String,
}
```

```rust
pub enum StreamEvent {
    OrderCreated { order_id: String, venue: domain::Venue, symbol: String },
    OrderUpdated { order_id: String, status: String, qty: f64, limit_price: Option<f64> },
    OrderCancelled { order_id: String },
    OrderReplaced { order_id: String, qty: f64, limit_price: Option<f64> },
    Error { error_code: String, message: String },
}
```

- [ ] **Step 4: Run the API tests to verify they pass**

Run: `cargo test -p api terminal_trading_smoke -- --nocapture`

Expected: PASS for submit/cancel/amend plus stream envelope tests for `order_updated`, `order_cancelled`, and `order_replaced`.

- [ ] **Step 5: Commit**

```bash
git add crates/api/src/api.rs crates/api/src/handlers.rs crates/api/src/ws.rs crates/api/src/error.rs crates/api/tests/terminal_trading_smoke.rs
git commit -m "feat: add terminal trading api routes"
```

## Task 4: Add Terminal Overview And Quote Read APIs

**Files:**
- Modify: `crates/api/src/handlers.rs`
- Modify: `crates/db/src/orders.rs`
- Create: `crates/api/tests/terminal_trading_smoke.rs`
- Test: `crates/api/tests/terminal_trading_smoke.rs`

- [ ] **Step 1: Extend the smoke test with overview and quote assertions**

```rust
#[tokio::test]
async fn terminal_overview_and_quote_routes_return_operator_facing_data() {
    let (app, _database) = test_app().await;

    let overview = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/terminal/overview?account_id=acc_mvp_paper")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(overview.status(), StatusCode::OK);

    let quote = app
        .oneshot(
            Request::builder()
                .uri("/v1/quotes/AAPL.US")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(quote.status(), StatusCode::OK);
}
```

- [ ] **Step 2: Run the overview/quote test to verify it fails**

Run: `cargo test -p api terminal_overview_and_quote_routes_return_operator_facing_data -- --exact`

Expected: FAIL because overview and quote handlers are not implemented.

- [ ] **Step 3: Implement overview and quote bodies**

```rust
#[derive(Serialize)]
pub struct TerminalOverviewBody {
    pub account_id: String,
    pub runtime_mode: String,
    pub watchlist: Vec<TerminalWatchRow>,
    pub positions: Vec<db::LocalPositionViewRow>,
    pub open_orders: Vec<db::OpenOrderViewRow>,
}

#[derive(Serialize)]
pub struct QuoteBody {
    pub symbol: String,
    pub venue: String,
    pub last_price: Option<f64>,
    pub day_high: Option<f64>,
    pub day_low: Option<f64>,
    pub bars: Vec<db::BarRow>,
}
```

```rust
pub async fn get_terminal_overview(
    State(state): State<Arc<AppState>>,
    Query(query): Query<RuntimeExecutionStateQuery>,
) -> Result<Json<TerminalOverviewBody>, ApiError> {
    let runtime_mode = db::get_runtime_control(state.database.pool(), RUNTIME_MODE_KEY)
        .await
        .map_err(ApiError::internal)?
        .unwrap_or_else(|| "observe_only".to_string());
    let positions = db::list_local_positions_for_account(state.database.pool(), &query.account_id)
        .await
        .map_err(ApiError::internal)?;
    let open_orders = db::list_open_orders_for_account(state.database.pool(), &query.account_id)
        .await
        .map_err(ApiError::internal)?;
    Ok(Json(TerminalOverviewBody {
        account_id: query.account_id,
        runtime_mode,
        watchlist: load_watchlist_rows(&state.database).await?,
        positions,
        open_orders,
    }))
}
```

- [ ] **Step 4: Run the API smoke tests to verify they pass**

Run: `cargo test -p api terminal_trading_smoke -- --nocapture`

Expected: PASS for overview and quote responses, including runtime mode and quote bar payloads.

- [ ] **Step 5: Commit**

```bash
git add crates/api/src/handlers.rs crates/db/src/orders.rs crates/api/tests/terminal_trading_smoke.rs
git commit -m "feat: add terminal overview and quote endpoints"
```

## Task 5: Scaffold Shared Terminal Core And Client Crates

**Files:**
- Modify: `Cargo.toml`
- Create: `crates/terminal_core/Cargo.toml`
- Create: `crates/terminal_core/src/terminal_core.rs`
- Create: `crates/terminal_core/src/models.rs`
- Create: `crates/terminal_core/src/errors.rs`
- Create: `crates/terminal_client/Cargo.toml`
- Create: `crates/terminal_client/src/terminal_client.rs`
- Create: `crates/terminal_client/src/http.rs`
- Create: `crates/terminal_client/src/stream.rs`
- Create: `crates/terminal_client/tests/http_client_tests.rs`
- Test: `crates/terminal_client/tests/http_client_tests.rs`

- [ ] **Step 1: Write the failing client contract tests**

```rust
#[tokio::test]
async fn http_client_maps_api_error_codes_into_terminal_errors() {
    let server = mockito::Server::new_async().await;
    let _mock = server
        .mock("POST", "/v1/orders")
        .with_status(403)
        .with_header("content-type", "application/json")
        .with_body(r#"{"error_code":"risk_denied","message":"observe_only"}"#)
        .create_async()
        .await;

    let client = terminal_client::QuantdHttpClient::new(server.url(), None);
    let err = client
        .submit_order(&terminal_core::models::SubmitOrderRequest {
            account_id: "acc_mvp_paper".to_string(),
            symbol: "AAPL.US".to_string(),
            side: "buy".to_string(),
            qty: 10.0,
            order_type: "limit".to_string(),
            limit_price: Some(123.45),
        })
        .await
        .expect_err("error");

    assert_eq!(err.code(), "risk_denied");
}
```

- [ ] **Step 2: Run the terminal client test to verify it fails**

Run: `cargo test -p terminal_client http_client_maps_api_error_codes_into_terminal_errors -- --exact`

Expected: FAIL because the `terminal_client` crate and shared request models do not exist.

- [ ] **Step 3: Add the shared models and HTTP/WS client**

```rust
pub struct SubmitOrderRequest {
    pub account_id: String,
    pub symbol: String,
    pub side: String,
    pub qty: f64,
    pub order_type: String,
    pub limit_price: Option<f64>,
}

pub struct QuantdHttpClient {
    base_url: String,
    http: reqwest::Client,
    api_key: Option<String>,
}

impl QuantdHttpClient {
    pub async fn submit_order(
        &self,
        request: &SubmitOrderRequest,
    ) -> Result<OrderActionResult, terminal_core::errors::TerminalError> {
        let response = self.http.post(format!("{}/v1/orders", self.base_url)).json(request).send().await?;
        decode_json(response).await
    }
}
```

```rust
pub enum StreamMessage {
    Hello,
    OrderCreated { order_id: String, symbol: String },
    OrderUpdated { order_id: String, status: String },
    Error { error_code: String, message: String },
}
```

- [ ] **Step 4: Run the new crate tests to verify they pass**

Run: `cargo test -p terminal_client -- --nocapture`

Expected: PASS for request/response decoding and API error mapping.

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml crates/terminal_core/Cargo.toml crates/terminal_core/src/terminal_core.rs crates/terminal_core/src/models.rs crates/terminal_core/src/errors.rs crates/terminal_client/Cargo.toml crates/terminal_client/src/terminal_client.rs crates/terminal_client/src/http.rs crates/terminal_client/src/stream.rs crates/terminal_client/tests/http_client_tests.rs
git commit -m "feat: add shared terminal client crates"
```

## Task 6: Build The Unified `trader` CLI Binary

**Files:**
- Modify: `Cargo.toml`
- Create: `crates/trader/Cargo.toml`
- Create: `crates/trader/src/main.rs`
- Create: `crates/trader/src/cli.rs`
- Create: `crates/trader/src/output.rs`
- Create: `crates/trader/tests/cli_smoke.rs`
- Test: `crates/trader/tests/cli_smoke.rs`

- [ ] **Step 1: Write the failing CLI parsing and output tests**

```rust
#[test]
fn submit_command_parses_limit_order_arguments() {
    let cli = trader::cli::Cli::parse_from([
        "trader",
        "order",
        "submit",
        "--account-id",
        "acc_mvp_paper",
        "--symbol",
        "AAPL.US",
        "--side",
        "buy",
        "--qty",
        "10",
        "--limit-price",
        "123.45",
    ]);

    match cli.command {
        trader::cli::Command::Order { action } => match action {
            trader::cli::OrderCommand::Submit(body) => assert_eq!(body.symbol, "AAPL.US"),
            other => panic!("unexpected order command: {other:?}"),
        },
        other => panic!("unexpected command: {other:?}"),
    }
}
```

- [ ] **Step 2: Run the CLI test to verify it fails**

Run: `cargo test -p trader submit_command_parses_limit_order_arguments -- --exact`

Expected: FAIL because the `trader` binary crate and clap command tree do not exist.

- [ ] **Step 3: Implement the CLI subcommands and output adapters**

```rust
#[derive(clap::Parser, Debug)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(clap::Subcommand, Debug)]
pub enum Command {
    Tui,
    Quote { symbol: String },
    Orders { #[command(subcommand)] action: OrdersCommand },
    Order { #[command(subcommand)] action: OrderCommand },
}
```

```rust
match cli.command {
    Command::Tui => terminal_tui::run(app_client).await?,
    Command::Quote { symbol } => output::print_quote(client.get_quote(&symbol).await?, json)?,
    Command::Order { action: OrderCommand::Submit(body) } => {
        output::print_order_action(client.submit_order(&body).await?, json)?
    }
    Command::Order { action: OrderCommand::Cancel { order_id, account_id } } => {
        client.cancel_order(&account_id, &order_id).await?;
    }
    Command::Order { action: OrderCommand::Amend(body) } => {
        output::print_order_action(client.amend_order(&body).await?, json)?
    }
    _ => {}
}
```

- [ ] **Step 4: Run the CLI tests to verify they pass**

Run: `cargo test -p trader cli_smoke -- --nocapture`

Expected: PASS for clap parsing and human-readable / JSON output smoke tests.

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml crates/trader/Cargo.toml crates/trader/src/main.rs crates/trader/src/cli.rs crates/trader/src/output.rs crates/trader/tests/cli_smoke.rs
git commit -m "feat: add trader cli entrypoint"
```

## Task 7: Add The Multi-Panel TUI State Machine And Order Forms

**Files:**
- Create: `crates/terminal_tui/Cargo.toml`
- Create: `crates/terminal_tui/src/terminal_tui.rs`
- Create: `crates/terminal_tui/src/app.rs`
- Create: `crates/terminal_tui/src/actions.rs`
- Create: `crates/terminal_tui/src/panels.rs`
- Create: `crates/terminal_tui/src/forms.rs`
- Create: `crates/terminal_tui/tests/app_state_tests.rs`
- Test: `crates/terminal_tui/tests/app_state_tests.rs`

- [ ] **Step 1: Write the failing TUI state tests**

```rust
#[test]
fn live_account_submit_requires_two_confirmation_steps() {
    let mut app = terminal_tui::app::AppState::new();
    app.active_account = "acc_lb_live".to_string();
    app.begin_submit("AAPL.US".to_string(), "buy".to_string());
    assert_eq!(app.confirmation_state, terminal_tui::app::ConfirmationState::Review);
    app.confirm_current_action();
    assert_eq!(app.confirmation_state, terminal_tui::app::ConfirmationState::Final);
}

#[test]
fn websocket_disconnect_marks_terminal_degraded() {
    let mut app = terminal_tui::app::AppState::new();
    app.handle_stream_disconnected();
    assert!(app.connection_degraded);
    assert_eq!(app.status_message.as_deref(), Some("stream disconnected; polling only"));
}
```

- [ ] **Step 2: Run the TUI state test to verify it fails**

Run: `cargo test -p terminal_tui live_account_submit_requires_two_confirmation_steps -- --exact`

Expected: FAIL because the `terminal_tui` crate and `AppState` reducer do not exist.

- [ ] **Step 3: Implement the app state, confirmation policy, and render hooks**

```rust
pub enum ActivePanel {
    Watchlist,
    Quote,
    Orders,
    Positions,
}

pub enum ConfirmationState {
    None,
    Review,
    Final,
}

pub struct AppState {
    pub active_panel: ActivePanel,
    pub active_account: String,
    pub selected_symbol: Option<String>,
    pub confirmation_state: ConfirmationState,
    pub connection_degraded: bool,
    pub status_message: Option<String>,
}
```

```rust
impl AppState {
    pub fn begin_submit(&mut self, symbol: String, side: String) {
        self.selected_symbol = Some(symbol);
        self.confirmation_state = if self.active_account == "acc_lb_live" {
            ConfirmationState::Review
        } else {
            ConfirmationState::Final
        };
        self.status_message = Some(format!("submit {side} order pending confirmation"));
    }
}
```

```rust
pub fn render(frame: &mut ratatui::Frame, app: &AppState) {
    panels::render_watchlist(frame, app);
    panels::render_quote(frame, app);
    panels::render_orders(frame, app);
    panels::render_positions(frame, app);
    panels::render_status_bar(frame, app);
}
```

- [ ] **Step 4: Run the TUI tests to verify they pass**

Run: `cargo test -p terminal_tui -- --nocapture`

Expected: PASS for focus movement, confirmation stages, and degraded stream state.

- [ ] **Step 5: Commit**

```bash
git add crates/terminal_tui/Cargo.toml crates/terminal_tui/src/terminal_tui.rs crates/terminal_tui/src/app.rs crates/terminal_tui/src/actions.rs crates/terminal_tui/src/panels.rs crates/terminal_tui/src/forms.rs crates/terminal_tui/tests/app_state_tests.rs
git commit -m "feat: add trader terminal tui scaffold"
```

## Task 8: Wire End-To-End Docs And Verification

**Files:**
- Modify: `README.md`
- Modify: `tech.md`
- Modify: `Cargo.toml`
- Test: `crates/api/tests/terminal_trading_smoke.rs`
- Test: `crates/trader/tests/cli_smoke.rs`
- Test: `crates/terminal_tui/tests/app_state_tests.rs`

- [ ] **Step 1: Add a failing README/tech.md checklist item to your review notes**

```text
Missing after implementation:
- README documents `cargo run -p trader -- tui`
- README documents submit/cancel/amend CLI examples
- tech.md documents terminal crates and new terminal HTTP routes
```

- [ ] **Step 2: Run the full verification set before updating docs**

Run: `cargo test -p db -p exec -p api -p terminal_client -p terminal_tui -p trader`

Expected: PASS for the new DB, execution, API, client, TUI, and CLI suites.

- [ ] **Step 3: Update the docs with the final terminal surface**

```md
## Terminal

```bash
cargo run -p trader -- tui
cargo run -p trader -- quote AAPL.US
cargo run -p trader -- order submit --account-id acc_mvp_paper --symbol AAPL.US --side buy --qty 10 --limit-price 123.45
```

New APIs:

- `POST /v1/orders`
- `POST /v1/orders/:order_id/cancel`
- `POST /v1/orders/:order_id/amend`
- `GET /v1/terminal/overview`
- `GET /v1/quotes/:symbol`
```

- [ ] **Step 4: Re-run the full verification and confirm docs match shipped behavior**

Run: `cargo test -p db -p exec -p api -p terminal_client -p terminal_tui -p trader`

Expected: PASS again, with README and `tech.md` now reflecting the new terminal surface.

- [ ] **Step 5: Commit**

```bash
git add README.md tech.md Cargo.toml
git commit -m "docs: describe trader terminal workflows"
```
