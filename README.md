# Meridian

A minimap, world map and waypoint mod for [Horizon](https://github.com/arismoko/horizon) (Minecraft 26.3 in Rust), with [Seed Atlas](https://github.com/DUzzL/Seed-Atlas) built in: biomes, terrain and structures of a world from its seed.

Meridian is an independent project. It is not affiliated with or endorsed by the authors of other map mods.

## Install

This branch ports Meridian to the **arismoko/horizon mod API fork**. It uses the
fork launcher and the flat `.hmod` format; the build pins are in
`mixins/meridian/horizon-mod.toml`. See [the port validation record](docs/FORK_PORT.md)
for the exact tested build and any outstanding limitations. The published macOS
SDK and launcher currently need the local corrections recorded there before a
native game can start; this branch is not yet an out-of-the-box macOS release.

A built package is in [`dist/meridian-0.1.0.hmod`](dist/meridian-0.1.0.hmod)
(SHA-256 in `dist/meridian-0.1.0.hmod.sha256`), for the fork at
`afe471cf15df56439f23cd991d0d4b85563a14fc`. It holds the mod's source; the
fork launcher compiles it into the game.

To build the package yourself, use the fork's `cargo-horizon-mod` tool:

```bash
tools/pack-hmod.sh
```

Install `meridian-0.1.0.hmod` in the instance's `horizon-mods/` folder,
enable `mod.meridian:true` in `horizon-mods.txt`, and use **Build** on the fork
launcher's Mods page. A native source mod must be compiled before it can run.
From source, with the fork launcher on your PATH:

```bash
horizon-launcher native-build mixins/meridian --instance "My Instance" --trust-native-code
```

The game instance must use the exact Horizon revision the mod pins. A newer
published SDK may require a local developer base until a matching game release
ships; the validation record describes this case.

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
- Waypoints, map data and settings are Meridian's own (`<instance>/horizon-mods/.horizon-mod-data/meridian`, settings in the game's config folder). Waypoints shared in chat in the usual text form get a clickable "[Add waypoint]" link that adds them as temporary waypoints (`/meridian waypoint shared` lists the last ones, `/meridian waypoint accept [n]` adds one).
- Seed Atlas users: the commands, options, marker types and defaults match the Fabric version 2.0.

## Development

```bash
cargo test --workspace
```

```bash
tools/check-adapter.sh
```

The API check uses a clean source checkout at the exact fork pin (`HORIZON_DIR`
can name one). It checks the public adapter, not the game-side native callbacks.
The full native build checks those too. For an isolated launcher test, set
`HORIZON_LAUNCHER` to the fork executable and use:

```bash
tools/fork-launcher.sh install nightly
tools/fork-launcher.sh prepare --minecraft "/path/to/your/Minecraft-or-Prism"
tools/fork-launcher.sh native-build mixins/meridian --trust-native-code
```

This helper keeps all launcher settings, saves and logs under `target/fork-runtime`.
Do not confuse an API check with a full native build or an in-game test.

`docs/` holds the feature list, key list, decisions and progress; `docs/SEEDATLAS_PORT.md` describes how the Seed Atlas engine is built in and tested.

## Licence

GPL-3.0-or-later (`LICENSE`). Third-party notices: `THIRD_PARTY_NOTICES.md`.
