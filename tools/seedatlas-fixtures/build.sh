#!/bin/bash
# Builds the fixture tools against a Seed Atlas engine checkout and the
# Fabric version's native glue, then writes the fixtures.
#   SEEDATLAS_ENGINE_DIR  .../Seed-Atlas/seedatlas-engine
#   SEEDATLAS_GLUE_DIR    .../seed-atlas-xaero/src/main/native
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
engine=${SEEDATLAS_ENGINE_DIR:?set SEEDATLAS_ENGINE_DIR}
glue=${SEEDATLAS_GLUE_DIR:?set SEEDATLAS_GLUE_DIR}
out=${TMPDIR:-/tmp}/meridian-seedatlas-fixtures
mkdir -p "$out"
sources="$engine/biomes.c $engine/util.c $engine/finders.c $engine/generator.c $engine/layers.c $engine/biomenoise.c $engine/noise.c $engine/quadbase.c"
cc -O2 -w -I"$engine" -I"$glue" -DSEEDATLAS_XAERO_BUILD "$here/biome_fixtures.c" "$glue/seedatlas_xaero.c" $sources -lm -o "$out/biome_fixtures"
cc -O2 -w -I"$engine" -I"$glue" -DSEEDATLAS_XAERO_BUILD "$here/structure_fixtures.c" "$glue/seedatlas_xaero.c" $sources -lm -o "$out/structure_fixtures"
cc -O2 -w -I"$engine" "$here/biome_table.c" $sources -lm -o "$out/biome_table"
fixtures="$here/../../mixins/meridian/engine/seedatlas/tests/fixtures"
mkdir -p "$fixtures"
"$out/biome_fixtures" | gzip -9 > "$fixtures/biomes.txt.gz"
"$out/structure_fixtures" | gzip -9 > "$fixtures/structures.txt.gz"
"$out/biome_table" > "$here/../../mixins/meridian/engine/seedatlas/src/biome_table.rs"
ls -la "$fixtures"
python3 "$here/btree_table.py" "$engine/tables/btree21wd.h" "$here/../../mixins/meridian/engine/seedatlas/src/btree21wd.rs"
