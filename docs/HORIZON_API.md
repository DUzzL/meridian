> Historical upstream audit (2026-11-06). The `port/arismoko-horizon` branch
> targets the mod API fork; see [FORK_PORT.md](FORK_PORT.md) for its current
> package, API mapping and validation. The upstream API details below remain
> as the rationale for the original bridge.

# Horizon: Client-Mod-API (Phase 0)

Untersucht am 2026-11-06.

- Quellcode: `horizon-rust/horizon`, `main` @ `41f822b659789780f58d32c63ddfb6887ea1b8bf` (Lizenz MIT).
- Installiert: Nightly `nightly-2026-11-05-66f3981` (Minecraft Java 26.3).
- Weitere Quellen: `horizon-rust/horizon-mods` (offizielle native Mods, darunter `native/minimap`), `horizon-rust/horizon-native-mod-template` und `horizon-rust/horizon-mixins`.

Maßgebliche Dateien:

- `docs/mods.md`, `docs/native-mods.md`, `docs/api/mod-files.md`, `docs/api/stability.md`
- `crates/horizon-client/src/mod_api.rs` (Trait-Mod-API)
- `crates/horizon-client/src/game/native_hooks.rs` (Hook-Punkte, `NATIVE_HOOKS_API = 411`)
- `native-hooks.json` (Katalog: 432 Hooks, 174 Host-Helfer)

## Kurzfazit

**Horizon ist nicht rein serverseitig.** Es ist ein vollständiger Client (wgpu-Renderer, HUD, Screens) mit einer nativen Mod-Schnittstelle. HUD-Overlays, eigene Screens, Keybinds, Client-Chatbefehle, Weltprimitive (Beams, Text durch Wände), Einstellungen und Persistenz sind als **stabile Hook-Punkte** vorhanden.

Für eine Karten-Mod auf dem Niveau von Xaero fehlen drei Dinge:

1. **effizienter clientseitiger Chunk-/Spaltenzugriff samt Client-Chunk-Events**, auch auf fremden Servern,
2. **mehrere dynamische GPU-Texturen** im HUD bzw. in Screens,
3. **ein Entity-Radar mit Entity-Typ**, auch im Singleplayer.

Diese Lücken sind unten mit Erweiterungsvorschlägen beschrieben.

## Wie Mods geladen werden

| Art | Datei | Inhalt | Für uns |
|---|---|---|---|
| Content-Mod | `.hmod` / Ordner mit `horizon-mod.toml` in `horizon-mods/` | nur Daten und Assets, kein Code | ungeeignet |
| **Native Mod** | `.hnmod` bzw. Bundle-Ordner, Basis `horizon-native-mods/` | Rust-Code, den der Launcher **in eine eigene Game-Executable kompiliert** | **unser Weg** |

Native Mods gibt es in zwei Formen. Beide lassen sich in einem Bundle kombinieren:

- **Trait-Mod** (`[api] path = "…"` in `horizon-mixin.toml`): ein Rust-Crate mit `pub fn hooks() -> Box<dyn horizon_client::mod_api::Hooks>`. Es implementiert Trait-Methoden (`hud`, `key`, `command`, `world_primitives`, …) und ruft den Spielzustand über `&dyn Host` ab. Bleibt das Crate bei Katalog-Hooks und `Host`, deklariert es `"hooks": N` und baut **ohne Pin** auf jedem Horizon mit `NATIVE_HOOKS_API >= N`. Ein Schalter unter Optionen > Mods wirkt sofort.
- **Mixin-Injektionen** (horizon-mixins: HEAD, RETURN, BEFORE_CALL, MODIFY_ARG, OVERWRITE …) auf beliebige Funktionen im Quellcode. Wer Funktionen außerhalb des Katalogs anspricht, muss `[compatibility] horizon_revision` auf einen Nightly-Commit **pinnen**.

Build und Verteilung laufen so:

1. Der Launcher lädt eine eigene Rust-Toolchain.
2. Er baut die Game-Executable mit allen nativen Mods (`native-build`).
3. Er cacht das Ergebnis pro Instanz.

Für die Entwicklung gibt es `cargo horizon-mod new … --template native`, `native-build <pfad>` und das Template-Repo mit CI. Ein Paket darf nur Crates und Versionen aus Horizons `Cargo.lock` verwenden. Native Mods laufen ohne Sandbox, und die Registry nimmt sie erst nach einem Review auf.

Tests ohne laufendes Spiel: Jede `Host`-Methode panict standardmäßig. Ein Test-Double implementiert also nur, was gebraucht wird, und treibt die Hooks über `mod_api::dispatch_<hook>(…)`. Im Spiel gibt es `horizon-ctl` (Steuersocket, `--offscreen`) und `horizon-client --frames/--smoke-report` für automatisierte Läufe; Horizons eigene Minimap nutzt das für ihren `proof.py`.

## Was existiert (clientseitig relevant)

| Bedarf | Vorhanden | Hook / Helfer | Bewertung |
|---|---|---|---|
| HUD-Overlay | ja | `hud(batch, corners)`, `gui_layer_pre/post`, `gui_render_post`; Helfer `fill`, `quad` (beliebiges Viereck, ARGB), `text`, `text_width`, `sprite`, `gui_paint`, `gui_nine_slice`, `corner_shown` | gut |
| Eigene Screens | ja | `mod_screen_open/close`, `mod_screen`, `screen_draw`, `screen_click`, `screen_mouse_*_pre`, `screen_key_pressed_pre`, `screen_char_typed_pre`, `screen_closing` | gut (Vollbildkarte, Wegpunkt-Editor, Einstellungen) |
| Keybinds | ja | `mods` (Mod deklariert Tasten mit Label und Standard wie `key.keyboard.y`), `mod_key`, `set_mod_key`, `key(key, pressed)`, `input_key` | gut, eigene Defaults möglich |
| Client-Chatbefehle | ja | `commands` (Namen), `command(cmd, &mut ran)`, `tab_complete` | gut (`/seedatlas …`) |
| Chat senden / empfangen | ja | `send_chat` (auch `/tp …`), `client_chat_received` (abbrechbar) | gut (Wegpunkte teilen, Teleport) |
| 3D-Weltanzeige | ja | `world_primitives`: `line`, `box_outline`, `filled_box`, `beam`, `text(...).through_walls().scale_with_distance().background()` | gut (Wegpunkt-Beams, Distanzlabel) |
| Eigene Render-Pässe | ja | `world_drawn`, `hand_drawn` (wgpu-Device, Queue, Encoder) | nur Weltpass, nicht HUD |
| Einstellungen | ja | `settings`, `settings_click`, `setting`/`set_setting`, `register_configs`, `config_loading` | gut |
| Persistenz | ja | `data_dir(mod_id)` → `horizon-mods/.horizon-mod-data/<id>`, `config_dir`, `instance_dir` | gut |
| Spieler, Kamera | ja | `player` (Position …), `camera` (Pos, Yaw, Pitch, Dimension), `client_state` (Fokus, `gui_hidden`, `gui_size`, **`server_address`**, `logged_in`) | gut |
| Welt-/Dimensionswechsel | ja | `client_level_load/unload(dimension)`, `client_disconnecting`, `client_start/stopping` | gut |
| Zeit, Licht, Wetter | ja | `clock`, `day_time`, `light`, `weather` | gut (Uhr, Tag/Nacht-Licht) |
| Einzelblock lesen | ja | `block(pos) -> String`, `client_block_obstruction(pos)` | **zu langsam** für Karten (ein String pro Block) |
| Spalten lesen | nur Server | `read_columns`, `surface_heights` (inkl. Biom-Tints) | im SP nutzbar, **im MP nicht** |
| Chunk-Events | nur Server | `chunk_load/unload` (`side: server`); `native_chunks_changed` existiert, ist aber kein Katalog-Hook | **Lücke** |
| Entities | teilweise | `client_entities` (Position, UUID/Name bei Spielern, Unsichtbarkeit); `client_heads`, `client_profiles` | **leer ohne Remote-Server** (SP), **kein Entity-Typ** |
| Eigene Bilder | minimal | `mod_image(w, h, rgba)`: **ein** gemeinsamer Atlas-Slot, Größe begrenzt, „nicht jeden Frame“ | **Lücke** |
| Tod des Spielers | nur Server | `player_died` (`ServerPlayer.die`) | im SP ok; im MP über den Todesscreen (`screen_opening`) ableitbar |
| Weltordner / Seed (SP) | ja | `world_dir` → `level.dat` lesen | Auto-Erkennung im SP machbar |
| Horizons Minimap | ja | `minimap` (Ecke, Größe, Zoom, Rotation, rund) | zu wenig Funktionsumfang; nur als Fallback |

Horizon zeichnet seine eigene Minimap intern in `crates/horizon-client-render/src/minimap.rs`: ein 16×16-Tile pro geladenem Chunk mit `MapColor`, Höhe und Wassertiefe, nach Zeitbudget befüllt. Die Daten wären also vorhanden, sind aber nicht als Mod-API freigegeben.

## Was fehlt: Erweiterungsvorschläge für spätere PRs an Horizon

Jeder Vorschlag ist als **reiner Zusatz** formuliert, passend zu Horizons Regel, dass Hook-Punkte und `Host`-Methoden nur hinzukommen. Jeder bekommt ein Fragment in `tools/native-hook-fragments/`.

### E1: Client-Chunk-Events (Priorität: kritisch)

- `client_chunk_loaded(dimension, [cx, cz])`, `client_chunk_changed(dimension, [cx, cz])` und `client_chunk_unloaded(dimension, [cx, cz])`.
- Sie feuern im Fenster, auch auf fremden Servern.
- „changed“ wird pro Tick gebündelt, wie Horizons Minimap mit `RESCAN` drosselt.

### E2: Client-Spaltenlesen (Priorität: kritisch)

- `client_read_chunk_columns(dimension, [cx, cz], options) -> Option<ClientChunkColumns>` liefert für die 16×16 Spalten eines **geladenen Client-Chunks**:
  - oberster nicht-leerer Block (State-ID) und seine Höhe (Heightmap `WORLD_SURFACE`),
  - optional erster Block unter einer Y-Grenze (für den Höhlenmodus),
  - Flüssigkeit oben samt Tiefe,
  - Biom-ID an der Oberfläche, Block- und Himmelslicht.
- Dazu `block_state_info(state) -> {name, map_color, tint_source}` einmal pro State, für eine Tabelle statt Strings pro Block.
- Das ist im Kern das, was `minimap.rs` intern ohnehin tut, als Snapshot statt Live-Referenz und damit auch im Hintergrund-Thread verarbeitbar.

### E3: Dynamische Mod-Texturen (Priorität: kritisch)

- `mod_texture_create(w, h) -> TextureId`, `mod_texture_update(id, [x, y, w, h], rgba)` (Teilupdate), `mod_texture_drop(id)` und `mod_texture_quad(batch, id, points[4], uvs[4], argb)`.
- Dazu `push_clip(rect)`/`pop_clip` oder eine Kreismaske für die runde Minimap.
- Der heutige einzelne `mod_image`-Slot reicht weder für Minimap und Weltkarte gleichzeitig noch für Regionstexturen.

### E4: Entity-Radar (Priorität: hoch)

- `NativeClientEntity` ist `#[non_exhaustive]`, deshalb lässt sich das Struct rückwärtskompatibel erweitern:
  - `kind: String` (z. B. `minecraft:zombie`),
  - `category` (Monster, Tier, Wasser, Item, Spieler, Fahrzeug, …),
  - `custom_name`, `tamed_owner`, `glowing`.
- Außerdem soll `client_entities` auch im Singleplayer Daten liefern. Heute steht dort `self.remote.is_none() → leer`.

### E5: Biom an Position (Priorität: mittel)

- `client_biome(pos) -> Option<String>` für die Biomanzeige unter der Minimap.
- Teilweise über E2 abgedeckt.

### E6: Seed im Singleplayer (Priorität: niedrig)

- `singleplayer_seed() -> Option<i64>`.
- Ersatzweise funktioniert heute schon `world_dir()` plus selbst gelesene `level.dat` (gzip-NBT).

## Wie die Mod trotzdem jetzt gebaut werden kann

Die Mod hängt **nicht direkt** an Horizon, sondern an einer eigenen, klar abgegrenzten Schnittstelle (Details in `DECISIONS.md`, D-003):

```
map-core      (reines Rust: Datenmodell, Farben, Regionen, Cache, Wegpunkte, Konfig, Seed Atlas)
   ▲
map-host      (Trait `MapHost`: genau die Fähigkeiten aus E1–E6 + vorhandene Hooks)
   ▲
horizon-adapter  (Horizon-Bundle: Trait-Mod implementiert `Hooks`, übersetzt nach `MapHost`)
```

Für die Lücken gibt es zwei Möglichkeiten, bis E1–E4 upstream sind:

- **(a) Übergangs-Mixins:** Injektionen in Horizon-Interna (z. B. `ChunkMap`-Inserts und die interne Minimap-Tile-Erzeugung), auf einen Nightly gepinnt. Das liefert volle Qualität, muss aber pro Nightly nachgezogen werden.
- **(b) Nur stabile Hooks:** Spaltenlesen über `block(pos)` mit Zeitbudget, und die Karte über `quad`/`fill` statt Texturen zeichnen. Das läuft überall, ist aber langsamer und optisch gröber und verletzt die Qualitätsanforderung.

Der Adapter wird so geschnitten, dass er beim Upstream-Merge von E1–E4 nur die Implementierung tauscht; `map-core` bleibt unverändert.
