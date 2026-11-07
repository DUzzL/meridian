# Audit prompt: Meridian

Paste everything below the line into a new Claude Code session opened in this repository.

---

You are auditing **Meridian**, a minimap, world map and Seed Atlas mod for Horizon, a Rust reimplementation of Minecraft 26.3. Meridian aims for feature, default and hotkey parity with Xaero's Minimap and Xaero's World Map. It also ports Seed Atlas, the owner's own Fabric mod (https://github.com/DUzzL/seed-atlas-xaero), which draws biomes and structure markers computed from the world seed.

**Your job is to find what is wrong, missing or slower than it should be, and to prove each point with evidence.** This is a report-only audit: do not change, commit or push anything in the repository except the report file named below.

## The project

- `TASK.md`: the original brief and phases (German).
- `docs/FEATURES.md`: the parity inventory, 115 rows with IDs (MM-, CV-, INFO-, OV-, RAD-, WP-, WM-, CTX-, SET-, SRV-…), each with priority P0–P3, acceptance criterion and source.
- `docs/KEYBINDS.md`: every key binding and its default.
- `docs/DECISIONS.md`: decisions D-001 to D-038, including **deliberate deviations** from the references. Examples:
  - all-or-nothing structure scans instead of thinning (D-036, replacing D-031);
  - half-resolution terrain refinement and zoom-dependent detail (D-034, D-035);
  - landmark zoom thresholds (D-033, D-036);
  - the redesigned settings screen and the map toolbar (D-037).
- `docs/PROGRESS.md`: what was done per phase, and what was and was not verified in game.
- `docs/CLEAN_ROOM.md`: the allowed reference sources (S1–S4 public mod pages, O1 `options.txt`, O2 the game-written Xaero config files, O3 observation) and what is forbidden.
- `docs/HORIZON_API.md` lists Horizon API gaps; `docs/SEEDATLAS_PORT.md` describes the port.
- **Code** lives in `mixins/meridian/`:
  - `bridge/`: Rust injected into Horizon. It has a `VERSION` constant that must equal `BRIDGE_VERSION` in `engine/lib.rs`.
  - `engine/lib.rs`: the adapter to Horizon's native mod API.
  - `engine/gpu.rs`, `engine/gpu_map.rs`: rendering.
  - `engine/terrain_sampler.rs`: vanilla terrain for Seed Atlas.
  - `engine/core/`: the game-independent logic (minimap, worldmap, radar, waypoints, settings_screen, seedatlas, …).
  - `engine/seedatlas/`: the Seed Atlas generator port.
  - `engine/host/`: host traits.

## Hard rules (stop and ask the user before breaking any)

1. **Clean room.**
   - Never open, list or read anything in folders named `xaero-decompiled` or `xaerolib-decompiled`.
   - Never read Xaero `.class` or `.jar` contents, and never copy Xaero assets (textures, icons, sounds, language files).
   - Allowed Xaero sources:
     - its public mod pages;
     - the game-written config files under `~/Library/Application Support/PrismLauncher/instances/26.3/minecraft/config/xaero/` (option names, defaults, and the radar category tree in `default_radar_categories_*.json`);
     - observation of the running mod, if the user provides it.
   - The Seed Atlas Fabric source on GitHub is the owner's own code and may be read freely.
2. **Do not touch the user's own game or launcher.**
   - Do not start, quit or click in the user's running Horizon or the Horizon launcher.
   - Run your own test game with its own game directory.
   - Building with the launcher command (below) selects the new build for the user's `horizon` instance. Ask before the first build.
3. **Report only.** Make no code changes, commits, pushes, branches or PRs. Write the report to `docs/AUDIT_REPORT.md`.
4. **Hygiene.**
   - Do not write to the system clipboard.
   - Never print credentials or tokens.
   - Do not send world seeds anywhere.
5. Use the repository's tooling. Run anything that takes longer than about two minutes in the background.

## How to build, test and run

Run from the repository root.

**Tests and checks:**
- `cargo test -p meridian-core --lib` runs the core tests, about 200, in seconds.
- `cargo test` runs everything, including the Seed Atlas parity test, which takes about 10 minutes in debug.
- `cargo clippy --all-targets` should give no warnings.
- `tools/check-adapter.sh --release` compiles the adapter against the pinned Horizon commit with the mixins applied, and checks that the lockfile only adds Meridian's own crates.

**Ignored measurement tests** (run by hand):
- `cargo test -p meridian-core --release --test perf -- --ignored --nocapture` measures a large explored map.
- `tile_cost` in `engine/core/src/seedatlas.rs`.
- `lattice_columns_match_single_columns` in `engine/terrain_sampler.rs`. It needs `MERIDIAN_RESOURCES=<version dir>/.local/horizon-client/26.3`. It lives in the adapter crate, which only builds through `tools/check-adapter.sh`: copy that script to your scratch folder, set `root` in the copy to this repository's absolute path, change its final `cargo check` to `cargo test`, and run it with `--release --lib lattice_columns -- --ignored --nocapture`.

**Game build** (ask first, see rule 2):

```
/Applications/Horizon.app/Contents/MacOS/horizon-launcher native-build mixins/meridian --instance horizon --trust-native-code
```

It prints the build name `native-66f3981-<hash>`. The client lives in `~/Library/Application Support/horizon-launcher/versions/<build>/`.

**Test game:** use `horizon-ctl` from that version folder, with `HORIZON_HOME` pointing into your own scratch folder:

```
horizon-ctl launch --world <AuditWorld> --creative --wait --game-dir <your scratch game dir> --width 1512 --height 891
horizon-ctl wait-ready
```

Then drive it with `key <name> tap|down|up`, `chat "/command"`, `scroll-at X Y N`, `click-at X Y [right]`, `drag-at …`, `screenshot <file>`, `state` and `quit`. Run `horizon-ctl help` for more.

Known quirks of the test tool:
- At 3024×1720 physical pixels the GUI scale is about 6.7, so the GUI is about 450×256 pixels. Compute click coordinates from screenshots accordingly.
- `click-at` uses the previous cursor position. Move the cursor first with `scroll-at X Y 0`, then click. On the world map, `scroll-at … 0` can still zoom.
- `key <k> up` does not send key-release events to mods. Hold-to-show features (Tab faces) cannot be released through it.
- `quit` your test game when done. Do not kill the background shell that launched it while the game should keep running.

**Default keys:**

| Key | Action |
|---|---|
| M | world map |
| Y | settings |
| U | waypoint list |
| B | new waypoint |
| Z | enlarge minimap |
| Tab | entity faces while held |
| Num+ | instant waypoint |
| ] | world map settings |

`/meridian …` and `/seedatlas …` are the commands.

## What to audit

### A. Xaero parity

For **every row of `docs/FEATURES.md`**, decide one of these statuses:
- Implemented
- Partial
- Missing
- Deviates (intended, cite the DECISIONS entry)
- Not verifiable here (say why, e.g. needs a server, another player or a claims mod)

Check the code path, and check in the test game wherever it can be observed.

Pay particular attention to:
- **Defaults** of every option against the O2 config files: option names map to Meridian's own names in `engine/core/src/config.rs`.
- **Key bindings** against `docs/KEYBINDS.md` and O1.
- **Entity radar** against `default_radar_categories_client.json`:
  - colours per category;
  - render order, including tamed animals;
  - dot size, height limit (players 2050), height fade, maximum entity count;
  - which entities are excluded (item frames, glow item frames, armor stands as "other");
  - when icons and names show. The numeric `icons` and `names` codes are not documented. Say what you can infer and what needs observation.
- **Cave mode:** auto roof sizes, toggle delay, legible cave maps, extra zoom.
- **Waypoints:**
  - sets, colours, deathpoints, temporary waypoints;
  - sharing, teleport format;
  - the world/dimension dropdowns, import/export.
- **World map:** zoom and pan, the right-click menu, dimensions with Nether scaling, export, multiplayer map selection.

### B. Seed Atlas parity

Compare the port with the Fabric mod's source:
- seed detection and storage per world/server (the seed must never leave the client);
- the `/seedatlas` commands;
- biome colours and the terrain look;
- structure kinds and their positions. The parity test covers generation: confirm it passes and what it covers.
- marker zoom visibility (`visible_at_zoom` in `engine/core/src/seedatlas.rs` against the Fabric `visibleAtZoom`);
- completed structures, nearest biome/structure search, biome highlighting.

For each difference, say whether it is a bug or a deliberate deviation recorded in DECISIONS (D-027 to D-038).

### C. Performance

Measure in the test game. Report numbers, method and repeat count.
- **CPU** of the client process (`top -l 2 -s 4 -pid <pid> -stats cpu`) in these states:
  - playing without the map (baseline);
  - world map open and idle;
  - right after zooming far out, and how many seconds until it settles back to baseline;
  - after zooming in to 0.7x and back out (should need no recomputation);
  - with Seed Atlas on and off.
- **Profile** hot spots with `sample <pid> 3`. Per-thread load: Seed Atlas workers are named `meridian-seedatlas-*` and should run at lowered priority; map workers are `meridian-worker-*`.
- **Memory** (RSS) before, during and after the map is open. Tiles are freed two seconds after the map closes; note that the allocator may keep freed pages.
- **Frame time:** whether the map or the minimap cause stutter when panning, zooming or walking.
- **Code paths** that do avoidable work per frame. Examples: allocations or string formatting per marker or per entity, sorting large lists each frame, re-uploading textures whose revision did not change.

### D. Code quality and rules

- **Bugs:** logic errors, panics (`unwrap`/`expect` on data from disk or the network), races between the worker threads and the main thread, unbounded caches, state not reset when screens or worlds change. The minimap vanishing after Escape was one such bug.
- **Tests:** what the tests cover, important gaps, any test that asserts the wrong thing.
- **Persistence:** atomic writes, recovery from broken files (`.broken`), behaviour on world or server switch.
- **Clean-room compliance:** no Xaero code, identifiers or assets copied, and no traces of decompiled sources.
- **Code comments** are English only.
- **No Claude attribution:**
  - git authors, committers and trailers must not name Claude, Claude Code or Anthropic;
  - no session links in commits, PRs or the README.

  Check `git log --format='%an %ae %cn %ce%n%b'`.
- **Bridge version:** `VERSION` in `bridge/bridge.rs` equals `BRIDGE_VERSION` in `engine/lib.rs`.
- **Docs drift:** claims in PROGRESS or DECISIONS that the code no longer matches.

## Report format (`docs/AUDIT_REPORT.md`, in English)

1. **Summary:** overall state in a few sentences; counts of findings by severity; the top five issues.
2. **Findings**, most severe first. For each:
   - ID (`AUD-001`…), severity (Critical / High / Medium / Low), area (Parity-Xaero, Parity-SeedAtlas, Performance, Bug, Tests, Rules, Docs);
   - what is wrong, in one or two sentences;
   - evidence: `path:line`, command and output excerpt, screenshot path, or the reference row (FEATURES ID, O2 key, Fabric file and line);
   - expected versus actual;
   - a suggested fix (do not apply it);
   - whether it was verified in game or from code only.
3. **Parity matrix:** one table row per FEATURES ID with its status and a short note.
4. **Seed Atlas comparison:** a table of the differences, each marked bug or intended (with its DECISIONS entry).
5. **Performance measurements:** a table of state, CPU %, seconds to settle and RSS, plus the hot spots found.
6. **Not verified:** everything you could not check and why.

Be precise and sceptical. Do not report intended deviations as bugs, but do say if one looks like a poor trade-off. Do not guess: when something can only be settled by observation or by the user, say so.
