1. `cd services/model`
2. `uv sync`
3. Optional: set a custom artifact directory
   * Windows PowerShell: `$env:MODEL_ARTIFACTS_DIR = '.\\models'`
   * macOS/Linux: `export MODEL_ARTIFACTS_DIR=./models`
4. `uv run uvicorn main:app --host 127.0.0.1 --port 8000`
