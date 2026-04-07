import pytest
from fastapi.testclient import TestClient
from main import app

client = TestClient(app)

SAMPLE_BARS = [
    {"ts_ms": 1700000000000 + i * 86400000,
     "open": 180.0 + i * 0.1, "high": 182.0, "low": 179.0,
     "close": 181.0 + i * 0.05, "volume": 50_000_000}
    for i in range(60)
]

def test_predict_missing_model_returns_404():
    resp = client.post("/predict", json={
        "symbol": "NONEXISTENT.US",
        "model_type": "lstm",
        "bars": SAMPLE_BARS,
    })
    assert resp.status_code == 404

def test_predict_too_few_bars_returns_422():
    resp = client.post("/predict", json={
        "symbol": "AAPL.US",
        "model_type": "lstm",
        "bars": SAMPLE_BARS[:5],  # only 5, need 60
    })
    assert resp.status_code == 422

def test_predict_schema():
    """Response shape is correct when a saved model exists (smoke test with mock)."""
    import torch, os
    from pathlib import Path
    # Create a trivial saved model stub for testing
    models_dir = Path(os.getenv("LSTM_MODELS_DIR", "models"))
    models_dir.mkdir(parents=True, exist_ok=True)
    stub_path = models_dir / "AAPL_US_lstm.pt"
    # save minimal state dict
    from qlib_pipeline.predict import _SimpleLSTM
    import torch.nn as nn
    m = _SimpleLSTM()
    torch.save({"model_state": m.state_dict(), "model_type": "lstm",
                "input_size": 158, "hidden_size": 64, "num_layers": 2,
                "lookback": 60}, stub_path)
    try:
        resp = client.post("/predict", json={
            "symbol": "AAPL.US",
            "model_type": "lstm",
            "bars": SAMPLE_BARS,
        })
        assert resp.status_code == 200
        data = resp.json()
        assert "score" in data
        assert "side" in data
        assert data["side"] in ("buy", "sell", "hold")
        assert "confidence" in data
    finally:
        stub_path.unlink(missing_ok=True)
