from __future__ import annotations

import os
from pathlib import Path

from fastapi import APIRouter, HTTPException

router = APIRouter()


@router.get("/features/{symbol}")
async def get_features(symbol: str) -> dict:
    try:
        import qlib
        from qlib.constant import REG_US
        from qlib.contrib.data.handler import Alpha158
        qlib_dir = Path(os.getenv("QLIB_DATA_DIR", "~/.qlib/qlib_data/us_data")).expanduser()
        qlib.init(provider_uri=str(qlib_dir), region=REG_US)
        qlib_symbol = symbol.split(".")[0]
        handler = Alpha158(instruments=[qlib_symbol], infer_processors=[])
        data_frame = handler.fetch()
        if data_frame.empty:
            raise HTTPException(status_code=404, detail=f"No data for {symbol}")

        last = data_frame.iloc[-1]
        ts_ms = int(last.name[0].timestamp() * 1000) if hasattr(last.name[0], "timestamp") else 0
        alpha158 = {key: round(float(value), 6) for key, value in last.drop("label", errors="ignore").items()}
        return {"symbol": symbol, "ts_ms": ts_ms, "alpha158": alpha158}
    except ImportError:
        raise HTTPException(status_code=404, detail=f"No data for {symbol} (qlib not available)")
    except HTTPException:
        raise
    except Exception as exc:
        raise HTTPException(status_code=404, detail=str(exc))
