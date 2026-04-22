from __future__ import annotations

import json

import torch
from fastapi.testclient import TestClient

from main import app
from runtime.networks import ALSTM, SimpleLSTM
from tests.support import cleanup_case_dir, make_case_dir

SAMPLE_BARS = [
    {
        "ts_ms": 1700000000000 + index * 86400000,
        "open": 180.0 + index * 0.1,
        "high": 182.0,
        "low": 179.0,
        "close": 181.0 + index * 0.05,
        "volume": 50_000_000,
    }
    for index in range(60)
]

def setup_predict_env(monkeypatch):
    models_dir = make_case_dir("predict_models")
    monkeypatch.setenv("MODEL_ARTIFACTS_DIR", str(models_dir))
    monkeypatch.delenv("LSTM_MODELS_DIR", raising=False)
    return models_dir


def test_predict_missing_model_returns_404(monkeypatch):
    models_dir = setup_predict_env(monkeypatch)
    try:
        with TestClient(app) as client:
            resp = client.post(
                "/predict",
                json={"symbol": "NONEXISTENT.US", "model_type": "lstm", "bars": SAMPLE_BARS},
            )
            assert resp.status_code == 404
    finally:
        cleanup_case_dir(models_dir)


def test_predict_too_few_bars_returns_422(monkeypatch):
    models_dir = setup_predict_env(monkeypatch)
    try:
        with TestClient(app) as client:
            resp = client.post(
                "/predict",
                json={"symbol": "AAPL.US", "model_type": "lstm", "bars": SAMPLE_BARS[:5]},
            )
            assert resp.status_code == 422
    finally:
        cleanup_case_dir(models_dir)


def test_predict_schema_with_legacy_checkpoint(monkeypatch):
    models_dir = setup_predict_env(monkeypatch)
    torch.save(
        {
            "model_state": SimpleLSTM().state_dict(),
            "model_type": "lstm",
            "input_size": 158,
            "hidden_size": 64,
            "num_layers": 2,
            "lookback": 60,
        },
        models_dir / "AAPL_US_lstm.pt",
    )
    try:
        with TestClient(app) as client:
            resp = client.post(
                "/predict",
                json={"symbol": "AAPL.US", "model_type": "lstm", "bars": SAMPLE_BARS},
            )
            assert resp.status_code == 200
            data = resp.json()
            assert data["side"] in ("buy", "sell", "hold")
            assert "score" in data
            assert "confidence" in data
    finally:
        cleanup_case_dir(models_dir)


def test_predict_schema_with_artifact_dir(monkeypatch):
    models_dir = setup_predict_env(monkeypatch)
    artifact_dir = models_dir / "aapl_us_alstm"
    artifact_dir.mkdir(parents=True, exist_ok=True)
    torch.save(
        {
            "model_state": ALSTM().state_dict(),
            "model_type": "alstm",
            "input_size": 158,
            "hidden_size": 64,
            "num_layers": 2,
            "lookback": 60,
        },
        artifact_dir / "model.pt",
    )
    (artifact_dir / "metadata.json").write_text(
        json.dumps(
            {
                "model_id": "model_AAPL_US_alstm_20260422",
                "model_type": "alstm",
                "symbol_universe": ["AAPL.US"],
                "feature_set": "Alpha360",
                "lookback": 60,
                "prediction_semantics": "score in [-1,1]",
            }
        ),
        encoding="utf-8",
    )
    try:
        with TestClient(app) as client:
            resp = client.post(
                "/predict",
                json={"symbol": "AAPL.US", "model_type": "alstm", "bars": SAMPLE_BARS},
            )
            assert resp.status_code == 200
            assert resp.json()["side"] in ("buy", "sell", "hold")
    finally:
        cleanup_case_dir(models_dir)


def test_predict_rejects_path_traversal(monkeypatch):
    models_dir = setup_predict_env(monkeypatch)
    try:
        with TestClient(app) as client:
            resp = client.post(
                "/predict",
                json={"symbol": "../secret", "model_type": "lstm", "bars": SAMPLE_BARS},
            )
            assert resp.status_code == 422
            assert "Invalid symbol" in resp.json()["detail"]
    finally:
        cleanup_case_dir(models_dir)
