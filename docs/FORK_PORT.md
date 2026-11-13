# Meridian on the Horizon mod API fork

Validation record for `port/arismoko-horizon`, based on `xaero-look` at
`adaa608`. The native build and package validate; gameplay validation is tracked
separately below.

## Pins and package

- Game/SDK: `arismoko/horizon` at
  `afe471cf15df56439f23cd991d0d4b85563a14fc` (published arm64 SDK v5).
- Native mixin tool: `e59da9094f0c2fdda6c627fe9eff22dec6a906f0`.
- Launcher source used for testing: `14056d4356414be02f70a3b7dd5e6a67f43ae8f8`.
- Rust: 1.99.1; platform: macOS arm64.
- New format: one flat `.hmod` with `horizon-mod.toml`, Cargo manifests,
  `src/`, `assets/meridian/` and `data/meridian/`.
- The native registry lock preserves the SDK's registry versions and checksums;
  Meridian's own local package entries are included and ambiguous package names
  carry their exact versions. The launcher still checks its immutable game graph.

The SDK above was newer than the signed Nightly used during the initial setup
(`nightly-2026-11-11-514ecf9`). The launcher correctly refused mixing those game
commits. Testing therefore uses an explicitly installed unsigned developer base
of the exact SDK commit. Its game executable was linked from the published SDK;
its companion tools and Java exporters were built from the clean pinned source.
No game assets are included in this repository or the mod package.

## API mapping

The adapter uses `horizon-mod-api`, `ClientHost`, `Registrar` key registrations,
`register_client_commands`, `input_key`, `render_frame_pre`, `render_gui_post`,
and call-scoped `DrawContext` / `WorldDrawContext` values. Chat uses the API's
owned text forms and the game's conversion at the native boundary.

Chunk sampling, biome/radar metadata and physical-window information remain in
pinned native support modules. These write queues owned by the mod; the adapter
no longer imports a module injected into `horizon-client`. The GPU renderer is
inside a `world_drawn` native callback and shares only image/layout data with the
mod. The seed terrain sampler receives an owned snapshot of the already
published registry, biome and item tables; it does not reload the process-global
block registry on a worker.

The lower-level `World::set` and `World::edit_runs` targets are not replaceable
in this SDK. The current bridge instead observes the client's `Game::set_block`
path, which handles normal server block updates, section updates, predictions
and reconciliation. Public API coverage for all client writes is requested in
[arismoko/horizon#4053](https://github.com/arismoko/horizon/issues/4053).

## Validation

Verified so far:

- Workspace tests: 268 core tests, 2 storage-safety tests, 2 host tests,
  2 Seed Atlas unit tests and the Seed Atlas parity fixture test passed.
  Measurement tests remain intentionally ignored; the storage child probe is
  run by the cross-process parent test.
- Workspace clippy with `--all-targets --locked -D warnings` passed.
- The public adapter passed `tools/check-adapter.sh --all-targets` against the
  exact clean SDK source checkout.
- The fork's package writer and `cargo-horizon-mod check --json` accepted the
  regenerated flat package, `target/meridian-0.1.0.hmod`.
- The full optimized native build succeeded against the exact published SDK,
  including all six native callbacks and both game-side support modules.
  This used the local launcher corrections described below.
- An adapter regression fails before, and passes after, preserving the ARGB
  alpha channel when calling the fork's text drawing API. It covers normal,
  scaled and wrapped text. The former RGB mask would make labels transparent.

Game test (2026-11-13, macOS arm64, offline developer single player, creative,
local SDK built from the clean pin with the `system-allocator` feature by
`tools/build-system-sdk.py`):

- The game starts and opens a world with Meridian loaded; no Meridian errors
  in the log.
- Minimap: terrain, water, radar dots, coordinates; it switches to cave mode
  under a roof, as intended.
- World map (M): explored terrain, the Seed Atlas layer beyond it, toolbar,
  zoom, centring (Space).
- Block changes reach the map: a `/fill` of red wool and a `setblock` gold
  block appear at the right place on the world map.
- Waypoints: instant waypoint (Num +), waypoint list (U), export with
  `/meridian waypoint export`; the waypoint survives a restart.
- Settings screen (Y) opens with all categories.
- `/seedatlas status | seed | clear | version [<version>|auto]`, `/meridian
  waypoint set list`.
- Settings and data stay in the isolated test home
  (`target/fork-runtime/HORIZON_HOME`).

Fixed during the game test: `/seedatlas version 1.21.4` and the other
versions were registered as command words, which the fork refuses (words
cannot contain dots), so they were dropped at start-up with a warning. The
version is now one `<version:word>` argument.

Not tested: multiplayer, resource reload, gestures beyond the above, Seed
Atlas structure markers, Nether/End.

Keys sent through `horizon-ctl key` do not reach a mod's `input_key` at this
pin (known, [#3921](https://github.com/arismoko/horizon/issues/3921), fixed
after the pin). The game test sent real key presses through macOS System
Events instead.

## Fork problems found

1. [#3938](https://github.com/arismoko/horizon/issues/3938): developer-install's
   preflight and guide omit the required `horizon-wasm-std.wasm`. Producing it
   with the pinned `cargo-horizon-mod wasm-std` command allowed packaging to
   continue.
2. [#4053](https://github.com/arismoko/horizon/issues/4053): missing portable
   client block-change notifications for map invalidation. The pinned native
   bridge uses the client-side game write path in the meantime.
3. [#4058](https://github.com/arismoko/horizon/issues/4058): native raw-render
   replacement emits `horizon_mod_render` although the SDK library is named
   `mod_render`, causing E0433. A local launcher-only correction, with a
   regression test that fails before and passes after, is recorded in
   `tools/patches/render-installer-library-name.patch`.
4. [#4063](https://github.com/arismoko/horizon/issues/4063): the macOS game app
   bundle omits the SDK's runtime dylibs and the `horizon-native-mods` package
   directory. Bare `--launch-api` succeeds while its bundled counterpart fails
   in dyld; after linking the libraries, package verification exposes the
   missing companion directory.
5. [#4065](https://github.com/arismoko/horizon/issues/4065): the native wrapper's
   private-holder capability marker resides only in a dylib. The launcher
   scans the executable, chooses its legacy route and the game then refuses
   unregistered cache leases. A wrapper-owned static marker makes managed
   startup progress. Local diagnostic corrections for this and #4063 are in
   `tools/patches/macos-native-launch.patch`.
6. [#3877](https://github.com/arismoko/horizon/issues/3877), already reported by
   another tester and fixed in source by #3879: the published macOS SDK at the
   pin still mixes allocators across its dylibs. After the bundle and holder
   problems are corrected, Play aborts with `pointer being freed was not
   allocated`. No duplicate issue was filed.

7. [#4086](https://github.com/arismoko/horizon/issues/4086): the API names no
   folder for a mod's bulk data. `.horizon-mod-data/<mod id>/` is the
   settings store (one file per key); Meridian's map data subfolders there
   make every settings snapshot log `Mod settings snapshot omitted keys`.
   Meridian keeps its data where it was on upstream Horizon until the API
   answers.

These patches are local test aids, not fork PRs. They do not modify the clean
pinned game source or claim that the stock launcher/SDK works on macOS.

## Reproduce checks without touching normal player data

```sh
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
HORIZON_DIR=/clean/pinned/fork tools/check-adapter.sh --all-targets
HORIZON_MOD_TOOL=/path/to/cargo-horizon-mod tools/pack-hmod.sh
```

`tools/fork-launcher.sh` confines launcher homes, configuration and logs to
`target/fork-runtime` (override with `MERIDIAN_FORK_TEST_HOME`). It verifies that
`HORIZON_LAUNCHER` identifies the fork launcher. A full native build additionally
needs a prepared matching game base:

```sh
HORIZON_LAUNCHER=/path/to/fork-launcher \
HORIZON_LAUNCHER_DEVELOPER_SDK=/path/to/exact/sdk \
tools/fork-launcher.sh native-build mixins/meridian \
  --version developer-EXACT_INSTALLED_BASE --trust-native-code
```

For a local developer base, follow the fork's developer-install guide, including
its matching companion programs, exporters and wasm standard-library member.
The source checkout must be clean. The SDK's Rust compiler, target and revision
must match exactly. Testing a developer base offline uses the fork's documented
`HORIZON_LAUNCHER_DEVELOPER_OFFLINE=1` singleplayer path.

On macOS the published SDK at the pin aborts in the allocator (#3877). Build a
local one from the same clean checkout and point
`HORIZON_LAUNCHER_DEVELOPER_SDK` at it:

```sh
tools/build-system-sdk.py /clean/pinned/fork /path/to/horizon-stage /path/to/sdk-system
```

Then start a world in the test home:

```sh
HORIZON_LAUNCHER=/path/to/fork-launcher HORIZON_LAUNCHER_DEVELOPER_OFFLINE=1 \
tools/fork-launcher.sh play --version native-afe471c-BUILT --world Test --creative
```
