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

## D-014: `core/` und `host/` liegen im `[api]`-Ordner (ersetzt Teile von D-008)

Der Launcher kopiert nur den `[api]`-Ordner (`engine/`) nach `native-api/<id>` und trägt Pfad-Pakete selbst in den Lockfile ein (`launcher/src/native_mods/api.rs`). Pfad-Abhängigkeiten nach außerhalb (`../core`) würden im Launcher-Build ins Leere zeigen.

Deshalb gilt jetzt:

- `mixins/meridian/engine/{core,host}`,
- `engine/Cargo.toml` verweist mit `path = "core"` bzw. `path = "host"` darauf,
- der Repo-Workspace listet beide Crates und schließt `engine` aus.

## D-015: Übergangs-Bridge für E1/E2 als Mixin-Module

Die Bridge besteht aus zwei Modulen und drei HEAD-Injektionen (`mixins/meridian/bridge/`):

- `world::meridian_changes` im Render-Crate: Injektionen in `World::set` und `World::edit_runs` melden geschriebene Chunks, mit einem Atomic-Schalter, solange Meridian aus ist.
- `game::meridian_bridge` im Spiel: Die Injektion in `native_frame` meldet einmal pro Frame
  - Laden und Entladen aus den Membership-Deltas der `ChunkMap`,
  - Änderungen,
  - angefragte Chunk-Snapshots (Scan wie Horizons Minimap, Budget 1,5 ms/Frame),
  - Block- und Biom-Aussehen und das Biom am Spieler.
- Der Adapter linkt gegen `horizon_client` und erreicht die Bridge als `horizon_client::game::meridian_bridge`.
- Nur einfache Daten kreuzen die Grenze, `VERSION` sichert das Format ab.

Folge für die Host-Schnittstelle: Chunks werden **angefragt** (`request_chunk`) und **abgeholt** (`take_chunks`), ein Frame später. Die Engine verfolgt offene Anfragen, fragt nach 200 Ticks neu und gibt nach 3 Versuchen auf, bis zum nächsten Lade- oder Änderungsereignis.

`tools/check-adapter.sh` wendet die Mixins mit der horizon-mixins-Revision des Launchers an und kompiliert dagegen. Eigenheiten, die das Skript berücksichtigt:

- `git archive`-Export ohne Symlinks,
- `.horizon-revision` im Export,
- frischer Stage-Ordner auf macOS,
- `[api]` wird vor horizon-mixins entfernt,
- Module dürfen keine `//!`-Kommentare enthalten.

## D-016: Minimap vorerst aus HUD-Flächen statt Textur

Der einzige Bild-Slot für Mods (`mod_image`) ist im Quellcode nicht einmal definiert (siehe PROGRESS) und würde von allen Mods geteilt. Bis E3 (dynamische Texturen) gilt deshalb:

- Die Minimap wird als Raster aus `fill`-Rechtecken gezeichnet, höchstens 160 Zellen pro Seite.
- Jede Zelle tastet die Welt an ihrer gedrehten Position ab; gleichfarbige Nachbarn einer Zeile werden zusammengefasst.
- Die runde Form entsteht durch ausgelassene Zellen.

Das funktioniert nur mit stabilen APIs, ist aber auf etwa 1 Zelle pro GUI-Pixel begrenzt. Für die Vollbildkarte (Phase 4) reicht das nicht, dort wird E3 nötig (Übergangs-Mixin oder Upstream).

## D-017: Texturfarben vorläufig

Die mittlere Texturfarbe pro Block ist noch nicht verfügbar: Die Sprite-Arrays liegen auf der GPU. Bis Phase 4 gilt:

- „Genau“ nutzt für eingefärbte Blöcke Grau × Biom-Tint (Tint-Quelle aus Horizons `block_sprites`, Oberseite),
- für alle anderen Blöcke die Kartenfarbe.

Die beiden Farbstile unterscheiden sich daher vorerst kaum.

## D-018: Todespunkte nur im Singleplayer, Wegpunkt-Editor folgt

- `player_died` ist ein Server-Hook und liefert nur im Singleplayer den eigenen Tod. Für Multiplayer kommt eine Erkennung über den Todesbildschirm.
- **B** legt vorerst wie **Num +** sofort einen Wegpunkt an. Editor und Liste als eigene Screens folgen im nächsten Schritt.

## D-019: Optik auf den ersten Blick wie die Referenz (Wunsch des Auftraggebers, 2026-11-06)

Abweichend vom ursprünglichen „Optik darf abweichen“ soll Meridian auf den ersten Blick wie die Referenz aussehen.

Umsetzung clean-room:

- Beobachtung der laufenden Referenz in der Prism-Instanz (Bildschirmaufnahmen, Quelle O3).
- Nachbau mit eigenen Zeichenmitteln: Flächen, Text, Shader.
- Keine Textur, kein Icon und keine Sprachdatei wird übernommen. Name und Menütexte bleiben Meridian.
- Die Referenz-Screenshots bleiben lokal und kommen nicht ins Repo.

Gemessene Werte (`core/src/minimap.rs`):

- Minimap oben links. Eine eigene Einheit = Bildschirmhöhe / 594, auf ganze physische Pixel gerundet.
- Seite = 23,2 % der GUI-Höhe, 2 Blöcke pro Einheit.
- Rahmen: Schwarz 1, Hellgrau 2 (oben/links heller), Dunkel 1.
- Abstand zum Rand: 5 Einheiten.
- Himmelsrichtungen: 6×6-Einheiten-Kästchen in Blau-Violett mit weißem Buchstaben (Schrift 0,75 Einheiten).
- Spieler: 5-Einheiten-Kreuz, Weiß mit violetten Enden (Pfeil nur bei Nord-Lock).
- Infozeile: nur Koordinaten, Schrift 1 Einheit, 3 Einheiten unter dem Rahmen. Biom ist wie in der Referenz standardmäßig aus.
- Wegpunkte:
  - auf der Minimap 9-Einheiten-Quadrat in Wegpunktfarbe mit weißem Kürzel;
  - in der Welt Quadrat (60 % Deckkraft), darunter der Name (Schrift 0,64) auf dunkler Platte, darunter die Distanz (Schrift 1) mit einer Nachkommastelle und dem Dezimaltrenner des Systems („30,1m“);
  - kein Beam (abschaltbare Option `waypoints.beam`).
- Gelände: mittlere Texturfarben der Blocktexturen des Spielers × Biom-Tint. Ebene Flächen behalten ihre Farbe, Hänge werden heller bzw. dunkler. Tageslicht folgt Minecrafts Himmelskurve und ist tagsüber voll hell.

## D-020: Minimap-Gelände auf der GPU über `RenderHooks::world_drawn` (ersetzt D-016)

Flächen-Rechtecke reichen für die Detaildichte der Referenz (rund 270 Blöcke Kartenbreite, ein Texel pro Block) nicht aus. Deshalb:

- Der Adapter gibt über `Hooks::render()` einen Renderer an Horizon (`engine/gpu.rs`).
- Der Spiel-Thread stellt ein nordausgerichtetes Bild aus den Regionspixeln zusammen. Das geschieht nur bei einem Blockwechsel des Spielers oder bei neuen Regionsrevisionen.
- Der Render-Thread lädt das Bild hoch und zeichnet ein Rechteck. Der Shader übernimmt Drehung, Subpixel-Position, runde Maske, Deckkraft und den Hintergrund.
- Das HUD zeichnet Rahmen, Kompass, Marken und Text darüber.
- Läuft das HUD nicht (F1, Screens), verfällt das Bild nach 200 ms.

Einschränkung: Der Hook läuft vor Horizons Nachbearbeitung. Morgens und abends dunkelt Horizon die Karte deshalb mit der Welt ab, die Referenz nicht. Eine saubere Lösung braucht eine HUD-Textur-API (E3).

## D-021: Screens im Layout der Referenz

Wegpunkt-Editor (B), Wegpunktliste (U) und Einstellungen (Y) sind nach den Referenzaufnahmen (O3) in GUI-Pixeln um die Bildschirmmitte gesetzt (`core/src/screens.rs`, Tests prüfen die gemessenen Positionen bei 432×246).

- Optik: Vanilla-Knöpfe aus den GUI-Sprites des Spielers (`gui/sprites/widget/button*`, nine-slice über Horizons `gui_nine_slice`); schwarze Felder und Auswahlboxen mit grauem Rand; Platzhalter in Grau; Farbname in seiner Farbe.
- Hintergrund: Meridian fordert über `menu_background` die normale Menü-Unschärfe an (Wert aus den Spieleinstellungen) und dunkelt leicht ab, wie die Menüs im Spiel.
- Eingaben laufen über Horizons Screen-Hooks, die nur lesen dürfen. Schließen, Teleport und Teilen stellt Meridian deshalb in eine Warteschlange, die der nächste `frame`-Hook abarbeitet.
- Escape schließt wie im Spiel.
- B öffnet jetzt den Editor (Name nötig, leere Koordinaten = Spielerposition, zufällige helle Farbe); Num + legt weiterhin sofort einen Wegpunkt an.
- Einstellungen: Hauptseite wie die Referenz; Unterseiten mit „Name: Wert“-Knöpfen (Linksklick weiter, Rechtsklick zurück); Suche über alle Optionen. „Profile Name“, „Change Position“ und „Delete Profile“ sind noch deaktiviert.

## D-022: Entity-Radar über die Bridge (E4 vorerst überbrückt)

Horizons `client_entities` kennt keinen Entity-Typ. Die Bridge (Version 3) kopiert deshalb pro Frame die Entities des Clients, aber nur solange das Radar sie anfordert:

- Mobs mit Typ, Klasse (Spieler über `remote_player_uuid`, feindlich über `MobType.category == "monster"`, sonst friedlich), Unsichtbarkeit und Namen,
- abgelegte Items,
- Positionen auf den Frame interpoliert.

Rüstungsständer sind ausgeschlossen. Boote und Loren (Klasse „Sonstige“) fehlen noch.

Die Darstellung folgt den Standard-Kategorien der Referenz (O2):

- Lebewesen gelb (feindliche wie friedliche), Spieler weiß, Items rot, Sonstige violett,
- Rauten mit schwarzem Rand, 2 Einheiten,
- ±20 Blöcke Höhenlimit (Spieler 2050), Abdunklung nach Höhenunterschied,
- Zeichenreihenfolge Items → Lebewesen → Feindliche → Spieler,
- Filter pro Klasse, Umkehren per Taste.

Symbole bzw. Köpfe statt Punkten (RAD-03) und Namen folgen.

## D-023: Weltkarte als eigener Screen mit GPU-Regionstexturen

M öffnet die Weltkarte als Meridian-Screen (`core/src/worldmap.rs`, Adapter `gpu_map.rs`).

- Gelände: `RenderHooks::world_drawn` zeichnet jede sichtbare Region als Quad. Die Textur hat je nach Bildschirmgröße der Region 512, 128 oder 32 Texel (Mittelwert der erkundeten Pixel). Pro Frame werden höchstens 6 Texturen hochgeladen, die nächsten zuerst. Bis zu 256 MB bleiben im Cache, auch für Regionen, die nicht mehr im Speicher sind. So zeigt eine weit herausgezoomte Karte Gesehenes, ohne es neu zu laden.
- Geladen werden höchstens 15×15 Regionen um die Kartenmitte, gezeichnet höchstens 97×97.
- HUD und Hand blendet Meridian unter der Karte aus (`gui_layer_pre`, `render_hand`), statt die Karte als HUD-Bild zu malen. Keine Menü-Unschärfe.
- Maße nach Referenzaufnahme r07 in Minimap-Einheiten:
  - Zoom: 3,0x ≈ 1,9 Einheiten pro Block, Mausrad ×1,2 pro Raste um den Cursor.
  - Texte: 2,3 Einheiten pro Schriftpixel, auf dunklen Plaketten. Oben stehen die Koordinaten des Blocks unter dem Cursor und sein Biom, unten der Zoom.
  - Kompass oben rechts, Zoomknöpfe rechts auf 31 % und 39 % der Höhe.
  - Roter Spielerpfeil, Wegpunkte als Schild mit Initiale, Entities als Punkte, Fußspuren.
- Bedienung:
  - Ziehen verschiebt, Pfeiltasten/WASD verschieben, Leertaste zentriert auf den Spieler, M schließt.
  - B legt am Cursor einen Wegpunkt an.
  - Rechtsklick öffnet ein Menü: Koordinaten, Wegpunkt erstellen, hierher teleportieren, Position teilen; auf einem Wegpunkt zusätzlich bearbeiten, hinteleportieren, löschen. Vom Menü geöffnete Screens kehren zur Karte zurück.

## D-024: Biome pro Spalte im Regionsformat (Version 2)

Die Karte zeigt das Biom unter dem Cursor. Dafür speichert jede Kachel eine Biom-Palette (höchstens 255 Namen) und jede Spalte einen Index (Spalte jetzt 15 Byte).

- `.mrg` Version 2 schreibt die Palette vor die Spalten jeder Kachel.
- Version-1-Dateien werden weiter gelesen: Ihr Biom ist unbekannt, bis der Chunk neu erfasst wird.

## D-025: Seed Atlas als reiner Rust-Port

Die Fabric-Version rechnet mit einer C-Bibliothek (Seed-Atlas-Engine, aus cubiomes abgeleitet). Horizon-Bundles dürfen nur Crates aus Horizons Lockfile nutzen, und `cc` bzw. `libloading` sind dort nicht enthalten. Meridian portiert die Engine deshalb nach Rust (`engine/seedatlas`, `forbid(unsafe_code)`), und zwar nur den Pfad für Minecraft 26.3.

- Die API spiegelt die native Schnittstelle der Fabric-Version (`sax_*`, ABI 4) Aufruf für Aufruf.
- Parität: `tools/seedatlas-fixtures/` baut lokal die echte Glue-Schicht der Fabric-Version samt Engine und schreibt Fixtures. Die Tests vergleichen jede Zahl:
  - Biome: Punkte, Flächen mit Schrittweite 1 bis 1024, Chunks;
  - drei Dimensionen, normale und große Biome, sechs Seeds.
- Gleitkomma: Die Fabric-Bibliothek für Apple Silicon ist mit clang gebaut, das `a*b + c` zu FMA zusammenzieht. Weit draußen im End (über rund 78.000 Blöcke) ändert das die Inselgewichte. Meridian rechnet dort mit `mul_add` wie dieser Build. Auf x86-Builds der Fabric-Version können End-Biome so weit draußen abweichen.
- Biomnamen, Farben und Existenz kommen generiert aus der Engine (`biome_table.c`), der Biom-Suchbaum aus ihrer Tabelle (`btree_table.py`).
- Strukturen: Positionen, Biom- und Geländeprüfung, Varianten, Festungsringe, Spawn, Schleimchunks, Erzadern, End-Inseln, End-Gateways und End-Schiffe (Teile der End-Stadt) sind portiert. Die Paritätstests vergleichen jeden Treffer der Fabric-Schnittstelle für 25 Strukturtypen mit und ohne Biomprüfung, über fünf Seeds (`structures.txt.gz`).

## D-026: Seed Atlas auf dem Stand von main, alle Minecraft-Versionen

Der Port folgt jetzt DUzzL/Seed-Atlas main (Engine, identisch mit fc2c628) und DUzzL/seed-atlas-xaero main (Glue ABI 6, Version 2.0.0). Wie in der Fabric-Version sind alle wählbaren Versionen von 1.1 bis 26.3 portiert:

- Layer-Stack bis 1.17, Klima-Bäume ab 1.18;
- Nether und End je Version;
- Strukturen, Festungen und Spawn je Version;
- die neuen Funktionen für Geländehöhen und den Nether-Boden (mit und ohne Carving-Maske).

Fixtures liegen pro Version in `seedatlas/tests/fixtures/<version>.txt.gz`. Sie stammen aus der echten Glue-Schicht (`tools/seedatlas-fixtures/build.sh`). `tests/parity.rs` prüft jede Zeile aller 38 Versionen, alle stimmen exakt überein. Die Arbeit lief in getrennten Teilaufgaben (Layer, Klima, Gelände, Strukturpositionen, Strukturprüfungen). Die Regeln dafür stehen in `docs/SEEDATLAS_PORT.md`.

## D-027: Übersichtsbilder für die herausgezoomte Weltkarte

Eine voll erkundete Region braucht im Speicher rund 5 MiB (1024 Kacheln mit je 256 Spalten plus 512 × 512 Pixel). Herausgezoomt hielt die Karte bisher bis zu 15 × 15 Regionen ganz, bei großen Karten also über 1 GiB.

- Ist eine Region auf dem Bildschirm kleiner als 96 GUI-Pixel, bleiben nur die 5 × 5 Regionen um die Kartenmitte ganz im Speicher.
- Alle anderen zeichnet die Karte aus Übersichtsbildern: 128 × 128 Pixel (64 KiB), der Mittelwert aus je 4 × 4 Pixeln.
  - Sie entstehen, wenn eine Region aus dem Speicher fällt.
  - Für sichtbare Regionen ohne Bild lädt die Engine bis zu vier Regionsdateien gleichzeitig, zeichnet sie, behält das Bild und gibt die Region wieder frei.
  - Höchstens 4096 Bilder (256 MiB) bleiben, die ältesten fallen zuerst.
- Ändert sich die Schattierung (Tageszeit, Stil), gelten die Bilder als veraltet. Sie werden nach und nach erneuert und bis dahin weiter gezeigt.

## D-028: Profile und Positionseditor

- Profile: Die Hauptseite der Einstellungen wechselt das Profil (Klick vor, Rechtsklick zurück) und legt neue Profile an („New Profile“, eine Kopie des aktuellen). Ein Klick auf „ID“ benennt das Profil um, „Delete Profile“ löscht es, solange es nicht das letzte ist.
- Positionseditor: „Change Position“ zeigt die Minimap über dem Screen. Man zieht sie mit der Maus. Sie rastet an der nächsten Ecke ein und behält ihren Abstand dazu (`corner` + `offset`), auch bei anderer Fenstergröße.
- Die Taste `]` und der Knopf „M“ auf der Karte öffnen die eigene Seite „World Map Settings“. „Back“ schließt sie dort direkt, wie bei der Seed-Atlas-Seite.
