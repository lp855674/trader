Set-Location E:\code\trader\services\lstm-service
$env:LSTM_MODELS_DIR = '.\models'
uv run uvicorn main:app --host 127.0.0.1 --port 8000
