# Entscheidungen

Kleinere Entscheidungen, die ich selbst getroffen habe. Größere sind als **offen** markiert und warten auf Rückmeldung.

## D-001: Entwicklungssystem ist macOS, nicht Windows

`TASK.md` nennt einen Windows-PC. Die tatsächliche Umgebung ist macOS (Apple Silicon):

- Horizon-Nightly unter `~/Library/Application Support/horizon-launcher/versions/`,
- Prism unter `~/Library/Application Support/PrismLauncher` mit der Instanz `26.3`.

Ich arbeite mit diesen Pfaden. CI baut zusätzlich auf Windows und Linux.

## D-002: Minecraft-Version 26.3

`TASK.md` nennt 26.2 für Seed Atlas. Der lokale Seed-Atlas-Stand (`seed-atlas-xaero` 0.5.1), die Prism-Instanz und Horizon verwenden aber **26.3**. Ziel und Paritäts-Fixtures richten sich deshalb nach 26.3.

## D-003: Schichtenarchitektur mit eigener Host-Schnittstelle

- `map-core` kennt Horizon nicht.
- `map-host` definiert das Trait `MapHost`.
- Der Horizon-Adapter ist das einzige Crate mit Abhängigkeit auf `horizon-client`.

Begründung:

- Kern und Seed Atlas sind ohne Spiel testbar (CI ohne Minecraft-Ressourcen).
- Fehlende Horizon-APIs (E1–E6 in `HORIZON_API.md`) lassen sich später austauschen, ohne `map-core` umzubauen.

## D-004: Trait-Mod statt reiner Mixins

Wo Katalog-Hooks reichen, nutzt der Adapter die Trait-Mod-API (`"hooks": N`, kein Revision-Pin). Mixins kommen nur für Lücken zum Einsatz (siehe D-005).

## D-005: Umgang mit fehlenden Client-APIs (entschieden 2026-11-06)

Zur Wahl stehen:

- **(a)** Übergangs-Mixins, auf einen Nightly gepinnt, parallel dazu Upstream-PRs E1–E4,
- **(b)** nur stabile Hooks mit langsamen Fallbacks,
- **(c)** zuerst die Upstream-PRs.

**Entschieden: (a).** Übergangs-Mixins im Horizon-Adapter, gepinnt auf einen Nightly; parallel Upstream-PRs E1–E4 vorbereiten.

## D-006: Lizenz GPLv3 (entschieden 2026-11-06)

Seed Atlas (`seed-atlas-xaero`) steht unter GPLv3. Seine Engine stammt aus `DUzzL/Seed-Atlas`, abgeleitet von cubiomes (MIT) und cubiomes-viewer (GPLv3).

- Ein Port der GPL-Teile macht das neue Repo zu GPLv3.
- Horizon (MIT) ist damit kompatibel.

**Entschieden: GPLv3** für das gesamte Repo.

## D-007: Name Meridian (entschieden 2026-11-06)

**Entschieden: Meridian** (Minimap + Weltkarte), ohne Bezug zu Xaero.

## D-008: Repo-Layout = Bundle-Layout

Das Repo enthält das native Bundle direkt unter `mixins/meridian/`:

- `horizon-mixin.toml` (Manifest des Bundles),
- `engine/` (das `[api]`-Crate `meridian-horizon`, der Horizon-Adapter),
- `core/` (`meridian-core`) und `host/` (`meridian-host`).

Der Launcher baut den Ordner so, wie er ist. Es gibt keinen Kopier- oder Generierungsschritt.

Folge: Die Crates dürfen **nichts aus dem Repo-Workspace erben** (`version.workspace`, `[workspace.dependencies]`, `[workspace.lints]`). Im Bundle existiert dieser Workspace nicht. Jedes Crate führt seine Versionen und Lints selbst. Der Root-`Cargo.toml` listet nur die Mitglieder auf und schließt `engine/` aus.

## D-009: Nur Crates in Horizons Versionen

Native Mods dürfen nur Crates und Versionen aus Horizons `Cargo.lock` nutzen. Daraus folgt:

- Der Repo-Lockfile wird aus Horizons Lockfile erzeugt (kopieren, dann von Cargo kürzen lassen).
- `tools/check_lock.py` prüft das in CI.
- Abhängigkeiten werden exakt gepinnt (`=x.y.z`).
- Erlaubt sind derzeit serde, serde_json, toml, flate2, parking_lot, log und thiserror. Für Threads gibt es kein crossbeam und kein rayon; der Worker-Pool nutzt `std::sync::mpsc`.

## D-010: Adapter-Prüfung außerhalb des Repos

`tools/check-adapter.sh` holt den gepinnten Horizon-Commit nach `~/.cache/meridian/horizon-<rev>`. Innerhalb des Repos würde Cargo unseren Workspace für Horizons halten.

Geprüft wird mit Horizons Lockfile und Horizons `[patch.crates-io]` (wgpu-hal, naga, libmimalloc-sys), also so, wie der Launcher das Spiel baut. Dauer lokal etwa 25 s inkrementell.

## D-011: Eigenes Regionsformat, gespeichert werden Tiles statt Pixel

- Region = 32×32 Chunks = 512×512 Blöcke.
- Gespeichert werden pro Spalte die Farben beider Stile, Höhe, Wasser und Licht (14 Byte), zlib-komprimiert (`.mrg`, Details in `core/src/codec.rs`).
- Pixel werden nach dem Laden neu gezeichnet. Ein Stilwechsel (Farben, Schattierung, Licht) braucht also weder Neu-Lesen noch Dateikonvertierung.
- Pfad: `horizon-mods/.horizon-mod-data/meridian/worlds/<sp-welt|mp-adresse>/<karte>/<dimension>/regions/x_z.mrg`. Jeder Teil ist escaped (Windows-sicher, Groß-/Kleinschreibung-neutral).

## D-012: Standardwerte mit Unsicherheit

- `minimap_block_colors = 0` in der Referenzkonfiguration ist als „genau“ (Texturfarben) interpretiert, nicht als „Vanilla“.
- `minimap_auto_cave_mode = 2` ist als „Dach ≥ 3×3“ interpretiert.

Beides ist im Spiel zu bestätigen (O3, `KEYBINDS.md`).

## D-013: Was der Adapter bis Phase 3 nicht kann

Chunk-Ereignisse (E1) und das Lesen von Chunks samt Blockfarben (E2) fehlen noch:

- `HorizonHost::read_chunk` liefert `None`.
- Engine, Speicherung, Tasten, Einstellungen und Welt-/Dimensionswechsel laufen bereits.
- Die Übergangs-Mixins für E1/E2 kommen in Phase 3. Sie brauchen den Pin `horizon_revision`, der bereits auf die installierte Nightly gesetzt ist.
