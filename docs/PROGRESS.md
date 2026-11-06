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
  - `worker`: Thread-Pool; Panics in Jobs stoppen ihn nicht; beim Beenden werden offene Speicher-Jobs abgearbeitet.
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
