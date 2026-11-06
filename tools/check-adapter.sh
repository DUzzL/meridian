#!/usr/bin/env bash
# Compiles Meridian's Horizon adapter against the Horizon commit the bundle
# pins, as the launcher would: Horizon's own Cargo.lock, horizon-client from
# local source.
#
#   tools/check-adapter.sh            fetch the pinned Horizon into ~/.cache/meridian/
#   HORIZON_DIR=/path tools/check-adapter.sh   use an existing checkout
#   ALLOW_OTHER_REVISION=1 ...        accept a checkout at another commit
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
bundle="$root/mixins/meridian"
rev="$(sed -n 's/^horizon_revision = "\([0-9a-f]\{40\}\)"/\1/p' "$bundle/horizon-mixin.toml")"
[ -n "$rev" ] || { echo "no horizon_revision in horizon-mixin.toml" >&2; exit 1; }

# Outside the repository: inside it, Cargo would take our workspace for
# Horizon's.
horizon="${HORIZON_DIR:-${XDG_CACHE_HOME:-$HOME/.cache}/meridian/horizon-$rev}"
if [ ! -d "$horizon/.git" ]; then
    echo "fetching Horizon $rev into $horizon"
    mkdir -p "$horizon"
    git -C "$horizon" init -q
    git -C "$horizon" remote add origin https://github.com/horizon-rust/horizon.git
    git -C "$horizon" fetch -q --depth 1 origin "$rev"
    git -C "$horizon" checkout -q FETCH_HEAD
fi
head="$(git -C "$horizon" rev-parse HEAD)"
if [ "$head" != "$rev" ] && [ "${ALLOW_OTHER_REVISION:-0}" != 1 ]; then
    echo "Horizon checkout is at $head, the bundle pins $rev (set ALLOW_OTHER_REVISION=1 to use it anyway)" >&2
    exit 1
fi

# The adapter may only use what Horizon locks: start from Horizon's lockfile
# and refuse any change to it beyond adding Meridian's own crates.
cp "$horizon/Cargo.lock" "$bundle/engine/Cargo.lock"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$root/target/adapter}"

# Horizon's workspace patches crates.io crates (wgpu-hal, naga, ...); the game
# is built with them, so the adapter is checked with them too.
patches=()
while IFS= read -r line; do patches+=(--config "$line"); done < <(python3 - "$horizon" <<'EOF'
import pathlib, sys, tomllib
h = pathlib.Path(sys.argv[1])
for name, spec in tomllib.loads((h / "Cargo.toml").read_text()).get("patch", {}).get("crates-io", {}).items():
    if "path" in spec:
        print(f"patch.crates-io.{name}.path='{h / spec['path']}'")
EOF
)
cargo check \
    --manifest-path "$bundle/engine/Cargo.toml" \
    --config "patch.'https://github.com/horizon-rust/horizon'.horizon-client.path='$horizon/crates/horizon-client'" \
    "${patches[@]}" \
    "$@"
python3 "$root/tools/check_lock.py" --against "$horizon/Cargo.lock" "$bundle/engine/Cargo.lock"
echo "adapter OK against Horizon $head"
