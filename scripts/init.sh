#!/usr/bin/env bash
set -euo pipefail

project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$project_root"

if ! command -v node >/dev/null 2>&1; then
  echo "Node.js 20.19+ is required."
  exit 1
fi

node -e '
  const [major, minor] = process.versions.node.split(".").map(Number);
  if (major < 20 || (major === 20 && minor < 19)) {
    console.error(`Node.js 20.19+ is required; found ${process.versions.node}.`);
    process.exit(1);
  }
'

if ! command -v npm >/dev/null 2>&1; then
  echo "npm 10+ is required."
  exit 1
fi

echo "Installing frontend dependencies..."
npm install

if command -v cargo >/dev/null 2>&1; then
  if [[ ! -f Cargo.lock ]]; then
    echo "Generating the initial Cargo lockfile..."
    cargo generate-lockfile
  fi
  echo "Fetching Rust dependencies..."
  cargo fetch --locked
else
  echo "Rust is not installed. Install stable Rust with rustup, then rerun this script."
fi

echo "Initialization complete."
