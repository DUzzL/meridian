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

Folge für die Host-Schnittstelle: Chunks werden **angefragt** (`request_chunk`) und **abgeholt** (`take_chunks`), ein Frame später. Die Engine verfolgt offene Anfragen, fragt nach 10 s neu (seit D-039 Wanduhrzeit; vorher zählten die „200 Ticks“ tatsächlich Frames) und gibt nach 3 Versuchen auf, bis zum nächsten Lade- oder Änderungsereignis.

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

Rüstungsständer waren hier ausgeschlossen (seit D-037 zählen sie zu den sonstigen Entities). Boote und Loren liest die Bridge ebenfalls, als Klasse „Sonstige“.

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
  - drei Dimensionen, normale und große Biome (große nur für Seed 0), drei Seeds (`fixtures.c`).
- Gleitkomma: Die Fabric-Bibliothek für Apple Silicon ist mit clang gebaut, das `a*b + c` zu FMA zusammenzieht. Weit draußen im End (über rund 78.000 Blöcke) ändert das die Inselgewichte. Meridian rechnet dort mit `mul_add` wie dieser Build. Auf x86-Builds der Fabric-Version können End-Biome so weit draußen abweichen.
- Biomnamen, Farben und Existenz kommen generiert aus der Engine (`biome_table.c`), der Biom-Suchbaum aus ihrer Tabelle (`btree_table.py`).
- Strukturen: Positionen, Biom- und Geländeprüfung, Varianten, Festungsringe, Spawn, Schleimchunks, Erzadern, End-Inseln, End-Gateways und End-Schiffe (Teile der End-Stadt) sind portiert. Die Paritätstests vergleichen jeden Treffer der Fabric-Schnittstelle für 25 Strukturtypen mit und ohne Biomprüfung, über drei Seeds (`structures.txt.gz`).

## D-026: Seed Atlas auf dem Stand von main, alle Minecraft-Versionen

Der Port folgt jetzt DUzzL/Seed-Atlas main (Engine, identisch mit fc2c628) und DUzzL/seed-atlas-xaero main (Glue ABI 6, Version 2.0.0). Wie in der Fabric-Version sind alle wählbaren Versionen von 1.1 bis 26.3 portiert:

- Layer-Stack bis 1.17, Klima-Bäume ab 1.18;
- Nether und End je Version;
- Strukturen, Festungen und Spawn je Version;
- die neuen Funktionen für Geländehöhen und den Nether-Boden (mit und ohne Carving-Maske).

Fixtures liegen pro Version in `seedatlas/tests/fixtures/<version>.txt.gz`. Sie stammen aus der echten Glue-Schicht (`tools/seedatlas-fixtures/build.sh`). `tests/parity.rs` prüft jede Zeile aller 38 Versionen: Biome, Strukturen und Spawn stimmen exakt überein, Geländehöhen auf eine relative Toleranz von 1e-6. Die Arbeit lief in getrennten Teilaufgaben (Layer, Klima, Gelände, Strukturpositionen, Strukturprüfungen). Die Regeln dafür stehen in `docs/SEEDATLAS_PORT.md`.

## D-027: Übersichtsbilder für die herausgezoomte Weltkarte

Eine voll erkundete Region braucht im Speicher rund 5 MiB (1024 Kacheln mit je 256 Spalten plus 512 × 512 Pixel). Herausgezoomt hielt die Karte bisher bis zu 15 × 15 Regionen ganz, bei großen Karten also über 1 GiB.

- Ist eine Region auf dem Bildschirm kleiner als 96 GUI-Pixel, bleiben nur die 5 × 5 Regionen um die Kartenmitte ganz im Speicher. (Bis D-039 hörte die Karte dort nur auf, neue Regionen anzufordern; schon geladene blieben bis zum Regionslimit. Seit D-039 werden alle anderen tatsächlich zu Übersichtsbildern, und ein Speicherbudget begrenzt die ganzen Regionen.)
- Alle anderen zeichnet die Karte aus Übersichtsbildern: 128 × 128 Pixel (64 KiB), der Mittelwert aus je 4 × 4 Pixeln.
  - Sie entstehen, wenn eine Region aus dem Speicher fällt.
  - Für sichtbare Regionen ohne Bild lädt die Engine bis zu vier Regionsdateien gleichzeitig, zeichnet sie, behält das Bild und gibt die Region wieder frei.
  - Höchstens 4096 Bilder (256 MiB) bleiben, die ältesten fallen zuerst.
- Ändert sich die Schattierung (Tageszeit, Stil), gelten die Bilder als veraltet. Sie werden nach und nach erneuert und bis dahin weiter gezeigt.

## D-028: Profile und Positionseditor

- Profile: Die Hauptseite der Einstellungen wechselt das Profil (Klick vor, Rechtsklick zurück) und legt neue Profile an („New Profile“, eine Kopie des aktuellen). Ein Klick auf „ID“ benennt das Profil um, „Delete Profile“ löscht es, solange es nicht das letzte ist.
- Positionseditor: „Change Position“ zeigt die Minimap über dem Screen. Man zieht sie mit der Maus. Sie rastet an der nächsten Ecke ein und behält ihren Abstand dazu (`corner` + `offset`), auch bei anderer Fenstergröße.
- Die Taste `]` und der Knopf „M“ auf der Karte öffnen die eigene Seite „World Map Settings“. „Back“ schließt sie dort direkt, wie bei der Seed-Atlas-Seite.

## D-029: Mehrere Karten je Server nach Weltspawn

Server-Netzwerke zeigen hinter einer Adresse mehrere Welten (Lobby, Modi). Meridian ordnet Karten deshalb standardmäßig nach dem Weltspawn, den der Server meldet (`SetDefaultSpawnPosition`, über die Bridge, Version 6).

- Jeder neue Spawn bekommt eine eigene Karte (`map-2`, `map-3`, …).
- Der erste Spawn übernimmt die Standardkarte. Ein Server mit einer einzigen Welt behält so genau eine Karte, und bisherige Daten bleiben erhalten.
- Die Liste liegt pro Server in `worlds/<server>/maps.toml`.
- Wegpunkte und Seed-Atlas-Seeds gehören zur jeweiligen Karte. Der Seed-Kontext anderer Karten ist `<server>/<karte>`.
- Beim Laden eines Levels gilt zunächst die zuletzt gezeigte Karte, weil der gemeldete Spawn noch vom vorigen Server stammen kann. Sobald der neue Spawn bekannt ist, wechselt die Karte.
- Einzelspieler hat immer genau eine Karte.
- Ändert ein Server seinen Spawn (`/setworldspawn`), entsteht eine neue Karte. Mit `/meridian map use default` kehrt man zur alten zurück; das stellt die Auswahl auf manuell.

## D-030: Seed-Atlas-Kacheln: Warteschlange pro Frame, ein Sample je zwei Pixel

Im Spiel lud die Seed-Atlas-Ebene von außen nach innen, die Mitte blieb oft schwarz, und der Rechner lief dauerhaft unter Last. Drei Ursachen:

- **Reihenfolge:**
  - Die Karte fragte die Kacheln von der Mitte nach außen an, die Worker nahmen aber die neueste Anfrage zuerst.
  - Die Warteschlange hielt nur 64 Einträge und warf die ältesten weg, also genau die Mitte.
  - Jeden Frame wurde die Mitte neu angefragt und wieder verdrängt.
- **Detail:** ein Sample je Bildschirmpixel. Eine Kachel kostet unabhängig von der Schrittweite 20–50 ms. Ein Bildschirm mit 3024 × 1720 Pixeln brauchte so rund 320 Kacheln, etwa 13 s CPU-Zeit.
- **Zoomen:** Beim Wechsel der Schrittweite gab es nur einen Ersatz, nämlich Kacheln mit vierfacher Schrittweite.

Lösung:

- **Warteschlange pro Frame:**
  - Was die Karte in einem Frame anfragt, ersetzt beim nächsten `poll` die Warteschlange, in derselben Reihenfolge (Mitte zuerst).
  - Die Worker nehmen vorne.
  - Was nicht mehr angefragt wird, wird nie berechnet. Ist die Karte zu, leert sich die Warteschlange.
  - Laufende Jobs dürfen fertig werden.
- **Ein Sample je zwei physische Pixel:** Das ist ein Viertel der Arbeit. Auf Retina-Bildschirmen ist es weiterhin ein Sample je Punkt.
- **Ersatz beim Zoomen:**
  - Beim Herauszoomen dienen die vier Kacheln der nächstfeineren Schrittweite.
  - Beim Hineinzoomen dient die nächste vorhandene gröbere Kachel (doppelte bis achtfache Schrittweite).
- **Gelände-Regionen:** Weit herausgezoomt zeichnet die Karte nur Regionen mit Daten statt bis zu 97 × 97 leerer Flächen je Frame.

Gemessen im Spiel bei 0,1x:

- Der Bildschirm ist nach rund 2 s gefüllt, von der Mitte nach außen.
- Danach liegt die CPU-Last auf dem Niveau des Spiels ohne Karte.
- Zurückschieben zeigt die Kacheln sofort aus dem Cache.

## D-031: Strukturmarker für die ganze Sicht, wie in der Fabric-Version

Bisher fragte Meridian Marker je Region (512 Blöcke) an, nur im Umkreis von 7 Regionen um die Kartenmitte. Herausgezoomt standen die Marker deshalb in einem Quadrat. Meridian folgt jetzt `SeedAtlasStructureMarkerSource` der Fabric-Version 2.0 (`seedatlas.rs`: `view_box`, `visible_at_zoom`, `SeedService::view_markers`):

- **Ein Scan je Strukturart** für ein Quadrat um die Kartenmitte:
  - Radius: halbe Sicht plus 1024 Blöcke, mindestens 1024, höchstens 65536, aufgerundet auf ein Raster.
  - Raster: die höchste Zweierpotenz von `Radius / 4`, begrenzt auf 1024 bis 16384.
  - Die Mitte rastet auf dieses Raster ein, kleine Bewegungen behalten den Scan.
  - Die Sicht in Blöcken ist wie in der Fabric-Version (`request`): Fensterpixel geteilt durch den Maßstab der Karte (ihren Zoom, mindestens 1/512).
- **Ergebnisgrenzen der Glue-Schicht:** 1024 je Art, 4096 Schleimchunks (`scan_structures_capped`, `scan_slime_chunks_capped`, wie `append_result`).
  - In der Fabric-Version endet der Scan bei der Grenze, also in Zeilen von Nord nach Süd. Weit herausgezoomt zeigten dichte Arten (Ozeanmonumente) deshalb nur im Norden Marker.
  - **Bewusste Abweichung (auf Wunsch):** Meridian verteilt die Treffer gleichmäßig (`Atlas::scan_spread`, `thin_evenly`).
    - Hat das Quadrat weit mehr Kandidaten-Zellen als das Vierfache der Grenze, prüft der Scan nur jede k-te Zelle in beide Richtungen (Region, Erzader-Zelle, Chunk).
    - Danach bleibt höchstens ein Treffer je Zelle eines Zweierpotenz-Rasters, bis die Grenze erreicht ist.
    - Beide Raster sind an der Welt ausgerichtet, damit Marker beim Verschieben an ihrem Platz bleiben.
    - Kleine Quadrate behalten alle Treffer, genau wie die Glue-Schicht.
- **Zoomfilter (`visibleAtZoom`)** mit dem Zoom der Karte (wie Xaeros Anzeige, z. B. „0.1x“):

  | Arten | sichtbar ab Zoom | höchstens bis Radius |
  |---|---|---|
  | Schleimchunks | 0,5 | 2048 |
  | Erzadern, Geoden, Brunnen, Schätze | 0,25 | 8192 |
  | Minenschächte | 0,25 | 12288 |
  | Lager, Dörfer, Schiffswracks, Ruinen, Portale, Pfade, Kammern, Außenposten, Tempel, Hütten, Iglus | 0,125 | 24576 |
  | Ozeanmonumente (auf Wunsch; in der Java-Mod nur im offenen PR #12) | 0,0625 | – |
  | Spawn, Festungen, Anwesen, antike Städte, Netherfestungen, Bastionen, End-Städte, Gateways | immer | – |
  | Übrige | 0,03125 | – |

- **Caching:**
  - Scans werden erst nach 150 ms Stillstand gestartet.
  - Die Ergebnisse der letzten 8 Sichten bleiben.
  - Bis ein neuer Scan fertig ist, bleiben die alten Marker sichtbar.
  - Vom Zoom ausgeblendete Arten bleiben im Speicher (`mergeWithSkipped`).
  - Scans kommen in der Warteschlange vor den Kacheln.

## D-032: Seed-Atlas-Einstellungen als eigene Screens wie in der Fabric-Version

Die Seed-Atlas-Einstellungen waren bisher eine Gruppe im Raster der Minimap-Einstellungen. Jetzt sind es eigene Screens nach `SeedAtlasSettingsScreen`, `MarkerSettingsScreen` mit `MarkerToggleList` und `PerformanceSettingsScreen` der Fabric-Version 2.0 (`screens_sa.rs`): gleiche Knöpfe, Texte und Positionen in GUI-Pixeln.

- **„Seed Atlas Settings“:**
  - Inhalt 340 px breit, Reihen bei y 34/59/98/125/150/175 mit zwei bzw. drei Spalten.
  - Ebene, Welt, Deckkraft als Schieberegler, Überlagern, Y-Felder für Oberwelt, Nether und End (übernommen beim Verlassen, `commitValues`), Strukturen, Strukturarten, Leistung, Gelände-Look (vor 1.18 inaktiv), Biom-Hervorhebung, nächstes Biom, nächste Struktur.
  - Darunter der aktive Seed und „Done“; Tooltips wie im Original.
- **„Seed Atlas Markers“:**
  - Kopfzeile: Schieberegler „Completed“, „Marker size“ (50–250 %), „Reset completed (n)“.
  - Darunter eine scrollbare Liste aller 28 Arten: Zeilen 28 px hoch, grüner bzw. grauer Streifen, Icon, Name, Zurücksetzen-Knopf mit Abzeichen und Anzahl, „On/Off“; Scrollbalken zum Ziehen.
  - Das Zurücksetzen einzelner Arten fragt wie im Original nach.
- **„Seed Atlas Performance“:** Biomauflösung, Worker-Threads, Hintergrund-Prefetch und der Beschreibungstext.
- **Hintergrund:** Menüs, die von der Karte aus geöffnet werden, zeigen die Karte dahinter, ohne Weichzeichner und mit `0x88000000` abgedunkelt (`SeedAtlasScreen`). Die Karte bekommt den Cursor bei (−1, −1).
- **Neue Bausteine:**
  - Schieberegler aus den Sprites des Spiels;
  - Listenpanel;
  - Struktur-Icons in Menüs (pixelgenau unter skalierter Pose);
  - umbrochener Text, Tooltips (umbrochen auf 170 px).
- Die HUD-Schnittstelle von Horizon kennt kein Clipping (Scissor). Zeilen am Listenrand zeigen deshalb nur ihren Hintergrund, bis sie ganz sichtbar sind.

## D-033: Strukturmenü, Monumente, Icongröße der antiken Städte

- **Rechtsklick auf eine Struktur** öffnet wie in der Fabric-Version ein eigenes Menü (`getRightClickOptions`): der Name grau als Kopf, dann „Mark as completed/incomplete“, „Copy coordinates“, „Create waypoint“, „Create temporary waypoint“.
  - Bisher hingen diese Einträge am allgemeinen Kartenmenü, und das Feld war für „Create temporary waypoint“ zu schmal.
  - Menüs sind jetzt so breit wie ihr längster Eintrag, gemessen mit den Zeichenbreiten der Spielschrift (`ui::text_width`).
  - „Copy coordinates“ geht über die Bridge (Version 7) in die Zwischenablage des Spiels.
  - Tooltips verschwinden, solange ein Menü offen ist.
- **Auf Wunsch, in Meridian und in der Java-Mod gleich** (Branch `marker-zoom-and-ancient-city-size` in DUzzL/seed-atlas-xaero, PR #12, Commit `007f94f`; Stand des Audits: **nicht gemergt**, `main` (`58c70ff`) zeigt Monumente weiter immer und antike Städte mit 18 Pixeln):
  - Ozeanmonumente erst ab Zoom 0,0625 statt immer, weil sie in Ozeanen dicht stehen.
  - Antike Städte auf der Karte 24 statt 18 Pixel: Der Wardenkopf ist breit und dunkel und wirkte neben den Block-Icons klein.
  - Die Markerliste in den Einstellungen behält 18 Pixel.

## D-034: Minimap nach ESC, Rechenlast der Seed-Atlas-Weltkarte

- **Minimap nach ESC:** Horizon meldet das Schließen eines Mod-Screens erst im Frame danach, wenn er schon zu ist. Die Prüfung „unser Screen ist offen“ in `screen_closing` griff deshalb nie, `map_open` blieb gesetzt und das HUD ließ die Minimap weg.
  - Jetzt setzt `screen_closing` den Zustand zurück, sobald unser Screen nicht mehr offen ist.
  - Zusätzlich gleicht das HUD jeden Frame ab: Karte als offen gemerkt, aber kein Screen von uns offen, dann wird zurückgesetzt.
- **Ursache der Dauerlast:** Gemessen im Testspiel (M-Serie, 3024×1720): Mit offener Seed-Atlas-Karte bei 0,3x lag der Prozess über 90 s bei ~290 % CPU.
  - Profil: drei Seed-Atlas-Worker in der Gelände-Verfeinerung (Vanilla-Oberfläche).
  - Keine Kachel wurde doppelt gerechnet. Die Verfeinerung war einfach zu teuer: ~3 s pro Kachel und Kern, ~200 Kacheln je Bildschirm.
- **Gemeinsames Dichtegitter:** Die Oberfläche einer 16×16-Kachel kommt jetzt aus einem gemeinsamen Dichtegitter (`lod_grid` von Horizon, Bandmodus um die Oberfläche) statt aus 256 einzeln gerechneten Spalten (`surface_column`).
  - Gemessen 11,9× schneller.
  - Gegen die Einzelspalten: 99,0 % gleiche Höhe, 99,5 % höchstens einen Block daneben, 99,3 % gleiche Farbe.
  - Weggelassen werden Aquifer-Taschen; Meerwasser bleibt.
  - Test: `lattice_columns_match_single_columns`, braucht die Horizon-Ressourcen.
- **Halbe Auflösung:** Die Verfeinerung tastet die Oberfläche mit halber Kachelauflösung ab (`SURFACE_SPACING` = 2). Höhen und Böden werden bilinear verteilt, Farben vom nächsten Sample übernommen (`upsample_surface`).
  - Bei ein bis zwei Bildschirmpixeln pro Sample zeigt das dasselbe Relief, kostet aber ein Viertel.
  - Bewusste Abweichung von der Fabric-Version, die jede Probe einzeln rechnet.
- **Niedrige Priorität:** Seed-Atlas-Worker senken sich beim Start wie die Fern-Gelände-Threads des Spiels: macOS-QoS „utility“, unter Windows „below normal“.
  - Über die Bridge, Version 8 (`lower_current_thread`, `seedatlas::WORKER_START`).
- **Ergebnis im Spiel:** Nach starkem Herauszoomen rechnen die Worker ~18 s statt minutenlang. Danach liegt der Prozess wieder bei ~50 %, dem Wert beim Spielen.

## D-035: Detailstufen der Seed-Atlas-Karte nach Zoom, Cache nur im Arbeitsspeicher

Auf Wunsch, mit Vorrang für die Leistung: Weit draußen genügt ein ungefähres Bild, nah dran soll es stimmen.

- **Proben pro Pixel nach Zoom:** Bisher war es fest eine Probe auf 2 Bildschirmpixel, sodass jede Zoomstufe gleich viele Kacheln brauchte (~200).
  - Jetzt wächst der Abstand mit der Entfernung: `pixels_per_sample` = 2 × (Blöcke pro Pixel)^0,3, begrenzt auf 2 bis 8.
  - Werte: 2 nah dran, ~4,6 bei 16 Blöcken pro Pixel, 8 ab etwa 100.
  - Weit draußen braucht eine Ansicht dadurch ein Viertel bis ein Sechzehntel der Proben.
- **Detailliertes Gelände nur nah dran:** Die Vanilla-Oberfläche (D-034) wird nur gerechnet, solange ein Block mindestens einen Bildschirmpixel breit ist (`refine_allowed`). Bei 2-fachem GUI-Maßstab heißt das ab 0,5x.
  - Weiter draußen bleibt die schnelle Vorschau mit geschätzten Höhen.
  - Bewusste Abweichung von der Fabric-Version, die bis Schritt 4 verfeinert.
- **Grobe Kacheln aus feinen:** Fehlt eine Kachel, liegen aber die vier eine Stufe feineren im Speicher, wird sie aus ihnen zusammengesetzt (`SeedService::compose`), statt gerechnet zu werden.
  - Jedes Pixel ist das Mittel der 2×2 darunter.
  - Fehlende feinere Kacheln entstehen genauso, bis drei Stufen tief.
  - Höchstens 48 Kacheln pro Frame.
  - Waren alle vier verfeinert, gilt die neue Kachel als verfeinert.
  - Herauszoomen nach dem Hineinzoomen kostet so keine Rechenzeit.
- **Nur Arbeitsspeicher:** Kacheln werden zwei Sekunden nach der letzten Anfrage der Karte verworfen (`RELEASE_AFTER`), also wenn sie zu ist; ebenso die Seed-Atlas-Texturen auf der Grafikkarte. Nichts wird auf die Festplatte geschrieben.
  - Strukturmarker bleiben, sie sind klein.
- **Gemessen im Testspiel:**
  - Bei 0,3x rechnen die Worker ~4 s statt ~18 s; bei 0,018x ~3 s.
  - Hineinzoomen auf 0,7x mit vollem Detail: ~4 s.
  - Wieder herauszoomen: keine Rechenphase, die Karte ist sofort voll.

## D-036: Landmarken weit draußen, nie zusammengelegte Marker, Gesichter mit Tab, Icongrößen

- **Landmarken verschwinden weit draußen:**
  - Antike Städte unter 0,0625x, wie Monumente.
  - Waldanwesen und Strongholds unter 0,03125x.
  - Nether- und End-Landmarken sowie der Spawn bleiben immer. (Das hielt das Scan-Budget nicht ein; seit D-039 gelten für Festungen, Bastionen, End-Städte, End-Gateways und End-Schiffe Radiusgrenzen, siehe dort.)
- **Kein Zusammenlegen, nirgends:** Bisher prüfte ein Scan mit mehr Kandidaten als dem Limit nur jede k-te Zelle und dünnte die Treffer auf ein Raster aus (D-031). Nahe Marker fielen dabei weg, das wirkte wie Zusammenlegen.
  - Jetzt gilt „alles oder nichts“ (`Atlas::scan_complete`).
  - Der Scan prüft jede Zelle. Passt die Zahl der Zellen nicht ins Budget (2²⁰ für Arten pro Chunk oder Erzader-Zelle, 2¹⁵ für die übrigen) oder findet er mehr als 8192 Treffer, bleibt die Art bei diesem Zoom ganz weg.
  - Damit ist D-031 (gleichmäßiges Ausdünnen) ersetzt.
- **Gesichter mit Tab:** Solange die neue Taste `radar.faces` (Standard Tab, wie die Spielerliste) gehalten wird, zeigen Minimap und Weltkarte Gesichter statt Punkten. Abschaltbar mit „Faces While Tab Held“; „Mob Faces“ zeigt sie weiter dauerhaft.
  - Mobs: das Gesicht aus der Spieltextur. Neu sind Piglins, Wanderhändler, Schneegolem und Wolf; die Pfade von Lohe und Hexe waren falsch und sind korrigiert.
  - Spieler: ihr Skin, wenn das Spiel ihn aus seinen Texturen zeichnet, sonst der Standard-Skin ihrer UUID (Bridge Version 9). Heruntergeladene Skins liegen in Horizon in internen Slots, an die die HUD-Zeichnung nicht kommt.
  - Fehlt eine Textur, etwa in einem anderen Ressourcenpaket, bleibt es beim Punkt.
- **Icongrößen:**
  - Struktur-Icons wachsen beim Hineinzoomen höchstens auf das Doppelte ihrer Grundgröße statt bis auf ein Viertel des Bildes, und werden nie kleiner als die Grundgröße.
  - Entity-Punkte und -Gesichter haben auf Karte und Minimap eine feste Größe in GUI-Einheiten.

## D-037: Neue Einstellungen, Werkzeugleiste der Karte, alle Mob-Gesichter, Radar nach den Standard-Kategorien

- **Einstellungen neu gestaltet** (ersetzt die Xaero-artigen Rasterseiten und die Seed-Atlas-Screens nach der Fabric-Version aus D-032):
  - Ein Screen (`settings_screen.rs`): links elf Kategorien (Allgemein, Minimap, Information, Entity-Radar, Wegpunkte, Gelände, Overlays, Weltkarte, Seed Atlas, Strukturen, Leistung), rechts eine scrollbare Liste.
  - Jede Zeile hat Name, eine Zeile Beschreibung (gekürzt; ganz als Tooltip) und ihr Steuerelement am Ende: Schalter, Wertknopf (Rechtsklick zurück), Schieberegler oder Knopf für einen Unterscreen.
  - Die Suche oben in der Liste durchsucht alle Kategorien nach Namen und Beschreibung.
  - Die Strukturarten stehen mit Icon, Anzahl erledigter und Zurücksetzen-Knopf in „Strukturen“; Profile in „Allgemein“.
  - Die Werte kommen weiter aus dem Optionskatalog (`screens::options`). Ein Test stellt sicher, dass jede Option eine Zeile hat.
- **Werkzeugleiste der Weltkarte:** Eine Spalte am rechten Rand, senkrecht zentriert, mit Tooltips links daneben. Sie ersetzt die Knöpfe M/SA oben links, SA unten links und +/−/? rechts.
  - Knöpfe: Zoom +/−, Zentrieren (Kompass), Seed-Ebene ein/aus (Karte, grau wenn aus), Seed-Atlas-Einstellungen (Enderauge), Wegpunkte (Namensschild), Einstellungen (Buch), Steuerung (?).
  - Die Seed-Atlas-Knöpfe erscheinen nur bei eingeschaltetem Seed Atlas. Icons sind die Item-Texturen des Spiels.
- **Mob-Gesichter:** Ein Subagent hat alle 89 Lebewesen von 26.3 gegen das Modell-Export des Spiels geprüft.
  - Neu sind 58 Arten (u. a. Biene, Pferd, Katze, Fuchs, Eisengolem, Wächter, Warden), dazu die Schaufensterpuppe mit dem Spielermodell. Damit haben alle Lebewesen außer Magmawürfel und Enderdrache ein Gesicht.
  - Gesichter stehen jetzt in den nativen Texturkoordinaten mit der Breite der Vanilla-Textur (`Face::base`), sodass auch 128 und 192 Pixel breite Texturen und hochaufgelöste Pakete stimmen.
  - Ohne Gesicht bleiben der Magmawürfel (kein einzelnes Rechteck) und der Enderdrache (er erreicht das Radar nicht).
  - Babys zeigen das Gesicht der Erwachsenen.
- **Radar nach den Standard-Kategorien der Referenz** (O2 `default_radar_categories_client.json`):
  - Schon gleich: Farben (Lebewesen gelb 14, Spieler weiß 15, Items rot 12, Sonstige dunkellila 5), Reihenfolge (Sonstige 0, Items 1, friedlich 2, feindlich 3, Spieler 6), Punktgröße 2, Höhenlimit 20 bzw. 2050 für Spieler, Höhen-Ausblendung ab 0 Blöcken, höchstens 1000 Entities, Icons standardmäßig nur mit der Spielerliste-Taste.
  - Neu: Gezähmte friedliche Tiere zeichnen über wilden (Reihenfolge 4), gezähmte feindliche mit 5.
  - Neu: Rüstungsständer zählen wie in der Referenz nicht zu den Lebewesen, sondern zu den sonstigen Entities (dunkellila) statt ganz zu fehlen. Bridge Version 10.
  - Offen: Die Datei nennt Zahlencodes für die Namensanzeige (`names` 0). Ihre Bedeutung lässt sich aus erlaubten Quellen nicht bestimmen, Meridian bleibt bei „Spielernamen“.

## D-038: Strukturen mit der Seed-Ebene, Welt- und Dimensionsauswahl der Wegpunkte

- **Strukturen mit der Seed-Ebene:** Ist die Seed-Kartenebene aus, zeigt die Weltkarte auch keine Strukturmarker; „Structures“ allein schaltet sie weiterhin ab.
- **Welt- und Dimensionsauswahl:** Die Felder „World/Server“ und „Sub-World/Dimension“ in der Wegpunktliste und im Editor waren bisher nur Anzeigen. Jetzt öffnen sie eine Liste unter dem Feld.
  - **Welten:** Oben steht die aktuelle Welt („(auto)“), darunter jede Welt und jede Serverkarte, für die Wegpunkte gespeichert sind (`waypoint::book_places`).
  - **Dimensionen:** Die automatische, dann die Dimensionen der gewählten Welt sowie Overworld, Nether und End.
  - Die Liste zeigt die Wegpunkte der gewählten Stelle; neue und bearbeitete Wegpunkte werden dort gespeichert (bei einer anderen Welt sofort in deren Datei).
  - Teleportieren und Teilen gehen nur in der Welt, in der man ist.
  - Beim Schließen der Screens springt die Auswahl auf „auto“ zurück.

## D-039: Korrekturen aus dem Audit (`docs/AUDIT_REPORT.md`)

Das Audit vom 2026-11-07 (Commit `92c0fd5`) fand 42 Punkte (AUD-001 bis AUD-042). Was dabei bewusst entschieden wurde:

**Wegpunkte**

- **Todespunkte (AUD-001):** „Erreichte Todespunkte löschen“ wirkt nur im Spiel (nicht auf dem Todesbildschirm) und erst, nachdem der Spieler seit dem Tod einmal weiter als 4 Blöcke vom Punkt weg war. Vorher löschte der nächste Frame den gerade gesetzten Punkt.
- **Unbekannte Höhe (AUD-007):** Wegpunkte können eine unbekannte Höhe haben (`y_unknown`). Teleport-Befehle senden dann `~` für `{y}`; Teilen, Import und Editor zeigen `~`. „Hierher teleportieren“, B auf der Karte und Strukturwegpunkte nutzen das statt Y 64. Im Nether gilt eine Spalte, deren Oberkante das Bedrock-Dach ist (Y ≥ 127), als unbekannt. Ingame-Markierungen ohne Höhe stehen auf Augenhöhe.
- **Teleport (AUD-004, AUD-035):** Ein Wegpunkt einer anderen Dimension nutzt das Dimensionsformat der Weltkarte, einer mit Blickrichtung das Rotationsformat. In eine andere Welt bleibt der Teleport gesperrt.
- **Geteilte Wegpunkte (AUD-008):** Eine Chatzeile im Teilen-Format wird nicht mehr automatisch übernommen. Sie bekommt einen anklickbaren Link „[Add waypoint]“ (`/meridian waypoint accept <n>`); die letzten 8 bleiben abrufbar (`/meridian waypoint shared`). Namen höchstens 32 Zeichen, höchstens 16 temporäre Wegpunkte pro Dimension.
- **Temporäre Wegpunkte** sind wie in O2 global und werden nie gespeichert.
- **Sets (AUD-005):** Anlegen, Umbenennen und Löschen in der Liste und mit `/meridian waypoint set [list | new | use | rename | delete]`. Das letzte Set bleibt als leeres Standardset.
- **Liste (AUD-024):** Sortierung (neueste, Name, Entfernung, Initialen; `waypoints.sort`), Entfernungsspalte für die eigene Welt und Dimension, Mehrfachauswahl mit Umschalt- und Strg/Cmd-Klick.
- **Weltkarte (AUD-011, AUD-026):** Aktionen und Editor arbeiten in der gezeigten Dimension. Die Wegpunkte der Partnerdimension (Oberwelt ↔ Nether) erscheinen abgeblendet im Maßstab 1:8. Die Werkzeugleiste hat einen Dimensionsknopf.
- **Tod:** Statt „Set beim Tod wechseln“ (Meridian hat keine feste manuelle Setwahl) springen die Wegpunkt-Screens beim Tod auf die eigene Welt und Dimension zurück.
- **Tasten (AUD-034):** `quick_confirm` bestätigt Editor, Setnamen, Löschabfragen und „Reset completed“. Sechs Tasten ohne Funktion tragen „(not available)“ im Namen.

**Speichern und Laden**

- **Unlesbare Dateien werden nie überschrieben (AUD-003, AUD-015):** Ein gemeinsamer Lader (`persist::load_or_set_aside`) gilt für Regionen, `waypoints.json`, `meridian.toml`, `seeds.toml`, `maps.toml` und `completed.json`.
  - Fehlt die Datei: Standardwerte.
  - Ist sie kaputt: Sie wird zu `<name>.broken.<unix-zeit>` umbenannt, dann frisch begonnen.
  - Stammt sie von einem neueren Meridian oder bleibt sie nach drei Versuchen unlesbar: Sie bleibt unberührt, und der Speicher wird in dieser Sitzung nie geschrieben. Regionen werden dann trotzdem angezeigt.
- **Beenden (AUD-023):** Beenden und Trennen warten bis zu 5 s auf offene Speicher-Jobs (`flush_and_wait`).
- **Reihenfolge (AUD-037):** Laden wartet auf frühere Speicher-Jobs derselben Datei. Temporäre Dateien heißen nach Prozess und Zähler; der Ordner wird unter Unix mit `fsync` gesichert.
- **Zweites Spiel (WM-20):** Eine Sperrdatei `meridian.lock` mit Betriebssystem-Sperre; ein zweites Spiel speichert keine Regionen und zeigt einen Hinweis. Wegpunkte, Seeds und Einstellungen sind davon nicht erfasst.
- **Kartennamen** werden ohne Groß-/Kleinschreibung verglichen. Umbenennen nimmt Seeds, erledigte Strukturen und Spielversionen mit.

**Weltkarte und Engine**

- **Speicherbudget (AUD-009):** Neben `max_loaded_regions` gilt `world_map.max_region_mib` (Standard 512 MiB). Weit herausgezoomt werden alle Regionen außerhalb der 5 × 5 um die Kartenmitte und der 3 × 3 um den Spieler zu Übersichtsbildern. D-027 stimmt damit. GPU-Texturen werden beim Trennen und beim Kartenwechsel verworfen.
- **Höhlenebenen (AUD-012):** Chunk-Antworten tragen ihren Lesemodus; Antworten für eine andere Ebene werden verworfen. Die Bridge führt Anfragen nach Position und Modus (Version 11).
- **Revisionen (AUD-013)** kommen aus einem prozessweiten Zähler; der Texturschlüssel enthält den Kartennamen.
- **Spawn auf Servern (AUD-022):** Horizon meldet nicht, wann ein Server einen neuen Spawn angekündigt hat. Nach dem Laden einer Ebene gilt ein Spawn erst, wenn ein anderer gemeldet wird oder 5 s vergangen sind (`SPAWN_GRACE`). Bis dahin liest die Engine weiter, speichert aber nichts. Fällt die Wahl auf eine andere Karte, wird das Gelesene verworfen und neu gelesen.
- **Zeit statt Frames (AUD-038):** Das Anfrage-Timeout sind 10 s, `chunk_reads_per_tick` gilt pro 50 ms Wanduhrzeit (nicht Spielzeit, die unter `/tick freeze` steht). Standard jetzt 64 (rund 1280 Chunks/s); vorher waren es 16 pro Frame. `frame_budget_us` und das 1,5-ms-Budget der Bridge bleiben bewusst pro Frame.
- **Threads (AUD-039):** Kartenworker, Nächste-Suche und PNG-Export laufen mit niedriger Priorität. Die Ansicht einer anderen Dimension hat höchstens 2 Worker und wird auf einem eigenen Thread abgebaut.

**Minimap, Radar, Einstellungen**

- **Minimap-Bild (AUD-006):** Die Textur läuft ringförmig um und bewegt sich in Zellen zu 32 × 32 Texeln. Beim Gehen werden nur hereinrollende Zellen zusammengesetzt und als Teilrechtecke hochgeladen, bei neu gezeichneten Regionen nur deren Zellen. Herausgezoomt deckt ein Texel 2 bis 16 Blöcke (etwa ein physischer Pixel). Schleimchunks werden pro Chunk geprüft. Gemessen (Release, vergrößerte Minimap): ~1 µs pro Frame beim Gehen statt 3,4 ms plus 9 MB Upload je Blockschritt. Im Spiel noch nicht gemessen.
- **Radar (AUD-016):** Filtern, auf die Karte beschränken, nach Entfernung sortieren, kappen, dann nach Zeichenreihenfolge sortieren. Über dem Limit bleiben also die nächsten Entities. Minimap und Weltkarte teilen den Filter (Höhenlimit, Ausblendung, Limit, Reihenfolge, Punktgröße).
- **Glatte Entity-Punkte (AUD-010):** Die Bridge liefert neben der interpolierten auch die Tick-Position; „Smooth Entity Dots“ aus zeigt die Tick-Position.
- **Biom-Übergänge (AUD-010):** „Biome Blending“ mittelt Gras-, Laub- und Wasserfarben über ein 5 × 5-Fenster, aber nur innerhalb eines Chunks, weil ein Chunk-Abzug seine Nachbarn nicht kennt. Grenzen auf einer Chunkkante bleiben hart. Umschalten liest geladene Chunks neu.
- **Höhlentiefe** wirkt: Boden tiefer als Ebene − Tiefe bleibt unerkundet.
- **Entfernte Felder:** `minimap.frame`, `share_world_map_tiles`, `minimap.terrain` und `world_map.cave` taten nichts und sind entfernt; alte Dateien laden weiter. „Lighting“ unter Gelände gilt für beide Karten; die doppelte Zeile „Map Lighting“ entfällt.
- **Lesbare Höhlenkarten (AUD-027):** Standard jetzt aus, wie O2. Bereits gespeicherte Dateien enthalten den Wert und behalten ihn.
- **Einstellungen (AUD-021, AUD-025):** Schieberegler wirken sofort und speichern beim Loslassen. Neues Textfeld (Enter oder Klick daneben übernimmt, Escape verwirft) für die Teleport-Formate (müssen mit `/` beginnen), maximale Entities, km-Schwelle und Höhlen-Dimensionen. Dazu Schieberegler und Wertknöpfe für Deckkräfte, Skalen, Lichtminimum, Höhlentiefe und die Leistungsbudgets.

**Seed Atlas**

- **Erzader-Marker (AUD-002)** werden nach Marker-ID zusammengeführt, nicht nach Trefferart, und nur, wenn ein Scan fertig wird oder sich die sichtbaren Arten ändern.
- **Seed-Ebene über erkundetem Gelände (AUD-010, AUD-017):** Ein zweiter Durchgang zeichnet die Seed-Kacheln über das Gelände: für „Over Explored Terrain“ mit der Deckkraft der Ebene, für die Biom-Hervorhebung deckend wie in der Fabric-Version.
- **Prefetch (AUD-010):** ein Ring von 0 bis 3 Kacheln um den Spieler in der feinsten Stufe, nur bei offener Karte in der eigenen Dimension, hinter allem, was die Ansicht braucht.
- **Cache (AUD-030, ändert D-035):** Kacheln und ihre Texturen bleiben 45 s nach der letzten Anfrage, höchstens 64 MB (die am längsten unbenutzten gehen zuerst). Die Caches des Gelände-Samplers werden mit den Kacheln freigegeben.
- **Detailgrenze (AUD-019, ändert D-035):** „≥ 1 Pixel pro Block“ gilt nur für die Oberwelt-Oberfläche; Nether-Boden und End-Höhen werden bei jedem Zoom verfeinert.
- **Scan-Bereich (AUD-018):** in Blöcken aus der Kartentransformation statt `Fensterpixel / Zoom`.
- **Sichtweite der Marker (AUD-020, ändert D-036):** `visible_at_zoom` ist die einzige Quelle. Die Zellbudgets (2²² für günstige Arten, sonst 2¹⁷) schneiden keine Art mehr vor ihrer Grenze ab; die Grenzen sind so gewählt, dass ein typischer Seed unter 8192 Treffern bleibt (gemessen mit Seed 42):

  | Art | Sichtbar |
  |---|---|
  | Schleimchunks | ab 0,5x, r ≤ 2048 |
  | Geoden | ab 0,25x, r ≤ 3072 |
  | Erzadern | ab 0,25x, r ≤ 6144 |
  | Brunnen, Schatz | ab 0,25x, r ≤ 8192 |
  | Minen | ab 0,25x, r ≤ 10240 |
  | Trial Chambers | ab 0,125x, r ≤ 20480 |
  | Dörfer, Ruinen und die anderen mittleren Arten | ab 0,125x, r ≤ 24576 |
  | Monumente, antike Städte | ab 0,0625x |
  | Strongholds, Anwesen | ab 0,03125x |
  | Festungen, Bastionen | r ≤ 24576 |
  | End-Gateways | r ≤ 14336 |
  | End-Städte | r ≤ 40960 |
  | End-Schiffe | ab 0,03125x, r ≤ 40960 |
  | Spawn | immer |

- **Kleinster Zoom:** mit Seed-Ebene 1/512 wie in der Fabric-Version.
- **Y-Werte:** Oberwelt-, Nether- und End-Y sind Textfelder (−2048 bis 2048) wie in der Fabric-Version, statt 9 Voreinstellungen.
- **Spielversion pro Welt** liegt jetzt in `seeds.toml` (`[versions]`) statt im Einstellungsprofil; alte Einträge werden beim Start übernommen.
- **Fabric-Details (AUD-036):** Spawn als Schätzung mit „(approx.)“; einzeiliger Marker-Tooltip mit Lagerbiom und `Y=?`; Biomnamen der gewählten Version; Ozeanruinen-Icon mit Rahmen; die Wahl eines Bioms schaltet die Hervorhebung ein; Ergebnisse der Nächste-Suche verfallen, wenn der Screen verlassen oder die Welt gewechselt wurde; „Copy coordinates“ mit Y; „Create waypoint“ öffnet den Editor.
- **Worker** starten erst mit dem ersten Auftrag, laufen also nicht bei abgeschaltetem Seed Atlas. Die Nether-Höhlenmasken werden außerhalb der globalen Sperre geschnitzt.
- **Fixtures (AUD-029):** zusätzlich Schrittweiten 32/128/512, je Dimension eine ganze 128 × 128-Kachel und Höhenschritte 8 und 32 (End 8). Alle bisherigen Zeilen sind unverändert.

**Bisher nicht festgehalten (AUD-040), jetzt dokumentiert**

- Im Einzelspieler kommt „Large Biomes“ aus dem Welt-Preset statt aus der Option; die Option gilt für Seeds, die von Hand eingegeben werden.
- Ein von Hand eingegebener Seed hat im Einzelspieler Vorrang vor dem erkannten (die Fabric-Version setzt bei jedem Laden den erkannten wieder ein). So lässt sich ein anderer Seed über eine Welt legen.
- Automatische Seed-Atlas-Worker: `clamp(Kerne − 2, 1, 4)` in einem Pool, mit niedriger Priorität (Fabric: `clamp(Kerne − 2, 2, 8)` plus ein Strukturpool), wegen der Last aus D-034.

**Nicht umgesetzt**

- `/seedatlas` ohne Tab-Vervollständigung: Horizons `tab_complete` gilt nur serverseitig für LAN-Gäste.
- Antworten von `/seedatlas` bleiben in einer Zeile der Aktionsleiste, weil Horizon keine lokale Chatzeile ausgeben kann.
- Die maximale Wegpunkt-Entfernung wird nicht nach Dimension skaliert (O2 `waypoint_max_distance_dimension_scale`): Was die Option genau tut, lässt sich aus erlaubten Quellen nicht sicher ablesen, und beim Standard 0 wirkt sie nicht.
- Offene O3-Beobachtungen (D-012, KEYBINDS „Offene Beobachtungen“) bleiben offen; sie brauchen die Referenz im Spiel.
