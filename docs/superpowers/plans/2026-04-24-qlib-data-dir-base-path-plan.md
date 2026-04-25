# QLIB_DATA_DIR Base Path Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Change the Python model service so `QLIB_DATA_DIR` means a base directory and always resolves the Qlib provider path as `<base>/qlib/us_data`.

**Architecture:** Add one shared path helper in `workflow/shared.py`, then replace direct `QLIB_DATA_DIR` reads in `data_update.py`, `features.py`, and `train.py` with that helper. Keep dataset existence checks in the feature/train/update flows, and make old `.../us_data` environment values fail fast with a clear error.

**Tech Stack:** FastAPI, Pydantic, Python `pathlib`, pytest, uv, pyqlib

---

## File Structure

- Modify: `services/model/workflow/shared.py`
  - Add the shared Qlib base-dir resolver and validation message.
- Modify: `services/model/workflow/data_update.py`
  - Replace local provider-path env lookup with the shared helper.
- Modify: `services/model/workflow/features.py`
  - Replace local provider-path env lookup with the shared helper.
- Modify: `services/model/workflow/train.py`
  - Replace local provider-path env lookup with the shared helper.
- Modify: `services/model/tests/test_data_update.py`
  - Add tests for default/base-dir path resolution and invalid old-style env values.
- Modify: `services/model/tests/test_models_features.py`
  - Add/adjust tests for the new provider-path contract in `/features`.
- Modify: `services/model/tests/test_train.py`
  - Add/adjust tests for the new provider-path contract in `/train`.
- Modify: `docs/runbook.md`
  - Update the `QLIB_DATA_DIR` example and wording to describe base-dir semantics.

### Task 1: Add Shared Qlib Path Resolution

**Files:**
- Modify: `services/model/workflow/shared.py`
- Test: `services/model/tests/test_data_update.py`

- [ ] **Step 1: Write the failing tests for path resolution**

```python
from pathlib import Path

import pytest
from fastapi import HTTPException

from workflow.shared import get_qlib_provider_dir


def test_get_qlib_provider_dir_uses_default_base(monkeypatch):
    monkeypatch.delenv("QLIB_DATA_DIR", raising=False)
    provider = get_qlib_provider_dir()
    assert provider == Path("~/.qlib").expanduser() / "qlib" / "us_data"


def test_get_qlib_provider_dir_uses_base_dir_env(monkeypatch, tmp_path):
    monkeypatch.setenv("QLIB_DATA_DIR", str(tmp_path / "market"))
    provider = get_qlib_provider_dir()
    assert provider == (tmp_path / "market" / "qlib" / "us_data")


def test_get_qlib_provider_dir_rejects_old_provider_path(monkeypatch, tmp_path):
    legacy = tmp_path / "qlib_data" / "us_data"
    monkeypatch.setenv("QLIB_DATA_DIR", str(legacy))

    with pytest.raises(HTTPException) as exc_info:
        get_qlib_provider_dir()

    assert exc_info.value.status_code == 500
    assert "base directory" in exc_info.value.detail
    assert "<base>/qlib/us_data" in exc_info.value.detail
```

- [ ] **Step 2: Run the targeted tests to verify they fail**

Run:

```powershell
uv run pytest tests/test_data_update.py -k "get_qlib_provider_dir" -v
```

Expected:

- `ImportError` or `AttributeError` because `get_qlib_provider_dir` does not exist yet

- [ ] **Step 3: Add the minimal shared helper in `workflow/shared.py`**

```python
import os
from pathlib import Path

from fastapi import HTTPException


def get_qlib_base_dir() -> Path:
    return Path(os.getenv("QLIB_DATA_DIR", "~/.qlib")).expanduser()


def get_qlib_provider_dir() -> Path:
    base_dir = get_qlib_base_dir()
    normalized_parts = [part.lower() for part in base_dir.parts]
    if normalized_parts[-1:] == ["us_data"]:
        raise HTTPException(
            status_code=500,
            detail=(
                "QLIB_DATA_DIR must be a base directory, not a direct provider path. "
                "Set QLIB_DATA_DIR to the base path and the service will use <base>/qlib/us_data."
            ),
        )
    return base_dir / "qlib" / "us_data"
```

- [ ] **Step 4: Run the targeted tests to verify they pass**

Run:

```powershell
uv run pytest tests/test_data_update.py -k "get_qlib_provider_dir" -v
```

Expected:

- all 3 selected tests `PASS`

- [ ] **Step 5: Commit**

```bash
git add services/model/workflow/shared.py services/model/tests/test_data_update.py
git commit -m "feat: add shared qlib provider path resolver"
```

### Task 2: Switch `/data/update` to the Shared Helper

**Files:**
- Modify: `services/model/workflow/data_update.py`
- Test: `services/model/tests/test_data_update.py`

- [ ] **Step 1: Write the failing `/data/update` path-contract tests**

```python
from pathlib import Path

from workflow import data_update


def test_update_data_uses_resolved_provider_dir(monkeypatch, tmp_path):
    base_dir = tmp_path / "market"
    provider = base_dir / "qlib" / "us_data"
    (provider / "calendars").mkdir(parents=True)
    (provider / "instruments").mkdir(parents=True)
    (provider / "features" / "aapl").mkdir(parents=True)
    (provider / "calendars" / "day.txt").write_text("2020-01-01\n2025-12-31\n", encoding="utf-8")
    (provider / "instruments" / "all.txt").write_text("AAPL\t2020-01-02\t2025-12-31\n", encoding="utf-8")
    np.array([0, 1.0, 2.0], dtype="<f4").tofile(provider / "features" / "aapl" / "close.day.bin")

    monkeypatch.setenv("QLIB_DATA_DIR", str(base_dir))
    monkeypatch.setattr(data_update.subprocess, "run", lambda command, **kwargs: subprocess.CompletedProcess(command, 0, "", ""))

    with TestClient(app) as client:
        resp = client.post("/data/update", json={"symbols": ["AAPL.US"]})

    assert resp.status_code == 200
    assert resp.json()["provider_uri"] == str(provider)


def test_update_data_rejects_old_style_qlib_data_dir(monkeypatch, tmp_path):
    monkeypatch.setenv("QLIB_DATA_DIR", str(tmp_path / "qlib_data" / "us_data"))

    with TestClient(app) as client:
        resp = client.post("/data/update", json={"symbols": ["AAPL.US"]})

    assert resp.status_code == 500
    assert "base directory" in resp.json()["detail"]
```

- [ ] **Step 2: Run the targeted tests to verify they fail**

Run:

```powershell
uv run pytest tests/test_data_update.py -k "resolved_provider_dir or old_style_qlib_data_dir" -v
```

Expected:

- first test fails because `/data/update` still resolves the old default directly
- second test fails because the old-style env value is not rejected yet

- [ ] **Step 3: Replace the local env lookup in `data_update.py`**

```python
from workflow.shared import get_qlib_provider_dir


def _provider_uri() -> Path:
    return get_qlib_provider_dir()
```

- [ ] **Step 4: Run the `/data/update` tests to verify they pass**

Run:

```powershell
uv run pytest tests/test_data_update.py -v
```

Expected:

- all tests in `tests/test_data_update.py` `PASS`

- [ ] **Step 5: Commit**

```bash
git add services/model/workflow/data_update.py services/model/tests/test_data_update.py
git commit -m "refactor: use shared qlib path in data update"
```

### Task 3: Switch `/features` to the Shared Helper

**Files:**
- Modify: `services/model/workflow/features.py`
- Test: `services/model/tests/test_models_features.py`

- [ ] **Step 1: Write the failing `/features` provider-path test**

```python
from pathlib import Path
from types import SimpleNamespace

from workflow import features


def test_features_uses_resolved_qlib_provider_dir(monkeypatch, tmp_path):
    calls = {}

    def fake_init(*, provider_uri, region):
        calls["provider_uri"] = provider_uri
        calls["region"] = region

    class FakeHandler:
        def __init__(self, instruments, infer_processors):
            self.instruments = instruments
            self.infer_processors = infer_processors

        def fetch(self):
            return make_feature_frame()

    monkeypatch.setenv("QLIB_DATA_DIR", str(tmp_path / "market"))
    monkeypatch.setitem(sys.modules, "qlib", SimpleNamespace(init=fake_init))
    monkeypatch.setitem(sys.modules, "qlib.constant", SimpleNamespace(REG_US="us"))
    monkeypatch.setitem(sys.modules, "qlib.contrib.data.handler", SimpleNamespace(Alpha158=FakeHandler))

    with TestClient(app) as client:
        resp = client.get("/features/AAPL.US")

    assert resp.status_code == 200
    assert calls["provider_uri"] == str(tmp_path / "market" / "qlib" / "us_data")
```

- [ ] **Step 2: Run the targeted test to verify it fails**

Run:

```powershell
uv run pytest tests/test_models_features.py -k "resolved_qlib_provider_dir" -v
```

Expected:

- test fails because `/features` still initializes qlib with the old direct env value

- [ ] **Step 3: Replace the local env lookup in `features.py`**

```python
from workflow.shared import get_qlib_provider_dir


qlib_dir = get_qlib_provider_dir()
qlib.init(provider_uri=str(qlib_dir), region=REG_US)
```

- [ ] **Step 4: Run the `/features` tests to verify they pass**

Run:

```powershell
uv run pytest tests/test_models_features.py -v
```

Expected:

- all tests in `tests/test_models_features.py` `PASS`

- [ ] **Step 5: Commit**

```bash
git add services/model/workflow/features.py services/model/tests/test_models_features.py
git commit -m "refactor: use shared qlib path in features"
```

### Task 4: Switch `/train` to the Shared Helper

**Files:**
- Modify: `services/model/workflow/train.py`
- Test: `services/model/tests/test_train.py`

- [ ] **Step 1: Write the failing `/train` provider-path tests**

```python
def test_train_uses_resolved_qlib_provider_dir(monkeypatch, tmp_path):
    models_dir = prepare_train_env(monkeypatch)
    install_fake_qlib()
    captured = {}

    import qlib

    def fake_init(*, provider_uri, region):
        captured["provider_uri"] = provider_uri
        captured["region"] = region

    monkeypatch.setattr(qlib, "init", fake_init)
    monkeypatch.setenv("QLIB_DATA_DIR", str(tmp_path / "market"))

    try:
        with TestClient(app) as client:
            resp = client.post(
                "/train",
                json={"symbol": "AAPL.US", "model_type": "alstm", "start": "2020-01-01", "end": "2023-03-01"},
            )
        assert resp.status_code == 200
        assert captured["provider_uri"] == str(tmp_path / "market" / "qlib" / "us_data")
    finally:
        uninstall_fake_qlib()


def test_train_rejects_old_style_qlib_data_dir(monkeypatch, tmp_path):
    prepare_train_env(monkeypatch)
    install_fake_qlib()
    monkeypatch.setenv("QLIB_DATA_DIR", str(tmp_path / "qlib_data" / "us_data"))

    try:
        with TestClient(app) as client:
            resp = client.post(
                "/train",
                json={"symbol": "AAPL.US", "model_type": "alstm", "start": "2020-01-01", "end": "2023-03-01"},
            )
        assert resp.status_code == 500
        assert "base directory" in resp.json()["detail"]
    finally:
        uninstall_fake_qlib()
```

- [ ] **Step 2: Run the targeted `/train` tests to verify they fail**

Run:

```powershell
uv run pytest tests/test_train.py -k "resolved_qlib_provider_dir or old_style_qlib_data_dir" -v
```

Expected:

- one or both tests fail because `train.py` still reads `QLIB_DATA_DIR` directly

- [ ] **Step 3: Replace the local fallback env lookup in `train.py`**

```python
from workflow.shared import get_qlib_provider_dir


    qlib_dir = get_models_dir().parent / ".qlib"
    qlib_dir = qlib_dir if qlib_dir.exists() else None
    provider_uri = str(qlib_dir) if qlib_dir else None
    if provider_uri is None:
        provider_uri = str(get_qlib_provider_dir())
```

- [ ] **Step 4: Run the `/train` tests to verify they pass**

Run:

```powershell
uv run pytest tests/test_train.py -v
```

Expected:

- all non-skipped tests in `tests/test_train.py` `PASS`
- the integration case that requires a real Yahoo provider may still `SKIP`

- [ ] **Step 5: Commit**

```bash
git add services/model/workflow/train.py services/model/tests/test_train.py
git commit -m "refactor: use shared qlib path in training"
```

### Task 5: Update Operator Docs and Run Full Regression

**Files:**
- Modify: `docs/runbook.md`
- Test: `services/model/tests/test_data_update.py`
- Test: `services/model/tests/test_models_features.py`
- Test: `services/model/tests/test_train.py`

- [ ] **Step 1: Write the documentation change**

```md
If you need to explicitly set the local Qlib base directory, set:

```powershell
$env:QLIB_DATA_DIR = 'C:\Users\Hi\.qlib'
```

The model service resolves the actual provider path as:

```text
<QLIB_DATA_DIR>\qlib\us_data
```

Do not set `QLIB_DATA_DIR` directly to `...\us_data`.
```

- [ ] **Step 2: Update `docs/runbook.md`**

Replace the old example:

```powershell
$env:QLIB_DATA_DIR = 'C:\Users\Hi\.qlib\qlib_data\us_data'
```

With the new example:

```powershell
$env:QLIB_DATA_DIR = 'C:\Users\Hi\.qlib'
```

And add the explicit note that the provider path becomes `<QLIB_DATA_DIR>/qlib/us_data`.

- [ ] **Step 3: Run the model-service regression suite**

Run:

```powershell
uv run pytest tests/test_data_update.py tests/test_models_features.py tests/test_train.py -v
```

Expected:

- all selected tests `PASS`
- any real-provider integration case remains an explicit `SKIP`

- [ ] **Step 4: Run the full model-service test suite**

Run:

```powershell
uv run pytest tests -v
```

Expected:

- full suite passes
- only known integration/provider-dependent cases skip

- [ ] **Step 5: Commit**

```bash
git add docs/runbook.md services/model/workflow/shared.py services/model/workflow/data_update.py services/model/workflow/features.py services/model/workflow/train.py services/model/tests/test_data_update.py services/model/tests/test_models_features.py services/model/tests/test_train.py
git commit -m "docs: update qlib data dir base path contract"
```

## Self-Review

- Spec coverage check:
  - base-dir semantics: covered in Tasks 1, 5
  - shared helper: covered in Task 1
  - `data_update.py`, `features.py`, `train.py`: covered in Tasks 2, 3, 4
  - strict rejection of old `.../us_data` env values: covered in Tasks 1, 2, 4
  - doc updates: covered in Task 5
- Placeholder scan:
  - no `TODO`, `TBD`, or deferred implementation language remains
- Type consistency:
  - helper name is consistently `get_qlib_provider_dir`
  - provider path contract is consistently `<base>/qlib/us_data`
