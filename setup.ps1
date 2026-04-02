# One-time dev setup: verify Rust toolchain and build the claw CLI (including GUI).
$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent $MyInvocation.MyCommand.Path
Set-Location $Root

if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
    Write-Error @"
Rust (cargo) is not on PATH.
Install the toolchain: https://rustup.rs/
Then open a new terminal and run this script again.
"@
}

Write-Host "Building claw-cli (workspace root: $Root) ..."
cargo build -p claw-cli
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

Write-Host ""
Write-Host "Done. Examples:"
Write-Host "  cargo run -p claw-cli -- gui"
Write-Host "  cargo run -p claw-cli -- --help"
