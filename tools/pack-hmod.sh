#!/usr/bin/env bash
# Package and validate through the fork's own flat .hmod writer.
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
bundle="$root/mixins/meridian"
tool="${HORIZON_MOD_TOOL:-cargo-horizon-mod}"
version="$(python3 -c 'import sys,tomllib; print(tomllib.load(open(sys.argv[1],"rb"))["version"])' "$bundle/horizon-mod.toml")"
if ! command -v "$tool" >/dev/null; then
    echo "Install cargo-horizon-mod from arismoko/horizon, or set HORIZON_MOD_TOOL to its executable" >&2
    exit 1
fi
(cd "$bundle" && "$tool" build)
source="$bundle/target/horizon-mod/meridian.hmod"
"$tool" check "$source" --json
out="${1:-$root/target/meridian-$version.hmod}"
mkdir -p "$(dirname "$out")"
cp "$source" "$out"
echo "$out"
