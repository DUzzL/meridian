#!/bin/bash
# Builds the fixture tools against the Seed Atlas engine and the Fabric
# version's native glue (both from their main branches), then writes the
# fixtures for every Minecraft version the glue accepts.
#   SEEDATLAS_ENGINE_DIR  .../Seed-Atlas/seedatlas-engine        (main)
#   SEEDATLAS_GLUE_DIR    .../seed-atlas-xaero/src/main/native     (main)
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
engine=${SEEDATLAS_ENGINE_DIR:?set SEEDATLAS_ENGINE_DIR}
glue=${SEEDATLAS_GLUE_DIR:?set SEEDATLAS_GLUE_DIR}
out=${TMPDIR:-/tmp}/meridian-seedatlas-fixtures
mkdir -p "$out"
sources="$engine/biomes.c $engine/util.c $engine/finders.c $engine/generator.c $engine/layers.c $engine/biomenoise.c $engine/noise.c $engine/quadbase.c"
cc -O2 -w -I"$engine" -I"$glue" -DSEEDATLAS_XAERO_BUILD "$here/fixtures.c" "$glue/seedatlas_xaero.c" $sources -lm -o "$out/fixtures"
cc -O2 -w -I"$engine" "$here/biome_table.c" $sources -lm -o "$out/biome_table"
fixtures="$here/../../mixins/meridian/engine/seedatlas/tests/fixtures"
rm -rf "$fixtures"
mkdir -p "$fixtures"
versions="1.1 1.2 1.3 1.4 1.5 1.6 1.7 1.8 1.9 1.10 1.11 1.12 1.13 1.14 1.15 1.16.1 1.16 1.17 1.18 1.19.2 1.19 1.20 1.21.1 1.21.2 1.21.3 1.21.4 1.21.5 1.21.6 1.21.7 1.21.8 1.21.9 1.21.10 1.21.11 26.1 26.1.1 26.1.2 26.2 26.3"
for v in $versions; do
  "$out/fixtures" "$v" | gzip -9 > "$fixtures/$v.txt.gz" &
done
wait
"$out/biome_table" > "$here/../../mixins/meridian/engine/seedatlas/src/biome_table.rs"
python3 "$here/btree_table.py" "$engine/tables/btree21wd.h" "$here/../../mixins/meridian/engine/seedatlas/src/btree21wd.rs"
du -sh "$fixtures"
