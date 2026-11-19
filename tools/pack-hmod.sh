#!/usr/bin/env bash
# Package and validate with cargo-horizon-mod (horizon-rust/horizon).
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
bundle="$root/mixins/meridian"
tool="${HORIZON_MOD_TOOL:-cargo-horizon-mod}"
version="$(python3 -c 'import sys,tomllib; print(tomllib.load(open(sys.argv[1],"rb"))["version"])' "$bundle/horizon-mod.toml")"
if ! command -v "$tool" >/dev/null; then
    echo "Install cargo-horizon-mod (cargo install --locked --git https://github.com/horizon-rust/horizon --rev <pin> cargo-horizon-mod), or set HORIZON_MOD_TOOL" >&2
    exit 1
fi
(cd "$bundle" && "$tool" build)
source="$bundle/target/horizon-mod/meridian.hmod"
"$tool" check "$source" --json
out="${1:-$root/target/meridian-$version.hmod}"
mkdir -p "$(dirname "$out")"
cp "$source" "$out"
echo "$out"
