# tools/cli_test_server.ps1
# Build the CLI (debug) then start the test server.
# Usage:  .\tools\cli_test_server.ps1

$RepoRoot = Split-Path -Parent $PSScriptRoot
Push-Location $RepoRoot

Write-Host "-> building mdix-cli (debug)..." -ForegroundColor Yellow
cargo build -p mdix-cli --quiet
if ($LASTEXITCODE -ne 0) { Write-Host "build failed" -ForegroundColor Red; exit 1 }
Write-Host "✓ build ok" -ForegroundColor Green

Write-Host ""
Write-Host "-> starting test server..." -ForegroundColor Yellow
python tools/cli_test_server.py
