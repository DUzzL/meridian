#!/usr/bin/env bash
# Compile the adapter against the pinned SDK without touching the bundle's lock.
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
exec python3 "$root/tools/check-api.py" "$@"
