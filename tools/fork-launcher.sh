#!/usr/bin/env bash
# All launcher/game test state is confined to target/fork-runtime by default.
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
launcher="${HORIZON_LAUNCHER:-horizon-launcher}"
if ! "$launcher" --version | grep -q '^horizon-fork-launcher '; then
    echo "HORIZON_LAUNCHER must name the arismoko/horizon fork launcher" >&2
    exit 1
fi
scratch="${MERIDIAN_FORK_TEST_HOME:-$root/target/fork-runtime}"
mkdir -p "$scratch"
scratch="$(cd "$scratch" && pwd)"
for name in HORIZON_HOME HORIZON_LAUNCHER_HOME XDG_DATA_HOME XDG_CONFIG_HOME HORIZON_LAUNCHER_TRASH HORIZON_LAUNCHER_CLIPBOARD HORIZON_LAUNCHER_DESKTOP; do
    export "$name=$scratch/$name"
    mkdir -p "$scratch/$name"
done
export HORIZON_LAUNCHER_NO_SELF_UPDATE=1 HORIZON_NO_AUDIO=1
exec "$launcher" "$@"
