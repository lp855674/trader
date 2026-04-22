from __future__ import annotations

import json

import torch
from fastapi.testclient import TestClient

from main import app
from runtime.networks import ALSTM
from tests.support import cleanup_case_dir, make_case_dir


def write_stub_artifact(models_dir, symbol: str, model_type: str):
    artifact_dir = models_dir / f"{symbol.replace('.', '_')}_{model_type}"
    artifact_dir.mkdir(parents=True, exist_ok=True)
    torch.save(
        {
            "model_state": ALSTM().state_dict(),
            "model_type": model_type,
            "input_size": 158,
            "hidden_size": 64,
            "num_layers": 2,
            "lookback": 60,
            "symbol": symbol,
            "trained_at": "2026-04-07T10:00:00Z",
            "metrics": {"ic": 0.05, "icir": 0.4, "sharpe": 1.2, "annualized_return": 0.15},
        },
        artifact_dir / "model.pt",
    )
    (artifact_dir / "metadata.json").write_text(
        json.dumps(
            {
                "model_id": "model_AAPL_US_alstm_20260407",
                "model_type": model_type,
                "symbol_universe": [symbol],
                "feature_set": "Alpha158",
                "lookback": 60,
                "prediction_semantics": "score in [-1,1]",
            }
        ),
        encoding="utf-8",
    )


def test_get_models_lists_saved(monkeypatch):
    models_dir = make_case_dir("models_list")
    monkeypatch.setenv("MODEL_ARTIFACTS_DIR", str(models_dir))
    monkeypatch.delenv("LSTM_MODELS_DIR", raising=False)
    try:
        write_stub_artifact(models_dir, "AAPL.US", "alstm")
        with TestClient(app) as client:
            resp = client.get("/models")
            assert resp.status_code == 200
            data = resp.json()
            assert isinstance(data, list)
            assert any(model["model_id"] == "model_AAPL_US_alstm_20260407" for model in data)
            assert any(model["symbols"] == ["AAPL.US"] for model in data)
    finally:
        cleanup_case_dir(models_dir)


def test_features_no_data_returns_404():
    with TestClient(app) as client:
        resp = client.get("/features/NONEXISTENT.US")
        assert resp.status_code == 404


def test_backtest_missing_model_returns_404(monkeypatch):
    models_dir = make_case_dir("missing_models")
    monkeypatch.setenv("MODEL_ARTIFACTS_DIR", str(models_dir))
    monkeypatch.delenv("LSTM_MODELS_DIR", raising=False)
    try:
        with TestClient(app) as client:
            resp = client.post(
                "/backtest",
                json={
                    "symbol": "NONEXISTENT.US",
                    "model_type": "lstm",
                    "start": "2025-01-01",
                    "end": "2025-06-01",
                },
            )
            assert resp.status_code == 404
    finally:
        cleanup_case_dir(models_dir)
