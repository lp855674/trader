# Test LSTM strategy end-to-end
$env:QUANTD_ACCOUNT_ID = "acc_lb_paper"
$env:QUANTD_DATABASE_URL = "sqlite:quantd.db"
$env:RUST_LOG = "info"

# Start quantd in background job
$quantdJob = Start-Job -ScriptBlock {
    Set-Location "E:\code\trader"
    cargo run -p quantd
} 

# Wait for quantd to start
Start-Sleep -Seconds 5

# Send tick request
try {
    $body = @{
        venue = "US_EQUITY"
        symbol = "AAPL.US"
        account_id = "acc_lb_paper"
    } | ConvertTo-Json

    $resp = Invoke-RestMethod -Uri "http://127.0.0.1:8080/v1/tick" -Method Post -Body $body -ContentType "application/json"
    Write-Host "Tick Response: $($resp | ConvertTo-Json)"
} catch {
    Write-Host "Error: $_"
}

# Show some quantd logs
Get-Job -Id $quantdJob.Id | Receive-Job -Keep
