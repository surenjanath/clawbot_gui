#!/usr/bin/env bash
# One-time dev setup: verify Rust toolchain and build the claw CLI (including GUI).
set -euo pipefail
cd "$(dirname "$0")"

if ! command -v cargo >/dev/null 2>&1; then
  echo "Rust (cargo) not found. Install from https://rustup.rs/ then retry." >&2
  exit 1
fi

echo "Building claw-cli (workspace root: $(pwd)) ..."
cargo build -p claw-cli

echo ""
echo "Done. Examples:"
echo "  cargo run -p claw-cli -- gui"
echo "  cargo run -p claw-cli -- --help"
