#!/usr/bin/env bash
set -euo pipefail

project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$project_root"

echo "Running frontend checks..."
npm run check

if command -v cargo >/dev/null 2>&1; then
  echo "Running Rust formatting check..."
  cargo fmt --all -- --check

  echo "Running Rust compilation check..."
  cargo check --workspace

  echo "Running Rust tests..."
  cargo test --workspace
else
  echo "Rust checks skipped: cargo is not installed."
fi
