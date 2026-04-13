1. `cd services/lstm-service`
2. `uv sync`
3. (Optional) Point the service at a custom models directory:
   * Windows PowerShell: `$env:LSTM_MODELS_DIR = './models'`
   * macOS/Linux: `export LSTM_MODELS_DIR=./models`
   The directory will be created automatically.
4. `uv run uvicorn main:app --host 127.0.0.1 --port 8000`
