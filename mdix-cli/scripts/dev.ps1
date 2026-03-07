# scripts/dev.ps1
# Windows equivalent of dev.sh — rebuilds and runs any mdix command.
#
# Usage:
#   .\scripts\dev.ps1
#   .\scripts\dev.ps1 validate tests\fixtures\basic.mdix
#   .\scripts\dev.ps1 convert tests\fixtures\basic.mdix --to json

param(
    [Parameter(ValueFromRemainingArguments = $true)]
    [string[]]$CommandArgs
)

$RepoRoot  = Split-Path -Parent $PSScriptRoot
$Binary    = Join-Path $RepoRoot "target\debug\mdix.exe"

Write-Host "→ rebuilding mdix-cli (debug)..." -ForegroundColor Yellow
Push-Location $RepoRoot
cargo build -p mdix-cli --quiet 2>&1 | Select-Object -Last 5
if ($LASTEXITCODE -ne 0) {
    Write-Host "✗ build failed" -ForegroundColor Red
    exit 1
}
Write-Host "✓ build ok" -ForegroundColor Green
Pop-Location

if ($CommandArgs.Count -eq 0) {
    & $Binary --help
    Write-Host ""
    Write-Host "Usage: .\scripts\dev.ps1 <command> [args]" -ForegroundColor Yellow
    exit 0
}

Write-Host "→ running: mdix $CommandArgs" -ForegroundColor Yellow
& $Binary @CommandArgs
exit $LASTEXITCODE
