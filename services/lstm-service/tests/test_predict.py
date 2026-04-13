from pathlib import Path
import shutil

from fastapi.testclient import TestClient

from main import app

SAMPLE_BARS = [
    {"ts_ms": 1700000000000 + i * 86400000,
     "open": 180.0 + i * 0.1, "high": 182.0, "low": 179.0,
     "close": 181.0 + i * 0.05, "volume": 50_000_000}
    for i in range(60)
]

PREDICT_TEST_DIR = Path(__file__).resolve().parent / "predict_tmp"


def _prepare_models_dir():
    shutil.rmtree(PREDICT_TEST_DIR, ignore_errors=True)
    PREDICT_TEST_DIR.mkdir(parents=True, exist_ok=True)
    return PREDICT_TEST_DIR


def _setup_predict_env(monkeypatch):
    models_dir = _prepare_models_dir()
    monkeypatch.setenv("LSTM_MODELS_DIR", str(models_dir))
    return models_dir


def _clean_models_dir(models_dir: Path):
    shutil.rmtree(models_dir, ignore_errors=True)


def test_predict_missing_model_returns_404(monkeypatch):
    models_dir = _setup_predict_env(monkeypatch)
    try:
        with TestClient(app) as client:
            resp = client.post("/predict", json={
                "symbol": "NONEXISTENT.US",
                "model_type": "lstm",
                "bars": SAMPLE_BARS,
            })
            assert resp.status_code == 404
    finally:
        _clean_models_dir(models_dir)


def test_predict_too_few_bars_returns_422(monkeypatch):
    models_dir = _setup_predict_env(monkeypatch)
    try:
        with TestClient(app) as client:
            resp = client.post("/predict", json={
                "symbol": "AAPL.US",
                "model_type": "lstm",
                "bars": SAMPLE_BARS[:5],  # only 5, need 60
            })
            assert resp.status_code == 422
    finally:
        _clean_models_dir(models_dir)


def test_predict_schema(monkeypatch):
    """Response shape is correct when a saved model exists (smoke test with mock)."""
    from qlib_pipeline.predict import _SimpleLSTM
    import torch

    models_dir = _setup_predict_env(monkeypatch)
    stub_path = models_dir / "AAPL_US_lstm.pt"
    m = _SimpleLSTM()
    torch.save({"model_state": m.state_dict(), "model_type": "lstm",
                "input_size": 158, "hidden_size": 64, "num_layers": 2,
                "lookback": 60}, stub_path)
    try:
        with TestClient(app) as client:
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
        _clean_models_dir(models_dir)


def test_predict_schema_alstm(monkeypatch):
    """ALSTM model loads and predicts correctly."""
    from qlib_pipeline.predict import _ALSTM
    import torch

    models_dir = _setup_predict_env(monkeypatch)
    stub_path = models_dir / "AAPL_US_alstm.pt"
    m = _ALSTM()
    torch.save({"model_state": m.state_dict(), "model_type": "alstm",
                "input_size": 158, "hidden_size": 64, "num_layers": 2,
                "lookback": 60}, stub_path)
    try:
        with TestClient(app) as client:
            resp = client.post("/predict", json={
                "symbol": "AAPL.US",
                "model_type": "alstm",
                "bars": SAMPLE_BARS,
            })
            assert resp.status_code == 200
            data = resp.json()
            assert data["side"] in ("buy", "sell", "hold")
    finally:
        _clean_models_dir(models_dir)


def test_predict_rejects_path_traversal(monkeypatch):
    models_dir = _setup_predict_env(monkeypatch)
    try:
        with TestClient(app) as client:
            resp = client.post("/predict", json={
                "symbol": "../secret",
                "model_type": "lstm",
                "bars": SAMPLE_BARS,
            })
            assert resp.status_code == 422
            assert "Invalid symbol" in resp.json()["detail"]
    finally:
        _clean_models_dir(models_dir)
