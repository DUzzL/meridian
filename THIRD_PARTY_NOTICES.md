# Third-party notices

## Seed Atlas and cubiomes

`mixins/meridian/engine/seedatlas` is a Rust port of the Seed Atlas engine
(https://github.com/DUzzL/Seed-Atlas, GPLv3) and of the Fabric version's
native interface (https://github.com/DUzzL/seed-atlas-xaero, GPLv3).
The engine derives from cubiomes (Copyright (c) 2020 Cubitect, MIT License);
its license is included as `mixins/meridian/engine/seedatlas/ENGINE_LICENSE`.
Biome colours follow the engine's table, which is inspired by Amidst (GPLv3).

The development tools in `tools/seedatlas-fixtures/` compile against a local
checkout of those sources to produce test fixtures; no C code ships with the
mod.
