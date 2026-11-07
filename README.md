# Meridian

A minimap, world map and waypoint mod for [Horizon](https://github.com/horizon-rust/horizon) (Minecraft 26.3 in Rust), with [Seed Atlas](https://github.com/DUzzL/Seed-Atlas) built in: biomes, terrain and structures of a world from its seed.

Meridian is an independent project. It is not affiliated with or endorsed by the authors of other map mods.

## Install

Meridian is a Horizon native mod (`mixins/meridian`). Build it into your Horizon instance with the launcher:

```bash
/Applications/Horizon.app/Contents/MacOS/horizon-launcher native-build mixins/meridian --instance horizon --trust-native-code
```

The bundle targets the Horizon revision named in `mixins/meridian/horizon-mixin.toml`.

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

On the world map: drag to pan, scroll to zoom, arrow keys or WASD move, Space centres on you, right click opens the menu (waypoints, teleport to a spot or a player, share, PNG export, Seed Atlas structure actions). The "?" button on the right lists the controls.

Waypoints: `/meridian waypoint export` writes all of a world's waypoints to a JSON file in `exports/`; `/meridian waypoint import <file or folder>` reads such a file, or the plain-text waypoint files other map mods keep (a world folder with `dim%0`, `dim%-1`, … inside).

On servers that show several worlds behind one address, Meridian keeps one map per world spawn; `/meridian map list | use <name> | new <name>` picks a map by hand ("Multiplayer Maps" in the world map settings).

Settings profiles are switched, added, renamed and deleted on the main settings page; "Change Position" lets you drag the minimap where you want it.

## Seed Atlas

Seed Atlas is on by default and can be switched off in its settings. In singleplayer the seed is read from the world; on servers set it yourself. The seed never leaves your computer.

- `/seedatlas seed <number-or-text>`: the seed for this world or server
- `/seedatlas dimension-seed <nether|end> <number-or-text>`: a separate seed for the Nether or the End
- `/seedatlas clear [nether|end]`: remove it
- `/seedatlas version [<version>|auto]`: the Minecraft version the world was generated with (1.1 to 26.3)
- `/seedatlas status`, `/seedatlas settings`

On the world map, "SA" (bottom left) switches the seed layer, "M" and "SA" (top left) open the map and Seed Atlas settings. The seed layer fills unexplored areas with the seed's biomes and, from 1.18, its terrain; structure markers (off by default) show where structures generate, with a right-click menu to mark them completed or set a waypoint. The settings also offer nearest biome and structure search, biome highlighting and the world generation version.

The engine is a Rust port of the Seed Atlas engine and the Seed Atlas Fabric mod's native interface; parity tests compare it with the Fabric version for every Minecraft version (`mixins/meridian/engine/seedatlas/tests`).

## Coming from other map mods

- Default keys and most options follow the common conventions of popular minimap and world map mods, so the keys you know work.
- Waypoints, map data and settings are Meridian's own (`<instance>/horizon-mods/.horizon-mod-data/meridian`, settings in the game's config folder). Waypoints shared in chat in the usual text form are picked up as temporary waypoints.
- Seed Atlas users: the commands, options, marker types and defaults match the Fabric version 2.0.

## Development

```bash
cargo test --workspace
```

```bash
tools/check-adapter.sh
```

`docs/` holds the feature list, key list, decisions and progress; `docs/SEEDATLAS_PORT.md` describes the Seed Atlas port and its fixtures.

## Licence

GPL-3.0-or-later (`LICENSE`). Third-party notices: `THIRD_PARTY_NOTICES.md`.
