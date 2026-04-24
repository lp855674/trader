Set-Location E:\code\trader\services\model
$env:MODEL_ARTIFACTS_DIR = '.\models'
uv run uvicorn main:app --host 127.0.0.1 --port 8000
