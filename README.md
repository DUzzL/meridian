# Meridian

A minimap, world map and waypoint mod for [Horizon](https://github.com/arismoko/horizon) (Minecraft 26.3 in Rust), with [Seed Atlas](https://github.com/DUzzL/Seed-Atlas) built in: biomes, terrain and structures of a world from its seed.

Meridian is an independent project. It is not affiliated with or endorsed by the authors of other map mods.

## Install

Meridian is a native Horizon mod that uses only Horizon's public mod API
(no mixins, no game internals). It is built for the Horizon build pinned in
`mixins/meridian/horizon-mod.toml` (currently Nightly 2026-11-19, `f00125f`);
the launcher may also build it on later builds that keep the API items it
uses (`cargo horizon-mod fits` tells). See [the port record](docs/HORIZON_PORT.md)
for the tested build and the API's current limits.

A built package is in [`dist/meridian-0.1.0.hmod`](dist/meridian-0.1.0.hmod)
(SHA-256 in `dist/meridian-0.1.0.hmod.sha256`). It holds the mod's source; the
launcher compiles it against the game's published SDK.

Copy it into the instance's game folder as `horizon-native-mods/meridian.hmod`,
add the line `mod.meridian:true` to `horizon-mods.txt` beside `options.txt`
(or turn it on on the launcher's Mods page) and use **Build**. From the
command line:

```bash
horizon-launcher native-build --instance my-instance --trust-native-code
```

`native-build mixins/meridian` compiles the source folder into the
instance's build without installing the package, so the Mods page does not
list it.

To build the package yourself, with `cargo-horizon-mod` installed:

```bash
tools/pack-hmod.sh
```

## Keys

| Key | Action |
|---|---|
| M | World map |
| B | New waypoint (on the world map: at the cursor) |
| U | Waypoint list |
| Z | Enlarge the minimap |
| Y | Minimap settings |
| ] | World map settings |
| Num + | Instant waypoint |
| Right Shift | Quick confirm |

Everything else (zoom keys, waypoint sets, radar, cave mode, overlays, dimension switch on the map) is available and unbound by default; see `docs/KEYBINDS.md`. Keys are changed in the game's controls screen.

On the world map: drag to pan, scroll to zoom, arrow keys or WASD move, Space centres on you, right click opens the menu (waypoints, teleport to a spot or a player, share, PNG export, Seed Atlas structure actions). The "?" button in the toolbar on the right lists the controls.

Waypoints: `/meridian waypoint export` writes all of a world's waypoints to a JSON file in `exports/`; `/meridian waypoint import <file or folder>` reads such a file, or the plain-text waypoint files other map mods keep (a world folder with `dim%0`, `dim%-1`, … inside). Waypoint sets are created, renamed and deleted in the waypoint list or with `/meridian waypoint set list | new <name> | use <name> | rename <new name> | delete [name]`.

On servers that show several worlds behind one address, Meridian keeps one map per world spawn; `/meridian map list | use <name> | new <name> | rename <old> <new>` picks and names maps by hand ("Multiplayer Maps" in the world map settings).

Radar: `/meridian radar hide <entity id>` (and `show`, `hidden`) leaves single kinds out; "Fair Play" in the General settings category switches the radar and cave maps off.

Settings profiles are switched, added, renamed and deleted in the General settings category; "Minimap Position" there lets you drag the minimap where you want it.

## Seed Atlas

Seed Atlas is on by default and can be switched off in its settings. In singleplayer the seed is read from the world; on servers set it yourself. The seed never leaves your computer.

- `/seedatlas seed <number-or-text>`: the seed for this world or server
- `/seedatlas dimension-seed <nether|end> <number-or-text>`: a separate seed for the Nether or the End
- `/seedatlas clear [nether|end]`: remove it
- `/seedatlas version [<version>|auto]`: the Minecraft version the world was generated with (1.1 to 26.3)
- `/seedatlas status`, `/seedatlas settings`

On the world map, the toolbar on the right edge switches the seed layer and opens the Seed Atlas settings (both only while Seed Atlas is on), next to zoom, centring, waypoints, settings and the controls. The seed layer fills unexplored areas with the seed's biomes and, from 1.18, its terrain; structure markers (off by default) show where structures generate, with a right-click menu to mark them completed or set a waypoint. The settings also offer nearest biome and structure search, biome highlighting and the world generation version.

The engine is the Seed Atlas engine itself, with the C interface the Fabric version uses as well (`seedatlas-sax`), compiled into the mod: Meridian computes exactly what the Seed Atlas app and the Fabric version do, for every Minecraft version. Building it needs a C compiler, which the Horizon launcher brings.

## Coming from other map mods

- Default keys and most options follow the common conventions of popular minimap and world map mods, so the keys you know work.
- Waypoints, map data and settings are Meridian's own (`<instance>/horizon-mod-data/meridian`, settings in the game's config folder; data of earlier versions is moved there once). Waypoints shared in chat in the usual text form get a clickable "[Add waypoint]" link that adds them as temporary waypoints (`/meridian waypoint shared` lists the last ones, `/meridian waypoint accept [n]` adds one).
- Seed Atlas users: the commands, options, marker types and defaults match the Fabric version 2.0.

## Development

```bash
cargo test --workspace
```

```bash
tools/check-adapter.sh --all-targets
```

The adapter check compiles `mixins/meridian` against the SDK of the pinned
build (`mixins/meridian/.horizon/sdk`, fetched by `cargo horizon-mod`),
without touching the bundle's lock; `MERIDIAN_CARGO_COMMAND=clippy` or `test`
runs those instead. The launcher's native build stays the final check.
For an isolated launcher, set `HORIZON_LAUNCHER` to its executable:

```bash
tools/launcher.sh install nightly-2026-11-19-f00125f
tools/launcher.sh prepare --minecraft "/path/to/your/Minecraft-or-Prism"
tools/launcher.sh native-build mixins/meridian --version nightly-2026-11-19-f00125f --trust-native-code
```

It keeps all launcher settings, saves and logs under `target/launcher-runtime`.
Meridian writes its own log to `<instance>/horizon-mod-data/meridian/meridian.log`.

`docs/` holds the feature list, key list, decisions and progress; `docs/SEEDATLAS_PORT.md` describes how the Seed Atlas engine is built in and tested.

## Licence

GPL-3.0-or-later (`LICENSE`). Third-party notices: `THIRD_PARTY_NOTICES.md`.
