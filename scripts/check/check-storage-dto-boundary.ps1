$ErrorActionPreference = "Stop"

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..\..")
Set-Location $repoRoot

$patterns = @(
    "\bNewOrder\b",
    "\bNewFill\b",
    "\bNewLotSizeRule\b",
    "\bNewPriceLimitRule\b",
    "\bNewCryptoPosition\b",
    "\bNewFundingRate\b",
    "\bNewCryptoMarketMeta\b",
    "\bNewCorporateActionMeta\b",
    "\bNewCashSnapshot\b",
    "\bNewPositionSnapshot\b",
    "\bNewConfigRecord\b",
    "\bNewSystemLog\b",
    "NewOrder\s*\{",
    "NewFill\s*\{",
    "NewPortfolioSnapshot\s*\{",
    "NewEventRecord\s*\{",
    "NewAccountBalance\s*\{",
    "NewPosition\s*\{",
    "NewLotSizeRule\s*\{",
    "NewPriceLimitRule\s*\{",
    "NewCryptoPosition\s*\{",
    "NewFundingRate\s*\{",
    "NewCryptoMarketMeta\s*\{",
    "NewCorporateActionMeta\s*\{",
    "NewCashSnapshot\s*\{",
    "NewPositionSnapshot\s*\{",
    "NewConfigRecord\s*\{",
    "NewSystemLog\s*\{",
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
    "\.upsert_position\(",
    "\.insert_lot_size_rule\(",
    "\.insert_price_limit_rule\(",
    "\.upsert_crypto_position\(",
    "\.upsert_funding_rate\(",
    "\.upsert_crypto_market_meta\(",
    "\.insert_corporate_action_meta\(",
    "\.insert_cash_snapshot\(",
    "\.insert_position_snapshot\(",
    "\.upsert_config\(",
    "\.insert_system_log\("
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

$storageReadRegex = [regex]"StorageResult<.*New(Order|Fill|Position|AccountBalance|PortfolioSnapshot|StrategyRun|EventRecord|LotSizeRule|PriceLimitRule|CryptoPosition|FundingRate|CryptoMarketMeta|CorporateActionMeta|CashSnapshot|PositionSnapshot|ConfigRecord|SystemLog)"
$storageReadViolations = @()

$storageFiles = Get-ChildItem -Path crates/storage/src -Recurse -Filter *.rs
foreach ($file in $storageFiles) {
    $relativePath = Resolve-Path -Relative $file.FullName
    $lineNumber = 0
    foreach ($line in Get-Content $file.FullName) {
        $lineNumber += 1
        if ($storageReadRegex.IsMatch($line)) {
            $storageReadViolations += "${relativePath}:${lineNumber}:$line"
        }
    }
}

if ($storageReadViolations.Count -gt 0) {
    Write-Host "Storage read API returns write DTO type:"
    Write-Host $storageReadViolations
    exit 1
}

Write-Host "Storage DTO boundary check passed."
