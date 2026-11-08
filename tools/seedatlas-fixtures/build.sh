#!/bin/bash
# Builds the fixture tool against a Seed Atlas checkout (its engine and the
# engine's C ABI for the mods, seedatlas-sax, which the Fabric version builds
# as well), with the engine's flags, then writes the fixtures for every
# Minecraft version the ABI accepts. meridian-seedatlas compiles the same
# sources (its c/ folder), so the parity test checks its FFI layer.
#   SEEDATLAS_DIR  .../Seed-Atlas (a checkout; the commit of c/SEEDATLAS_COMMIT)
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
root=${SEEDATLAS_DIR:?set SEEDATLAS_DIR}
engine="$root/seedatlas-engine"
sax="$root/seedatlas-sax"
out=${TMPDIR:-/tmp}/meridian-seedatlas-fixtures
mkdir -p "$out"
sources=()
for f in biomes util finders generator layers biomenoise noise quadbase; do
  sources+=("$engine/$f.c")
done
cc -std=gnu11 -O2 -fwrapv -ffp-contract=off -w -I"$engine" -I"$sax" -DSAX_STATIC \
  "$here/fixtures.c" "$sax/seedatlas_sax.c" "${sources[@]}" -lm -o "$out/fixtures"
fixtures="$here/../../mixins/meridian/engine/seedatlas/tests/fixtures"
rm -rf "$fixtures"
mkdir -p "$fixtures"
versions="1.1 1.2 1.3 1.4 1.5 1.6 1.7 1.8 1.9 1.10 1.11 1.12 1.13 1.14 1.15 1.16.1 1.16 1.17 1.18 1.19.2 1.19 1.20 1.21.1 1.21.2 1.21.3 1.21.4 1.21.5 1.21.6 1.21.7 1.21.8 1.21.9 1.21.10 1.21.11 26.1 26.1.1 26.1.2 26.2 26.3"
for v in $versions; do
  "$out/fixtures" "$v" | gzip -9 > "$fixtures/$v.txt.gz" &
done
wait
du -sh "$fixtures"
