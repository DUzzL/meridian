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
