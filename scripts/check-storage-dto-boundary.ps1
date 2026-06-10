$ErrorActionPreference = "Stop"

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
Set-Location $repoRoot

$patterns = @(
    "NewOrder\s*\{",
    "NewFill\s*\{",
    "NewPortfolioSnapshot\s*\{",
    "NewEventRecord\s*\{",
    "NewAccountBalance\s*\{",
    "NewPosition\s*\{",
    "NewStrategyRun\s*\{",
    "StoredRuntimeEvent\s*\{",
    "BacktestExecutionRecord\s*\{",
    "BacktestPositionRecord\s*\{",
    "\.insert_strategy_run\(",
    "\.insert_order\(",
    "\.insert_fill\(",
    "\.insert_event\(",
    "\.insert_portfolio_snapshot\(",
    "\.upsert_account_balance\(",
    "\.upsert_position\("
)

$pattern = ($patterns -join "|")
$regex = [regex]$pattern
$violations = @()

$files = Get-ChildItem -Path crates, apps -Recurse -Filter *.rs |
    Where-Object { $_.FullName -notmatch "\\crates\\storage\\" }

foreach ($file in $files) {
    $relativePath = Resolve-Path -Relative $file.FullName
    $inTestModule = $false
    $lineNumber = 0
    foreach ($line in Get-Content $file.FullName) {
        $lineNumber += 1
        if ($line -match "#\[cfg\(test\)\]") {
            $inTestModule = $true
        }
        if ($inTestModule) {
            continue
        }
        if ($regex.IsMatch($line)) {
            $violations += "${relativePath}:${lineNumber}:$line"
        }
    }
}

if ($violations.Count -gt 0) {
    Write-Host "Storage DTO boundary violation found outside storage:"
    Write-Host $violations
    exit 1
}

Write-Host "Storage DTO boundary check passed."
