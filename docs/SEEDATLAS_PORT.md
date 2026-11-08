# Seed Atlas engine: guide

`mixins/meridian/engine/seedatlas` (`meridian-seedatlas`) runs the Seed Atlas
engine itself, not a translation of it: the engine (C, derived from cubiomes)
and its C ABI for the mods (`seedatlas-sax`, ABI 6), which the Fabric version
builds as well. So Meridian, the Fabric version and the Seed Atlas app compute
the same, bit for bit, for every Minecraft version (1.1 to 26.3). Until
2026-11 the crate was a Rust port of both (D-041).

## Where the C code comes from

- https://github.com/DUzzL/Seed-Atlas: `seedatlas-engine/` (`*.c`, `*.h`,
  `tables/`) and `seedatlas-sax/` (`seedatlas_sax.c`, `.h`), at the commit in
  `engine/data/seedatlas-c/SEEDATLAS_COMMIT`.
- The crate compiles a copy in `engine/data/seedatlas-c/`, because the Horizon
  launcher builds a mod's crates from the bundle alone, and a `.hnmod` carries
  only Rust, TOML and files under a `data/` folder (`engine/data/` is one, a
  crate's `seedatlas/data/` is not). `tools/vendor-seedatlas.sh
  <checkout>` makes the copy from a clean Seed Atlas checkout and records its
  commit; `tools/vendor-seedatlas.sh --check` (CI) compares the copy with that
  commit on GitHub. Never edit `seedatlas-c/seedatlas-*` by hand: change Seed Atlas,
  then copy again.
- `engine/data/seedatlas-c/shim.c` exports what the engine only offers as inline
  functions (`isSlimeChunk`).

## Building

`build.rs` compiles the C code with the `cc` crate (in Horizon's Cargo.lock)
and the engine's own flags: `-fwrapv` (the engine relies on wrapping signed
arithmetic) and `-ffp-contract=off` (no fused multiply-adds, which would make
results depend on the platform). The engine needs C11 variable-length arrays,
so it builds with GCC or clang, not MSVC's C compiler: Horizon's launcher brings
MinGW GCC on Windows x64 (`x86_64-pc-windows-gnu`), LLVM-MinGW on Windows ARM64,
the Command Line Tools on macOS and the system's `cc` on Linux.

## Rules

- `src/ffi.rs` is the only module with `unsafe` code (the crate denies it
  elsewhere): the C declarations and safe wrappers. A `sax_context` is
  read-only after it is created, so `Atlas` shares one between its clones and
  threads.
- `src/atlas.rs` keeps the API the map uses (`Atlas`, `Hit`, the marker kinds
  and result flags); every call goes to the `sax_*` function of the same name.
- `mc.rs`, `biomes.rs` and `structures.rs` keep the engine's numbers as Rust
  constants (versions, biome ids, structure kinds); names, colours, version
  names and structure configurations come from the engine (`mc2str`,
  `biome2str`, `initBiomeColors`, `getStructureConfig`). A test checks that
  the version constants match the engine's.
- Code comments in English only.

## Testing

`tests/parity.rs` checks every fixture (one gzip file per version in
`tests/fixtures/`), written by the engine's C ABI itself through
`tools/seedatlas-fixtures/` (`SEEDATLAS_DIR=<Seed Atlas checkout>
tools/seedatlas-fixtures/build.sh`, from the commit of `c/`): it checks the FFI
layer and the copy. Limit it to some versions while working:

```bash
SEEDATLAS_VERSIONS=1.12,1.7 cargo test -p meridian-seedatlas --release --test parity
```

Also run `cargo clippy -p meridian-seedatlas --all-targets` and
`cargo fmt --all`.

## Updating the engine

1. Change and commit the engine in Seed Atlas (its own tests:
   `make -C seedatlas-engine test-versions`, `make -C seedatlas-sax test`).
2. `tools/vendor-seedatlas.sh <Seed Atlas checkout>`.
3. Regenerate the fixtures (above) where the engine's results changed on
   purpose, and run the parity test.
