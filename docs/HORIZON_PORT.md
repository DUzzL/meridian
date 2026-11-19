# Meridian on Horizon's public mod API

Validation record for `main` since the mod API fork (arismoko/horizon#16941)
was merged into `horizon-rust/horizon` on 2026-11-16. The fork port
(`port/arismoko-horizon`) is the history; this page describes what ships now.

## Pins and package

- Game and SDK: `horizon-rust/horizon` at
  `f00125f61f794aaa3d4e950d301f353b35b6ee3d` (Nightly 2026-11-19, Mod SDK
  `sdk-f00125f61f…`, package format 6, content API `0.2`).
- Tools: Horizon launcher 0.2.81 (stock release, no local corrections);
  `cargo-horizon-mod` built from the pinned source.
- `horizon-mod.toml` keeps the `[native]` pins (`game`, `horizon`, `mixins`)
  and has **no** `[native.*]` table: no mixins, support modules or internals.
- `Cargo.toml` names the SDK by path, `.horizon/sdk/api/crates/horizon-mod-api`,
  which `cargo horizon-mod` fetches and checks (never committed or packed).
- `Cargo.lock` is the registry lock the launcher consumes. It starts from the
  game's lock at the pin, so the mod's registry packages (`png`, `log`,
  `parking_lot`, `chrono` and their dependencies) keep the game's versions;
  a lock resolved fresh by `native-lock --update-lock` picked `libc 0.2.191`
  and the build refused it against the game's graph.
- `icon.png` (512 x 512) is the launcher's mod list icon; the package also
  carries `uses.json`, the list `fits` reads.

## API mapping

Everything goes through `mod_api`:

| Need | Public API |
|---|---|
| Chunks in and out | `client_chunk_load`, `client_chunk_unload` (also in single player: the probe saw them on the main thread there) |
| Block writes | a block part on `horizon:class/block` with `on_block_state_change`; only `LevelSide::Client` notices are used |
| The loaded set after a level change | a sweep of `LevelRef::is_loaded` around the player (a client cannot list its chunks) |
| Chunk reads | `client_surface_heights`, `LevelRef::block_state`, `fluid_state`, `client_biome`, `brightness`, `height`; `BlockState::block().id`, `block_states().map_color` |
| Biome tints | `biome_effects` (water); grass and foliage from Meridian's vanilla table (no API) |
| Radar | `entities_in`, `EntityRef::{kind, position, classification, name, is_invisible}`, `living().is_tamed`, the tab list for players' default skins |
| Cave roof | `can_see_sky` |
| Textures (mean colours, icons, faces) | `resource(PackType::ClientResources, …)`, resource packs included |
| Map pictures | `ClientHost::mod_image` (the shared 1024 x 1024 HUD atlas slot) drawn with `DrawContext::sprite("horizon/modimage", …)` |
| Single-player seed and folder | `server_started`: `world_dir`, `LevelRef::seed`; Large Biomes from `data/minecraft/world_gen_settings.dat` |
| Chat replies, clipboard | `chat_line`; `set_clipboard_text` inside the input hooks |
| Data | `<game folder>/horizon-mod-data/meridian` (the folder `data_files()` names), written with `std::fs` from the map's threads |

The maps are composed on the CPU (`src/canvas.rs`), a port of the former GPU
passes: the minimap turns with the player, blends neighbouring texels, cuts a
round map and draws the grid at physical pixel resolution; the world map
draws regions at three levels of detail, the Seed Atlas layer under or over
the terrain and the marker icons. The picture is uploaded only when its
inputs change. `GuiDraw` has no rotation, so a turning minimap is uploaded
each frame while the player turns.

The Overworld's seed relief uses the Seed Atlas engine's height estimate
(the API has no terrain generator for an arbitrary seed); the Nether's caves
are the port of vanilla's carver, moved unchanged into `core/src/nether_caves.rs`.

## Validation

Checks (2026-11-19):

- `cargo test --workspace --locked`: 276 core tests and the Seed Atlas
  parity fixtures pass (measurements stay ignored).
- `cargo clippy --workspace --all-targets --locked -- -D warnings` and
  `MERIDIAN_CARGO_COMMAND=clippy tools/check-adapter.sh --all-targets -- -D warnings`
  pass; the adapter's own tests pass with `MERIDIAN_CARGO_COMMAND=test`.
- `tools/pack-hmod.sh` builds the package and `cargo-horizon-mod check` accepts it.
- `horizon-launcher native-build` (0.2.81, macOS arm64) builds it against the
  published SDK in about a minute, with no source build of the game.

Game test (macOS arm64, signed Nightly `nightly-2026-11-19-f00125f`, a
separate instance "Meridian Test" of the player's launcher, creative single
player, driven with `horizon-ctl` and screenshots):

- Minimap: terrain, water, shading, radar dots, compass, coordinates; it
  turns with the player; cave mode under a roof (the `/fill` roof switched it).
- Block changes: a `/fill` of red wool appears on the minimap and world map.
- World map (M): explored terrain, toolbar, zoom, drag; the Seed Atlas layer
  beyond the explored area (oceans with depth, biomes); about 450 FPS with the
  map open and while dragging, about 500 in the world.
- Nether: dimension change, the Seed Atlas layer with the carved floors.
- `/seedatlas status` (seed detected from the world), `/meridian waypoint add`,
  `export`, `set list`; notices drawn over the hotbar.
- Restart: map data and waypoints persist; a file in the old data folder
  (`horizon-mods/.horizon-mod-data/meridian`) is moved to the new one and the
  old folder is removed; `meridian.log` is written.

Not tested: a remote server (the dedicated server needs the player's EULA
consent), resource pack reload, the End, structure markers.

## Limits of the public API (current pin)

1. **World spawn on a client.** `level_respawn_data` answers on the server
   only, and `client_packet` gives the spawn packet's type without its
   position. On a server, Meridian therefore keeps one map per address:
   "Multiplayer Maps: World Spawn" cannot tell worlds behind one address
   apart. Single player is unaffected.
2. **Logging.** The launcher builds the mod's registry crates apart from the
   game's, so `log` reaches no logger. Meridian writes its own
   `horizon-mod-data/meridian/meridian.log`.
3. **No client action bar.** Meridian draws its notices itself where vanilla
   draws the action bar.
4. **No debug overlay state.** F3 is followed from the key (press and release
   with no other key between), as vanilla toggles it.
5. **No biome grass or foliage colours.** Water comes from `biome_effects`;
   grass and foliage from Meridian's vanilla table, so modded biomes use its
   defaults.
6. **Player skins.** Only default skins (by the tab list's UUID); downloaded
   skins are not available.
7. **The shared mod image.** One 1024 x 1024 slot for all mods; another mod
   that uploads to it replaces Meridian's picture until the next upload. On
   large windows the world map is composed at up to 1024 pixels a side and
   stretched.
8. **Saved worlds name code mods.** Horizon records every code mod in a
   single-player world's `level.dat`, client-only ones included; opening such
   a world without Meridian shows the "Missing mods" question once.
9. **Load warning.** `horizon:class/block` is generated when a world opens, so
   the registration logs "block tag … does not exist" at load; the part is
   attached when the world opens (block changes reach the map).
10. **`fits`.** Meridian has no `[native.*]` table, so the launcher may build
    it on later builds that keep what it uses. The scan reads
    `GuiDraw::Text { .. }` (scaled text; a pose ends with its `gui_paint`
    call, so the text must be in the same batch) as building `text::Text`;
    a build that changes `text::Text` is reported as not fitting
    (nightly-2026-11-18 is, against the pin). Other API-named types and
    variants of Meridian's own were renamed for the scan.

## Reproduce

```sh
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
tools/check-adapter.sh --all-targets
MERIDIAN_CARGO_COMMAND=clippy tools/check-adapter.sh --all-targets -- -D warnings
tools/pack-hmod.sh
(cd mixins/meridian && cargo horizon-mod fits ../../target/meridian-0.1.0.hmod)
```

`tools/launcher.sh` runs the launcher with every home folder under
`target/launcher-runtime` (override with `MERIDIAN_TEST_HOME`):

```sh
HORIZON_LAUNCHER=/path/to/horizon-launcher tools/launcher.sh install nightly-2026-11-19-f00125f
HORIZON_LAUNCHER=/path/to/horizon-launcher tools/launcher.sh prepare --minecraft "/path/to/Minecraft-or-Prism"
HORIZON_LAUNCHER=/path/to/horizon-launcher tools/launcher.sh native-build mixins/meridian \
  --version nightly-2026-11-19-f00125f --trust-native-code
```

A signed Nightly plays only with a signed-in account. The game test used the
player's own launcher with a separate instance; `play --arg --control` opens
the socket `horizon-ctl` drives (`key m tap`, `command …`, `screenshot …`).
