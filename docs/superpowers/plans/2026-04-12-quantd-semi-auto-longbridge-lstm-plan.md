# Quantd Semi-Auto Longbridge LSTM Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a semi-automatic US equities trading runtime that can reliably call `lstm-service`, score a symbol universe, apply hard risk gates, and place auditable Longbridge orders with reconciliation and operator controls.

**Architecture:** Keep `quantd` as the only runtime trading process and treat `services/lstm-service` as an external model service. Implement in four layers: stabilize the Python predict service first, then add universe-cycle orchestration in Rust, then hard risk + reconciliation, then operator control endpoints and staged rollout support.

**Tech Stack:** Rust workspace (`axum`, `tokio`, `sqlx`, `reqwest`, `tracing`, `longbridge`), Python FastAPI, PyTorch, pytest

---

## File Change Index

### Create
- `crates/pipeline/src/universe.rs` - universe batch cycle orchestration and related types
- `crates/pipeline/tests/universe_cycle_tests.rs` - batch scoring / ranking / risk / execution integration tests
- `crates/db/migrations/004_runtime_controls.sql` - runtime control tables and reconciliation snapshots
- `crates/db/src/runtime_controls.rs` - read/write runtime mode and symbol allowlist
- `crates/db/src/reconciliation.rs` - persist reconciliation results
- `crates/api/tests/runtime_controls_smoke.rs` - HTTP smoke tests for control endpoints
- `services/lstm-service/tests/test_predict_live.py` - service startup and predict smoke flow

### Modify
- `services/lstm-service/main.py`
- `services/lstm-service/qlib_pipeline/predict.py`
- `services/lstm-service/readme.md`
- `crates/strategy/src/lstm.rs`
- `crates/pipeline/src/pipeline.rs`
- `crates/pipeline/src/risk.rs`
- `crates/quantd/src/main.rs`
- `crates/api/src/api.rs`
- `crates/api/src/handlers.rs`
- `crates/db/src/db.rs`
- `crates/db/src/system_config.rs`
- `crates/longbridge_adapters/src/exec_lb.rs`
- `README.md`

---

### Task 1: Stabilize `lstm-service` startup and `/predict`

**Files:**
- Modify: `services/lstm-service/main.py`
- Modify: `services/lstm-service/qlib_pipeline/predict.py`
- Modify: `services/lstm-service/readme.md`
- Create: `services/lstm-service/tests/test_predict_live.py`

- [ ] **Step 1: Write the failing smoke test**

```python
from pathlib import Path
import torch
import torch.nn as nn
from fastapi.testclient import TestClient
from main import app

def test_health_reports_models_loaded(tmp_path, monkeypatch):
    monkeypatch.setenv("LSTM_MODELS_DIR", str(tmp_path))
    torch.save(
        {"model_state": nn.Linear(158, 1).state_dict(), "model_type": "lstm"},
        tmp_path / "AAPL_US_lstm.pt",
    )
    client = TestClient(app)
    response = client.get("/health")
    assert response.status_code == 200
    assert response.json() == {"status": "ok", "models_loaded": 1}

def test_predict_returns_structured_error_for_missing_model(tmp_path, monkeypatch):
    monkeypatch.setenv("LSTM_MODELS_DIR", str(tmp_path))
    client = TestClient(app)
    response = client.post("/predict", json={"symbol": "AAPL.US", "model_type": "lstm", "bars": [
        {"ts_ms": 1700000000000 + i, "open": 10.0, "high": 11.0, "low": 9.0, "close": 10.5, "volume": 1000.0}
        for i in range(60)
    ]})
    assert response.status_code == 404
    assert response.json()["detail"]["error_code"] == "model_not_found"
```

- [ ] **Step 2: Run the test to verify the current service shape fails**

Run: `cd services/lstm-service; pytest tests/test_predict_live.py -v`  
Expected: FAIL because `/health` does not re-read `LSTM_MODELS_DIR` dynamically and `/predict` returns a plain string detail.

- [ ] **Step 3: Implement explicit settings lookup and structured predict errors**

```python
# services/lstm-service/main.py
def models_dir() -> Path:
    path = Path(os.getenv("LSTM_MODELS_DIR", "models"))
    path.mkdir(parents=True, exist_ok=True)
    return path

@app.get("/health")
async def health() -> dict:
    return {"status": "ok", "models_loaded": len(list(models_dir().glob("*.pt")))}
```

```python
# services/lstm-service/qlib_pipeline/predict.py
class ErrorBody(BaseModel):
    error_code: str
    message: str

def models_dir() -> Path:
    path = Path(os.getenv("LSTM_MODELS_DIR", "models"))
    path.mkdir(parents=True, exist_ok=True)
    return path

def _model_path(symbol: str, model_type: str) -> Path:
    return models_dir() / f"{symbol.replace('.', '_')}_{model_type}.pt"

if not path.exists():
    raise HTTPException(
        status_code=404,
        detail=ErrorBody(
            error_code="model_not_found",
            message=f"no model found for {req.symbol}/{req.model_type}",
        ).model_dump(),
    )
```

- [ ] **Step 4: Add exact startup docs**

```md
## Quick Start
```bash
python -m venv .venv
. .venv/Scripts/Activate.ps1
pip install -r requirements.txt
uvicorn main:app --host 127.0.0.1 --port 8000
```
```

- [ ] **Step 5: Run tests to verify the service is stable**

Run: `cd services/lstm-service; pytest tests/test_predict_live.py tests/test_health.py tests/test_predict.py -v`  
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add services/lstm-service/main.py services/lstm-service/qlib_pipeline/predict.py services/lstm-service/readme.md services/lstm-service/tests/test_predict_live.py
git commit -m "feat(lstm-service): stabilize startup and predict contract"
```

### Task 2: Make Rust LSTM strategy fail loudly and predictably

**Files:**
- Modify: `crates/strategy/src/lstm.rs`
- Modify: `crates/pipeline/src/pipeline.rs`

- [ ] **Step 1: Write the failing Rust test for structured service failure mapping**

```rust
#[tokio::test]
async fn service_error_is_returned_as_strategy_error() {
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/predict"))
        .respond_with(ResponseTemplate::new(503).set_body_json(serde_json::json!({
            "detail": {"error_code": "model_service_unavailable", "message": "warmup"}
        })))
        .mount(&server)
        .await;

    let strategy = LstmStrategy::new(server.uri(), "lstm".to_string(), 60, 0.6, -0.6, None, "paper".to_string());
    let context = StrategyContext {
        instrument: InstrumentId::new(Venue::UsEquity, "AAPL.US"),
        instrument_db_id: 1,
        last_bar_close: Some(100.0),
        ts_ms: 1700000000000,
    };

    let error = strategy.evaluate_signal(&context).await.expect_err("strategy error");
    assert!(error.contains("model_service_unavailable"));
}
```

- [ ] **Step 2: Run the failing test**

Run: `cargo test -p strategy service_error_is_returned_as_strategy_error -- --nocapture`  
Expected: FAIL because current strategy returns `None` on service failure.

- [ ] **Step 3: Introduce a result-returning helper**

```rust
#[derive(serde::Deserialize)]
struct PredictErrorEnvelope {
    detail: PredictErrorDetail,
}

#[derive(serde::Deserialize)]
struct PredictErrorDetail {
    error_code: String,
    message: String,
}

impl LstmStrategy {
    pub async fn evaluate_signal(&self, context: &StrategyContext) -> Result<Option<Signal>, String> {
        let bars = self.load_bars(context).await?;
        let response = self.client.post(format!("{}/predict", self.service_url))
            .json(&PredictRequest { symbol: context.instrument.symbol.as_str(), model_type: &self.model_type, bars })
            .send().await.map_err(|e| format!("lstm_request_failed:{e}"))?;
        if !response.status().is_success() {
            let envelope = response.json::<PredictErrorEnvelope>().await
                .map_err(|e| format!("lstm_error_decode_failed:{e}"))?;
            return Err(format!("{}:{}", envelope.detail.error_code, envelope.detail.message));
        }
        let prediction = response.json::<PredictResponse>().await
            .map_err(|e| format!("lstm_response_decode_failed:{e}"))?;
        Ok(self.prediction_to_signal(context, prediction))
    }
}
```

- [ ] **Step 4: Run the strategy test suite**

Run: `cargo test -p strategy lstm -- --nocapture`  
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/strategy/src/lstm.rs crates/pipeline/src/pipeline.rs
git commit -m "feat(strategy): surface structured lstm service failures"
```

### Task 3: Add runtime control and reconciliation persistence

**Files:**
- Create: `crates/db/migrations/004_runtime_controls.sql`
- Create: `crates/db/src/runtime_controls.rs`
- Create: `crates/db/src/reconciliation.rs`
- Modify: `crates/db/src/db.rs`

- [ ] **Step 1: Write the failing DB migration test**

```rust
#[tokio::test]
async fn runtime_control_tables_exist_after_migrate() {
    let database = Db::connect("sqlite::memory:").await.expect("db");
    let controls = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='runtime_controls'",
    ).fetch_one(database.pool()).await.expect("runtime_controls exists");
    let reconciliation = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='reconciliation_snapshots'",
    ).fetch_one(database.pool()).await.expect("reconciliation exists");
    assert_eq!(controls, 1);
    assert_eq!(reconciliation, 1);
}
```

- [ ] **Step 2: Run the test**

Run: `cargo test -p db runtime_control_tables_exist_after_migrate -- --nocapture`  
Expected: FAIL because the tables do not exist yet.

- [ ] **Step 3: Add migration and helpers**

```sql
CREATE TABLE IF NOT EXISTS runtime_controls (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL,
    updated_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS symbol_allowlist (
    symbol TEXT PRIMARY KEY,
    enabled INTEGER NOT NULL DEFAULT 1,
    updated_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS reconciliation_snapshots (
    id TEXT PRIMARY KEY,
    account_id TEXT NOT NULL,
    broker_cash TEXT,
    local_cash TEXT,
    broker_positions_json TEXT,
    local_positions_json TEXT,
    mismatch_count INTEGER NOT NULL,
    status TEXT NOT NULL,
    created_at INTEGER NOT NULL
);
```

```rust
pub async fn set_runtime_control(pool: &SqlitePool, key: &str, value: &str) -> Result<(), DbError> { /* upsert */ }
pub async fn get_runtime_control(pool: &SqlitePool, key: &str) -> Result<Option<String>, DbError> { /* select */ }
pub async fn replace_symbol_allowlist(pool: &SqlitePool, symbols: &[String]) -> Result<(), DbError> { /* replace in tx */ }
pub async fn list_symbol_allowlist(pool: &SqlitePool) -> Result<Vec<String>, DbError> { /* select */ }
pub async fn insert_reconciliation_snapshot(/* ... */) -> Result<(), DbError> { /* insert */ }
```

- [ ] **Step 4: Run the DB tests**

Run: `cargo test -p db connect_migrate -- --nocapture`  
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/db/migrations/004_runtime_controls.sql crates/db/src/runtime_controls.rs crates/db/src/reconciliation.rs crates/db/src/db.rs crates/db/tests/connect_migrate.rs
git commit -m "feat(db): add runtime controls and reconciliation storage"
```

### Task 4: Introduce universe-cycle orchestration above single-symbol pipeline

**Files:**
- Create: `crates/pipeline/src/universe.rs`
- Modify: `crates/pipeline/src/pipeline.rs`
- Create: `crates/pipeline/tests/universe_cycle_tests.rs`

- [ ] **Step 1: Write the failing universe ranking test**

```rust
#[tokio::test]
async fn cycle_limits_orders_to_top_ranked_symbols() {
    let cycle = run_one_cycle_for_universe(
        vec![
            CycleDecision::new("AAPL.US".to_string(), 0.95, 0.95),
            CycleDecision::new("MSFT.US".to_string(), 0.70, 0.70),
            CycleDecision::new("NVDA.US".to_string(), 0.40, 0.40),
        ],
        UniverseCycleConfig { max_open_positions: 2, min_confidence: 0.5 },
    );
    assert_eq!(cycle.selected_symbols, vec!["AAPL.US", "MSFT.US"]);
    assert_eq!(cycle.rejected_symbols, vec!["NVDA.US"]);
}
```

- [ ] **Step 2: Run the test**

Run: `cargo test -p pipeline cycle_limits_orders_to_top_ranked_symbols -- --nocapture`  
Expected: FAIL because the orchestration API does not exist yet.

- [ ] **Step 3: Add universe types and selection logic**

```rust
pub struct CycleDecision { pub symbol: String, pub score: f64, pub confidence: f64 }
impl CycleDecision { pub fn new(symbol: String, score: f64, confidence: f64) -> Self { Self { symbol, score, confidence } } }
pub struct UniverseCycleConfig { pub max_open_positions: usize, pub min_confidence: f64 }
pub struct UniverseCycleResult { pub selected_symbols: Vec<String>, pub rejected_symbols: Vec<String> }

pub fn run_one_cycle_for_universe(mut decisions: Vec<CycleDecision>, config: UniverseCycleConfig) -> UniverseCycleResult {
    decisions.sort_by(|l, r| r.score.partial_cmp(&l.score).unwrap_or(std::cmp::Ordering::Equal));
    let mut selected = Vec::new();
    let mut rejected = Vec::new();
    for decision in decisions {
        if decision.confidence < config.min_confidence || selected.len() >= config.max_open_positions {
            rejected.push(decision.symbol);
        } else {
            selected.push(decision.symbol);
        }
    }
    UniverseCycleResult { selected_symbols: selected, rejected_symbols: rejected }
}
```

- [ ] **Step 4: Run the pipeline tests**

Run: `cargo test -p pipeline universe_cycle_tests -- --nocapture`  
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/pipeline/src/universe.rs crates/pipeline/src/pipeline.rs crates/pipeline/tests/universe_cycle_tests.rs
git commit -m "feat(pipeline): add universe cycle selection primitives"
```

### Task 5: Add runtime mode and allowlist endpoints

**Files:**
- Modify: `crates/api/src/api.rs`
- Modify: `crates/api/src/handlers.rs`
- Create: `crates/api/tests/runtime_controls_smoke.rs`

- [ ] **Step 1: Write the failing API smoke test**

```rust
#[tokio::test]
async fn runtime_controls_round_trip() {
    let state = api::test_support::app_state().await;
    let app = api::router(state);
    let put = app.clone().oneshot(
        Request::builder().method("PUT").uri("/v1/runtime/mode")
            .header("content-type", "application/json")
            .body(Body::from(r#"{"mode":"observe_only"}"#)).unwrap()
    ).await.unwrap();
    assert_eq!(put.status(), StatusCode::NO_CONTENT);
}
```

- [ ] **Step 2: Run the smoke test**

Run: `cargo test -p api runtime_controls_round_trip -- --nocapture`  
Expected: FAIL with 404 route not found.

- [ ] **Step 3: Implement handlers and routes**

```rust
#[derive(Deserialize)] pub struct RuntimeModeUpdate { pub mode: String }
#[derive(Serialize)] pub struct RuntimeModeBody { pub mode: String }
#[derive(Deserialize)] pub struct SymbolAllowlistUpdate { pub symbols: Vec<String> }
#[derive(Serialize)] pub struct SymbolAllowlistBody { pub symbols: Vec<String> }

pub async fn get_runtime_mode(State(state): State<Arc<AppState>>) -> Result<Json<RuntimeModeBody>, ApiError> {
    let mode = db::get_runtime_control(state.database.pool(), "mode").await.map_err(ApiError::internal)?
        .unwrap_or_else(|| "observe_only".to_string());
    Ok(Json(RuntimeModeBody { mode }))
}
pub async fn put_runtime_mode(State(state): State<Arc<AppState>>, Json(body): Json<RuntimeModeUpdate>) -> Result<StatusCode, ApiError> {
    db::set_runtime_control(state.database.pool(), "mode", &body.mode).await.map_err(ApiError::internal)?;
    Ok(StatusCode::NO_CONTENT)
}
```

```rust
.route("/runtime/mode", get(handlers::get_runtime_mode))
.route("/runtime/mode", put(handlers::put_runtime_mode))
.route("/runtime/allowlist", get(handlers::get_symbol_allowlist))
.route("/runtime/allowlist", put(handlers::put_symbol_allowlist))
```

- [ ] **Step 4: Run the API smoke tests**

Run: `cargo test -p api runtime_controls_smoke -- --nocapture`  
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/api/src/api.rs crates/api/src/handlers.rs crates/api/tests/runtime_controls_smoke.rs
git commit -m "feat(api): add runtime mode and allowlist endpoints"
```

### Task 6: Default to `observe_only`, add reconciliation guardrail, and verify

**Files:**
- Modify: `crates/quantd/src/main.rs`
- Modify: `crates/longbridge_adapters/src/exec_lb.rs`
- Modify: `README.md`

- [ ] **Step 1: Write the failing quantd test for default runtime mode**

```rust
#[tokio::test]
async fn startup_sets_observe_only_mode_when_missing() {
    let database = db::Db::connect("sqlite::memory:").await.expect("db");
    quantd::init_runtime_defaults(&database).await.expect("init defaults");
    let mode = db::get_runtime_control(database.pool(), "mode").await.expect("mode");
    assert_eq!(mode.as_deref(), Some("observe_only"));
}
```

- [ ] **Step 2: Run the test**

Run: `cargo test -p quantd startup_sets_observe_only_mode_when_missing -- --nocapture`  
Expected: FAIL because `init_runtime_defaults` is missing.

- [ ] **Step 3: Add startup defaults and reconciliation failure persistence**

```rust
pub async fn init_runtime_defaults(database: &db::Db) -> Result<(), db::DbError> {
    if db::get_runtime_control(database.pool(), "mode").await?.is_none() {
        db::set_runtime_control(database.pool(), "mode", "observe_only").await?;
    }
    Ok(())
}

async fn record_reconciliation_failure(database: &db::Db, account_id: &str, reason: &str) {
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64).unwrap_or(0);
    let _ = db::insert_reconciliation_snapshot(
        database.pool(),
        &uuid::Uuid::new_v4().to_string(),
        account_id, "", "", "[]", "[]", 1, reason, now,
    ).await;
}
```

```rust
pub struct BrokerAccountSnapshot {
    pub cash: String,
    pub positions_json: String,
}
```

- [ ] **Step 4: Add rollout checklist and run verification**

```md
## Semi-Auto Operator Checklist
1. Start `lstm-service`
2. Verify `curl http://127.0.0.1:8000/health`
3. Put runtime mode to `observe_only`
4. Set `/v1/runtime/allowlist`
5. Trigger one manual cycle and inspect logs
6. Verify reconciliation snapshot status
7. Switch to live mode only after logs, DB state, and broker state agree
```

Run: `powershell -ExecutionPolicy Bypass -File .\script\verify.ps1`  
Expected: PASS

Run: `cd services/lstm-service; pytest tests/test_health.py tests/test_predict.py tests/test_predict_live.py -v`  
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/quantd/src/main.rs crates/longbridge_adapters/src/exec_lb.rs README.md
git commit -m "feat(ops): default semi-auto runtime to observe-only"
```

---

## Self-Review

**Spec coverage**
- `lstm-service` first: Task 1.
- distinguish service failure from hold: Task 2.
- runtime controls and persistence: Task 3 and Task 5.
- batch universe selection: Task 4.
- observe-only default, reconciliation, staged rollout: Task 6.

**Placeholder scan**
- No `TODO` / `TBD` placeholders remain.

**Type consistency**
- Runtime control key `"mode"` is used consistently.
- Universe cycle types use `CycleDecision`, `UniverseCycleConfig`, `UniverseCycleResult` consistently.
- Error payload consistently uses `detail.error_code` and `detail.message`.

