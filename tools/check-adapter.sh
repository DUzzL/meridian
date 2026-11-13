#!/usr/bin/env bash
# Check the API-facing crate without rebuilding the game. Native callbacks are
# compiled separately by tools/fork-launcher.sh native-build.
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
exec python3 "$root/tools/check-fork-api.py" "$@"
