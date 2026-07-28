#!/usr/bin/env bash
set -euo pipefail

project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$project_root"

echo "Refreshing the npm lockfile without running dependency scripts..."
npm install --package-lock-only --ignore-scripts

if command -v cargo >/dev/null 2>&1; then
  echo "Refreshing the Cargo lockfile..."
  cargo generate-lockfile
else
  echo "Cargo lockfile refresh skipped: cargo is not installed."
fi
