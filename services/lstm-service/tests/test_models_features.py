import torch
import torch.nn as nn
from pathlib import Path
import os
from fastapi.testclient import TestClient
from main import app

client = TestClient(app)
MODELS_DIR = Path(os.getenv("LSTM_MODELS_DIR", "models"))


def _write_stub(symbol, model_type):
    from qlib_pipeline.predict import _SimpleLSTM, _ALSTM
    MODELS_DIR.mkdir(parents=True, exist_ok=True)
    safe = symbol.replace(".", "_")
    path = MODELS_DIR / f"{safe}_{model_type}.pt"
    m = _SimpleLSTM() if model_type != "alstm" else _ALSTM()
    torch.save({
        "model_state": m.state_dict(), "model_type": model_type,
        "input_size": 158, "hidden_size": 64, "num_layers": 2,
        "lookback": 60, "symbol": symbol,
        "trained_at": "2026-04-07T10:00:00",
        "metrics": {"ic": 0.05, "icir": 0.4, "sharpe": 1.2, "annualized_return": 0.15},
    }, path)
    return path


def test_get_models_lists_saved():
    path = _write_stub("AAPL.US", "alstm")
    try:
        resp = client.get("/models")
        assert resp.status_code == 200
        data = resp.json()
        assert isinstance(data, list)
        symbols = [m["symbol"] for m in data]
        assert "AAPL.US" in symbols
    finally:
        path.unlink(missing_ok=True)


def test_features_no_data_returns_404():
    resp = client.get("/features/NONEXISTENT.US")
    assert resp.status_code == 404


def test_backtest_missing_model_returns_404():
    resp = client.post("/backtest", json={
        "symbol": "NONEXISTENT.US",
        "model_type": "lstm",
        "start": "2025-01-01",
        "end": "2025-06-01",
    })
    assert resp.status_code == 404
