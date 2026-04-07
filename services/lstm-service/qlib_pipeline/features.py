# services/lstm-service/qlib_pipeline/features.py
from __future__ import annotations

import os
from pathlib import Path

from fastapi import APIRouter, HTTPException

MODELS_DIR = Path(os.getenv("LSTM_MODELS_DIR", "models"))

router = APIRouter()


@router.get("/features/{symbol}")
async def get_features(symbol: str) -> dict:
    """Return the latest Alpha158 feature vector for a symbol (diagnostic endpoint).
    Returns 404 if Qlib is not available or no data found for the symbol."""
    try:
        import qlib
        from qlib.constant import REG_US
        from qlib.contrib.data.handler import Alpha158

        qlib_dir = Path(os.getenv("QLIB_DATA_DIR", "~/.qlib/qlib_data/us_data")).expanduser()
        qlib.init(provider_uri=str(qlib_dir), region=REG_US)

        qlib_symbol = symbol.split(".")[0]
        handler = Alpha158(instruments=[qlib_symbol], infer_processors=[])
        df = handler.fetch()
        if df.empty:
            raise HTTPException(status_code=404, detail=f"No data for {symbol}")

        last = df.iloc[-1]
        ts_ms = int(last.name[0].timestamp() * 1000) if hasattr(last.name[0], "timestamp") else 0
        alpha = {k: round(float(v), 6) for k, v in last.drop("label", errors="ignore").items()}
        return {"symbol": symbol, "ts_ms": ts_ms, "alpha158": alpha}

    except ImportError:
        raise HTTPException(status_code=404, detail=f"No data for {symbol} (qlib not available)")
    except HTTPException:
        raise
    except Exception as exc:
        raise HTTPException(status_code=404, detail=str(exc))
