#!/bin/bash
# Packs mixins/meridian into the .hnmod a player installs (Horizon's
# docs/api/mod-files.md), the way the launcher packs a mod folder from GitHub
# (launcher/src/github_mods.rs, pack): horizon-native.json made from
# horizon-mixin.toml, then every file of the bundle Horizon allows in one
# (Rust, TOML, data/ folders, crates' README/LICENSE), under mixins/meridian/.
# The zip is the same for the same files.
#
#   tools/pack-hnmod.sh [OUT]          default: target/meridian-<version>.hnmod
#   HORIZON_DIR=/path tools/pack-hnmod.sh   read MIXINS_REV from a checkout
#                                           instead of GitHub
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
bundle="$root/mixins/meridian"
toml="$bundle/horizon-mixin.toml"

rev="$(sed -n 's/^horizon_revision = "\([0-9a-f]\{40\}\)"/\1/p' "$toml")"
version="$(sed -n '/^\[mod\]/,/^\[/s/^version = "\(.*\)"/\1/p' "$toml" | head -1)"
[ -n "$rev" ] && [ -n "$version" ] || { echo "no horizon_revision or version in $toml" >&2; exit 1; }

# The horizon-mixins commit the launcher of that Horizon applies bundles with.
if [ -n "${HORIZON_DIR:-}" ]; then
    lines="$(cat "$HORIZON_DIR/launcher/src/native_mods.rs")"
else
    lines="$(curl -fsSL "https://raw.githubusercontent.com/horizon-rust/horizon/$rev/launcher/src/native_mods.rs")"
fi
mixins="$(sed -n 's/^pub const MIXINS_REV: &str = "\([0-9a-f]\{40\}\)";/\1/p' <<<"$lines")"
[ -n "$mixins" ] || { echo "no MIXINS_REV for Horizon $rev" >&2; exit 1; }

out="${1:-$root/target/meridian-$version.hnmod}"
mkdir -p "$(dirname "$out")"
python3 -I - "$bundle" "$out" "$version" "$rev" "$mixins" <<'EOF'
import json, os, sys, zipfile

bundle, out, version, horizon, mixins = sys.argv[1:]
ID = "meridian"

def in_data(parts):
    return len(parts) >= 2 and (parts[0] == "data" or (len(parts) >= 3 and parts[1] == "data"))

def text(parts):
    name = parts[-1]
    return len(parts) <= 2 and (
        name.startswith(("README", "LICENSE", "LICENCE", "COPYING", "NOTICE")) or name == ".gitignore")

def allowed(rest):
    parts = rest.split("/")
    return rest.endswith((".rs", ".toml")) or in_data(parts) or text(parts)

files = []
for dirpath, dirnames, filenames in os.walk(bundle, followlinks=False):
    for d in dirnames:
        if os.path.islink(os.path.join(dirpath, d)):
            sys.exit(f"{os.path.join(dirpath, d)} is a link: not allowed in a mod")
    dirnames.sort()
    for f in filenames:
        path = os.path.join(dirpath, f)
        if os.path.islink(path):
            sys.exit(f"{path} is a link: not allowed in a mod")
        rest = os.path.relpath(path, bundle).replace(os.sep, "/")
        if f == ".DS_Store":
            continue
        if allowed(rest):
            files.append((rest, path))
files.sort()
if len(files) + 1 > 512:
    sys.exit(f"{len(files) + 1} entries: a .hnmod holds at most 512")

manifest = {"schema": 1, "id": ID, "version": version, "game": "26.3",
            "horizon": horizon, "mixins": mixins}
total = 0
with zipfile.ZipFile(out, "w", zipfile.ZIP_DEFLATED) as z:
    def put(name, data):
        info = zipfile.ZipInfo(name, date_time=(1980, 1, 1, 0, 0, 0))
        info.compress_type = zipfile.ZIP_DEFLATED
        info.external_attr = 0o100644 << 16
        z.writestr(info, data)
    put("horizon-native.json", json.dumps(manifest, indent=2))
    for rest, path in files:
        with open(path, "rb") as fh:
            data = fh.read()
        total += len(data)
        put(f"mixins/{ID}/{rest}", data)
if total > 64 << 20:
    sys.exit("more than 64 MiB unpacked: too big for a .hnmod")
print(f"{out}: {len(files) + 1} entries, {total / 1024:.0f} KiB unpacked")
EOF
