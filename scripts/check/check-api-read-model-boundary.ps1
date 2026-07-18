$ErrorActionPreference = "Stop"

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..\..")
Set-Location $repoRoot

$patterns = @(
    "Json<.*storage::[A-Za-z0-9_]*Record"
)

$pattern = ($patterns -join "|")
$regex = [regex]$pattern
$violations = @()

$files = Get-ChildItem -Path crates/api/src -Recurse -Filter *.rs
foreach ($file in $files) {
    $relativePath = Resolve-Path -Relative $file.FullName
    $lineNumber = 0
    foreach ($line in Get-Content $file.FullName) {
        $lineNumber += 1
        if ($regex.IsMatch($line)) {
            $violations += "${relativePath}:${lineNumber}:$line"
        }
    }
}

if ($violations.Count -gt 0) {
    Write-Host "API read model boundary violation found:"
    Write-Host $violations
    exit 1
}

Write-Host "API read model boundary check passed."
