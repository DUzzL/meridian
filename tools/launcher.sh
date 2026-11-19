#!/usr/bin/env bash
# Runs the Horizon launcher with all launcher/game test state confined to
# target/launcher-runtime (override with MERIDIAN_TEST_HOME), so a test never
# touches the player's own instances, worlds or settings.
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
launcher="${HORIZON_LAUNCHER:-horizon-launcher}"
if ! "$launcher" --version | grep -q '^horizon-launcher '; then
    echo "HORIZON_LAUNCHER must name the Horizon launcher (horizon-rust/horizon)" >&2
    exit 1
fi
scratch="${MERIDIAN_TEST_HOME:-$root/target/launcher-runtime}"
mkdir -p "$scratch"
scratch="$(cd "$scratch" && pwd)"
for name in HORIZON_HOME HORIZON_LAUNCHER_HOME XDG_DATA_HOME XDG_CONFIG_HOME HORIZON_LAUNCHER_TRASH HORIZON_LAUNCHER_CLIPBOARD HORIZON_LAUNCHER_DESKTOP; do
    export "$name=$scratch/$name"
    mkdir -p "$scratch/$name"
done
export HORIZON_LAUNCHER_NO_SELF_UPDATE=1 HORIZON_NO_AUDIO=1
exec "$launcher" "$@"
