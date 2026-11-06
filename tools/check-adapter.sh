#!/usr/bin/env bash
# Builds Meridian against the Horizon commit the bundle pins, the way the
# launcher does: apply the bundle's mixins with horizon-mixins (the launcher's
# revision), then compile the trait mod against the staged source, with
# Horizon's Cargo.lock and its [patch.crates-io].
#
#   tools/check-adapter.sh                    fetch what is missing into ~/.cache/meridian/
#   HORIZON_DIR=/path tools/check-adapter.sh  use an existing Horizon checkout
#   ALLOW_OTHER_REVISION=1 ...                accept a checkout at another commit
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
bundle="$root/mixins/meridian"
cache="${XDG_CACHE_HOME:-$HOME/.cache}/meridian"
rev="$(sed -n 's/^horizon_revision = "\([0-9a-f]\{40\}\)"/\1/p' "$bundle/horizon-mixin.toml")"
[ -n "$rev" ] || { echo "no horizon_revision in horizon-mixin.toml" >&2; exit 1; }

# Outside the repository: inside it, Cargo would take our workspace for Horizon's.
horizon="${HORIZON_DIR:-$cache/horizon-$rev}"
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

# horizon-mixins at the revision this Horizon's launcher uses.
mixins_rev="$(sed -n 's/^pub const MIXINS_REV: &str = "\([0-9a-f]\{40\}\)";/\1/p' "$horizon/launcher/src/native_mods.rs")"
mixins="$cache/mixins-${mixins_rev:0:7}/bin/horizon-mixins"
if [ ! -x "$mixins" ]; then
    echo "building horizon-mixins $mixins_rev"
    cargo install -q --locked --git https://github.com/horizon-rust/horizon-mixins --rev "$mixins_rev" \
        --root "$cache/mixins-${mixins_rev:0:7}" horizon-mixins-cli
fi

# The launcher hands horizon-mixins the bundle without its [api] table and
# without the trait package folder.
mods="$cache/mods"
rm -rf "$mods" && mkdir -p "$mods/meridian"
cp -R "$bundle/bridge" "$mods/meridian/"
python3 - "$bundle/horizon-mixin.toml" "$mods/meridian/horizon-mixin.toml" <<'EOF'
import sys
out, skipping = [], False
for line in open(sys.argv[1]).read().splitlines(keepends=True):
    if line.strip().startswith("["):
        skipping = line.strip() == "[api]"
    if not skipping:
        out.append(line)
open(sys.argv[2], "w").write("".join(out))
EOF
# A clean export of the pinned commit (horizon-mixins refuses symlinks, which
# a checkout may hold for tooling files).
export_dir="$cache/export-${rev:0:7}"
if [ ! -f "$export_dir/Cargo.toml" ]; then
    rm -rf "$export_dir" && mkdir -p "$export_dir"
    git -C "$horizon" archive "$head" | tar -x -C "$export_dir"
    find "$export_dir" -type l -delete
    # How horizon-mixins learns an export's commit (no .git in it).
    printf '%s\n' "$head" > "$export_dir/.horizon-revision"
fi
stage="$cache/stage-${rev:0:7}"
# Replacing a stage in place needs Linux; start fresh elsewhere.
[ "$(uname)" = Linux ] || rm -rf "$stage"
"$mixins" apply --workspace "$export_dir" --mods "$mods" --out "$stage"

# The adapter may only use what Horizon locks: start from the stage's lockfile
# and refuse any change to it beyond adding Meridian's own crates.
cp "$stage/Cargo.lock" "$bundle/engine/Cargo.lock"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$root/target/adapter}"
patches=()
while IFS= read -r line; do patches+=(--config "$line"); done < <(python3 - "$stage" <<'EOF'
import pathlib, sys, tomllib
h = pathlib.Path(sys.argv[1])
for name, spec in tomllib.loads((h / "Cargo.toml").read_text()).get("patch", {}).get("crates-io", {}).items():
    if "path" in spec:
        print(f"patch.crates-io.{name}.path='{h / spec['path']}'")
EOF
)
cargo check \
    --manifest-path "$bundle/engine/Cargo.toml" \
    --config "patch.'https://github.com/horizon-rust/horizon'.horizon-client.path='$stage/crates/horizon-client'" \
    "${patches[@]}" \
    "$@"
python3 "$root/tools/check_lock.py" --against "$horizon/Cargo.lock" "$bundle/engine/Cargo.lock"
echo "adapter OK against Horizon $head with its mixins applied"
