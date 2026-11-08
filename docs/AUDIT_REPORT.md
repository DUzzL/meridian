# Audit report: Meridian

> **Status: complete.** This file was written while the audit ran. Section 0 is the work log; sections 1–6 are the report the audit prompt asks for.

Audited commit: `92c0fd5` (`main`, clean tree). Audit date: 2026-11-07. Report only: nothing in the repository was changed except this file.

---

## 0. Work log and method

### Setup

- Read `docs/AUDIT_PROMPT.md`, `TASK.md`, `README.md` and all of `docs/` (FEATURES, KEYBINDS, DECISIONS D-001–D-038, PROGRESS, CLEAN_ROOM, SEEDATLAS_PORT).
- Copied the allowed O2 files (`config/xaero/**`, game-written) to a scratch folder `/tmp/meridian-audit/o2/` and read the O1 `key_gui.xaero_*` lines of the Prism `options.txt`. Nothing else of Xaero was opened: no jars, no `mods/`, no decompiled folders.
- Scratch folder for everything else: `/tmp/meridian-audit/` (outside the repository): logs, screenshots (`shots/`), profiles, the test game's home and game folders, the Fabric clone (`upstream/`).

### Game build: no new build

- The prompt asks to confirm before the first launcher build, because a build selects itself for the user's `horizon` instance.
- No build was needed. `native-66f3981-2096c8af0689` was built at 19:55, after the last source edit (19:53:34), and PROGRESS names it as the build of D-038, i.e. the code of `6d39abb` = `92c0fd5` minus the audit prompt. All in-game checks used that build. The user's instance and launcher were not touched.

### Test game

- Started with that build's own `horizon-ctl`, with its own `HORIZON_HOME=/tmp/meridian-audit/home`, `--game-dir /tmp/meridian-audit/game`, its own control socket `/tmp/meridian-audit/ctl.sock`, world `AuditWorld`, seed 12345, creative, 1512×891 window (3024×1720 physical, GUI 432×246, GUI scale 7).
- Checked that Horizon's `native_instance_dir()` is the game folder, so Meridian wrote only into `/tmp/meridian-audit/game/horizon-mods/...` and `/tmp/meridian-audit/game/config/meridian.toml`.
- Set `pauseOnLostFocus false` in the test game's own `options.txt`.
- Quit the test game at the end. The user's launcher process (pid 88707) kept running untouched.

### Timeline

1. **Tests started in the background:** `cargo test -p meridian-core --lib`, `cargo clippy --all-targets`, `cargo test` (all), then the release perf test.
2. **Code reading split up:** four read-only helper agents, each told the clean-room rules and to write to `/tmp/meridian-audit/`:
   - Xaero parity matrix;
   - Seed Atlas comparison against the Fabric source (cloned from GitHub into `/tmp/meridian-audit/upstream/`, not built);
   - bugs, persistence and tests;
   - per-frame performance paths.

   Every finding of theirs that is listed below as verified was re-checked by me in the code or in the game.
3. **Rules checks:**
   - git authors and trailers;
   - Claude mentions;
   - comment language;
   - bridge version;
   - clean-room traces;
   - assets.
4. **In-game performance:**
   - CPU with `top -l 2 -s 4 -pid` per state;
   - Seed Atlas worker CPU-seconds per action from `ps -M` thread times, 3 repeats;
   - frame times with `horizon-ctl stats`;
   - profiles with `sample <pid> 3`;
   - RSS.
5. **In-game parity checks:**
   - world map, toolbar, right-click menu, "Teleport Here";
   - deathpoints (`/kill`, then a control run with the option off);
   - waypoint list and Options;
   - auto cave mode with a 1×1 and a 5×5 roof;
   - `/seedatlas status`;
   - chat share lines;
   - minimap after Escape;
   - B editor;
   - the radar test with summoned entities, which was inconclusive (see section 6).
6. **Reproductions:**
   - a region file marked as a newer format version (overwritten);
   - ore vein markers (frame-time collapse).
7. **Measurement tests:**
   - `tile_cost`;
   - `tools/check-adapter.sh --release`;
   - `lattice_columns_match_single_columns`, via a scratch copy of the script as the prompt describes.

   `check-adapter.sh` rewrites the ignored `mixins/meridian/engine/Cargo.lock`. It came out byte-identical to the copy taken before, and `git status` stayed clean.
8. **Report:**
   - Written in this order: 5 (performance), 2 (findings), 3 (parity matrix; counts computed from the table), 4 (Seed Atlas), 6 (not verified), 1 (summary).
   - The helper agents' severity suggestions were re-graded. Their cost estimates were checked against measurements: the compose path (D-035) did **not** stutter in game, so it is not reported as a finding.
9. **End state:** the test game was quit, and only the user's launcher kept running. `git status` was clean apart from this file.

### Hygiene

- Nothing was written to the clipboard: "Copy coordinates" was not clicked.
- No credentials were printed.
- The only seed used is the test world's own (12345). It stayed in the local test game.

---

## 1. Summary

### What holds up

- **Build and tests:**
  - All tests pass: 197 core (1 ignored), 2 host, 8 engine, and the Seed Atlas parity test over all 38 versions (578 s, debug).
  - `cargo clippy --all-targets` is clean.
  - `tools/check-adapter.sh --release` passes against Horizon `66f3981` with only Meridian's own crates added.
  - The ignored measurement tests run: `perf`, `tile_cost`, and `lattice_columns_match_single_columns`, which confirms D-034's 99.0 % / 99.5 % / 99.3 % and about 11×.
- **Defaults and keys:**
  - All 31 key defaults match O1.
  - Every option default that has an O2 counterpart matches, except "legible cave maps".
  - Radar colours, render order, dot size and height limits match the O2 category file.
- **Seed Atlas engine:** the port is exact where the fixtures test it, and the seed never leaves the client.
- **Performance in game:**
  - The world map runs at 120 fps without stutter while zooming and panning.
  - After zooming far out, the Seed Atlas workers finish within about 1.6 s (about 6 CPU-seconds, at lowered priority).
  - Zooming back out after zooming in costs nothing (D-035 works).
  - Memory is returned after the map closes.
- **Rules:**
  - Git authorship is clean (69 commits, all by DUzzL, no trailers).
  - Code comments are in English.
  - The bridge versions match (10 = 10).
  - No Xaero code or assets were found.

### What does not

- **Feature parity is incomplete**: 47 of 115 FEATURES rows are Implemented, 43 Partial, 23 Missing. 19 P0 rows are not fully implemented, most of them in waypoints.
- **Bugs reproduced in the test game:**
  - **Deathpoints are deleted as soon as they are made** with default settings.
  - **Ore vein markers duplicate every frame** until the game is unplayable.
  - **An unreadable region file is silently overwritten.**
- **Robustness is weaker than the docs suggest:** several stores silently fall back to defaults and are overwritten (AUD-015), chat input is unbounded (AUD-008), and the adapter's state handling has no tests (AUD-029).

### Findings by severity

| Severity | Count |
|---|---|
| Critical | 0 |
| High | 6 (AUD-001 to AUD-006) |
| Medium | 23 (AUD-007 to AUD-029) |
| Low | 13 (AUD-030 to AUD-042) |

### Top five

1. **AUD-001 (High, verified in game):** deathpoints are removed in the same frame they are created, because "delete reached deathpoints" runs while the player is dead. WP-08 (P0) never works with defaults.
2. **AUD-002 (High, verified in game):** with "Ore Vein" markers on, the shown marker list grows every frame (hit kind 1004/1005 vs job kind 1007). The frame time goes from 7.6 to 51 ms and memory grows within 40 s.
3. **AUD-003 (High, verified in game):** a region file that fails to load (corrupt, newer format, I/O error) is replaced by a fresh region and overwritten within seconds, with no `.broken` copy. Old exploration is lost.
4. **AUD-006 (High, measured in game):** the minimap image is recomposed and re-uploaded in full on every block step. With the enlarged minimap (Z), sprinting gives regular 19–25 ms frames (p99) instead of about 5 ms.
5. **AUD-004 (High, code):** "Teleport" in the waypoint list, with another dimension selected in the dropdown (D-038), sends a plain `/tp` with that dimension's coordinates in the current dimension.

Close behind:
- AUD-005: waypoint sets can't be created (P0).
- AUD-007: "Teleport Here" over unexplored land goes to Y 64; verified.
- AUD-008: any chat line in the share format adds a saved temporary waypoint; verified.

## 2. Findings

Most severe first.

- **Verified in game** means reproduced in the test game; **code** means traced in the code only.
- Path shorthands: `lib.rs` = `mixins/meridian/engine/lib.rs`, `core/` = `mixins/meridian/engine/core/src/`, `seedatlas/` = `mixins/meridian/engine/seedatlas/src/`, `bridge.rs` = `mixins/meridian/bridge/bridge.rs`.
- Screenshots are in `/tmp/meridian-audit/shots/` (a scratch folder, not in the repo).
- None of these fixes were applied.

### High

#### AUD-001 · High · Bug: a deathpoint is deleted as soon as it is created
- **What:** `record_death` adds the deathpoint while the death screen renders, at the player's position. The `frame` hook runs "delete reached deathpoints" every frame with no alive check, at a reach of 4 blocks. With the default `delete_reached_deathpoints = true` (O2 default too), the new deathpoint is removed at once.
- **Evidence:**
  - `lib.rs:1101-1103` (`dead` screen → `record_death`), `lib.rs:2126-2139`.
  - `lib.rs:1585-1590` (`remove_reached_deathpoints(&dim, p.position)` every frame).
  - `core/waypoint.rs:20` (`REACHED_DISTANCE = 4.0`), `core/waypoint.rs:472-486`.
  - In game: after `/kill`, `waypoints.json` showed `"next_id":2` with an empty `default` set, and the U list was empty after respawn (`shots/09-waypoint-list-after-death.png`).
  - Control run with "Delete Reached Deathpoints" off: the deathpoint `{"name":"Death","kind":"death","pos":[96,144,-34]}` stays.
- **Expected vs actual:** expected a skull deathpoint after every death (WP-08, P0). Actually, with default settings there never is one.
- **Fix:** skip reached-deletion while `!host.playing()`, or while a deathpoint was recorded and the player has not yet been more than N blocks away. Add a test of the sequence death → frame → respawn.
- **Verified:** in game, with a control run.

#### AUD-002 · High · Bug/Performance: ore vein markers are duplicated every frame (frame time and memory grow without limit)
- **What:** `view_markers` keeps markers of kinds "not in this scan" by comparing hit kinds with requested job kinds. An ore vein scan is requested as kind 1007 but returns hits typed 1004/1005. So every frame all shown ore veins are kept *and* added again. The list grows by the number of veins on every frame, and every copy is drawn and keyed.
- **Evidence:**
  - `core/seedatlas.rs:1436-1450`: `filter(|h| !enabled.contains(&h.kind))`.
  - `core/seedatlas.rs:326` (`kinds: &[marker::ORE_VEIN]`), `seedatlas/atlas.rs:366-370,422-433` (hit `kind: t` = 1004/1005).
  - In game: Structures on, "Ore Vein" on, map open at 3.0x. Over 40 s: frame CPU avg 7.6 → 51 ms, fps 99 → 19.5, RSS 1263 → 1396 MB (`perf-ingame.log`, `shots/24-orevein-40s.png`).
- **Expected vs actual:** expected a stable marker list, as Fabric's `mergeWithSkipped`, which compares by marker id. Actually the game becomes unplayable within a minute while the map is open.
- **Fix:** compare by marker id (`marker_for(h.kind).map(|m| m.id)`), or map 1004/1005 to 1007 before the membership test. Add a test that calls `view_markers` repeatedly with ore veins and asserts a stable length.
- **Note:** "Ore Vein" is a helper marker and off by default, hence High rather than Critical.
- **Verified:** in game.

#### AUD-003 · High · Persistence: an unreadable region file is replaced, and its old data lost
- **What:** any region load error starts the region empty: corrupt bytes, a truncated file, an I/O error, or a file from a newer Meridian (`UnsupportedVersion`). The next save writes over the old file. No `.broken` copy is kept, and only a `log::warn` is left.
- **Evidence:**
  - `core/engine.rs:616-621` (every non-`NotFound` error → `Err`), `core/engine.rs:707-712` (`unwrap_or_else(... None)` → `Region::new(pos)`), `core/codec.rs:124-126`.
  - In game: with the test game closed, I set the version field of `0_-1.mrg` (150,583 bytes) to 3 and relaunched. Within about 10 s the file was rewritten as version 2, 139,669 bytes, holding only this session's chunks. No `.broken` file appeared.
- **Expected vs actual:** expected that an unreadable file is never overwritten: set it aside, or keep the region read-only. Actually, exploration in that region is silently and permanently lost. This can follow a downgrade after trying a newer build, one damaged tile, or a transient Windows sharing violation.
- **Fix:** rename failing files to `.broken.<timestamp>` (not on `NotFound`). Never save a region whose file is from a newer version. Retry transient I/O errors. Add engine-level tests.
- **Verified:** in game, for the newer-version case. Corrupt and I/O cases from code.

#### AUD-004 · High · Bug: teleporting to a waypoint of another dimension lands in the wrong dimension
- **What:** the waypoint list's dimension dropdown (D-038) may show the Nether while the player is in the Overworld. "Teleport (T)" then sends the plain `teleport_format` (`/tp @s x y z`) with Nether coordinates, in the current dimension. Only another *world* is blocked.
- **Evidence:** `lib.rs:1930-1931` (`here` compares only the world; `dim` comes from the dropdown), `lib.rs:2031-2037` (guard `if !here`, then `teleport_format`). O2: `waypoint_teleport_cross_dimension = true`, `allow_wrong_world_teleport = false`. FEATURES WP-11.
- **Expected vs actual:** expected a cross-dimension command (the `{d}` dimension format, as the map uses), or a blocked button. Actually the player is teleported to the wrong place, possibly inside terrain or high in the air.
- **Fix:** when `dim != s.dimension`, use `world_map.map_teleport_dimension_format` (`/execute as @s in {d} run tp …`), or disable Teleport.
- **Verified:** code only. Not clicked in game.

#### AUD-005 · High · Parity-Xaero: waypoint sets cannot be created or deleted
- **What:** WP-05 (P0) needs sets to be created and switched. `DimensionWaypoints::create_set` exists but only a unit test calls it. The list has "next set" and "Clear Set" only, and neither `/meridian` nor the Options button creates or deletes a set.
- **Evidence:**
  - `core/waypoint.rs:217`; `grep create_set` finds only `core/waypoint.rs:568-569` (test).
  - `core/screens.rs:836-861`.
  - In game: the list's "Options" opens the general settings at "Waypoints", which has no set management (`shots/10-waypoint-options.png`).
- **Expected vs actual:** expected to create, rename and delete sets. Actually only the default set (or sets that arrive with an import) exist.
- **Fix:** add "New set" / "Delete set" to the list (and `/meridian waypoint set new|delete <name>`).
- **Verified:** code and in game.

#### AUD-006 · High · Performance: the minimap image is rebuilt and uploaded in full on every block step, so the enlarged minimap stutters while moving
- **What:** the image key holds the player's block position and one hash over all region revisions under the image. Every block crossing, and every redraw batch in any region under it, recomposes the whole image on the main thread. The image is always one texel per block, up to 2048². The render thread then converts and uploads all of it again. The slime-chunk test runs per pixel instead of per chunk.
- **Evidence:**
  - `lib.rs:1818-1846` (`compose_map`: `origin` from `player.position.floor()`, `revisions` hash, full `size×size` loop), `lib.rs:1861` (slime test per pixel), `engine/gpu.rs:246-264` (full ARGB→RGBA conversion and `write_texture`).
  - In game, sprinting 5 s, frame CPU p99/max:
    - normal minimap: 3.8–4.9 / 4.2–6.6 ms;
    - **enlarged (Z): 19.4–22.4 / 22.9–24.7 ms**, i.e. 2–3 frames' budget at 120 Hz (section 5).
- **Expected vs actual:** expected no stutter when walking (FEATURES performance goals, TASK "kein Ruckeln"). Actually there are regular 20–25 ms frames with the enlarged map. It will be worse with elytra, with a zoomed-out minimap, and on slower machines.
- **Fix:**
  - Snap the image origin to a coarse grid; the shader already gets the centre.
  - Recompose only changed regions and upload sub-rectangles.
  - Compose at the displayed resolution rather than one texel per block.
  - Test slime chunks per chunk.
- **Verified:** in game (measurement), cause from code.

### Medium

#### AUD-007 · Medium · Parity-Xaero: unknown heights become Y 64 for "Teleport Here", B on the map and structure waypoints
- **What:** where the column is unexplored, the right-click menu header shows `Y: ~`, but the action uses `64`. There is no partial-Y teleport (`~`) and no ambiguous-Y detection (Nether roof).
- **Evidence:**
  - `core/worldmap.rs:342` (`height.map_or(64, …)`), `lib.rs:2282` (B key), `lib.rs:2923-2930` (structure waypoints, Y 64 even when the hit has a Y), `core/waypoint_io.rs:76` (import `~` → 64).
  - O2: `partial_y_teleport = true`, `waypoint_partial_y_teleport = true`, `detect_ambiguous_y = true`.
  - In game: "Teleport Here" on an unexplored spot (menu `X: -340, Y: ~, Z: -183`) put the player at exactly `[-339.5, 64.0, -182.5]` (`shots/17-map-rightclick.png`).
- **Expected vs actual:** expected a teleport keeping the Y relative or unknown (`~`), or a safe height. Actually Y 64, which is inside terrain in mountains and in the air over deep valleys.
- **Fix:** make the height optional end to end and send `~` for `{y}` when it is unknown or ambiguous.
- **Verified:** in game.

#### AUD-008 · Medium · Bug: any chat line in the share format adds a temporary waypoint, without limit, and it is saved
- **What:** `client_chat_received` adds every `[Meridian] waypoint "…" at x y z in <dim>` line at once. There is no confirmation, rate limit, count cap or name-length cap. Temporary waypoints are written into `waypoints.json` every 2 s and only dropped at the next load. Any player or the server can fill a user's list and HUD.
- **Evidence:**
  - `lib.rs:1372-1403`, `core/waypoint.rs:155-170` (`parse_shared`), `core/waypoint.rs:361-370` (`save` writes all kinds), `lib.rs:1591-1593` (save every 2 s).
  - The doc comment at `core/waypoint.rs:156` claims bare `x y z` triples are parsed; they are not.
  - In game: 3 such chat lines → 3 `temporary` entries in `waypoints.json` and on the minimap rim.
- **Expected vs actual:** expected a clickable message that creates the waypoint on click (FEATURES WP-10). Actually waypoints are created automatically and unbounded.
- **Fix:** make the line clickable or confirm it. Cap temporaries per dimension. Do not persist `Kind::Temporary`. Limit names to about 32 characters.
- **Verified:** in game.

#### AUD-009 · Medium · Performance: worst-case memory is above 1.4 GiB, and D-027's "only 5×5 regions stay loaded" is not what the code does
- **What:**
  - The release perf test measured **about 1443 MiB** for 300 fully explored regions, which is the default `max_loaded_regions`.
  - Zoomed out, the code only stops *requesting* new regions; already loaded ones (up to 15×15 from close zoom) stay until the 300-region or 96 k-chunk limit.
  - Caches add up: 256 MiB overviews, 256 MB GPU textures (unified memory on Apple Silicon), about 77 MB Seed Atlas tiles, ≤51 MB marker scans, and the `terrain_sampler` caches (never freed).
  - GPU region textures are not cleared on world change or disconnect.
- **Evidence:**
  - Perf test output (section 5); `core/engine.rs:34-40,846-851`.
  - D-027 at `docs/DECISIONS.md:282`.
  - `engine/gpu_map.rs:102,370-384`; `core/seedatlas.rs:489,555`; `engine/terrain_sampler.rs:36`.
- **Expected vs actual:** expected memory bounded as D-027 describes (about 125 MiB of full regions far out). Actually up to about 1.4 GiB of regions plus about 0.6 GB of caches.
- **Fix:**
  - Evict regions outside the 5×5 when far out (make overviews first), or add a byte budget next to the region count.
  - Clear GPU textures of other worlds on world change.
  - Release the `terrain_sampler` caches together with the tiles.
- **Verified:** measurement (perf test) and code. In game the explored area was too small to reach the limit.

#### AUD-010 · Medium · Parity: settings that do nothing
- **What:** four options are shown and described in the settings screen but no code reads them:
  - "Biome Blending" (`terrain.biome_blending`, MM-10): no blending code.
  - "Smooth Entity Dots" (`radar.smooth`, RAD-06): the bridge always interpolates.
  - "Background Prefetch" (`seed_atlas.prefetch_radius`, SA-08).
  - Seed Atlas "Over Explored Terrain" (`overlay_explored`).

  `/seedatlas status` even reports "prefetch Near".

  Fields that are stored but unused, without a UI: `minimap.cave.depth`, `minimap.frame`, `share_world_map_tiles`, `waypoints.teleport_rotation_format`, the whole `minimap.terrain` copy, and the whole `world_map.cave` block. The Terrain "Lighting" and World Map "Map Lighting" rows change the same field.
- **Evidence:**
  - `core/config.rs:105,392,457,470`; `grep` finds no readers outside `config.rs`, `screens.rs`, `settings_screen.rs` and tests.
  - The renderer draws the seed layer only under the terrain (`lib.rs:2477-2478`, `engine/gpu_map.rs:442-459`).
  - In game: the `/seedatlas status` action bar shows "prefetch Near".
- **Fix:** implement them, or hide the rows until they work.
- **Verified:** code. The status output was seen in game.

#### AUD-011 · Medium · Bug: on the world map, viewing another dimension saves waypoints into the current one
- **What:** while the map shows another dimension (`worldmap.toggle_dimension`), "Create Waypoint", B and "Create waypoint" on a structure use the viewed dimension's coordinates but save into the player's current dimension. Right-click hit-testing and heights also come from the current dimension, not the drawn one.
- **Evidence:** `lib.rs:2678-2683,2776-2780,1931,2250-2256,1130,2282,2579-2580,2919-2925` (`book_here()`).
- **Expected vs actual:** expected the waypoint to land in the dimension shown. Actually it is saved in the current dimension, at the wrong place.
- **Fix:** when `viewer` is set, use the viewed dimension for `waypoint_place`, hits and heights.
- **Verified:** code only.

#### AUD-012 · Medium · Bug: cave layers can receive chunks read for the other layer
- **What:**
  - Chunk snapshots don't carry the read mode (surface or cave ceiling). `set_layer` runs before `tick` in the same frame, so answers to last frame's requests are applied to the new layer.
  - The bridge de-duplicates requests by position only, so a re-read in the new mode can be dropped. The wrong tile then persists in `caves/<y>/…` or the surface files.
- **Evidence:** `engine/host/lib.rs:165-174` (`ChunkSnapshot` without mode), `lib.rs:1577,1581`, `core/engine.rs:738-753`, `bridge.rs:210-216`, `bridge.rs:383-407` (1.5 ms budget).
- **Fix:** add the mode to snapshots and drop mismatches. Key bridge requests by `(pos, mode)` and clear them on a layer switch.
- **Verified:** code only, medium confidence. Frequency depends on render distance and how fast the player moves through layers.

#### AUD-013 · Medium · Bug: the world-map texture cache can show stale terrain, or another map's terrain
- **What:** GPU region textures are keyed by a hash of `engine.context()` and the layer, without the server map name (D-029). `Region::revision` restarts at 0 for every loaded region and is reused when equal. So a texture can be taken for content it no longer matches, for example after switching server maps or reloading a region that reaches the same revision count.
- **Evidence:** `lib.rs:2382-2388` (no `engine.map()`), `core/region.rs:62,189`, `engine/gpu_map.rs:325-327,474`.
- **Fix:** include the map name and an engine epoch in the key, or draw revisions from one global counter.
- **Verified:** code only. Needs a server with several maps to show.

#### AUD-014 · Medium · Bug: Seed Atlas tiles and markers can get stuck after a seed, version, world-type or kind change
- **What:**
  - `configure_dimensions` bumps `generation` and clears `pending` but leaves queued `Work` in the worker queue. `commit` re-queues that old work and puts the job back into `pending`.
  - Its result is discarded as stale before `pending.remove`. The job then stays "running" forever.
  - Tiles recover only after the map has been closed for 2 s; marker scans until the view box changes.
- **Evidence:** `core/seedatlas.rs:886-915`, `1061-1063`, `1144-1168` (`Some(w) => w`; `None if self.pending.contains(&job) => continue`).
- **Fix:** clear the queue on generation changes, or drop `waiting` entries of an old generation. Remove stale jobs from `pending` in `poll`. Add a test.
- **Verified:** code only. A settings change behind the open map (D-032) triggers it.

#### AUD-015 · Medium · Persistence: several stores silently fall back to defaults and are overwritten later
- **What:** a parse error, and for some any read error, turns these files into defaults without keeping a copy. The next change then writes over them:
  - `completed.json` (all completed structures);
  - `seeds.toml` (all seeds of all servers);
  - `maps.toml` (a server's spawn-to-map table);
  - `waypoints.json` on any read error other than a parse error;
  - `meridian.toml` on a read error, including invalid UTF-8.
- **Evidence:** `core/completed.rs:96-103`, `core/seedatlas.rs:98-105`, `core/subworld.rs:50-57`, `core/waypoint.rs:334-342` (`Err(_) => default`, path kept), `core/config.rs:706-716`. Parse errors of `waypoints.json` and `meridian.toml` do go to `.broken`; the others don't.
- **Fix:** one shared `load_or_set_aside` helper. `NotFound` gives the default; a parse error moves the file to `.broken.<timestamp>`; other I/O errors give a read-only state that refuses to save.
- **Verified:** code only.

#### AUD-016 · Medium · Parity-Xaero: the radar cap drops the wrong entities, and the world-map radar ignores the category limits
- **What:**
  - The list is sorted by `(render order, id)` and truncated to `max_entities` (1000) *before* the in-map check. Past 1000, players (order 6), then tamed and hostile mobs, are dropped first, and off-map entities count against the cap.
  - The world map applies no height limit, fade, cap, order or dot size (fixed 1.2 units).
- **Evidence:** `core/radar.rs:271-277`, `lib.rs:2571-2577`, `core/worldmap.rs:441-470`. O2 `entityNumber 1000`, `heightLimit 20/2050`, `heightBasedFade`.
- **Fix:** filter to the map area, sort by distance, cap, then sort by render order for drawing. Share the filter with the world map.
- **Verified:** code only. The in-game radar check was inconclusive (section 6).

#### AUD-017 · Medium · Parity-SeedAtlas: biome highlighting is hidden by explored terrain and drawn at layer opacity
- **What:** the highlight is baked into the tiles, which are drawn under explored terrain at `biome_opacity`. Fabric draws the focus pass above the terrain, fully opaque.
- **Evidence:** `core/seedatlas.rs:734`, `core/seedatlas_look.rs:529-556`, `lib.rs:2418`; Fabric `SeedAtlasBiomeOverlayRenderer.java:403-418,430-431` (commit `58c70ff`).
- **Fix:** add a foreground seed-tile pass (also needed for AUD-010 "Over Explored Terrain").
- **Verified:** code only.

#### AUD-018 · Medium · Parity-SeedAtlas: the marker scan box uses `window px / zoom`, but Meridian's zoom is not pixels per block
- **What:** the Fabric formula assumes the map scale is physical px per block. Meridian's map draws `zoom · 0.633 · unit` GUI px per block, where unit = round(window height / 594). So the scan box is about 0.63× the real view on windows under 891 px tall (markers missing at the east and south edges), and 1.3–1.9× on larger windows (dense kinds hidden earlier than needed, larger scans).
- **Evidence:** `lib.rs:2497-2501`, `core/worldmap.rs:30,235-237`, `core/minimap.rs:137-141`; Fabric `SeedAtlasStructureMarkerSource.java:103-110`.
- **Fix:** compute the view in blocks from the map transform (`gui / s.map.scale(unit)`, as the tile code does). Keep `zoom` only for the thresholds.
- **Verified:** code only. A small-window test is needed to see the gaps.

#### AUD-019 · Medium · Parity-SeedAtlas: below the detail zoom, the Nether floor and End heights are also switched off
- **What:** D-035 limits the detailed *Overworld vanilla surface* to ≥ 1 px per block. The same `refine_allowed` gate also blocks the Nether floor (Fabric: up to step 16 at any zoom) and End island heights (up to step 8). The Nether turns into flat colours at moderate zoom, with a band of cosmetic relief at step 32.
- **Evidence:** `core/seedatlas.rs:1237-1249`, `core/seedatlas_look.rs:1069-1088,1207-1221`; Fabric `SeedAtlasBiomeOverlayRenderer.java:670-677,1241-1272`.
- **Fix:** apply `refine_allowed` only to the Overworld surface pass.
- **Note:** outside D-035's stated scope, so treated as unintended, and a poor trade-off.
- **Verified:** code only. Visuals not checked in the Nether or End.

#### AUD-020 · Medium · Parity-SeedAtlas/Docs: the all-or-nothing scan budget hides kinds that D-036 and `visible_at_zoom` promise
- **What:** `Atlas::scan_complete` drops a kind when the box has more than 2²⁰ (cells ≤ 128 blocks) or 2¹⁵ cells, or more than 8192 hits (D-036). Effective limits:
  - End gateways disappear beyond radius 7168 (about 0.25x);
  - End cities and End ships beyond 28672;
  - fortresses and bastions beyond 32768;
  - mineshafts never show in their 8192–12288 band;
  - wells, geodes and treasure are lost at radius 8192, and geodes in practice beyond about 3072 (8192-hit cap).

  D-036 says "Nether- und End-Landmarken sowie der Spawn bleiben immer".
- **Evidence:** `seedatlas/atlas.rs:841-877`, `core/seedatlas.rs:483-528`, `docs/DECISIONS.md:454`. Per-kind table in section 4.
- **Fix:** raise the budget for cheap region kinds (End city, fortress, bastion), or scan landmarks per tile. Otherwise align `visible_at_zoom`, D-031 and D-036 with the real limits.
- **Note:** all-or-nothing itself is intended (D-036); this is about its side effects. On a 3840 px window, End cities already vanish at the default minimum zoom.
- **Verified:** code (arithmetic). Not checked in game.

#### AUD-021 · Medium · Performance: dragging a settings slider writes and `fsync`s the config for every drag event, on the main thread
- **What:** each drag event sets `settings_changed`, then `engine.set_settings(clone)` and `save_config()`, which goes through `write_atomic` and `sync_all`. On macOS `sync_all` is `F_FULLFSYNC`, which typically takes several milliseconds.
- **Evidence:** `core/settings_screen.rs:668-674`, `lib.rs:2000-2004`, `core/persist.rs:48-58`.
- **Fix:** apply the value live and save on release, or debounce.
- **Verified:** code only. Slider dragging can't be driven reliably with the test tool.

#### AUD-022 · Medium · Bug: multiplayer "map per world spawn" can use the previous server's spawn
- **What:** the stale-spawn guard (D-029) only covers `client_level_load`, which passes `None`. From the next frame `update_map` uses `bridge::world_spawn()`, which mirrors `game.spawn` with no idea which connection announced it. A spawn left from server A can create a bogus `map-N` on server B, or attach A's spawn to B's default map. Chunks read before the switch are flushed into the last shown map.
- **Evidence:** `lib.rs:1441`, `lib.rs:1529` → `lib.rs:455`, `bridge.rs:336`, `core/subworld.rs:90-106`, `core/engine.rs:259-270`.
- **Fix:** clear the spawn in the bridge on disconnect or level change, or accept only spawns announced after the level loaded. Hold chunk saves until the map is decided.
- **Verified:** code only, medium confidence. Depends on whether Horizon resets `game.spawn`; needs a server network.

#### AUD-023 · Medium · Persistence: quitting does not wait for region saves
- **What:** `client_stopping` calls `engine.flush()`, which only *queues* save jobs, and returns. The saves reach the disk only if Horizon later drops the hooks and the worker pool joins its threads. PROGRESS states "beim Beenden werden offene Speicher-Jobs abgearbeitet", which holds only under that assumption.
- **Evidence:** `lib.rs:1475-1481`, `core/engine.rs:574-587`, `core/worker.rs:130-137`.
- **Fix:** add `flush_and_wait(timeout)` and call it from `client_stopping` and `client_disconnecting`.
- **Verified:** code only. In the test game, `ctl quit` did leave fresh region files (mtimes at quit), so Horizon probably drops the hooks today, but nothing guarantees it.

#### AUD-024 · Medium · Parity-Xaero: the waypoint list has no sorting, no multi-selection and no distance column
- **What:** WP-03 (P0) asks for sorting by name, distance, creation and initials, and for multi-selection. O2 has `waypoint_distance_in_menu = true`. The list shows insertion order, one selection, and no distance.
- **Evidence:** `core/screens.rs:666-918`; in game `shots/09-waypoint-list-after-death.png`.
- **Fix:** add a sort selector, a distance column and multi-selection.
- **Verified:** code and in game.

#### AUD-025 · Medium · Parity-Xaero: many options can only be changed in `meridian.toml`
- **What:** SET-01 (P0) requires every option to be reachable in the menus. Missing from the settings screen:
  - waypoint, map and player teleport formats;
  - waypoint opacities in the world and on the minimap;
  - the km threshold;
  - undiscovered opacity;
  - arrow scale;
  - radar name scale and max entities;
  - the light-overlay minimum;
  - the cave allowed-dimensions list;
  - the Seed Atlas Y values beyond 9 presets;
  - performance budgets.
- **Evidence:** `core/screens.rs:977-1371` (option catalogue), `core/settings_screen.rs:137-260`.
- **Fix:** add rows (text fields for formats, sliders for opacities).
- **Verified:** code. The settings screen was looked at in game.

#### AUD-026 · Medium · Parity-Xaero: Overworld waypoints are not shown, scaled 1:8, on the Nether map
- **What:** WM-07 acceptance: "Overworld-Wegpunkte im Nether richtig skaliert". Only the shown dimension's waypoints are drawn. PROGRESS Phase 4 lists this as open.
- **Evidence:** `lib.rs:2578-2583`.
- **Fix:** also draw the paired dimension's waypoints, scaled by `dimension_scale`, perhaps dimmed.
- **Verified:** code only.

#### AUD-027 · Medium · Parity-Xaero: the "legible cave maps" default differs from O2 and is not recorded
- **What:** `legible_caves` defaults to `true`; O2 has `minimap_legible_cave_maps = false` and world-map `legible_cave_maps = false`. The code comment explains the choice (an old setting migration), but DECISIONS does not record it as a deviation.
- **Evidence:** `core/config.rs:158-159`; PROGRESS Phase 6 ("Standard an").
- **Fix:** check what "off" looks like in the reference (O3), then either change the default or add a D-entry.
- **Verified:** code. What "off" means in the reference needs observation.

#### AUD-028 · Medium · Performance: avoidable work in every frame
Each item is small, but together they cost main-thread time on every frame:

- **Structure markers** (map open, structures on):
  - `view_markers` copies and merges every cached hit each frame (`core/seedatlas.rs:1424-1450`).
  - The caller filters every hit again (`lib.rs:2502-2505`).
  - Each visible marker builds a `StructureKey` with 3 `String`s, and `is_completed` → `near()` → `group()` clones 3 more (`lib.rs:2525-2526`, `core/completed.rs:49-67,142-151`).
- **Every Meridian screen, every frame and input event:**
  - `waypoint::book_places` reads the `worlds/` folder tree from disk (`lib.rs:1922`, `core/waypoint.rs:286-313`);
  - the full `Settings` is cloned (`lib.rs:1955`);
  - completed structures are counted 29× (`lib.rs:1960-1966`).
- **`draw_map`:** sorts a dead `order` Vec of up to 9409 entries every frame (`lib.rs:2401-2402`).
- **`update_seeds`, every frame even with Seed Atlas off:** clones the Seed Atlas settings, formats the context and calls `available_parallelism()` (`lib.rs:2966-3006`, `core/seedatlas.rs:927-933`). It also starts the 4 Seed Atlas worker threads when Seed Atlas is disabled.
- **Radar:**
  - The bridge copies all client entities every frame. After the minimap hides (F1, F3, screens, minimap off) the request stays on (`lib.rs:1638-1641,1671-1675,1789`, `bridge.rs:337-339`).
  - Per entity per frame there are 4–6 String allocations and a global-mutex texture lookup (`lib.rs:2160,2177-2183`).

**Fix:** cache what only changes on events (marker list until a scan arrives, book places until a save, completed counts), delete `order`, start workers lazily, and expire `want_entities` unless renewed.

**Verified:** code only. No stutter was measured from these on the M6 (section 5).

#### AUD-029 · Medium · Tests: the adapter has no tests, a few tests check less than their names claim, and the parity fixtures miss what production uses
- **Coverage:**
  - Core: 197 passed, 1 ignored; host: 2; engine crate: 8; parity: 1 (578 s debug). All green.
  - Clippy is clean, and the adapter check passes.
  - But `lib.rs` (3138 lines of hooks and state, where AUD-001, AUD-004 and AUD-011 live) and `bridge/*` have **no tests**.
  - The `view_markers` merge (AUD-002), corrupt-file recovery at engine level (AUD-003), shutdown, the multiplayer map choice and Seed Atlas configuration changes are untested.
- **Weak tests:**
  - `dimension_change_drops_stale_work` (`core/engine.rs:1093-1123`) passes because the test host labels every snapshot as Overworld, not because of the generation check it is named after.
  - `evicts_least_recently_used_and_saves_them` (`core/engine.rs:1211-1240`) asserts `file.exists() || region.is_some()`, which is nearly always true.
  - `cave_layers_read_again_and_keep_their_own_files` only checks that files exist, not that their content differs.
- **Parity fixtures:**
  - They use **3 seeds** (`tools/seedatlas-fixtures/fixtures.c:26`), Large Biomes only for seed 0, and boxes ≤ ±3000 (±30 000 for mansions).
  - They cover no steps 32/128/512 and no 128×128 areas (both used in production), no height steps 8/32, and nothing above `atlas.rs`.
  - Heights are compared with a 1e-6 relative tolerance (`seedatlas/tests/parity.rs:187`).
- **Fix:** move adapter state transitions into the core so they can be tested; add the missing cases listed; add the production steps to `fixtures.c`.
- **Verified:** test runs plus code.

### Low

#### AUD-030 · Low · Performance (intended trade-off, D-035): the seed layer is recomputed on every map reopen and layer toggle
- **What:** tiles are dropped 2 s after the last request. Reopening the map, or switching the layer back on after more than 2 s, costs 4.1–4.4 CPU-seconds (about 1.6 s wall); opening and zooming out costs about 6.4 CPU-seconds (section 5).
- **Assessment:** intended (D-035, memory only). It is a reasonable choice but costly for players who glance at the map often. A small time- or size-bounded keep (e.g. 30–60 s, or about 64 MB) would remove most of it.
- **Verified:** in game.

#### AUD-031 · Low · Bug: footsteps are never cleared and are drawn unscaled in other dimensions
- **What:** `WorldMap::footsteps` lives in the state for the whole session. It is not cleared on level load, disconnect, dimension or map change. It is drawn even when the map views another dimension, without 1:8 scaling.
- **Evidence:** `lib.rs:1554,2602,2338-2339`, `core/worldmap.rs:296-308`.
- **Fix:** clear on level load, disconnect and `set_map`; hide when the shown dimension differs.
- **Verified:** code only.

#### AUD-032 · Low · Bug: key releases under foreign screens are lost
- **What:** the `key` hook returns when `!host.playing()`, and `screen_key_released_pre` only handles the faces key. Holding Z (hold mode), opening the inventory and releasing Z leaves the minimap enlarged. The HUD fallback that fixed D-034 resets only the map state, not other Meridian screens or `waypoint_place` (D-038 promises a reset to "auto").
- **Evidence:** `lib.rs:1317-1320,1239-1246,1649-1657`, `core/minimap.rs:653-656`.
- **Fix:** route releases from `screen_key_released_pre` to `on_key(false)`, and extend the fallback to `s.screen` and `waypoint_place`.
- **Verified:** code only. `horizon-ctl key … up` does not deliver releases to mods (a known quirk), so this can't be tested in game.

#### AUD-033 · Low · Bug: panics on unusual files
- **What:**
  - An explicit empty `"sets": {}` in `waypoints.json` makes "next set" divide by zero (`core/waypoint.rs:207-214`, `core/screens.rs:840-846`).
  - `unescape` slices a `&str` at a byte offset inside a UTF-8 character (`core/waypoint.rs:271-273`). It runs on every frame of every Meridian screen via `book_places` (`lib.rs:1922`), so a folder such as `sp-%a€` under `worlds/` crashes every Meridian screen.
- **Fix:** refill the default set after load; work on bytes or check char boundaries.
- **Verified:** code only.

#### AUD-034 · Low · Parity-Xaero: registered keys with no handler, and no dimension switch on the map toolbar
- **Keys:** `players.alt_list`, `players.tracked_map`, `players.tracked_world`, `overlay.claims`, `server_settings`, `worldmap.server_settings` and `quick_confirm` are registered in the controls list but have no handler. `quick_confirm` is even bound to Right Shift by default. Evidence: `core/keybinds.rs:39-44,48,51`; no arms in `lib.rs:853-930,2272-2286`.
- **Dimension switch:** the only way to switch the map's dimension is `worldmap.toggle_dimension`, which is unbound by default (O1). The new toolbar (D-037) has no dimension, map-selection or cave button (`core/worldmap.rs:847-880`; `shots/02-map-open.png`).
- **Fix:** implement the keys or label them "(not available)", and add a dimension button.
- **Verified:** code and in game (toolbar).

#### AUD-035 · Low · Parity-Xaero: smaller waypoint gaps against O2
- **Rotation format:** `teleport_rotation_format` (O2 `default_waypoint_teleport_rotation_format`) is never used, so a waypoint's yaw never affects a teleport (`lib.rs:2033,2672`).
- **Temporary waypoints:** created local; O2 has `temporary_waypoints_global = true` (`core/waypoint.rs:70`).
- **Switch set on death:** missing (O2 `switch_auto_waypoints_on_death = true`).
- **Max draw distance:** no dimension scaling (O2 `waypoint_max_distance_dimension_scale`; no effect at the default 0).
- **Editing:** every edit moves the waypoint to the top, and an edit from the world map moves it into the current set (`lib.rs:2694-2698`, `core/waypoint.rs:391-400`).
- **Import:** reverses the order in each set and doesn't restore the current set (`core/waypoint_io.rs:166-212`).
- **In-world mark opacity:** 0.8 × 0.6 = 48 % (`lib.rs:1690,1707`), while D-019 records "60 %".
- **Verified:** code only.

#### AUD-036 · Low · Parity-SeedAtlas: smaller differences from the Fabric version
Fabric reference: `DUzzL/seed-atlas-xaero` `main` at `58c70ff`. Details and Fabric lines are in section 4.

- **Spawn marker:** uses the detailed search and lacks "(approx.)" (`core/seedatlas.rs:836-846`).
- **"Create waypoint" on a structure:**
  - writes to the player's dimension with Y 64, colour 11, and opens no editor (`lib.rs:2919-2930`; overlaps AUD-007 and AUD-011);
  - "Copy coordinates" leaves out Y (`lib.rs:2920`).
- **Tooltips:**
  - the marker tooltip lacks the camp biome and is multi-line (`lib.rs:2542-2557`);
  - the biome hover tooltip uses newest-version names (`biomes::name` instead of `name_in`, `lib.rs:2620`).
- **Icons and highlighting:**
  - the Ocean Ruins icon lacks its frame (`engine/icons.rs:244`);
  - selecting a biome doesn't switch highlighting on (`core/screens_seedatlas.rs:579-586`).
- **Settings and commands:**
  - the per-world Minecraft version is stored per settings profile (`core/config.rs:460,585`);
  - `/seedatlas` has no tab completion although the suggestion lists exist (`core/seedatlas_commands.rs:87-101`);
  - replies are joined into one action-bar line (`lib.rs:1346-1349`; seen in game);
  - Y values are limited to 9 presets.
- **Nearest search:** results are applied after the screen was left, or in another world (`lib.rs:3094-3137`).
- **Zoom:** the seed layer does not extend the minimum zoom to 1/512 as Fabric does (`core/worldmap.rs:17-22`).
- **Dead code:** `scan_spread` and `thin_evenly` are dead since D-036 (`seedatlas/atlas.rs:770-902`).
- **Verified:** code. The `status` output was seen in game.

#### AUD-037 · Low · Persistence: smaller gaps
- **Load/save race:** saves and loads of the same region are separate pool jobs with no ordering per path (`core/engine.rs:543-572`). A load can read the pre-save file after a close/open of the same context. Speculative trigger.
- **`write_atomic`:** the temp name is unique per process only, the directory is not fsynced, and there is no lock against a second game instance. WM-20 (file locks) is missing (`core/persist.rs:51`).
- **Silent save failures:** a failed save after the world closed is dropped without a log (`core/engine.rs:687-689`).
- **Map names:**
  - compared case-sensitively but stored lowercased: `/meridian map new Default` shares `default/` (`core/subworld.rs:113-125`, `core/persist.rs:74`);
  - `rename` orphans seeds and completed structures keyed by `<source>/<map>` (`lib.rs:526-541`).
- **Verified:** code only.

#### AUD-038 · Low · Bug: coordinate and time-unit slips
- **Truncation:** `ChunkPos::containing(p.position[0] as i32, …)` and `RegionPos::of_block(p.position[0] as i32, …)` truncate instead of flooring (`core/engine.rs:693`, `lib.rs:1556`). Off by one for x or z in (−1, 0).
- **Frames vs ticks:** `REQUEST_TIMEOUT` and the "per tick" budgets (`chunk_reads_per_tick`, the frame budget) actually count frames, so they scale with FPS (`core/engine.rs:47-48,734,769-782`, `lib.rs:1579-1581`). D-015 says "nach 200 Ticks".
- **Verified:** code only.

#### AUD-039 · Low · Performance: thread priorities and extra pools
- **Priorities:** only the `meridian-seedatlas-*` threads are lowered (PRI 20 measured). `meridian-worker-*` (PRI 31) and the unnamed nearest-search and export threads run at normal priority.
- **Viewer pools:** each dimension switch on the map creates a new `MapEngine` with its own 1–4 `meridian-worker` threads. Dropping it joins them on the main thread after its queued loads (`lib.rs:2733`, `core/worker.rs:130-137`).
- **Nether cave mask:** carved under a global mutex, so Nether refinement runs on one thread at a time (`engine/terrain_sampler.rs:718-731`).
- **Verified:** priorities in game (`ps -M`); the rest from code.

#### AUD-040 · Low · Docs: drift between the docs and the code
- **README:**
  - `README.md:52` describes "SA" (bottom left) and "M"/"SA" (top left); D-037 replaced them with the right-edge toolbar.
  - `README.md:38` "Fair Play in the miscellaneous settings" and `README.md:40` "main settings page" predate the category screen.
- **DECISIONS:**
  - D-022 (`docs/DECISIONS.md:219`) says boats and carts are missing; the bridge copies them (`bridge.rs:596-613`).
  - D-025 (`:262`) "sechs Seeds" and PROGRESS (`:275`) "fünf Seeds" vs 3 seeds in `fixtures.c:26`. D-026 and PROGRESS (`:273`) "alle stimmen exakt", but heights use a 1e-6 tolerance.
  - D-027 (`:282`) "nur die 5 × 5 Regionen … bleiben ganz im Speicher": see AUD-009.
  - D-031 (`:361`) and D-033: monuments from 0.0625 "auch in der Java-Mod". The Java change is only on the **unmerged** PR #12 (`007f94f`); Fabric `main` (`58c70ff`) still shows monuments always.
  - D-036 (`:454`) "Nether- und End-Landmarken … bleiben immer": see AUD-020.
  - D-015 "nach 200 Ticks": frames (AUD-038).
  - D-012 and `KEYBINDS.md` "Offene Beobachtungen" are still marked as open O3 checks.
- **PROGRESS:** `:56` "beim Beenden werden offene Speicher-Jobs abgearbeitet": see AUD-023.
- **Code comment:** `core/waypoint.rs:156` promises bare `x y z` triple parsing that doesn't exist.
- **Not recorded in DECISIONS:**
  - Large Biomes detected from the world;
  - a hand-entered seed overriding the detected one in singleplayer (`core/seedatlas_state.rs:224-228`; Fabric re-applies the world seed);
  - Seed Atlas auto worker count clamp(cpus−2, 1, 4) vs Fabric clamp(cpus−2, 2, 8) plus a structure pool.
- **Verified:** code and docs.

#### AUD-041 · Low · Rules: `docs/SEEDATLAS_PORT.md` contains Claude session scratchpad paths
- **What:** lines 11 and 14 point to local clones under `/private/tmp/claude-501/-Users-noahwill--claude-spares-spare-MV41O6/663dbc40-…/scratchpad/…`. These paths are tool-session specific: they name Claude and a session UUID, and they are ephemeral and useless to anyone else.
- **Rule:** the rule forbids session links in commits, PRs and the README. This file is none of those, but it is the same kind of trace.
- **Fix:** replace them with the GitHub URLs and commits (`DUzzL/Seed-Atlas` `fc2c628`/`2539698`, `DUzzL/seed-atlas-xaero` `58c70ff`).
- **Clean:** git authors and committers (69 commits, all `DUzzL <duzzl.lp@gmail.com>`), commit bodies (no `Co-authored-by`, Claude or Anthropic), README and code (only `.gitignore`'s `.claude/worktrees/` and TASK.md's rule text mention Claude).
- **Verified:** `git log --format='%an %ae %cn %ce%n%b'`, `git grep -i 'claude|anthropic'`.

#### AUD-042 · Low · Rules/Docs: clean room is fine, but one CLEAN_ROOM.md claim is too strong
- **Clean:**
  - No Xaero code, identifiers, assets or traces of decompiled sources were found.
  - "xaero" appears only in the owner's Seed Atlas glue names (`seedatlas_xaero.c/.h`) in `seedatlas/atlas.rs:2`, `terrain.rs:2`, `tests/parity.rs:1` and `tools/seedatlas-fixtures/*`.
  - Assets are the 7 Seed Atlas PNGs in `engine/assets/seedatlas/`.
  - Code comments are English only (no umlauts or German words; the only umlauts are test strings in `core/persist.rs:183` and `core/waypoint.rs:522`).
  - The bridge version matches: `VERSION = 10` (`bridge.rs:22`) = `BRIDGE_VERSION = 10` (`lib.rs:50`).
- **Too strong:** `docs/CLEAN_ROOM.md:26` says "Meridian verwendet eigene Schlüsselnamen". In fact 15 of Meridian's 119 TOML keys are identical to O2 option names: `biome_blending`, `biome_colors_in_vanilla`, `block_colors`, `current_profile`, `deathpoints`, `delete_reached_deathpoints`, `footsteps`, `lighting`, `load_new_chunks`, `max_loaded_regions`, `opening_animation`, `terrain_depth`, `unlimited_zoom_in`, `unlimited_zoom_out`, `update_chunks`. These are plain descriptive names from an allowed source (O2), not copied code. Soften the sentence or rename them.
- **Verified:** code and `meridian.toml` written by the test game.

## 3. Parity matrix

One row for each of the 115 rows in `docs/FEATURES.md`.

- "In game" marks rows I observed in the test game; all others are from the code.
- Statuses:
  - **Implemented**;
  - **Partial**: an acceptance criterion is only partly met;
  - **Missing**;
  - **Deviates**: intended, with its DECISIONS entry;
  - **Not verifiable here**: why is given.
- Counts are at the end of this section.

### Defaults, keys and radar against O2/O1

- **Option defaults:** every Meridian option that has an O2 counterpart has the O2 default, with one exception: `legible_caves` is `true`, O2 `false` (AUD-027).
- **O2 numbers that need observation:** what these values mean cannot be read from allowed sources; Meridian's interpretation is plausible but unconfirmed:
  - `minimap_block_colors 0`, `minimap_auto_cave_mode 2` (D-012);
  - `minimap_size 0`, `minimap_zoom 1`, `minimap_terrain_slopes 2`, `minimap_chunk_grid -1`, `hide_world_names 1`, `arrow_color -2`, world-map `auto_cave_mode -1`.
- **Toggle delays:** O2 `minimap_cave_mode_toggle_timer 1000.0` is milliseconds and world-map `cave_mode_toggle_timer 1.0` is seconds. Both are 1 s, which matches Meridian's 1000 ms.
- **Behaviour that differs from O2 although no option exists:**
  - anti-aliasing (O2 on; none in Meridian);
  - temporary waypoints global (O2) vs local;
  - map distances (O2 on; missing);
  - light-overlay, highlights and sub-world info lines (O2 on; missing);
  - cross-dimension teleport and partial Y (AUD-004, AUD-007).
- **Info lines:** order and default visibility match `info_display_config` (only coordinates on, colour 15).
- **Keys:** all 31 defaults (30 from O1, plus Meridian's own Tab for faces) match O1 and `KEYBINDS.md`; `core/keybinds.rs:85-106` tests this. 7 keys have no handler (AUD-034).
- **Radar against `default_radar_categories_client.json`:**
  - These match:
    - colours (living 14 yellow, players 15 white, items 12 red, other 5 dark purple);
    - render order (other 0, items 1, friendly 2, hostile 3, tamed friendly 4, tamed hostile 5, players 6);
    - dot size 2;
    - height limit 20, players 2050;
    - fade from 0 blocks;
    - cap 1000;
    - armour stands as "other".
  - Not modelled:
    - the root exclusion of `item_frame`, `glow_item_frame` and `cushion`. The bridge reads only mobs, dropped items, boats and carts, so frames would only appear if Horizon lists them as mobs; this needs observation;
    - the player sub-categories (friend, tracked, same team, other teams; order 7; tracked `icons 2`);
    - `displayNameWhenIconFails`;
    - `renderOverMinimapFrame`.
  - `icons 1` / `names 0`: the codes are undocumented. Meridian reads them as "faces only while the player-list key is held" and "player names", both plausible guesses (D-036/D-037); confirming them needs observation of the reference.

### Matrix

| ID | Status | Note |
|---|---|---|
| MM-01 | Implemented | Toggle key, saved. In game: shown by default |
| MM-02 | Implemented | Square and round (shader discard) |
| MM-03 | Implemented | Auto plus 4 sizes |
| MM-04 | Implemented | Position editor, corner plus offset (D-028) |
| MM-05 | Implemented | 5 steps, +2 with further zoom-out |
| MM-06 | Implemented | Rotates; north lock with arrow. In game: compass turns correctly |
| MM-07 | Partial | Hold/toggle, centred and own zoom work (in game). Missing `keep_enlarged_minimap_unlocked`; a release under a foreign screen is lost (AUD-032); stutters while moving (AUD-006) |
| MM-08 | Implemented | Accurate (texture) and Vanilla colours; reading O2 0 as "accurate" per D-012 |
| MM-09 | Implemented | Terrain depth; slopes Off/Vanilla/Smooth/Strong |
| MM-10 | Partial | Switch exists but nothing reads it (AUD-010) |
| MM-11 | Implemented | Day/night curve, block light, switchable |
| MM-12 | Implemented | Redstone, flowers, transparency, stained glass, short blocks (newly read chunks) |
| MM-13 | Missing | No anti-aliasing (O2 default on) |
| MM-14 | Partial | Opacity only; no frame style or colour, UI scale, arrow colour or opacity; arrow scale only for the north-locked arrow |
| MM-15 | Partial | Compass turns; position, scale, shadow colours and "over everything" fixed |
| MM-16 | Implemented | Hidden under screens and F3. In game: back after Escape from the map (D-034 regression OK) |
| MM-17 | Missing | No boss-bar or effect push |
| MM-18 | Implemented | Undiscovered backdrop at 50 % (TOML only, AUD-025) |
| MM-19 | Implemented | One engine and tile cache for minimap and world map |
| MM-20 | Missing | No safe mode |
| MM-21 | Missing | No item requirement (O2 default "-" equals the current behaviour) |
| MM-22 | Missing | No HUD render layer |
| CV-01 | Implemented | In game: no cave mode under a 1×1 roof; switches under 5×5 (`shots/19…`, `shots/20…`); delay 1 s |
| CV-02 | Partial | Manual key works; start Y is always eye height, not configurable |
| CV-03 | Partial | Legible caves and extra zoom work; `depth` dead; legible default differs (AUD-027); extra zoom only 1 step at the default zoom |
| CV-04 | Partial | Allow list applied but only editable in the TOML |
| CV-05 | Partial | Map shows the current or newest cave layer; no cave menu or icon, no mode type, top Y or per-dimension settings; `world_map.cave` dead |
| INFO-01 | Implemented | In game: "96, 144, -31" under the minimap |
| INFO-02 | Implemented | ×8 in the Nether |
| INFO-03 | Implemented | Chunk and position in chunk |
| INFO-04 | Implemented | Yaw and pitch |
| INFO-05 | Implemented | Dimension |
| INFO-06 | Implemented | Biome via the bridge, off by default as in O2 |
| INFO-07 | Implemented | Clear, rain, thunder |
| INFO-08 | Implemented | Block, sky or both |
| INFO-09 | Implemented | Game time 12/24 h and real time |
| INFO-10 | Partial | Only the manual cave indicator; highlights, light-overlay indicator and sub-world name missing (all on in O2) |
| INFO-11 | Missing | No background or alignment option |
| INFO-12 | Missing | No hiding of waypoint coordinates or server names |
| OV-01 | Partial | Grid in the shader; no colour or width option |
| OV-02 | Implemented | Slime chunks from the Seed Atlas seed |
| OV-03 | Partial | Block light in [min, max], light purple; colour and type fixed, min only in the TOML |
| OV-04 | Implemented | Footsteps (2048 points); never cleared across worlds (AUD-031) |
| OV-05 | Missing | No guide line or tracked waypoint |
| OV-06 | Missing | Key registered without a handler, no provider interface (P3; no claims mod in Horizon to test with) |
| RAD-01 | Implemented | Toggle and reverse keys |
| RAD-02 | Partial | Five flat classes with global settings; no inheritable tree or per-category settings |
| RAD-03 | Implemented | Dots, or faces from the game's textures; player skins; dot fallback |
| RAD-04 | Implemented | Colours as O2 (see above) |
| RAD-05 | Implemented | Names Off/Players/All, name scale (TOML) |
| RAD-06 | Partial | Hiding invisible entities works; the smoothing switch is dead (always smooth) |
| RAD-07 | Partial | Only an exclusion list (`/meridian radar hide`), no custom categories |
| RAD-08 | Partial | Faces follow the resource pack's entity textures; no `assets/meridian/…` override path |
| RAD-09 | Missing | No tracked players; keys without a handler. A second player is needed to test |
| WP-01 | Partial | In game: editor has name, colour, X/Y/Z/yaw, set, local/global, initials, enabled. **No symbol** |
| WP-02 | Implemented | Num+ instant waypoint |
| WP-03 | Partial | Edit, delete, enable, teleport, share; **no sorting, multi-select or distance** (AUD-024) |
| WP-04 | Partial | 16 colours and initials; no symbols |
| WP-05 | Partial | Switching sets and "all sets" work; **sets can't be created or deleted** (AUD-005) |
| WP-06 | Partial | Global/local and max distance; no dimension scaling |
| WP-07 | Implemented | Per world or server map and dimension (`waypoints.json`) |
| WP-08 | Partial | **Broken by default: the deathpoint is deleted at once** (AUD-001, in game); "switch set on death" missing |
| WP-09 | Partial | Temporary waypoints, dropped on world change; always local (O2 global) |
| WP-10 | Partial | Share works; received lines are **auto-added**, not clickable, unbounded (AUD-008, in game) |
| WP-11 | Partial | Plain format OK; rotation format unused, no cross-dimension (AUD-004), no partial Y (AUD-007), button never disabled |
| WP-12 | Partial | Mark, name and distance through walls, km from 10000; no scales or pointing-angle highlight |
| WP-13 | Implemented | Beam in waypoint colour; off by default (D-019) |
| WP-14 | Implemented | Opacity 90, clamped to the rim (in game: shared waypoints on the rim) |
| WP-15 | Missing | Always on top (equals the O2 default) |
| WP-16 | Partial | The toolbar's waypoint button opens the list (D-037 replaced the icon); no filter, hover highlight or "show disabled" on the map |
| WP-17 | Implemented | One book for minimap, map and screens |
| WM-01 | Implemented | In game: M opens with an animation. "< 100 ms with 300 regions" not measured |
| WM-02 | Implemented | In game: wheel, drag, toolbar ±; 120 fps with no frame over 9.4 ms while zooming or panning |
| WM-03 | Implemented | Incremental background saves; but see AUD-003 (unreadable files overwritten) and AUD-023 (quit) |
| WM-04 | Partial | `max_loaded_regions` 300 works; `reload_viewed` missing; memory see AUD-009 |
| WM-05 | Missing | No map from the world save |
| WM-06 | Missing | – |
| WM-07 | Partial | Dimension switch only via an unbound key (AUD-034); Overworld waypoints not shown in the Nether (AUD-026); wrong dimension when creating while viewing (AUD-011) |
| WM-08 | Partial | In game: header, Create Waypoint, Teleport Here, Share Location, Export Map as PNG (`shots/17-map-rightclick.png`). Teleport Here goes to Y 64 when Y is unknown (AUD-007). Player teleport needs a second player |
| WM-09 | Partial | All three default formats equal O2; dimension format used; no partial or ambiguous Y (AUD-007, in game) |
| WM-10 | Implemented | In game: coordinates, biome and zoom live |
| WM-11 | Partial | Arrow or dot (fixed red), radar entities; no tracked players; the world-map radar ignores the limits (AUD-016) |
| WM-12 | Partial | Pins drawn, but gated by the minimap's waypoint switch; no backgrounds, scale, min zoom, "show disabled" or "current map only" options |
| WM-13 | Missing | No map distances (O2 default on) |
| WM-14 | Missing | No paper-map style |
| WM-15 | Partial | A resource reload recolours only loaded chunks; stored tiles keep baked colours |
| WM-16 | Partial | Streamed PNG, scaled down above 16384 px; no night, split or highlight options |
| WM-17 | Missing | – |
| WM-18 | Missing | – |
| WM-19 | Implemented | "?" lists the controls (B and M hard-coded in the text) |
| WM-20 | Missing | No file locks |
| CTX-01 | Partial | Always separates by server address; no switch. Behaviour needs a server |
| CTX-02 | Not verifiable here | Code: one map per server, per world spawn (default, D-029) or manual via `/meridian map`; no 4th mode and no selection UI; see AUD-022. Needs a server network |
| CTX-03 | Missing | No confirmation dialog; `quick_confirm` has no handler |
| CTX-04 | Partial | "(auto)" lists and other worlds (D-038); no moving of data between sub-worlds |
| CTX-05 | Partial | Old waypoint data selectable (D-038); no recovery path for map data |
| SET-01 | Partial | Y and ] open the new settings screen (D-037, in game); many options only in the TOML (AUD-025) |
| SET-02 | Implemented | Profiles create, switch, rename, delete (D-028) |
| SET-03 | Missing | – |
| SET-04 | Deviates | Intended: FEATURES itself says "Entfällt (Horizon-Registry übernimmt)" (P3); no DECISIONS entry needed |
| IO-01 | Partial | JSON export and import; import reverses the order in each set, loses the current set, renumbers ids |
| IO-02 | Implemented | Reads the reference's text waypoint files (`waypoint:` lines, `dim%` folders); "~" Y becomes 64 |
| IO-03 | Partial | As WM-16 |
| IO-04 | Missing | No settings export or import |
| SRV-01 | Missing | No server-driven locks. Not testable here anyway (needs a server) |
| SRV-02 | Implemented | Fair Play switch (no server signal) |
| SRV-03 | Missing | Needs a documented signal source |
| SA-01 | Partial | Biome parity passes (all 38 versions). No minimap layer; "Over Explored Terrain" dead (AUD-010); half-resolution refinement (intended, D-034); Nether and End relief lost at moderate zoom (AUD-019) |
| SA-02 | Implemented | All 28 marker kinds, size, per-kind filter, completed; positions pass parity. Visibility per D-033/D-036. Bugs: AUD-002, AUD-018, AUD-020 |
| SA-03 | Implemented | Same command tree and texts as Fabric; in game `/seedatlas status` works (reply squashed into the action bar, AUD-036) |
| SA-04 | Implemented | In game: seed 12345 detected automatically |
| SA-05 | Implemented | No path found where the seed leaves the client (commands cancelled locally, no chat, logs or clipboard); stored in `seeds.toml` |
| SA-06 | Implemented | Normal and Large Biomes (parity covers Large Biomes for one seed) |
| SA-07 | Partial | Highlighting exists but is hidden by explored terrain (AUD-017); structure waypoints get Y 64 and the player's dimension (AUD-036) |
| SA-08 | Partial | Worker threads at lowered priority (in game: PRI 20); **prefetch not implemented** (AUD-010) |

**Counts (115 rows):**

| Status | Rows |
|---|---|
| Implemented | 47 |
| Partial | 43 |
| Missing | 23 |
| Deviates (intended) | 1 |
| Not verifiable here | 1 |

**By priority, rows not Implemented:**

- **P0: 19 rows, all Partial except CTX-02.**
  - Minimap and radar: MM-07, RAD-02.
  - Waypoints: WP-01, WP-03, WP-04, WP-05, WP-08, WP-10, WP-11, WP-12, WP-16.
  - World map: WM-04, WM-07, WM-08, WM-12.
  - Other: CTX-01, CTX-02 (Not verifiable here), SET-01, SA-01.
- **P1: 28 rows.** Missing: INFO-12, OV-05, RAD-09, WM-05, CTX-03. The rest are Partial.
- **P2–P3:** the remaining rows.

The P0 gaps cluster in waypoints. Four of them are bugs, not missing options:
- WP-08: deathpoint deleted at once (AUD-001);
- WP-11: cross-dimension teleport (AUD-004);
- WP-10: chat auto-add (AUD-008);
- WM-08: Y 64 teleports (AUD-007).

## 4. Seed Atlas comparison

### Reference

| Repo | Commit | Notes |
|---|---|---|
| `DUzzL/seed-atlas-xaero` `main` | `58c70ff4` (release 2.0.0, glue ABI 6) | the Fabric mod compared against |
| Branch `marker-zoom-and-ancient-city-size` | `007f94f` | PR #12, **open, not merged** |
| `DUzzL/Seed-Atlas` `main` | `2539698` | since `fc2c628` (D-026) only build and test files changed, so the fixtures are still valid |

Fabric paths below are relative to `common/src/main/java/org/seedatlas/xaero/`.

### Parity test (`cargo test`, debug): passes

`every_version_matches_the_fabric_version` passed in 577.8 s.

**What it covers**, compared exactly unless noted. Fixtures are written by the real glue on Apple Silicon (clang):
- **Versions:** all 38 versions (1.1–26.3).
- **Seeds:** 3 seeds (`0`, `-4172144997902289642`, `8675309`); Large Biomes only for seed 0.
- **Biomes and colours:**
  - name and colour table per version;
  - `biome_at` points (±30 000, some ±3 000 000);
  - `biome_area_sampled` at steps 1/2/4/8/16/64/256/1024 (16×16 or 20×20);
  - `biome_chunk_sampled` at steps 1/4/16.
- **Terrain:**
  - Overworld and End heights at steps 1/2/4/16/64 (1e-6 relative tolerance);
  - Nether floors with and without random carving masks.
- **Structure scans:**
  - kinds 1–25, End ship and ore veins, with and without the biome check;
  - boxes ±3000 (mansion ±30 000; small kinds ±700, ore veins and End ship ±600);
  - no truncation.
- **Other searches:** strongholds (±100 000), spawn (estimated and detailed, x/z), slime chunks.

**What it does not cover:**
- Steps 32/128/512 and 128×128 tiles (both used by Meridian).
- Height steps 8/32, End step 8.
- Scattered End gateways and End ships *with* the biome check (none in the boxes).
- Large Biomes structure scans.
- Everything above `atlas.rs`:
  - `scan_complete` budgets, `view_box`, `visible_at_zoom` and the `view_markers` merge (where AUD-002 lives);
  - completed structures, nearest search;
  - the terrain look against Fabric's `BiomeTerrainStyler`;
  - `terrain_sampler` against Fabric's `OverworldTerrainSampler`;
  - Nether mask generation.

### Differences

| # | Area | Fabric | Meridian | Bug or intended |
|---|---|---|---|---|
| 1 | Singleplayer seed detection | `SeedAtlasClientState.java:561-567` | `lib.rs:1406-1416` (`level.dat`) | Same. In game: seed 12345 detected |
| 2 | Seed storage and privacy | `seedatlas_xaero.json`, base + Nether + End | `seeds.toml` in the data folder, same semantics; plain-text context keys (Fabric hashes them) | Same, local only. No path found where a seed leaves the client (commands cancelled locally; no chat, log or clipboard use) |
| 3 | Large Biomes | one global option | detected from the world preset (`lib.rs:1415`) | Intended in code, **not in DECISIONS**; `status` still prints the option (AUD-040) |
| 4 | Hand-entered vs detected seed (singleplayer) | the detected seed is re-applied on every load | the hand-entered seed wins (`seedatlas_state.rs:224-228`) | **Not in DECISIONS** (AUD-040) |
| 5 | Game version per world | global map | stored in each settings profile (`config.rs:460,585`) | **Bug** (AUD-036) |
| 6 | `/seedatlas` commands | `SeedAtlasCommands.java:25-60` | same words, arguments and texts (`seedatlas_commands.rs:122-161`) | Same |
| 7 | Command replies | one chat line each, errors in red | joined with " · " into one action-bar line, no tab completion | **Bug** (AUD-036; seen in game) |
| 8 | Biome colours | engine table | engine port (`biomes::color_in`) | Same (parity `K` lines) |
| 9 | Terrain look (relief, water, Nether, End, highlight constants) | `BiomeTerrainStyler`, `BiomeSurfacePalette` | `seedatlas_look.rs` | Same constants; no golden test against Java output |
| 10 | Overworld refinement | full resolution, steps ≤ 4 | half resolution + upsampling, only ≥ 1 px per block | **Intended** (D-034, D-035). `lattice_columns_match_single_columns` passes (99.0 % same height) |
| 11 | Nether floor / End heights at moderate zoom | refined at any zoom (Nether to step 16, End to step 8) | blocked by the same `refine_allowed` gate | **Bug**: outside D-035's scope, poor trade-off (AUD-019) |
| 12 | Sample steps and tile size | bands 1/4/16/64/256/1024; 256- or 128-sample tiles; parent LODs + overscan ring | any power of two, 2–8 px per sample; 128 always; only visible tiles; compose from finer tiles | **Intended** (D-030, D-035) |
| 13 | Seed layer over explored terrain ("Overlay explored") | drawn above explored terrain | option stored and shown, **never read** | **Bug** (AUD-010) |
| 14 | Biome highlight | focus pass above terrain, opaque | baked into tiles under the terrain, at layer opacity | **Bug** (AUD-017) |
| 15 | Selecting a highlight biome | switches highlighting on | does not | **Bug**, Low (AUD-036) |
| 16 | Structure kinds | 28 `MarkerType`s; helpers geode, orevein, slime, spawn, well off | same 28 ids, names, helper flags | Same |
| 17 | Marker merge between scans | `mergeWithSkipped` by **marker id** | by **hit kind** (`seedatlas.rs:1436-1450`) | **Bug**, High (AUD-002, reproduced) |
| 18 | Result caps | 1024 per kind (slime 4096), truncated in scan order | 8192, all-or-nothing with cell budgets | **Intended** (D-036, replacing D-031). Side effects contradict D-036's text (AUD-020) |
| 19 | Scan box | `window px / scale`, where scale is px per block | same formula, but Meridian's zoom isn't px per block | **Bug** (AUD-018) |
| 20 | Zoom thresholds | see the table below | see the table below | Intended where noted (D-033, D-036) |
| 21 | Structures need the seed layer | `isLayerActive` includes the layer | `structures && biome_layer` | Same (D-038) |
| 22 | Ancient city icon | 18 px on `main`; 24 px only on unmerged PR #12 | 24 px | **Intended** (D-033); Fabric `main` differs because PR #12 is open (AUD-040) |
| 23 | Icon growth | up to ¼ of the viewport's short side | at most 2× the base size | **Intended** (D-036) |
| 24 | Ocean Ruins icon | composed with a dark frame | no frame (`icons.rs:244`) | **Bug**, Low (AUD-036) |
| 25 | Spawn marker | estimated, "(approx.)" | detailed search, no "(approx.)" | **Bug**, Low (AUD-036) |
| 26 | Marker tooltip | one line with camp biome, `Y=?` | several lines, no camp biome, Y left out | **Bug**, Low (AUD-036) |
| 27 | Biome hover tooltip | version-specific name | newest-version name (`lib.rs:2620`) | **Bug**, Low (AUD-036) |
| 28 | Right-click on a marker | menu only with a key; Copy with Y; Create waypoint opens the editor with real Y; refuses another dimension | menu always; Copy without Y; waypoint added directly with Y 64 in the player's dimension | **Bug**, Low/Medium (AUD-036, AUD-007, AUD-011) |
| 29 | Hover outline and hit box | white outline; badge counts in the hit box | none; icon only | **Bug**, Low (cosmetic) |
| 30 | Completed structures | key (world, seed, large, dim, type, x, z), tolerance 12, reset per type or all | same (`completed.rs`) | Same; storage failure mode see AUD-015 |
| 31 | Nearest search | radii 1024/4096/16384; result caps; result dropped if the screen was left | same radii, no cap; applied after the screen was left | No cap = better; stale result = **bug**, Low (AUD-036) |
| 32 | Background prefetch | ring of 0–3 tiles around the player while the map is open | shown and described, **not implemented** | **Bug** (AUD-010) |
| 33 | Minimum zoom with the seed layer | 1/512 | 0.0625 (1/64 with "unlimited zoom out") | Not in DECISIONS: undocumented deviation (AUD-036) |
| 34 | Nether sample Y | 127 in full cave mode, else `netherY` | always `nether_y` | Possible bug, Low; visual only (needs observation) |
| 35 | Y sample settings | free text −2048…2048 | 9 presets (−64…320) | Side effect of D-037; Low (AUD-025) |
| 36 | Worker threads (auto) | clamp(cpus−2, 2, 8) + a structure pool ≤ 4 | one pool clamp(cpus−2, 1, 4), lowered priority | Deliberate (D-034 load concerns) but the counts are not recorded (AUD-040) |
| 37 | Settings screens and map buttons | five Fabric-style screens; M/SA buttons | one categorised screen; right-edge toolbar | **Intended** (D-037) |
| 38 | Cache | tiles and textures trimmed with the view | memory only, released 2 s after the map closes; recomputed on reopen | **Intended** (D-035); trade-off see AUD-030 |
| 39 | Localisation | en_us, de_de | English only | Not recorded; minor |

### Marker visibility per kind

Meridian's `visible_at_zoom` (`core/seedatlas.rs:512-528`) against Fabric `SeedAtlasStructureMarkerSource.java:371-395`. "Effective" is what `scan_complete`'s budget (2²⁰ cells for cells ≤ 128 blocks, else 2¹⁵) and the 8192-hit cap actually allow.

| Kind | Fabric `main` | Meridian | Verdict | Effective limit in Meridian |
|---|---|---|---|---|
| Slime chunks | ≥ 0.5, r ≤ 2048 | same | same | same |
| Ore veins | ≥ 0.25, r ≤ 8192 | same | same | r ≤ 8192, cap may bind; **duplication bug AUD-002** |
| Geode, well, buried treasure | ≥ 0.25, r ≤ 8192 | same | same | **r ≤ 7168** (budget); geodes about r ≤ 3072 (cap) |
| Mineshaft | ≥ 0.25, r ≤ 12288 | same | same | **r ≤ 7168**: the 8192–12288 band never shows |
| Camp, village, shipwreck, ruins, portal, trails, chambers, outpost, temples, hut, igloo | ≥ 0.125, r ≤ 24576 | same | same | budget fine; cap may bind for ocean kinds near 24576 |
| Ocean monument | always (landmark) | ≥ 0.0625 | **Intended** (D-033; same as unmerged PR #12) | r ≤ 40960 |
| Ancient city | always | ≥ 0.0625 | **Intended** (D-036) | r ≤ 32768 |
| Stronghold, mansion | always | ≥ 0.03125 | **Intended** (D-036) | none |
| Spawn | always | always | same | none |
| Fortress, bastion | always | always | same in the table | **r ≤ 32768** (AUD-020) |
| End city | always | always | same in the table | **r ≤ 28672** (AUD-020) |
| End gateway | always | always | same in the table | **r ≤ 7168**, about < 0.25x on a 3024 px window (AUD-020; Fabric itself stops at r > 16376) |
| End ship | ≥ 0.03125 | same | same | r ≤ 28672 |

The radius `r` comes from the scan box formula, which itself is off by the factor in AUD-018. So the zoom at which these cut-offs happen depends on the window height.

## 5. Performance measurements

### Conditions

- **Machine:** Apple M6, Metal; 3024×1720 physical px.
- **Build:** `native-66f3981-2096c8af0689` (profile "performance").
- **World:** test world `AuditWorld` (seed 12345), creative, render distance 12, explored area about 600×600 blocks around spawn.
- **Defaults:** all Meridian settings at default (Seed Atlas on, layer on, structures off, 4 + 4 worker threads), except where noted.

Caveats:

- **Frame rate.** While the test window was in the background, Horizon capped it at 30 fps (first session); later it ran at 120 fps (vsync, 8.33 ms). Each table says which.
- **Baseline drift.** Process CPU drifted from about 8 % to 30–50 % during the session. `sample` attributes the drift to Horizon's own `horizon-net-*` threads (`lan::io_pool::run`) and the server thread, not to Meridian. That is why Seed Atlas cost is also given as **worker CPU-seconds** (from `ps -M` thread times), which don't depend on the baseline.
- **Concurrent builds:**
  - The first session's measurements ran while `cargo test` (the single-threaded parity test) was running.
  - Runs r3–r5 had nothing else running.
  - The ore-vein run ran while the adapter build was running. Its effect (×7 frame time) is far beyond build noise.

### Process CPU and RSS per state

First session, 30 fps, `top -l 2 -s 4 -pid` per sample:

| State | CPU % (samples) | Time to settle | RSS |
|---|---|---|---|
| Playing, map closed (baseline) | 14.5 / 12.3 / 8.0 | – | 1571 MB |
| World map open, idle at 3.0x | 7.9 / 7.9 / 7.6 / 8.0 | – | 1600 MB |
| Right after zooming out to the minimum (0.0625x) | 122 (first 3 s window), then 15.1, 14.6, 14.3… | **≤ 7 s** (1 run here; worker time below) | 1601 MB |
| Zoom in to 0.7x | 173, 218, then 14.7 | **7–11 s** (this run overlapped the parity test) | 1610 MB |
| Zoom back out to the minimum | 16.5 / 14.7 / 15.1 / 14.2 | **0 s, no recomputation** | 1610–1618 MB |
| Map open at 0.0625x, seed layer **off** | 13.8 / 13.9 / 14.1 | – | 1636 MB |
| Seed layer back **on** after more than 2 s off | 167 (first window), 13.4, 52, 13.5 | about 7 s (recomputes: tiles were released) | 1639 MB |
| Map open at 0.0625x, seed layer **on**, settled | 14.5 / 13.9 / 14.1 | – | 1639 MB |
| Map closed, after 1 s / 6 s | 13.1 / 12.7, 12.7, 12.3 | – | 1679 MB (not returned yet) |
| 3 open → zoom out → zoom in → close cycles | – | – | open 1528 → closed 1529; open 1534 → closed **1307**; open 1311 → closed 1311 |

RSS before, during and after:

- RSS rose by about 30 MB on opening the map and by about 110 MB over the first session's zooming.
- It did not drop within 6 s of closing.
- Over the next cycles it fell to 1307 MB: memory is returned, with a delay (allocator), and there is **no growth across cycles**.

### Seed Atlas worker cost per action

CPU-seconds summed over the 4 `meridian-seedatlas-*` threads (from `ps -M` UTIME + STIME). "Busy until" is the last moment their CPU time grew after the action (0.5 s polling). Runs r4 and r5 had nothing else running; r3 is the same script with a mislabelled first step.

| Action | r3 | r4 | r5 | Busy until (wall) |
|---|---|---|---|---|
| Open the map (tiles were released) and zoom to 0.0625x | 6.00 s | 6.40 s | 6.36 s | about 1.6 s |
| Zoom in to ~2.9x | – | 0.12 s | 0.13 s | about 0.5 s |
| Zoom in to 0.7x (detailed terrain) | 4.33 s | 2.93 s | 2.84 s | about 1.1 s |
| **Zoom back out to 0.0625x** | **0.00 s** | **0.00 s** | **0.00 s** | – |
| Zoom in to 0.7x again | 0.00 s | 0.00 s | 0.00 s | – |
| Seed layer off for 3 s | 0.00 s | 0.00 s | 0.00 s | – |
| Seed layer on again | 4.18 s | 4.16 s | 4.13 s | about 1.6 s |
| Close the map, wait 3 s | 0.00 s | 0.00 s | 0.00 s | – |
| Reopen the map | 4.34 s | 4.29 s | 4.38 s | about 1.6 s |

`meridian-worker-*` (region work) used 0.00–0.13 s per step.

What this shows:

- Zooming back out after zooming in needs no recomputation (D-035 holds).
- Switching the layer off for more than 2 s, or closing the map, throws the tiles away; turning the layer back on or reopening costs about 4–6 CPU-seconds each time (AUD-030).

### Frame times

Second session, 120 fps, `horizon-ctl stats`. "cpu" is Horizon's per-frame CPU time, which includes the mod hooks.

| Situation | Frames | Frame p99 / max (ms) | Frame CPU avg / p99 / max (ms) |
|---|---|---|---|
| Sprinting 6 s, map closed | 725 | 8.5 / 12.8 | 2.15 / 4.78 / 5.7 |
| Map open, idle | 604 | 8.5 / 8.6 | 2.13 / 4.99 / 5.17 |
| Map: wheel zoom ±10 notches ×2 | 348 | 8.5 / 8.5 | 1.06 / 4.91 / 5.17 |
| Map: panning with the arrow key held for 4 s | 967 | 8.5 / 8.5 | 1.92 / 4.85 / 5.77 |
| Map: zoom out from 0.7x to the minimum (compose path, D-035), run 1 / run 2 | 365 / 365 | 8.5 / 9.4; 8.5 / 8.5 | 1.83 / 3.56 / 4.64; 2.25 / 3.93 / 4.53 |
| Sprinting 5 s, minimap normal (two directions) | 603 / 604 | 8.6 / 13.5; 8.5 / 8.5 | 1.81 / 4.86 / 6.56; 2.07 / 3.75 / 4.16 |
| **Sprinting 5 s, minimap enlarged (Z)** (two directions) | 585 / 591 | **22.4 / 24.7; 19.4 / 23.2** | 1.86 / **22.36 / 24.62**; 1.66 / **19.38 / 22.94** |
| **Map open with Ore Vein markers on**, after 5 s … 40 s | – | p50 8.4 → **51.3** | avg **7.6 → 51.1** (fps 99 → 19.5); RSS 1263 → 1396 MB |

Conclusions:

- No stutter from the world map: zooming, panning and the compose path all stay within the 120 Hz budget.
- The **enlarged minimap stutters while moving**, at 19–25 ms frames (AUD-006).
- Ore vein markers make every frame slower over time (AUD-002).
- Mouse-drag panning could not be measured: `drag-at` runs within a few frames.

### Hot spots (`sample <pid> 3`)

**Zoom out to the minimum** (`sample-zoomout.txt`):
- Each of the 4 `meridian-seedatlas-*` threads was busy for about 210 of 1582 samples. The work was mostly done by the time sampling started, 1 s in.
- Leaf frames: `meridian_seedatlas::noise::Perlin::sample` (about 88 %), `biome_noise::Splines::get`, `np_dist`. This is the climate-noise biome sampling.

**Zoom in to 0.7x** (`sample-zoomin07.txt`):
- Three Seed Atlas workers were 100 % busy (1360/1361 samples). The fourth was idle, as designed: refinement uses at most workers − 1 (D-034).
- They were in `horizon_core::noise::perlin::PerlinNoise::add_volume` and `PerlinNoise::sample`, called from `meridian_horizon::terrain_sampler::sample_overworld`. That is the vanilla-surface density lattice.

**Steady state** (map open, settled):
- Meridian threads only show `meridian-worker-*` doing zlib `deflate_quick` and `__bzero` (region saves), under 1 % of samples.
- The main thread spends no measurable time in Meridian symbols.

**Thread priorities** (`ps -M`):

| Threads | PRI |
|---|---|
| `meridian-seedatlas-0..3` | 20 (lowered, macOS utility QoS) ✓ |
| `meridian-worker-0..3` | 31 (normal, **not lowered**) |
| main thread | 47 |

### Measurement tests

**`cargo test -p meridian-core --release --test perf -- --ignored --nocapture`:** 1 run, with the test game running at the same time.

| Measurement | Result | PROGRESS (Phase 6) |
|---|---|---|
| Overview of one region | 0.16 ms, 64 KiB | 0.07 ms |
| One region (1024 chunks) | build 1.3 ms, render 2.4 ms, encode 78 ms, decode 7.7 ms, file 1785 KiB | render 3–6 ms, encode 68 ms, decode 5 ms, 2.1 MiB |
| Write 400 regions | 27.9 s | – |
| Load and draw 300 regions | 2743 ms (9.1 ms each), **about 1443 MiB in memory** | 2.0 s (6.7 ms each) |
| PNG export of 20×20 regions | 3833 ms (5120×5120, scale_down 2) | 2.8 s |

The slower numbers are probably contention with the running game. The memory figure is what matters: 300 fully explored regions (the default `max_loaded_regions`) take about 1.4 GiB (AUD-009).

**`tile_cost`** (release), ms per 128×128 tile:

| Step | 1 | 2 | 4 | 8 | 16 | 32 | 64 | 128 | 256 |
|---|---|---|---|---|---|---|---|---|---|
| Biomes | 3.7 | 14.7 | 25.2 | 12.6 | 16.4 | 21.0 | 26.6 | 32.3 | 36.5 |
| Terrain look | 4.6 | 19.6 | 44.8 | 35.8 | 43.5 | 52.0 | 26.8 | 32.6 | 36.9 |

**`lattice_columns_match_single_columns`** (adapter crate, release, `MERIDIAN_RESOURCES` set):
- Single columns 1.32 s, lattice 125 ms (10.5×), band mode 110 ms (12.0×).
- Of 9216 columns: 9123 (99.0 %) have the same height, 9167 (99.5 %) are within one block, 9149 (99.3 %) have the same colour.
- This matches D-034's figures (11.9×, 99.0 / 99.5 / 99.3 %). **Passes.**

### Avoidable per-frame work found in the code

Not measured individually; see the findings.

- Minimap image fully rebuilt and re-uploaded on every block step (AUD-006).
- Structure markers re-merged and re-keyed, with String allocations per marker per frame (AUD-028).
- Settings slider saves with `fsync` per drag event (AUD-021).
- Every Meridian screen scans the waypoint folders on disk each frame (AUD-028).
- A 9409-entry `order` Vec is sorted each frame and never used (`lib.rs:2401-2402`).
- `update_seeds` clones the settings and calls `available_parallelism()` every frame, even with Seed Atlas off (AUD-028).
- Radar entity copies continue while the minimap is hidden (AUD-028).
- GPU upload budget counted in textures, not bytes (`gpu_map.rs:100`).

## 6. Not verified

| What | Why |
|---|---|
| Multiplayer: CTX-01…05, maps per world spawn (D-029, AUD-022), the GPU cache across server maps (AUD-013), server locks (SRV-01/03), deathpoints on servers | Needs a server or server network |
| Tracked players, "Teleport To <player>", player faces and skins, RAD-09 | Needs a second player |
| Claims (OV-06) | No claims mod in Horizon |
| Releasing Tab (faces) and Z under a screen (AUD-032) | `horizon-ctl key … up` does not deliver releases to mods (known quirk) |
| "Copy coordinates" | Would write to the system clipboard (rule 4) |
| Radar: item-frame and glow-item-frame exclusion, armour-stand colour, tamed-over-wild order on screen | The summoned frames popped off as items; the minimap centre was covered by the deathpoint and player marks. Colours and order were checked in code only |
| Cross-dimension teleport from the waypoint list (AUD-004), creating waypoints while viewing another dimension (AUD-011) | Code only; needs a waypoint in another dimension and dropdown clicks the test tool doesn't drive reliably |
| Cave-layer snapshot race (AUD-012), Seed Atlas jobs stuck after a configuration change (AUD-014), quit not waiting for saves (AUD-023), the load/save race (AUD-037) | Timing-dependent; traced in code only. AUD-023: files were written at quit in the test, so Horizon probably drops the hooks today |
| Corrupt-file and I/O-error variants of AUD-003; the AUD-015 stores | Only the "newer format version" variant was reproduced |
| Small-window marker gaps (AUD-018), Nether and End relief (AUD-019), highlight visuals (AUD-017), Nether sample Y | Needs a small window or the Nether and End; not run in the time available |
| Seed Atlas look against the Fabric mod pixel by pixel | The Fabric mod was not run (Prism instance not used); the PROGRESS item is still open |
| Xaero behaviour itself (O3): meaning of the O2 enum numbers, `icons`/`names` codes, "legible caves off", menu entries, keys in the list and editor | Needs observation of the reference mods, which only the user can provide (clean room) |
| WM-01 "opens in < 100 ms with 300 loaded regions" | Explored area too small; would need a large explored map |
| Elytra flight, a zoomed-out minimap (zoom 8) and slower machines | Only sprinting and the enlarged minimap were measured on an M6. AUD-006 will be worse there |
| Dragging a settings slider (AUD-021) | The test tool does not reliably drive slider drags in Meridian screens |
| Windows and Linux; x86 builds | Only macOS (Apple Silicon) was available. D-025 notes FMA differences in the far End on x86 |
| PNG export in the game | Not clicked; the export code path ran in the perf test (3.8 s for 5120² px) |
| Resource-pack change (WM-15) | No second pack in the test game |
| `cargo test` with Rust 1.95 (CI's MSRV) | Only the local 1.97.1 toolchain was used |
| Release `cargo test` of the whole workspace | Only the debug run (as the prompt describes) plus the release perf, `tile_cost` and lattice tests |
