# The Seed Atlas engine, copied

`seedatlas-engine/` and `seedatlas-sax/` are copies of the folders of the same
names in https://github.com/DUzzL/Seed-Atlas at the commit in
`SEEDATLAS_COMMIT` (GPLv3; the engine derives from cubiomes by Cubitect, MIT).
`tools/vendor-seedatlas.sh` makes them; do not edit them here. `shim.c` is
Meridian's own: it exports what the engine only offers as inline functions.
See `docs/SEEDATLAS_PORT.md`.
