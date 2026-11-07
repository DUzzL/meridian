# Fortschritt

## Phase 0: Recherche Horizon (abgeschlossen)

**Fertig**

- `docs/HORIZON_API.md`:
  - wie native Mods geladen werden (Trait-Mods und Mixins, Build durch den Launcher),
  - welche Client-Hooks existieren,
  - welche Lücken bestehen, mit Erweiterungsvorschlägen E1–E6 für spätere PRs an Horizon.
- Horizon ist **nicht** rein serverseitig. Phase 0 endet deshalb nicht mit einem Stopp wegen fehlendem Client.
- `docs/DECISIONS.md`: Architektur (D-003), Umgebung (D-001, D-002), Übergangs-Mixins plus Upstream-PRs (D-005), GPLv3 (D-006), Name Meridian (D-007).

**Offen**

- Noch kein Code, daher weder Build noch Tests. Beides beginnt in Phase 2.

**Bekannte Probleme**

- `gh` ist nicht angemeldet. Das Repo existiert bisher nur lokal; ein GitHub-Remote und CI-Läufe brauchen `gh auth login`.
- `horizon-client` referenziert in der Doku `crate::hud::MOD_IMAGE_SIZE`, die Konstante ist in `main` @ `41f822b` aber nicht auffindbar. Die genaue Größe des `mod_image`-Slots bleibt zu klären (im Spiel messen).

## Phase 1: Feature- und Hotkey-Inventar (abgeschlossen bis auf Beobachtungen im Spiel)

**Fertig**

- `docs/KEYBINDS.md`: 30 Standard-Hotkeys (23 Minimap, 7 Weltkarte), verifiziert über `options.txt` der Prism-Instanz mit Minimap 26.6.2 und Weltkarte 1.47.2. Standardtasten: B, U, Z, Y, Num +, M, ], rechte Umschalttaste; alle übrigen ungebunden.
- `docs/FEATURES.md`: rund 120 Features in 12 Bereichen, jeweils mit Priorität (P0–P3), Akzeptanzkriterium, Quelle und benötigter Horizon-Erweiterung (E1–E6).
- `docs/CLEAN_ROOM.md`: Quellen S1–S4 (öffentliche Modseiten), O1 (`options.txt`), O2 (vom Spiel erzeugte Konfigdateien), Protokoll.

**Offen**

- O3 Beobachtung im Spiel, 5 Punkte (Liste am Ende von `KEYBINDS.md`): Kategorien der Steuerungsliste, Steuerungs-Tooltip der Karte, Rechtsklickmenü, Tasten in Wegpunktliste und Editor, Z gehalten oder umgeschaltet. Nicht automatisierbar, weil Minecraft als `java`-Prozess ohne App-Bundle läuft. Muss bis Phase 4 nachgeholt werden.
- Weiterhin kein Code, also weder Build noch Tests. Beides beginnt in Phase 2.

**Bekannte Probleme**

- Die öffentlichen Seiten nennen I/O als Zoomtasten; die installierten Versionen haben keine Belegung. Maßgeblich ist `options.txt`.

## Phase 2: Grundgerüst (abgeschlossen)

**Fertig**

- Workspace, GPLv3-Lizenz, CI (`.github/workflows/ci.yml`):
  - fmt, clippy `-D warnings` und Tests mit Rust 1.95 auf Linux, Windows und macOS,
  - Lockfile-Prüfung gegen Horizon,
  - Adapter-Check gegen die gepinnte Nightly (nur `main` und manuell).
- `meridian-host`: die abgegrenzte Schnittstelle `MapHost` + `HostEvent` (D-003), inklusive der fehlenden Fähigkeiten E1/E2.
- `meridian-core`:
  - `config`: Einstellungen aller Bereiche mit Standardwerten aus O2, Profile, TOML-Datei; kaputte Datei wird beiseitegelegt.
  - `keybinds`: 30 Belegungen, Abgleich roher Tasten mit der Wahl des Spielers.
  - `persist`: atomares Schreiben, escapte Pfade pro Welt/Server/Karte/Dimension.
  - `geom`, `tile`, `region`: Datenmodell Chunk → Tile → Region; inkrementelles Neuzeichnen nur der geänderten Chunks und ihrer Süd-/Ost-Nachbarn, auch über Regionsgrenzen.
  - `color`: Vanilla- und Texturfarben, Biom-Tints, drei Hangmodi, Höhentiefe, Wasser nach Tiefe, Tag/Nacht- und Blocklicht.
  - `codec`: Regionsdateien, versioniert, gegen kaputte Dateien abgesichert.
  - `worker`: Thread-Pool; Panics in Jobs stoppen ihn nicht; beim Beenden werden offene Speicher-Jobs abgearbeitet (seit D-039 wartet Meridian beim Beenden und Trennen bis zu 5 s darauf, statt sich darauf zu verlassen, dass Horizon die Hooks freigibt).
  - `engine`:
    - Chunks lesen: nächste zuerst, begrenzt pro Tick;
    - Tiles werden auf Workern gebaut;
    - Regionen asynchron laden und speichern;
    - Zeichnen mit Zeitbudget pro Frame;
    - LRU-Verdrängung, verdrängte Regionen werden vorher gespeichert;
    - veraltete Ergebnisse nach Dimensionswechsel werden verworfen.
- `meridian-horizon` (Adapter, Trait-Mod):
  - registriert die Mod mit allen Tasten unter Optionen > Mods,
  - leitet Tasten, Welt laden/entladen, Trennen, Ressourcen-Reload und Beenden an die Engine weiter,
  - tickt die Engine pro Frame im Budget,
  - Welt-/Server-Erkennung über `client_state().server_address` bzw. `world_dir()`.
- `tools/check-adapter.sh`, `tools/check_lock.py`.

**Build und Tests**

- `cargo test --workspace --locked`: 55 Tests grün (53 Kern, 2 Host).
- `cargo clippy --workspace --all-targets -- -D warnings`: sauber.
- `tools/check-adapter.sh`: Der Adapter kompiliert gegen Horizon `66f3981` (installierte Nightly) mit dessen Lockfile und Patches; keine fremden Crates.

**Offen**

- E1/E2 als Übergangs-Mixins (Phase 3): Bis dahin schreibt die Mod im Spiel noch keine Kartendaten (D-013).
- Noch nicht im Spiel gebaut (`horizon-launcher native-build`) und gestartet. Das kommt mit Phase 3, sobald es etwas zu sehen gibt.
- CI ist nicht auf GitHub gelaufen: Es gibt noch kein Remote (`gh` nicht angemeldet). Die Pakete für den Adapter-Job unter Ubuntu sind nicht verifiziert.
- Mit Rust 1.95 (MSRV) wurde nur in CI geplant, lokal ist 1.97.1 installiert.
- Die Minimap braucht eventuell einen eigenen Terrain-Stil neben dem der Weltkarte; die Engine zeichnet derzeit nach dem Weltkarten-Stil (Phase 3).

**Bekannte Probleme / Risiken**

- Speicher: eine voll erkundete Region braucht etwa 3,6 MB Tiles + 1 MB Pixel. Bei 300 geladenen Regionen sind das im Extremfall rund 1,4 GB. Eine Messung und gegebenenfalls ein Speicherbudget zusätzlich zum Regionslimit folgen in Phase 6.
- Ob der Launcher Pfad-Abhängigkeiten zwischen Crates im Bundle (`engine` → `../core`) akzeptiert, ist dokumentiert, aber noch nicht praktisch getestet. Rückfalloption: `core`/`host` per `#[path]` als Module in `engine` einbinden.

## Phase 3: Minimap + Wegpunkte (in Arbeit)

**Fertig und im Spiel geprüft** (Horizon `66f3981`, mit dem Launcher gebaut, Testwelt per `horizon-ctl`, Screenshots)

- Übergangs-Bridge E1/E2 (D-015): Chunk-Ereignisse und Chunk-Lesen am Client funktionieren. Regionsdateien werden geschrieben.
- Launcher-Build: `horizon-launcher native-build mixins/meridian --instance horizon --trust-native-code`. Der erste Build dauert 5 min, inkrementelle Builds etwa 5 s. Die Pfad-Crates im `[api]`-Ordner werden akzeptiert (D-014).
- Minimap im HUD:
  - Gelände aus der Engine,
  - dreht mit dem Spieler (die Himmelsrichtungen stimmen),
  - quadratisch oder rund, Zoomstufen, Vergrößern mit Z (halten oder umschalten),
  - Spielerpfeil, Rahmen,
  - Koordinaten und Biomname darunter,
  - versteckt bei F1 und unter Screens.
- Singleplayer- und Server-Erkennung: Singleplayer läuft in Horizon über die Adresse „local“. Der Weltordner kommt deshalb vom integrierten Server (`world_lifecycle`).
- Wegpunkte:
  - Modell, Sets, global/lokal, deaktiviert, temporär,
  - Todespunkte (alte behalten oder löschen, erreichte löschen),
  - Speicherung pro Welt in `waypoints.json`.
- Tasten:
  - Num + und B: Wegpunkt an der Position anlegen,
  - nächstes Set, alle Sets anzeigen,
  - Wegpunkte in der Welt bzw. auf der Minimap ein/aus.
- `/meridian waypoint add|remove|tp|share|list [Name]`; Teleport über das einstellbare Befehlsformat.
- Teilen: geteilte Wegpunkte aus dem Chat werden als temporäre Wegpunkte übernommen.
- Anzeige in der Welt: Name und Distanz durch Wände, Beam (ab 8 Blöcken Abstand). Auf der Minimap: farbiges Kästchen mit Kürzel, am Rand festgehalten.

**Build und Tests**

- `cargo test --workspace --locked`: 75 Tests grün (73 Kern, 2 Host); Clippy sauber.
- `tools/check-adapter.sh`: Mixins angewendet und Adapter kompiliert.

**Offen in Phase 3**

- Wegpunkt-Editor (B) und Wegpunktliste (U) als eigene Screens (`mod_screen_open` + `screen_render_post`).
- Einstellungs-Screen (Y): Bis dahin gelten die Werte aus `config/meridian.toml`.
- Höhlenmodus (CV-01..04): Bridge und Engine können `CaveFrom` schon lesen, Erkennung und Umschaltung fehlen.
- Entity-Radar (RAD-*): braucht E4 (Entity-Typen, Singleplayer). Entweder kommt dafür ein weiteres Bridge-Stück oder ein Upstream-Hook.
- Restliche Infoanzeigen (Licht, Zeit, Winkel, Dimension, Wetter), Chunkgitter, Schleimchunks, Licht-Overlay.
- Todespunkte im Multiplayer: nur über den Todesbildschirm erkennbar.
- Die Minimap kann F3 nicht erkennen (`corner_shown` ist kein Host-Helfer) und überlappt Horizons Toasts oben rechts.

**Bekannte Probleme**

- Die Farbstile „genau“ und „Vanilla“ unterscheiden sich kaum (D-017).
- Die Minimap besteht aus Flächen statt einer Textur (D-016); bei sehr großer Karte gibt es gröbere Zellen.

### Phase 3, Zwischenstand 2: Optik wie die Referenz

**Fertig und im Spiel verglichen** (gleiche Welt, Position und Blickrichtung, Fenster 1512×891)

- Minimap-Layout, Rahmen, Himmelsrichtungen, Spielerkreuz, Koordinatenzeile, Maßstab und Wegpunkt-Darstellung in Welt und Minimap entsprechen den Messungen an der Referenz (D-019).
- Gelände auf der GPU mit echten Texturfarben (D-020).
- Neue Kernmodule bzw. Erweiterungen: `texture` (Texturmittelwerte aus PNG), Projektion Welt → Bildschirm, Distanztext, Minimap-Einheiten.

**Build und Tests**

- `cargo test --workspace --locked`: 80 Tests grün (78 Kern, 2 Host); Clippy sauber.
- Adapter-Check grün; Launcher-Build `native-66f3981-32cd9cb72d9f`.

**Offen**

- Wegpunkt-Editor (B) und -Liste (U) als Screens im Layout der Referenz (Aufnahmen liegen vor).
- Einstellungs-Screen (Y) im Layout der Referenz.
- Entity-Radar (Rauten-Punkte: gelb friedlich, rot feindlich, abgedunkelt auf anderer Höhe).
- Höhlenmodus, weitere Infoanzeigen.

**Bekannte Probleme**

- Bei Sonnenauf- und -untergang ist die Karte dunkler als bei der Referenz (Nachbearbeitung, D-020).
- Die Farbe neuer Wegpunkte weicht ab: Meridian beginnt mit Rot, die Referenz wählt zufällig.
- Die Prism-Referenzinstanz wurde für die Aufnahmen verändert: Welt `MeridianTest` hinzugefügt und `pauseOnLostFocus:false` gesetzt.

### Phase 3, Zwischenstand 3: Screens

**Fertig und im Spiel mit der Referenz verglichen**

- Wegpunkt-Editor (B):
  - Welt/Dimension, Reset, Set, Farbe (Links-/Rechtsklick), Name, X/Y/Z/Yaw mit Platzhaltern,
  - Lokal/Global, Kürzel, Aktiviert/Deaktiviert,
  - Bestätigen (erst mit Namen), Abbrechen,
  - Tab wechselt das Feld, Enter bestätigt.
- Wegpunktliste (U):
  - Welt/Dimension, Optionen, Set wechseln, Set leeren (mit Rückfrage), Filter,
  - Einträge mit farbigem Kürzel-Quadrat, Auswahl,
  - Hinzufügen/Bearbeiten, Teleport (auch Taste T), Teilen, Aktivieren/Deaktivieren, Löschen (auch Entf), Fertig, Scrollen.
- Einstellungen (Y):
  - Hauptseite wie die Referenz,
  - Unterseiten Ansicht, Blockkarte, Entity-Radar, Overlay, Info, Wegpunkte, Sonstiges,
  - Suche; Änderungen wirken sofort und werden gespeichert.

**Build und Tests**

- `cargo test --workspace --locked`: 93 Tests grün (91 Kern, 2 Host); Clippy sauber; Adapter-Check grün; Launcher-Build `native-66f3981-e01e5b5aa87b`.

**Offen in Phase 3**

- Entity-Radar (braucht E4 bzw. ein Bridge-Stück für Entity-Typen und den Singleplayer).
- Höhlenmodus, weitere Infoanzeigen (Licht, Zeit, Winkel, Dimension, Wetter), Chunkgitter, Schleimchunks, Licht-Overlay.
- Profile (Namen, Löschen), Positionseditor, Seite 2 der Einstellungen.
- Welt- und Dimensionsauswahl in Editor und Liste zeigen nur „(auto)“: Unterwelten folgen mit CTX-02.

### Phase 3, Zwischenstand 4: Entity-Radar

**Fertig und im Spiel verglichen**

- Radar auf der Minimap wie die Referenz (D-022): Rauten nach Klasse, Abdunklung nach Höhe, Höhenlimit, Reihenfolge, Clipping am Kartenrand.
- Einstellungen „Entity Radar Settings“: an/aus, Unsichtbare, Spieler, Feindliche, Friedliche, Items, Sonstige, Abdunklung, Umkehren, Höhenlimit, Punktgröße.
- Tasten zum Ein-/Ausschalten und Umkehren (standardmäßig ungebunden, wie in der Referenz).

**Build und Tests**: 97 Tests grün (95 Kern, 2 Host); Clippy sauber; Launcher-Build `native-66f3981-fe154f329614`.

**Offen**

- Boote, Loren und Projektile als „Sonstige“.
- Symbole und Köpfe statt Rauten, Namen.
- Die orangen Pfeile am Kartenrand der Referenz (vermutlich verfolgte Entities außerhalb der Karte) sind noch nicht geklärt.

### Phase 3, Abschluss

**Fertig und im Spiel geprüft**

- Höhlenmodus: automatisch unter festem Dach (1×1/3×3/5×5, Prüfung in der Bridge), manuell per Taste, Umschaltverzögerung; Höhlen-Layer in 16er-Schritten mit eigenen Dateien (`caves/<y>/`); lesbare Höhlenschattierung (Blocklicht hellt auf).
- Infozeilen: Koordinaten, Overworld-Koordinaten (Nether), Chunk, Winkel, Dimension, Biom, Wetter, Licht, Spielzeit, Echtzeit (12/24 h), Hinweis auf manuellen Höhlenmodus. Standard wie in der Referenz: nur Koordinaten.
- Chunkgitter (im Shader, dreht mit, 1 physischer Pixel breit), Licht-Overlay (Blocklicht im einstellbaren Bereich, hellviolett).
- Todespunkte im Singleplayer und Multiplayer über den Todesbildschirm (`dead`).

**Build und Tests**: 106 Tests grün (104 Kern, 2 Host); Clippy sauber; Adapter-Check grün.

**Offen aus Phase 3 (in Phase 6 nachziehen)**

- Schleimchunks (braucht den Seed: kommt mit Seed Atlas, Phase 5).
- Radar: Boote, Loren, Projektile; Symbole/Köpfe statt Rauten.
- Profile (Namen, Löschen), Positionseditor.

## Phase 4: Weltkarte (abgeschlossen)

**Fertig und im Spiel geprüft** (Vergleich mit Referenzaufnahme r07, D-023)

- Vollbildkarte mit M:
  - Gelände als GPU-Regionstexturen mit drei Detailstufen und Cache;
  - HUD und Hand ausgeblendet.
- Bedienung:
  - Zoom mit dem Mausrad um den Cursor und mit den Knöpfen + und −;
  - Verschieben per Ziehen und mit den Pfeiltasten/WASD;
  - Leertaste zentriert, M schließt.
- Anzeigen:
  - Koordinaten und Biom unter dem Cursor (Biome neu im Regionsformat v2, D-024);
  - Zoomanzeige, Kompass, roter Spielerpfeil;
  - Wegpunkte als Schilde, Entities, Fußspuren.
- Rechtsklickmenü:
  - Wegpunkt erstellen (Editor, danach zurück zur Karte), Teleport hierher, Position teilen, PNG-Export;
  - auf Wegpunkten zusätzlich bearbeiten, hinteleportieren, löschen.
  - B legt am Cursor einen Wegpunkt an.
- Dimensionen:
  - Die Taste „Switch map dimension“ wechselt zwischen Oberwelt, Nether und End. Andere Dimensionen werden nur aus ihren Dateien gelesen.
  - Koordinaten werden 1:8 umgerechnet, Teleports nutzen das Dimensionsformat.
  - Dimensionen mit Dach zeigen ihre zuletzt kartierte Höhlenebene.
- Fehler behoben: Beim Dimensionswechsel lädt Horizon das neue Level vor dem Entladen des alten. Die Karte blieb dadurch geschlossen; jetzt zählt Meridian die geladenen Level.
- PNG-Export:
  - zeilenweise gestreamt, Unerkundetes transparent;
  - über 16384 px Kantenlänge in Zweierpotenzen verkleinert;
  - Ablage in `.horizon-mod-data/meridian/exports/`.

**Build und Tests**: 115 Tests grün (113 Kern, 2 Host), Clippy sauber, Adapter-Check grün.

**Offen (Phase 6 oder später)**

- Öffnungsanimation (WM-01), Steuerungs-Tooltip (WM-19), Spieler-Teleport im Menü.
- Karte aus dem Weltspeicher erzeugen (WM-05/06).
- Oberwelt-Wegpunkte skaliert im Nether.
- Server-Chunk-Radius und Kartendistanzen (WM-13/17).
- Exportoptionen (Nacht, Teilung).
- Die Symbolleisten links und rechts der Referenzkarte (eigene Symbole wären nötig; keine Assets der Referenz).

## Phase 5: Seed Atlas (abgeschlossen)

**Fertig**

- Engine als Rust-Port (`engine/seedatlas`, D-025), nur für 26.3:
  - Biome aller drei Dimensionen (normal und große Biome, alle Auflösungen der Fabric-Version);
  - 25 Strukturtypen mit Biom- und Geländeprüfung, Varianten und Lagern mit Spezialbeute;
  - End-Städte mit Inselhöhe und Schiffen, End-Gateways (fest und gestreut), End-Inseln;
  - Festungen, Spawn, Schleimchunks, Erzadern.
- Parität: Fixtures direkt aus der nativen Schnittstelle der Fabric-Version (`tools/seedatlas-fixtures/build.sh`). Alle Werte stimmen exakt, Geländehöhen auf eine relative Toleranz von 1e-6:
  - Biome: über 50.000 Proben;
  - Strukturen: jeder Treffer für drei Seeds.
- Einbindung in Meridian:
  - Seed im Einzelspieler automatisch aus der Welt (Horizons `save::Level`), sonst `/seedatlas seed <seed>`. Gespeichert wird pro Welt bzw. Server lokal in `seeds.toml`, der Seed verlässt den Client nie.
  - `/seedatlas seed|clear|status|settings`.
  - Weltkarte:
    - Biom-Ebene unter dem erkundeten Gelände, Deckkraft einstellbar, Kacheln je nach Zoom in 1×, 4× oder 16×, berechnet auf Worker-Threads;
    - Struktur-Icons (die eigenen Icons von Seed Atlas) mit Tooltip (Name, „(approx.)“, Spezialbeute, Koordinaten);
    - Biom-Tooltip am Cursor wie in der Fabric-Version;
    - Knöpfe „SA“ (Ebene an/aus, grün/grau) und „...“ (Einstellungen) am linken Rand.
  - Einstellungsgruppe „Seed Atlas Settings“ mit allen Optionen der Fabric-Version und den 28 Markertypen. Die Standardwerte folgen der Fabric-Version: Ebene an, Strukturen aus, Hilfsmarker aus.

**Build und Tests**:

- 120 Tests grün (116 Kern, 2 Host, 2 Paritätstests mit Fixtures);
- Clippy sauber, Adapter-Check grün;
- im Spiel geprüft (Welt MeridianTest, Seed automatisch erkannt).

**Offen**

- Erledigte Strukturen markieren und ausblenden (Fabric: Kontextmenü), Wegpunkt aus Struktur (SA-07).
- Biom-Ebene optional auf der Minimap, Prefetch über die Sicht hinaus.

## Phase 5, Nachtrag: Seed Atlas auf dem Stand von main (2.0.0)

- **Engine:** alle Versionen von 1.1 bis 26.3 portiert (D-026). Die Parität für alle 38 Versionen ist mit Fixtures aus der echten nativen Bibliothek belegt.
- **Gelände-Look:** wie in der Fabric-Version, mit Wasser-, Relief-, End- und Nether-Darstellung und Biom-Hervorhebung.
- **Detailgelände:** Kacheln der Oberwelt bis Schrittweite 4 nutzen die echte Basisoberfläche aus Horizons Weltgenerierung (`terrain_sampler.rs`), der Nether die Höhlenmaske der Carver.
- **Zustand und Befehle:**
  - eigene Seeds für Nether und End, Minecraft-Version pro Welt;
  - alle `/seedatlas`-Befehle mit dem Wortlaut der Fabric-Version;
  - erledigte Strukturen (ausblenden, Abzeichen, zurücksetzen), Markergröße.
- **Weltkarte:**
  - Knöpfe M und SA oben links, SA-Umschalter unten links;
  - im Rechtsklickmenü auf Markern „Mark as completed“, „Create waypoint“ und „Create temporary waypoint“.
- **Screens:** nächstes Biom bzw. nächste Struktur (Suche im Hintergrund, Karte springt hin, Wegpunkt nach Wahl), Biom-Hervorhebung, Weltgenerierung (Version, Welttyp), Zurücksetzen bestätigen.
- **Im Spiel geprüft:**
  - Gelände-Look in unerkundeten Gebieten;
  - Knöpfe und Einstellungsseite;
  - `/seedatlas status`;
  - Suche „Abandoned Camp“ (Karte springt hin, temporärer Wegpunkt).
- **Build und Tests:** 164 Kern-Tests, 2 Host-Tests, 8 Engine-Tests und der Paritätstest (alle Versionen) grün; Clippy sauber, Adapter-Check grün.

**Nachtrag Icons und Tempo**: Die Marker-Icons kommen wie in Fabric 2.0 aus den Minecraft-Texturen, ergänzt um die aktualisierten eigenen Icons von Seed Atlas. Die Kacheln haben jetzt 128 Proben: erst eine schnelle Vorschau, dann die Verfeinerung. Im Spiel ist die Karte nach rund 1,5 s gefüllt.

**Offen**

- Hintergrund-Prefetch über die Sicht hinaus.
- Ein Vergleich des Gelände-Looks Pixel für Pixel mit der Fabric-Version steht aus.

## Phase 6: Feinschliff (Zwischenstand 1)

**Abgleich mit `FEATURES.md` und `KEYBINDS.md`.** Jede Einstellung und jede Taste wurde im Code geprüft. Nachgezogen:

- Tasten:
  - `]` öffnet die neue Seite „World Map Settings“ (vorher ohne Wirkung).
  - Chunkgitter, Schleimchunks und Licht-Overlay lassen sich jetzt per Taste schalten.
- Weltkarte:
  - Unbegrenzter Zoom (hinein und heraus) und eine Öffnungsanimation (WM-01).
  - Spieler als Pfeil oder als Punkt.
  - Knopf „?“ mit der Steuerung (WM-19).
  - „Teleport To <Spieler>“ im Rechtsklickmenü.
  - Max. geladene Regionen einstellbar.
- Minimap:
  - Schleimchunks (OV-02, Seed aus Seed Atlas).
  - Ausblenden unter F3 (die Bridge meldet den F3-Zustand, Bridge-Version 5).
  - Blumen und Redstone lassen sich ausblenden (MM-12, gilt für neu erfasste Chunks).
- Einstellungen:
  - Profile: wechseln, anlegen, umbenennen, löschen (SET-02, D-028).
  - Positionseditor (MM-04).
- Radar: Boote und Loren als „Sonstige“.

**Performance** (`core/tests/perf.rs`, synthetische Karte aus 20 × 20 voll erkundeten Regionen, 10.240 × 10.240 Blöcke, M-Serie, Release):

| Messung | Ergebnis |
|---|---|
| Region zeichnen (1024 Chunks) | 3–6 ms |
| Region kodieren / dekodieren | 68 ms / 5 ms (im Hintergrund) |
| Datei einer voll erkundeten Region | 2,1 MiB (Zufallsdaten, echtes Gelände ist kleiner) |
| 300 Regionen laden und zeichnen | 2,0 s (6,7 ms je Region, auf Worker-Threads) |
| PNG-Export der ganzen Karte | 2,8 s (5120 × 5120 px) |
| Übersichtsbild einer Region | 0,07 ms, 64 KiB |

- Gefunden: Herausgezoomt hielt die Karte bis zu 225 Regionen ganz im Speicher, über 1 GiB.
- Lösung: Übersichtsbilder (D-027). Herausgezoomt sind jetzt höchstens 25 Regionen ganz geladen (~125 MiB), dazu bis zu 256 MiB Übersichten.

**Build und Tests**:

- 173 Kern-Tests, 2 Host-Tests, 8 Engine-Tests und der Paritätstest grün;
- Clippy sauber, Adapter-Check grün;
- Launcher-Build `native-66f3981-8a035d6c07f1`.

**Im Spiel geprüft** (über `horizon-ctl`, Testwelt MeridianTest):

- `]` öffnet „World Map Settings“.
- Öffnungsanimation: Der erste Frame zeigt 2.7x, danach 3.0x.
- Hilfe „?“ und unbegrenzter Zoom heraus bis 0.016x: Die Seed-Atlas-Ebene ist nach rund 10 s gefüllt.
- F3 blendet die Minimap aus.
- Profile: anlegen, umbenennen, löschen; die Datei folgt jeweils.
- Positionseditor: Ziehen rastet unten rechts ein, „Reset“ setzt zurück.
- Schleimchunks erscheinen grün auf der Minimap.

Dabei behoben:

- Zoomanzeige „0.0x“ bei starkem Herauszoomen.
- Der Biom-Tooltip überdeckte das Hilfefenster.
- Die Minimap im Positionseditor war unscharf, weil der Menühintergrund sie weichzeichnete.

Nicht eigens geprüft: Spieler als Punkt, Teleport zu Spielern (braucht einen zweiten Spieler), Boote und Loren im Radar, Blumen und Redstone ausblenden.

## Phase 6: Feinschliff (Zwischenstand 2)

- **Höhlenmodus:** Die Einstellungen stehen jetzt in „Miscellaneous Settings“:
  - an/aus, automatisch (aus, 1×1, 3×3, 5×5 Dach), Umschaltverzögerung;
  - „Legible Cave Maps“: lesbar wie bisher, oder dunkel, sodass nur Fackeln aufhellen;
  - „Cave Mode Extra Zoom“: Die Minimap zoomt in Höhlen hinein.
- **Wegpunkte exportieren und importieren** (IO-01, IO-02, `waypoint_io.rs`):
  - `/meridian waypoint export` schreibt alle Dimensionen und Sets als JSON nach `exports/`.
  - `/meridian waypoint import <Datei oder Ordner>` liest:
    - ein solches Export-JSON;
    - eine `.txt`-Datei mit `waypoint:`-Zeilen;
    - einen Weltordner mit `dim%…`-Unterordnern im Textformat anderer Karten-Mods.
  - Das Format ist aus dem Kopf der Dateien abgeleitet, die die Prism-Instanz geschrieben hat.
  - Doppelte Wegpunkte (gleicher Name und Ort) werden übersprungen.
- **Mehrspieler-Karten** (CTX-02, `subworld.rs`, D-029):
  - Eine Serveradresse kann mehrere Karten haben; Standard ist eine Karte je Weltspawn, den der Server meldet.
  - Jede Karte hat eigene Wegpunkte und eigene Seed-Atlas-Seeds.
  - Alternativen: eine Karte je Server, oder manuell mit `/meridian map list | use <Name> | new <Name>`.
- **Radar:**
  - „Mob Faces“ zeigt Mobs als ihr Gesicht, ausgeschnitten aus den Texturen des Spiels.
  - „Entity Names“ (aus, Spieler, alle) schreibt Namen unter die Marke.

**Im Spiel geprüft:**

- Gesichter von Creeper, Skelett, Enderman, Zombie und Spinne sind erkennbar; der Name „Bob“ steht unter seiner Marke.
- Einzelspieler bleibt auf der Standardkarte.

- Export: `/meridian waypoint export` schreibt `exports/waypoints-MeridianTest.json`.
- Import: Der Re-Import dieser Datei und der Import des Prism-Weltordners melden beide „Imported 0 waypoints“. Der einzige Wegpunkt dort („Home“) ist schon vorhanden, Doppelte werden also übersprungen.
- `/meridian map` meldet im Einzelspieler „Worlds in singleplayer have one map“.

Nicht im Spiel geprüft: Mehrspieler-Karten (dafür braucht es einen Server; Unit-Tests decken die Auswahl ab), die Höhlenoptionen.

**Build und Tests**: 182 Kern-Tests und der Paritätstest grün; Clippy sauber, Adapter-Check grün; Launcher-Build `native-66f3981-4b682b06b3b7`.

## Phase 6: Feinschliff (Zwischenstand 3)

- **Blockkarte:** zwei neue Schalter, beide wirken auf neu erfasste Chunks:
  - „Display Stained Glass“: Ist er aus, sieht die Karte durch gefärbtes Glas hindurch.
  - „Adjust Height For Short Blocks“: Teppiche, Schneeschichten, Druckplatten, Schienen und Seerosen zählen einen Block tiefer und werfen keine Stufe.
- **Mehrspieler-Karten:**
  - Ein Kartenwechsel wird in der Aktionsleiste gemeldet.
  - `/meridian map rename <alt> <neu>` benennt eine Karte samt Ordner um. Die Standardkarte und die gerade gezeigte Karte sind davon ausgenommen. Seeds unter dem alten Namen müssen danach neu gesetzt werden.
- **Fair Play** (SRV-02): ein Schalter in „Miscellaneous Settings“, der Radar und Höhlenkarten abschaltet.
- **Radar:** `/meridian radar hide|show <Entity-ID>` und `/meridian radar hidden` blenden einzelne Entity-Arten aus (RAD-07, vereinfacht).
- **Fehler behoben:** Alte Einstellungsdateien enthielten das bisher wirkungslose `legible = false`. Mit dem neuen Schalter wären Höhlen dort plötzlich dunkel geworden. Das Feld heißt jetzt `legible_caves` (Standard an), der alte Wert wird ignoriert.

**Im Spiel geprüft:**

- `/meridian radar hide zombie` und `… creeper`: Beide verschwinden aus dem Radar, alles andere bleibt; „hidden“ listet sie.
- Fair Play blendet das Radar aus.
- Die Seite „Miscellaneous Settings“ zeigt die neuen Höhlen-Optionen.

Nicht im Spiel geprüft: gefärbtes Glas und kurze Blöcke (gelten nur für neu erfasste Chunks), Umbenennen von Karten (braucht einen Server).

**Build und Tests**: 184 Kern-Tests und der Paritätstest grün; Clippy sauber, Adapter-Check grün; Launcher-Build `native-66f3981-838e78aa4876`.

**Weiter offen:**

- Bestätigungsdialog beim Kartenwechsel mit Schnellbestätigung (CTX-03).
- Spielerköpfe mit ihrem Skin.
- Vom Server erzwungene Regeln über Signale (SRV-01, SRV-03): Dafür braucht es eine dokumentierte Quelle für die Signale.
- Claims (OV-06): Dafür gibt es in Horizon noch keinen Anbieter.

### Phase 6, Fehlerbehebung: Seed-Atlas-Ebene lädt von außen, keine Wiederverwendung, hohe Last

Rückmeldung aus dem Spiel: Die Ebene lud von außen nach innen und musste nach jedem Verschieben neu laden; der Lüfter lief sofort hoch. Ursachen und Lösung stehen in D-030.

| Messung (M6, 3024 × 1720 Pixel, 4 Worker) | vorher | nachher |
|---|---|---|
| Kacheln je Bildschirm | ~320 | ~80 |
| Zoom 0,1x: Bildschirm gefüllt | Mitte blieb schwarz | ~2 s, Mitte zuerst |
| Zurückschieben | neu berechnet | sofort aus dem Cache |
| CPU nach dem Laden (Karte offen, 0,016x) | dauerhaft hoch | ~40 %, wie im Spiel ohne Karte |

Neue Tests:

- `the_queue_follows_the_latest_frame`: Angefragte Kacheln eines alten Frames werden nicht mehr berechnet.
- `tile_cost` (ignoriert): misst die Kosten je Kachel.

**Build und Tests**: 185 Kern-Tests und der Paritätstest grün; Clippy sauber, Adapter-Check grün; Launcher-Build `native-66f3981-ce15819d4afb`.

### Phase 6: Strukturmarker wie in der Fabric-Version (D-031)

- Herausgezoomt standen die Marker in einem Quadrat um die Mitte. Jetzt gibt es einen Scan je Strukturart für die ganze Sicht, mit Zoomfilter und Ergebnisgrenzen der Fabric-Version 2.0.
- **Im Spiel geprüft:**
  - Bei 3x bis 0,2x füllen die Marker die ganze Karte.
  - Bei 0,018x zeigen sich nur noch Festungen, Anwesen, Monumente und antike Städte.
  - Nach dem Scan liegt die CPU-Last bei rund 39 %, so viel wie im Spiel ohne Karte.
- **Neue Tests:** `view_boxes_like_the_fabric_version`, `dense_markers_only_up_close`; der Servicetest nutzt jetzt den Sicht-Scan.

**Build und Tests**: 187 Kern-Tests, die Engine-Tests und der Paritätstest grün; Clippy sauber, Adapter-Check grün; Launcher-Build `native-66f3981-85c46d528fc6`.

- **Nachtrag:** Weit herausgezoomt verteilen sich die Marker dichter Arten jetzt gleichmäßig über die ganze Sicht, statt nur im Norden zu stehen (D-031, bewusste Abweichung von der Fabric-Version). Im Spiel bei 0,018x geprüft, auch nach dem Verschieben. Neuer Test: `capped_scans_spread_over_the_whole_box`. Der Paritätstest bleibt grün. Launcher-Build `native-66f3981-098c8f7d2b74`.
- **Nachtrag 2:** Der Radius des Sicht-Scans wird jetzt wie in der Fabric-Version aus den Fensterpixeln und dem Zoom berechnet.
  - Im Spiel geprüft: Bei 0,3x sind alle Arten sichtbar. Bei 0,2x bleiben Dörfer, Ruinen und Tempel, Schätze, Minenschächte und Erzadern verschwinden. Bei 0,094x bleiben nur Monumente, Festungen und antike Städte.
  - Ein Bildschirmfoto mit Markern im Quadrat stammte aus Build `5e33e0f60780` von vor dieser Änderung: Ein offener Launcher hatte die Instanz beim Starten auf diesen Build zurückgesetzt.
  - Launcher-Build `native-66f3981-436b2aa39fc3`.

### Phase 6: Seed-Atlas-Einstellungen wie in der Fabric-Version (D-032)

- „Seed Atlas Settings“, „Seed Atlas Markers“ und „Seed Atlas Performance“ sind eigene Screens im Layout der Fabric-Version 2.0, mit Schiebereglern, Markerliste und Tooltips.
- Von der Karte aus geöffnete Menüs zeigen die abgedunkelte Karte dahinter.
- Die Seed-Atlas-Seite der Minimap-Einstellungen, der Knopf „SA“ und `/seedatlas settings` öffnen den neuen Screen.
- **Im Spiel geprüft:** alle drei Screens, Scrollen der Markerliste, Umschalten einer Art, die Karte im Hintergrund.
- **Neue Tests:** Layout des Hauptscreens in Pixeln, Y-Felder werden beim Verlassen übernommen, Schieberegler, Markerliste (Umschalten, Scrollen, Markergröße), Leistungsoptionen.

**Build und Tests**: 193 Kern-Tests grün; Clippy sauber, Adapter-Check grün; Launcher-Build `native-66f3981-3d1bb3829116`.

### Phase 6: Strukturmenü, Monumente, antike Städte (D-033)

- Rechtsklick auf eine Struktur öffnet ihr eigenes Menü wie in der Fabric-Version; Menüs passen sich der Textbreite an.
- Monumente verschwinden unter 0,0625x; antike Städte sind auf der Karte größer. Beides auch in der Java-Mod (Commit `007f94f`, `./gradlew check` grün), dort aber nur im noch offenen PR DUzzL/seed-atlas-xaero#12, nicht auf `main`.
- **Im Spiel geprüft:** Strukturmenü auf einem Ruinenportal mit passender Breite.
- **Nicht geprüft:** „Copy coordinates“, weil der Test sonst die Zwischenablage überschrieben hätte; die Java-Mod im Spiel.

### Phase 6: Minimap nach ESC, Leistung der Seed-Atlas-Karte (D-034)

- Nach ESC aus der Weltkarte (oder aus einem Menü über ihr) erscheint die Minimap wieder.
- **Seed-Atlas-Karte:**
  - Die Gelände-Verfeinerung rechnet mit einem gemeinsamen Dichtegitter und halber Auflösung, zusammen etwa 40× weniger Arbeit.
  - Die Worker laufen mit niedriger Priorität.
  - Gemessen bei 0,3x: vorher über 90 s ~290 % CPU, jetzt ~18 s, danach ~50 % wie beim Spielen.
- **Im Spiel geprüft:** ESC aus der Karte und aus den Kartenoptionen (Minimap da); CPU-Verlauf nach Herauszoomen und Verschieben; verfeinerte Ansicht ohne sichtbare Treppen.
- **Neue Tests:** `coarse_surfaces_upsample_to_the_tile`, `lattice_columns_match_single_columns` (ignoriert, braucht die Horizon-Ressourcen).

**Build und Tests**: 195 Kern-Tests und der Paritätstest grün; Clippy sauber, Adapter-Check grün; Launcher-Build `native-66f3981-57ea076f40cc`.

### Phase 6: Detailstufen der Seed-Atlas-Karte (D-035)

- Weit draußen rechnet die Karte mit weniger Proben pro Pixel und ohne detailliertes Gelände; nah dran (ab einem Pixel pro Block) mit vollem Detail.
- Herauszoomen setzt grobe Kacheln aus den feinen im Speicher zusammen, statt sie zu rechnen.
- Kacheln und ihre Texturen werden zwei Sekunden nach dem Schließen der Karte freigegeben; es gibt keinen Festplatten-Cache.
- **Im Spiel geprüft:**
  - Rechenphase bei 0,3x ~4 s (vorher ~18 s), bei 0,018x ~3 s; hinein auf 0,7x ~4 s.
  - Zurück auf 0,018x ohne Rechenphase und sofort vollständig; Schließen und erneutes Öffnen.
- **Nicht sichtbar:** Der Prozessspeicher sinkt nach dem Schließen nicht messbar, weil der Allokator freie Seiten zur Wiederverwendung behält. Beim erneuten Öffnen kamen 8 MB dazu.
- **Neue Tests:** `zoomed_out_views_need_fewer_tiles`, `coarse_tiles_are_made_from_finer_ones_in_memory`.

**Build und Tests**: 197 Kern-Tests grün; Clippy sauber, Adapter-Check grün; Launcher-Build `native-66f3981-74e6e37c960d`.

### Phase 6: Landmarken, kein Zusammenlegen, Gesichter mit Tab (D-036)

- Antike Städte verschwinden unter 0,0625x, Waldanwesen und Strongholds unter 0,03125x.
- Struktur-Scans zeigen eine Art ganz oder gar nicht; nichts wird ausgedünnt oder zusammengelegt.
- Tab halten zeigt auf Minimap und Weltkarte die Gesichter von Mobs, Tieren und Spielern.
- Struktur-Icons wachsen beim Hineinzoomen höchstens auf das Doppelte.
- **Im Spiel geprüft:**
  - Weltkarte und Minimap mit gedrückter Tab-Taste zeigen Gesichter, ohne Tab Punkte.
  - Bei 0,1x Monumente, antike Städte, Strongholds; bei 0,054x keine Monumente und antiken Städte; ab 0,032x Anwesen, bei 0,016x keine Landmarken.
- **Nicht im Spiel geprüft:**
  - Das Loslassen von Tab: Das Steuer-Tool des Testspiels meldet losgelassene Tasten nicht an Mods, die echte Tastatur schon.
  - Strongholds zwischen 0,03 und 0,06x: Beim Zoomen mit dem Steuer-Tool wanderte die Karte vom Ursprung weg. Der Test `strongholds_and_mansions_far_out` deckt das ab.
  - Spielergesichter: keine Mitspieler im Test.
- **Neue Tests:** `scans_show_every_marker_or_none`, `strongholds_and_mansions_far_out`; Radar-Tests für Spielergesichter und den Punkt bei fehlender Textur.

**Build und Tests**: 198 Kern-Tests und der Paritätstest grün; Clippy sauber, Adapter-Check grün; Launcher-Build `native-66f3981-8a8c8e9d991a`.

### Phase 6: Neue Einstellungen, Werkzeugleiste, Mob-Gesichter (D-037)

- Ein Einstellungs-Screen mit Kategorien links, beschriebenen Optionen rechts und Suche ersetzt die alten Raster- und Seed-Atlas-Screens.
- Die Weltkarte hat eine Werkzeugleiste am rechten Rand statt verstreuter Knöpfe.
- 58 weitere Mob-Gesichter: alle Lebewesen außer Magmawürfel und Enderdrache; Gesichter in nativen Texturkoordinaten.
- Radar wie die Standard-Kategorien der Referenz: gezähmte Tiere über wilden, Rüstungsständer als sonstige Entities.
- **Im Spiel geprüft:** Werkzeugleiste auf der Karte; Einstellungen per Y-Taste bei kleinem GUI (etwa 440×250 Pixel), alle Kategorien und Zeilen passen.
- **Nicht im Spiel geprüft:** Klicks in den neuen Screens und Tooltips der Leiste. Das Steuer-Tool setzt den Mauszeiger dort nicht zuverlässig; die Klicklogik decken Unit-Tests ab.
- **Neue Tests:** `every_catalogue_option_has_a_row`, `categories_rows_and_clicks`, `search_scroll_and_slider`, `profiles`, `faces_lie_inside_their_textures`, `tamed_animals_over_wild_ones`, Werkzeugleiste in `zoom_buttons`.

**Build und Tests**: 195 Kern-Tests grün; Clippy sauber, Adapter-Check grün; Launcher-Build `native-66f3981-2984424c33ef`.

### Phase 6: Strukturen mit der Seed-Ebene, Wegpunkt-Auswahl (D-038)

- Seed-Ebene aus blendet die Strukturmarker mit aus.
- „World/Server“ und „Sub-World/Dimension“ in Wegpunktliste und Editor sind Auswahllisten; andere Welten und Dimensionen lassen sich ansehen und bearbeiten.
- **Im Spiel geprüft:**
  - Seed-Ebene per Leiste aus: Strukturen weg, Icon grau, Tooltip links.
  - Welt-Liste mit der aktuellen und einer zweiten Testwelt; Wechsel zeigt deren Wegpunkte.
  - Dimensions-Liste und Wechsel in den Nether.
- **Nicht im Spiel geprüft:** Speichern eines Wegpunkts in einer anderen Welt (Unit-Logik und Adapter-Pfad vorhanden, kein Klicktest).
- **Neue Tests:** `waypoint_world_and_dimension_dropdowns`, `waypoint_places_on_disk`.

**Build und Tests**: 197 Kern-Tests grün; Clippy sauber, Adapter-Check grün; Launcher-Build `native-66f3981-2096c8af0689`.

## Audit-Korrekturen (D-039)

Das Audit (`docs/AUDIT_REPORT.md`, Commit `92c0fd5`) fand 6 hohe, 23 mittlere und 13 niedrige Punkte. Alle sind bearbeitet; was bewusst entschieden wurde und was offen bleibt, steht in D-039.

- **Behoben, die hohen Punkte:**
  - Todespunkte bleiben, bis man einmal weg war und zurückkommt (AUD-001).
  - Erzader-Marker vervielfachen sich nicht mehr (AUD-002).
  - Unlesbare Dateien werden nie mehr überschrieben (AUD-003, AUD-015).
  - Teleport zu Wegpunkten anderer Dimensionen nutzt das Dimensionsformat (AUD-004).
  - Wegpunkt-Sets lassen sich anlegen, umbenennen und löschen (AUD-005).
  - Die Minimap setzt beim Gehen nur hereinrollende Zellen neu zusammen (AUD-006).
- **Bridge:** Version 11 (Chunk-Abzüge mit Lesemodus, Anfragen nach Position und Modus, Tick-Positionen der Entities, gemeinsame Entity-Liste).
- **Tests:** Kern 258 (dazu 3 ignorierte Messungen), Host 2, Paritätstest aller 38 Versionen mit den erweiterten Fixtures. Neu getestet sind unter anderem Todespunkt-Ablauf, Teleport-Formate, geteilte Wegpunkte, Sets, Sortierung, kaputte und neuere Dateien jeder Ablage, `flush_and_wait`, Speicherbudget, Spawn-Wache, Kartennamen, Sperrdatei, Marker-Stabilität, festhängende Seed-Atlas-Aufträge, Verfeinerung in Nether und End, Scan-Grenzen pro Art, Radar-Auswahl, Textfelder und das Minimap-Bild. Die schwachen Engine-Tests aus AUD-029 prüfen jetzt Inhalte.
- **Nicht im Spiel geprüft:** Die Korrekturen sind mit Unit-Tests und dem Adapter-Build geprüft, aber noch nicht in einem Testspiel. Offen sind vor allem: Bildrate der vergrößerten Minimap beim Sprinten (AUD-006), Erzader-Marker über eine Minute (AUD-002), die neueren-Version-Datei aus AUD-003, der „[Add waypoint]“-Link im Chat, Teleports in andere Dimensionen, das Verhalten der Spawn-Wache auf einem Servernetz und die neuen Sichtweiten der Marker (AUD-020).
- **Bekannte Grenzen:** keine Tab-Vervollständigung für `/seedatlas`, Antworten in einer Zeile der Aktionsleiste, keine Dimensionsskalierung der maximalen Wegpunkt-Entfernung, Biom-Übergänge nur innerhalb eines Chunks, Sperrdatei nur für Regionen (siehe D-039).
