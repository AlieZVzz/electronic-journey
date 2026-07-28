#!/usr/bin/env bash
set -euo pipefail

project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
debug_app="$project_root/target/debug/bundle/macos/Electronic Journey.app"
bundle_id="com.electronicjourney.app"

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "Debug app signing is only available on macOS." >&2
  exit 1
fi

if [[ ! -d "$debug_app" ]]; then
  echo "Debug app not found: $debug_app" >&2
  echo "Build it first with: npm run build:desktop:debug" >&2
  exit 1
fi

# This identifier-only ad-hoc requirement is intentionally limited to the
# local debug bundle. It keeps macOS TCC identity stable across rebuilds
# without weakening or replacing production Developer ID signing.
codesign \
  --force \
  --deep \
  --sign - \
  --identifier "$bundle_id" \
  --requirements "=designated => identifier \"$bundle_id\"" \
  "$debug_app"

codesign --verify --deep --strict --verbose=2 "$debug_app"
echo "Signed local debug app with stable TCC identity: $bundle_id"
