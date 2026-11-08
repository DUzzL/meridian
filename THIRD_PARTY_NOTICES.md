# Third-party notices

## Seed Atlas and cubiomes

`mixins/meridian/engine/seedatlas` compiles the Seed Atlas engine and its C
interface for the mods, `seedatlas-sax` (https://github.com/DUzzL/Seed-Atlas,
GPLv3; the interface was first the Fabric version's,
https://github.com/DUzzL/seed-atlas-xaero, GPLv3), from a copy in its `c/`
folder, and ships that C code with the mod.
The engine derives from cubiomes (Copyright (c) 2020 Cubitect, MIT License);
its license is included as `mixins/meridian/engine/seedatlas/ENGINE_LICENSE`.
Biome colours follow the engine's table, which is inspired by Amidst (GPLv3).

The development tools in `tools/seedatlas-fixtures/` compile against a local
Seed Atlas checkout to produce test fixtures.

## Seed Atlas for Xaero's World Map (Fabric)

The Seed Atlas features (terrain look, commands, options, screens, marker
types) follow DUzzL/seed-atlas-xaero main (GPLv3), ported to Rust. Its
structure icons come from the Seed Atlas project.

## Horizon

The Nether cave mask in `mixins/meridian/engine/terrain_sampler.rs` follows
Horizon's port of the vanilla cave carver (MIT, https://github.com/horizon-rust/horizon).
