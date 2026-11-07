# Seed Atlas port: guide

`mixins/meridian/engine/seedatlas` ports the Seed Atlas engine (C, derived
from cubiomes) and the Fabric version's native glue to Rust, for every
Minecraft version the Fabric version offers (1.1 to 26.3).

## Reference sources (read only)

- Engine: https://github.com/DUzzL/Seed-Atlas, `seedatlas-engine/` (`*.c`,
  `*.h`, `tables/`), ported at commit `fc2c628`; `main` at `2539698` only
  changed build and test files since.
- Glue: https://github.com/DUzzL/seed-atlas-xaero, `src/main/native/seedatlas_xaero.c`
  (ABI 6), compared at commit `58c70ff` (release 2.0.0).

Never open anything of Xaero's (no decompiled code, jars or assets).

## Rules for the port

- Translate the C faithfully, function by function, keeping its names in doc
  comments (`/// \`getStructurePos\`...`). Results must be bit-identical.
- Versions are `i32` constants in `src/mc.rs` (`MC_1_12`, `MC_1_18`...), with
  the same order as the C enum, so `mc <= MC_1_17` reads as in C.
- Integer arithmetic wraps where C's unsigned or overflowing arithmetic does:
  use `wrapping_*`, and cast through the same widths as C (`as i64 as u64`
  for an `int` added to a `uint64_t`). C `/` and `%` truncate like Rust's.
- Floats: keep `float` (`f32`) and `double` (`f64`) exactly where C has them,
  including the implicit conversions. The fixtures come from clang on Apple
  Silicon, which fuses `a*b + c` into FMA in a few places; where a result
  differs only far from the origin, try `mul_add` in the same order clang
  would (see `end_island_weight` in `biome_noise.rs`).
- No `unsafe`, no new dependencies.
- Code comments in English only.

## Testing

`tests/parity.rs` checks every fixture (one gzip file per version in
`tests/fixtures/`, written by the real glue via `tools/seedatlas-fixtures/`).
Limit it to some versions while working:

```bash
SEEDATLAS_VERSIONS=1.12,1.7 cargo test -p meridian-seedatlas --release --test parity
```

Every group of differences is listed with an example. Also run
`cargo clippy -p meridian-seedatlas --all-targets` and `cargo fmt --all`.

## Layout

| File | Content |
|---|---|
| `mc.rs` | versions, `mc2str`, `str2mc` |
| `rng.rs`, `noise.rs` | `rng.h`, `noise.c` |
| `biomes.rs`, `biome_table.rs` | `biomes.c`, names and colours (`util.c`) |
| `layers.rs` | `layers.c` and the layer stack of `generator.c` (1.0 to 1.17) |
| `biome_noise.rs`, `btree*.rs` | `biomenoise.c` (1.18+ climate, Nether, End, Voronoi) |
| `beta.rs`, `surface.rs` | Beta 1.7 biomes, `SurfaceNoise`, depth and scale |
| `generator.rs` | `generator.c` dispatch: `setupGenerator`, `applySeed`, `genBiomes` |
| `structures.rs`, `viability.rs`, `finders.rs`, `endcity.rs` | `finders.c` |
| `terrain.rs` | glue: terrain heights and the Nether floor (ABI 6) |
| `atlas.rs` | glue: the `sax_*` interface |
