#!/bin/bash
# The Seed Atlas engine and its C ABI for the mods (seedatlas-sax) are compiled
# into meridian-seedatlas from a copy in data/meridian/seedatlas_c/ (the .hmod
# includes these sources under the mod namespace): the Horizon launcher
# builds a mod's crate from the bundle alone. This script makes that copy.
#
#   tools/vendor-seedatlas.sh <Seed Atlas checkout>   copy its HEAD (must be clean)
#   tools/vendor-seedatlas.sh --check                 compare the copy with the commit
#                                                     in seedatlas_commit on GitHub
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
dest="$here/../mixins/meridian/data/meridian/seedatlas_c"

# the files of a Seed Atlas tree (engine sources and tables, the mod ABI)
copy_tree() {
    local src=$1 out=$2
    rm -rf "$out/seedatlas-engine" "$out/seedatlas-sax"
    mkdir -p "$out/seedatlas-engine/tables" "$out/seedatlas-sax"
    for f in "$src"/seedatlas-engine/*.c "$src"/seedatlas-engine/*.h; do
        case "$(basename "$f")" in tests*) continue ;; esac
        cp "$f" "$out/seedatlas-engine/"
    done
    cp "$src"/seedatlas-engine/tables/*.h "$out/seedatlas-engine/tables/"
    cp "$src"/seedatlas-sax/seedatlas_sax.c "$src"/seedatlas-sax/seedatlas_sax.h "$out/seedatlas-sax/"
}

if [[ "${1:-}" == "--check" ]]; then
    commit=$(cat "$dest/seedatlas_commit")
    tmp=$(mktemp -d)
    trap 'rm -rf "$tmp"' EXIT
    curl -fsSL "https://codeload.github.com/DUzzL/Seed-Atlas/tar.gz/$commit" | tar -xz -C "$tmp"
    mkdir -p "$tmp/expected"
    copy_tree "$tmp"/Seed-Atlas-* "$tmp/expected"
    diff -r "$tmp/expected/seedatlas-engine" "$dest/seedatlas-engine"
    diff -r "$tmp/expected/seedatlas-sax" "$dest/seedatlas-sax"
    echo "seedatlas-c/ matches Seed Atlas $commit"
    exit 0
fi

src=${1:?usage: vendor-seedatlas.sh <Seed Atlas checkout> | --check}
if [[ -n "$(git -C "$src" status --porcelain -- seedatlas-engine seedatlas-sax)" ]]; then
    echo "$src has uncommitted changes in seedatlas-engine or seedatlas-sax" >&2
    exit 1
fi
copy_tree "$src" "$dest"
git -C "$src" rev-parse HEAD > "$dest/seedatlas_commit"
echo "copied Seed Atlas $(cat "$dest/seedatlas_commit")"
