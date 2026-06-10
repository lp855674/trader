$ErrorActionPreference = "Stop"

$targetDir = $env:TRADER_SMOKE_TARGET_DIR
if (-not $targetDir) {
    $targetDir = Join-Path $env:TEMP "trader-smoke-target"
}

$env:CARGO_TARGET_DIR = $targetDir
$databasePath = Join-Path $env:TEMP ("trader-server-smoke-{0}.sqlite" -f [guid]::NewGuid().ToString("N"))
$env:TRADER_DATABASE_URL = "sqlite://$databasePath"
$stdoutPath = Join-Path $env:TEMP ("trader-server-smoke-{0}.out.log" -f [guid]::NewGuid().ToString("N"))
$stderrPath = Join-Path $env:TEMP ("trader-server-smoke-{0}.err.log" -f [guid]::NewGuid().ToString("N"))

$env:CARGO_BUILD_JOBS = if ($env:CARGO_BUILD_JOBS) { $env:CARGO_BUILD_JOBS } else { "1" }
cargo build -p trader-server
if ($LASTEXITCODE -ne 0) {
    throw "cargo build -p trader-server failed with exit code $LASTEXITCODE"
}

$serverExe = Join-Path $targetDir "debug\trader-server.exe"
$server = Start-Process -FilePath $serverExe `
    -WorkingDirectory (Get-Location) `
    -PassThru `
    -RedirectStandardOutput $stdoutPath `
    -RedirectStandardError $stderrPath `
    -WindowStyle Hidden

try {
    $ready = $false
    for ($i = 0; $i -lt 80; $i++) {
        if ($server.HasExited) { break }
        Start-Sleep -Milliseconds 500
        try {
            Invoke-RestMethod "http://127.0.0.1:8080/api/v1/health" | Out-Null
            $ready = $true
            break
        } catch {}
    }
    if (-not $ready) {
        if (Test-Path $stdoutPath) {
            Get-Content $stdoutPath
        }
        if (Test-Path $stderrPath) {
            Get-Content $stderrPath
        }
        throw "trader-server did not become ready"
    }

    powershell -ExecutionPolicy Bypass -File ".\scripts\rest-smoke.ps1"
} finally {
    if ($server -and -not $server.HasExited) {
        Stop-Process -Id $server.Id -Force
    }
}
