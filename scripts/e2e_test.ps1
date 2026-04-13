# Start lstm-service first (in background)
$lstmJob = Start-Job -ScriptBlock {
    Set-Location "E:\code\trader\services\lstm-service"
    python -m uvicorn main:app --port 8000 --host 127.0.0.1
}

# Wait for lstm-service to start
Start-Sleep -Seconds 3

# Test lstm-service
$test = Invoke-RestMethod -Uri "http://127.0.0.1:8000/health" -Method Get
Write-Host "lstm-service: $($test.status)"

# Start quantd
$quantdJob = Start-Job -ScriptBlock {
    Set-Location "E:\code\trader"
    $env:QUANTD_ACCOUNT_ID = "acc_lb_paper"
    $env:QUANTD_DATABASE_URL = "sqlite:quantd.db"
    cargo run -p quantd
}

# Wait for quantd to start
Start-Sleep -Seconds 5

# Send tick
$body = @{
    venue = "US_EQUITY"
    symbol = "AAPL.US"
    account_id = "acc_lb_paper"
} | ConvertTo-Json

$resp = Invoke-RestMethod -Uri "http://127.0.0.1:8080/v1/tick" -Method Post -Body $body -ContentType "application/json"
Write-Host "Tick response: $($resp | ConvertTo-Json)"
