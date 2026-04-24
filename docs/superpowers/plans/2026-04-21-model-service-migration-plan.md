# Model Service Migration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Rename `services/lstm-service` into a model-oriented subsystem, split workflow code from runtime serving code, and add compatibility for both legacy flat `.pt` files and new directory-based model artifacts.

**Architecture:** Keep the current FastAPI serving process, but reorganize it around a stable `services/model` boundary: `workflow/` owns training/export, `runtime/` owns loading/predicting, `main.py` wires routers and health reporting. During migration, the runtime loader must support both legacy `<symbol>_<model_type>.pt` files and new `<model_id>/model.pt + metadata.json` artifacts so `quantd` integration does not break mid-transition.

**Tech Stack:** Python 3, FastAPI, Pydantic, PyTorch, qlib-based workflow scripts, pytest, uv-managed environment.

---

## File Structure

### Existing files to modify or move

- `E:\code\trader\services\lstm-service\main.py`
  - Move to `services/model/main.py`; reduce to service assembly and health reporting.
- `E:\code\trader\services\lstm-service\readme.md`
  - Move to `services/model/readme.md`; document workflow vs runtime split and compatibility behavior.
- `E:\code\trader\services\lstm-service\qlib_pipeline\predict.py`
  - Split into runtime loader + predict router pieces under `services/model/runtime/`.
- `E:\code\trader\services\lstm-service\qlib_pipeline\train.py`
  - Move training entrypoint into `services/model/workflow/train.py`; keep API-triggered training only if explicitly still desired, otherwise convert to workflow script logic.
- `E:\code\trader\services\lstm-service\qlib_pipeline\__init__.py`
  - Replace with package layout under `services/model/workflow/` and `services/model/runtime/`.
- `E:\code\trader\services\lstm-service\tests\test_predict.py`
  - Update for new runtime loader and artifact compatibility.
- `E:\code\trader\services\lstm-service\tests\test_train.py`
  - Update for new workflow package path.
- `E:\code\trader\services\lstm-service\tests\test_health.py`
  - Update to assert separate counts for legacy and artifact models.
- `E:\code\trader\services\lstm-service\tests\test_predict_live.py`
  - Update imports and model fixture paths if still kept.
- `E:\code\trader\services\lstm-service\tests\test_models_features.py`
  - Update imports and fixture paths.

### New files to create

- `E:\code\trader\services\model\main.py`
- `E:\code\trader\services\model\readme.md`
- `E:\code\trader\services\model\runtime\__init__.py`
- `E:\code\trader\services\model\runtime\loader.py`
- `E:\code\trader\services\model\runtime\predict.py`
- `E:\code\trader\services\model\runtime\schemas.py`
- `E:\code\trader\services\model\workflow\__init__.py`
- `E:\code\trader\services\model\workflow\workflow_by_code.py`
- `E:\code\trader\services\model\workflow\train.py`
- `E:\code\trader\services\model\workflow\export.py`
- `E:\code\trader\services\model\workflow\configs\lstm_alpha360.yaml`
- `E:\code\trader\services\model\workflow\configs\alstm_alpha360.yaml`
- `E:\code\trader\services\model\models\.gitkeep`

### Existing files to inspect while implementing

- `E:\code\trader\services\lstm-service\pyproject.toml`
- `E:\code\trader\services\lstm-service\requirements.txt`
- `E:\code\trader\services\lstm-service\requirements-dev.txt`
- `E:\code\trader\services\lstm-service\qlib_pipeline\features.py`
- `E:\code\trader\services\lstm-service\qlib_pipeline\backtest.py`
- `E:\code\trader\docs\runbook.md`
- `E:\code\trader\docs\superpowers\specs\2026-04-21-quantd-paper-and-model-boundary-design.md`

---

### Task 1: Introduce Runtime Loader with Legacy + Artifact Compatibility

**Files:**
- Create: `E:\code\trader\services\model\runtime\loader.py`
- Create: `E:\code\trader\services\model\runtime\schemas.py`
- Modify: `E:\code\trader\services\lstm-service\tests\test_predict.py`
- Modify: `E:\code\trader\services\lstm-service\tests\test_health.py`

- [ ] **Step 1: Write failing tests for dual-format model discovery**

Update `services/lstm-service/tests/test_predict.py` (before moving it) to lock in both formats:

```python
def test_predict_supports_legacy_flat_pt(monkeypatch):
    models_dir = _setup_predict_env(monkeypatch)
    stub_path = models_dir / "AAPL_US_lstm.pt"
    torch.save({
        "model_state": _SimpleLSTM().state_dict(),
        "model_type": "lstm",
        "input_size": 158,
        "hidden_size": 64,
        "num_layers": 2,
        "lookback": 60,
    }, stub_path)

    with TestClient(app) as client:
        resp = client.post("/predict", json={
            "symbol": "AAPL.US",
            "model_type": "lstm",
            "bars": SAMPLE_BARS,
        })
        assert resp.status_code == 200
```

```python
def test_predict_supports_directory_artifact_with_metadata(monkeypatch):
    models_dir = _setup_predict_env(monkeypatch)
    artifact_dir = models_dir / "alstm_alpha360_us_v1"
    artifact_dir.mkdir(parents=True, exist_ok=True)
    torch.save({
        "model_state": _ALSTM().state_dict(),
        "model_type": "alstm",
        "input_size": 158,
        "hidden_size": 64,
        "num_layers": 2,
        "lookback": 60,
    }, artifact_dir / "model.pt")
    (artifact_dir / "metadata.json").write_text(json.dumps({
        "model_id": "alstm_alpha360_us_v1",
        "model_type": "alstm",
        "symbol_universe": ["AAPL.US"],
        "feature_set": "Alpha360",
        "lookback": 60,
        "prediction_semantics": "score in [-1,1]",
    }))

    with TestClient(app) as client:
        resp = client.post("/predict", json={
            "symbol": "AAPL.US",
            "model_type": "alstm",
            "bars": SAMPLE_BARS,
        })
        assert resp.status_code == 200
```

Update `services/lstm-service/tests/test_health.py` to require separate counts:

```python
def test_health_reports_legacy_and_artifact_model_counts(monkeypatch):
    ...
    assert resp.json()["legacy_models_loaded"] == 1
    assert resp.json()["artifact_models_loaded"] == 1
```

- [ ] **Step 2: Run the model-service predict/health tests and verify failure**

Run:

```powershell
Set-Location E:\code\trader\services\lstm-service
uv run pytest tests/test_predict.py tests/test_health.py -q
```

Expected: FAIL because current loader only scans `*.pt` in the models root and `/health` only returns `models_loaded`.

- [ ] **Step 3: Create runtime schemas for metadata and model handles**

Create `services/model/runtime/schemas.py`:

```python
from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path
from typing import Any


@dataclass(frozen=True)
class ModelMetadata:
    model_id: str
    model_type: str
    lookback: int
    symbol_universe: list[str] | None
    feature_set: str | None
    prediction_semantics: str | None
    artifact_dir: Path | None = None


@dataclass(frozen=True)
class LoadedModel:
    metadata: ModelMetadata
    checkpoint: dict[str, Any]
    source_path: Path
    source_kind: str  # "legacy_flat_pt" | "artifact_dir"
```

- [ ] **Step 4: Implement the compatibility loader**

Create `services/model/runtime/loader.py` with these responsibilities:

```python
def discover_legacy_models(models_dir: Path) -> list[LoadedModel]:
    ...

def discover_artifact_models(models_dir: Path) -> list[LoadedModel]:
    ...

def load_models(models_dir: Path) -> list[LoadedModel]:
    return discover_artifact_models(models_dir) + discover_legacy_models(models_dir)

def find_model_for_request(models: list[LoadedModel], symbol: str, model_type: str) -> LoadedModel | None:
    ...
```

Rules:

- Prefer artifact models first
- Fall back to legacy flat `.pt`
- For artifact models, read `metadata.json`
- For legacy flat `.pt`, synthesize minimal metadata from file name and checkpoint

- [ ] **Step 5: Re-run the predict/health tests**

Run:

```powershell
Set-Location E:\code\trader\services\lstm-service
uv run pytest tests/test_predict.py tests/test_health.py -q
```

Expected: PASS for dual-format model discovery and separate health counts.

- [ ] **Step 6: Commit**

```bash
git add services/model/runtime/loader.py services/model/runtime/schemas.py services/lstm-service/tests/test_predict.py services/lstm-service/tests/test_health.py
git commit -m "feat: add compatible model artifact loader"
```

---

### Task 2: Split Predict Serving into `runtime/` and Wire Health Reporting

**Files:**
- Create: `E:\code\trader\services\model\runtime\predict.py`
- Create: `E:\code\trader\services\model\main.py`
- Modify: `E:\code\trader\services\lstm-service\main.py`
- Modify: `E:\code\trader\services\lstm-service\tests\test_predict.py`
- Modify: `E:\code\trader\services\lstm-service\tests\test_health.py`

- [ ] **Step 1: Write failing tests for the new health payload and runtime import path**

Add/adjust tests so they import the app from the new path and assert:

```python
def test_health_schema(monkeypatch):
    with TestClient(app) as client:
        data = client.get("/health").json()
        assert data["status"] == "ok"
        assert "legacy_models_loaded" in data
        assert "artifact_models_loaded" in data
        assert "models_loaded" in data
```

- [ ] **Step 2: Run the targeted tests to confirm import and payload mismatch**

Run:

```powershell
Set-Location E:\code\trader\services\lstm-service
uv run pytest tests/test_predict.py tests/test_health.py -q
```

Expected: FAIL until the new `services/model` package and app wiring exist.

- [ ] **Step 3: Create `runtime/predict.py`**

Create `services/model/runtime/predict.py` by extracting serving logic from the old `qlib_pipeline/predict.py`. Keep the request/response contract stable:

```python
router = APIRouter()

@router.post("/predict", response_model=PredictResponse)
async def predict(req: PredictRequest) -> PredictResponse:
    models_dir = _get_models_dir()
    loaded_models = load_models(models_dir)
    loaded = find_model_for_request(loaded_models, req.symbol, req.model_type)
    if loaded is None:
        raise HTTPException(
            status_code=404,
            detail={
                "error_code": "model_not_found",
                "message": f"No model found for {req.symbol}/{req.model_type}. Train first.",
            },
        )
    ...
```

- [ ] **Step 4: Create `services/model/main.py`**

Create a thin app assembly:

```python
from fastapi import FastAPI

from runtime.loader import load_models
from runtime.predict import router as predict_router

app = FastAPI(title="model-service", version="0.1.0")
app.include_router(predict_router)

@app.get("/health")
async def health() -> dict:
    models = load_models(get_models_dir())
    legacy_count = sum(1 for item in models if item.source_kind == "legacy_flat_pt")
    artifact_count = sum(1 for item in models if item.source_kind == "artifact_dir")
    return {
        "status": "ok",
        "models_loaded": len(models),
        "legacy_models_loaded": legacy_count,
        "artifact_models_loaded": artifact_count,
    }
```

- [ ] **Step 5: Re-run the predict/health tests**

Run:

```powershell
Set-Location E:\code\trader\services\lstm-service
uv run pytest tests/test_predict.py tests/test_health.py -q
```

Expected: PASS with the same `/predict` contract and expanded `/health` payload.

- [ ] **Step 6: Commit**

```bash
git add services/model/main.py services/model/runtime/predict.py services/lstm-service/main.py services/lstm-service/tests/test_predict.py services/lstm-service/tests/test_health.py
git commit -m "feat: split model runtime serving from workflow code"
```

---

### Task 3: Move Training to `workflow/` and Add Config-Driven Entry Points

**Files:**
- Create: `E:\code\trader\services\model\workflow\train.py`
- Create: `E:\code\trader\services\model\workflow\workflow_by_code.py`
- Create: `E:\code\trader\services\model\workflow\export.py`
- Create: `E:\code\trader\services\model\workflow\configs\lstm_alpha360.yaml`
- Create: `E:\code\trader\services\model\workflow\configs\alstm_alpha360.yaml`
- Modify: `E:\code\trader\services\lstm-service\tests\test_train.py`

- [ ] **Step 1: Write failing tests for workflow package imports and train/export split**

Update `services/lstm-service/tests/test_train.py` so it imports from the new workflow path and asserts the training module can be called without touching serving-only modules:

```python
def test_train_uses_requested_date_range(monkeypatch):
    from workflow.train import train_model
    ...
```

Add one export-focused test:

```python
def test_export_writes_metadata_json(monkeypatch):
    from workflow.export import export_model_artifact
    artifact_dir = export_model_artifact(...)
    assert (artifact_dir / "metadata.json").exists()
```

- [ ] **Step 2: Run the train tests to verify import failures**

Run:

```powershell
Set-Location E:\code\trader\services\lstm-service
uv run pytest tests/test_train.py -q
```

Expected: FAIL because the new workflow package does not exist yet.

- [ ] **Step 3: Create `workflow/train.py` with callable training helpers**

Refactor training into functions rather than only an API route:

```python
def train_model(symbol: str, model_type: str, start: str, end: str) -> tuple[nn.Module, dict]:
    ...

def save_legacy_checkpoint(model: nn.Module, symbol: str, model_type: str, metrics: dict) -> Path:
    ...
```

Keep the qlib initialization and fake-qlib testability behavior from the existing implementation.

- [ ] **Step 4: Create `workflow/export.py` for artifact directories**

Implement:

```python
def export_model_artifact(
    model: nn.Module,
    *,
    model_id: str,
    model_type: str,
    lookback: int,
    feature_set: str,
    symbol_universe: list[str] | None,
    train_start: str,
    train_end: str,
    metrics: dict,
) -> Path:
    ...
```

Write both `model.pt` and `metadata.json`.

- [ ] **Step 5: Add config-driven workflow entrypoints**

Create:

- `workflow/workflow_by_code.py`
- `workflow/configs/lstm_alpha360.yaml`
- `workflow/configs/alstm_alpha360.yaml`

Use a minimal shape like:

```yaml
model:
  type: alstm
  lookback: 60
data:
  feature_set: Alpha360
  region: us
train:
  start: "2020-01-01"
  end: "2024-12-31"
```

`workflow_by_code.py` should load one config and call `train_model(...)` + `export_model_artifact(...)`.

- [ ] **Step 6: Re-run the train tests**

Run:

```powershell
Set-Location E:\code\trader\services\lstm-service
uv run pytest tests/test_train.py -q
```

Expected: PASS, including export metadata coverage.

- [ ] **Step 7: Commit**

```bash
git add services/model/workflow/train.py services/model/workflow/export.py services/model/workflow/workflow_by_code.py services/model/workflow/configs services/lstm-service/tests/test_train.py
git commit -m "feat: move model training to workflow package"
```

---

### Task 4: Rename the Service Directory and Update Tests/Docs

**Files:**
- Create: `E:\code\trader\services\model\readme.md`
- Modify: `E:\code\trader\services\lstm-service\readme.md`
- Modify: `E:\code\trader\docs\runbook.md`
- Modify: `E:\code\trader\services\lstm-service\tests\test_predict.py`
- Modify: `E:\code\trader\services\lstm-service\tests\test_train.py`
- Modify: `E:\code\trader\services\lstm-service\tests\test_predict_live.py`
- Modify: `E:\code\trader\services\lstm-service\tests\test_models_features.py`

- [ ] **Step 1: Write the failing test/doc assumptions for the new service path**

Add one import smoke test that uses the new package root:

```python
def test_main_module_imports_from_model_package():
    from services.model.main import app
    assert app is not None
```

Also note in `docs/runbook.md` that current path references will switch from `services/lstm-service` to `services/model`.

- [ ] **Step 2: Run the relevant tests to confirm path mismatch**

Run:

```powershell
Set-Location E:\code\trader\services\lstm-service
uv run pytest tests/test_predict.py tests/test_train.py -q
```

Expected: FAIL or import mismatch until the new path is fully wired.

- [ ] **Step 3: Create `services/model/readme.md`**

Document:

- workflow vs runtime split
- legacy `.pt` compatibility
- artifact directory format
- commands to run `workflow_by_code.py`
- command to start `main.py`

- [ ] **Step 4: Update `docs/runbook.md` to reference the new path**

Change the model-service section from:

```md
Set-Location E:\code\trader\services\lstm-service
```

to:

```md
Set-Location E:\code\trader\services\model
```

and note the compatibility period if legacy fixtures remain.

- [ ] **Step 5: Update the remaining test imports and fixture paths**

Adjust test modules to import from `services.model.*` or the new package-relative modules, and update temp directories if needed.

- [ ] **Step 6: Re-run the Python model-service test subset**

Run:

```powershell
Set-Location E:\code\trader\services\model
uv run pytest tests/test_health.py tests/test_predict.py tests/test_train.py -q
```

Expected: PASS for the core model-service subset.

- [ ] **Step 7: Commit**

```bash
git add services/model/readme.md docs/runbook.md services/lstm-service/tests/test_predict.py services/lstm-service/tests/test_train.py services/lstm-service/tests/test_predict_live.py services/lstm-service/tests/test_models_features.py
git commit -m "refactor: rename lstm service to model service"
```

---

### Task 5: End-to-End Verification for the Model Subsystem Migration

**Files:**
- Verify only; no new files

- [ ] **Step 1: Run the core Python test subset**

Run:

```powershell
Set-Location E:\code\trader\services\model
uv run pytest tests/test_health.py tests/test_predict.py tests/test_train.py -q
```

Expected: PASS.

- [ ] **Step 2: Start the service and verify the health payload**

Run:

```powershell
Set-Location E:\code\trader\services\model
$env:LSTM_MODELS_DIR = ".\models"
uv run uvicorn main:app --host 127.0.0.1 --port 8000
```

In another terminal:

```powershell
Invoke-RestMethod http://127.0.0.1:8000/health
```

Expected payload shape:

```json
{
  "status": "ok",
  "models_loaded": 0,
  "legacy_models_loaded": 0,
  "artifact_models_loaded": 0
}
```

- [ ] **Step 3: Verify one legacy model and one artifact model are both discoverable**

Prepare:

```text
models/AAPL_US_lstm.pt
models/alstm_alpha360_us_v1/model.pt
models/alstm_alpha360_us_v1/metadata.json
```

Then:

```powershell
Invoke-RestMethod http://127.0.0.1:8000/health
```

Expected:

- `legacy_models_loaded == 1`
- `artifact_models_loaded == 1`
- `models_loaded == 2`

- [ ] **Step 4: Run repo-level sanity for the docs path**

Run:

```powershell
Get-Content E:\code\trader\docs\runbook.md
Get-Content E:\code\trader\services\model\readme.md
```

Expected: both documents point to `services/model` as the active service path and describe the compatibility loader.

- [ ] **Step 5: Commit follow-up fixes only if verification found issues**

If the verification required last-mile adjustments:

```bash
git add services/model docs/runbook.md
git commit -m "test: finalize model service migration verification"
```

If no code/doc changes were needed after verification, skip this commit.

---

## Self-Review

### Spec coverage

- `services/lstm-service` to `services/model`: covered by Task 4.
- Workflow/runtime split: covered by Tasks 2 and 3.
- qlib workflow/config direction: covered by Task 3.
- Artifact directory + metadata contract: covered by Tasks 1 and 3.
- Legacy flat `.pt` compatibility period: covered by Task 1 and verified in Task 5.

### Placeholder scan

- No `TBD`, `TODO`, or generic “add tests later” placeholders remain.
- Each task includes file paths, commands, and expected outcomes.
- Code steps include concrete module/function skeletons.

### Type consistency

- `legacy_models_loaded`, `artifact_models_loaded`, and `models_loaded` are used consistently in tests and health payload.
- Artifact layout is consistently `models/<model_id>/model.pt + metadata.json`.
- Runtime loader source kinds are consistently `legacy_flat_pt` and `artifact_dir`.

