from __future__ import annotations

import json
import sys
from types import SimpleNamespace

import numpy as np
import pytest
from fastapi.testclient import TestClient

from main import app
from tests.support import cleanup_case_dir, make_case_dir

class FakeDataFrame:
    fail_symbol: str | None = None

    def __init__(self):
        self.columns = ["label"]
        self._values = np.ones((70, 158), dtype=np.float32)
        self._label = np.ones(70, dtype=np.float32)
        self.empty = False

    def xs(self, symbol, level):
        if self.fail_symbol and symbol == self.fail_symbol:
            raise KeyError(symbol)
        return self

    def drop(self, columns, errors="ignore"):
        return self

    @property
    def values(self):
        return self._values

    def __getitem__(self, key):
        if key == "label":
            class LabelSeries:
                def __init__(self, values):
                    self.values = values

            return LabelSeries(self._label)
        raise KeyError

    def __len__(self):
        return self._values.shape[0]


class FakeAlpha158:
    last_args: dict[str, str] | None = None

    def __init__(
        self,
        instruments,
        start_time,
        end_time,
        fit_start_time,
        fit_end_time,
        infer_processors,
        learn_processors,
    ):
        FakeAlpha158.last_args = {
            "start_time": start_time,
            "end_time": end_time,
            "fit_start_time": fit_start_time,
            "fit_end_time": fit_end_time,
        }

    def fetch(self):
        return FakeDataFrame()


def install_fake_qlib():
    fake_handler_module = SimpleNamespace(Alpha158=FakeAlpha158)
    fake_data_module = SimpleNamespace(handler=fake_handler_module)
    fake_contrib_module = SimpleNamespace(data=fake_data_module)
    fake_constant = SimpleNamespace(REG_US="US_TEST_REGION")
    fake_qlib = SimpleNamespace(
        init=lambda provider_uri, region: None,
        constant=fake_constant,
        contrib=fake_contrib_module,
    )
    sys.modules["qlib"] = fake_qlib
    sys.modules["qlib.constant"] = fake_constant
    sys.modules["qlib.contrib"] = fake_contrib_module
    sys.modules["qlib.contrib.data"] = fake_data_module
    sys.modules["qlib.contrib.data.handler"] = fake_handler_module


def uninstall_fake_qlib():
    for module in ["qlib", "qlib.constant", "qlib.contrib", "qlib.contrib.data", "qlib.contrib.data.handler"]:
        sys.modules.pop(module, None)


def prepare_train_env(monkeypatch):
    models_dir = make_case_dir("train_models")
    monkeypatch.setenv("MODEL_ARTIFACTS_DIR", str(models_dir))
    monkeypatch.delenv("LSTM_MODELS_DIR", raising=False)
    return models_dir


def test_train_unknown_model_type_returns_422(monkeypatch):
    models_dir = prepare_train_env(monkeypatch)
    try:
        with TestClient(app) as client:
            resp = client.post(
                "/train",
                json={
                    "symbol": "AAPL.US",
                    "model_type": "unknown_model",
                    "start": "2023-01-01",
                    "end": "2023-12-31",
                },
            )
            assert resp.status_code == 422
    finally:
        cleanup_case_dir(models_dir)


def test_train_symbol_validation(monkeypatch):
    models_dir = prepare_train_env(monkeypatch)
    try:
        with TestClient(app) as client:
            resp = client.post(
                "/train",
                json={
                    "symbol": "../../secret",
                    "model_type": "lstm",
                    "start": "2023-02-01",
                    "end": "2023-03-01",
                },
            )
            assert resp.status_code == 422
            assert "Invalid symbol" in resp.json()["detail"]
    finally:
        cleanup_case_dir(models_dir)


def test_train_uses_requested_date_range_and_writes_artifact(monkeypatch):
    models_dir = prepare_train_env(monkeypatch)
    install_fake_qlib()
    try:
        with TestClient(app) as client:
            resp = client.post(
                "/train",
                json={
                    "symbol": "AAPL.US",
                    "model_type": "lstm",
                    "start": "2023-02-01",
                    "end": "2023-03-01",
                },
            )
            assert resp.status_code == 200
            assert FakeAlpha158.last_args
            assert FakeAlpha158.last_args["start_time"] == "2023-02-01"
            assert FakeAlpha158.last_args["end_time"] == "2023-03-01"
            model_id = resp.json()["model_id"]
            artifact_dir = models_dir / "AAPL_US_lstm"
            assert artifact_dir.exists()
            assert (artifact_dir / "model.pt").exists()
            metadata = json.loads((artifact_dir / "metadata.json").read_text(encoding="utf-8"))
            assert metadata["model_id"] == model_id
            assert metadata["symbol_universe"] == ["AAPL.US"]
    finally:
        uninstall_fake_qlib()
        cleanup_case_dir(models_dir)


def test_train_missing_symbol_returns_422(monkeypatch):
    models_dir = prepare_train_env(monkeypatch)
    FakeDataFrame.fail_symbol = "NOSYMBOL"
    install_fake_qlib()
    try:
        with TestClient(app) as client:
            resp = client.post(
                "/train",
                json={
                    "symbol": "NOSYMBOL.US",
                    "model_type": "lstm",
                    "start": "2023-02-01",
                    "end": "2023-03-01",
                },
            )
            assert resp.status_code == 422
            assert "NOSYMBOL" in resp.json().get("detail", "")
    finally:
        FakeDataFrame.fail_symbol = None
        uninstall_fake_qlib()
        cleanup_case_dir(models_dir)


def test_train_response_schema():
    pytest.skip("Integration test: requires Qlib Yahoo data provider")
