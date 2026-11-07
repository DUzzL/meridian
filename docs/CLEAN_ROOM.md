# Clean-Room-Nachweis

Meridian ist eine Neuentwicklung. Das Verhalten der Referenz-Mods (Minimap und Weltkarte eines Drittautors) wird ausschließlich aus erlaubten Quellen abgeleitet.

## Regeln

- Kein dekompilierter, disassemblierter oder aus Jars extrahierter Code.
- Keine `.class`- oder `.jar`-Inhalte öffnen. Das gilt auch für Assets, Sprachdateien und `fabric.mod.json`.
- Keine Texturen, Icons, Sounds oder Übersetzungen übernehmen. Alle Grafiken von Meridian werden selbst erstellt oder kommen aus Horizon bzw. den Ressourcen des Spielers.
- Kein Name, Logo oder Hinweis, der eine Zugehörigkeit nahelegt.
- Seed Atlas ist Code des Auftraggebers (GPLv3) und wird frei portiert. Er ist keine Referenz-Mod im Sinne dieser Regeln.

## Quellen

| Kürzel | Quelle | Art | Abgerufen |
|---|---|---|---|
| S1 | https://modrinth.com/mod/xaeros-minimap | öffentliche Modseite (Beschreibung, FAQ) | 2026-11-06 |
| S2 | https://www.curseforge.com/minecraft/mc-mods/xaeros-minimap | öffentliche Modseite | 2026-11-06 |
| S3 | https://modrinth.com/mod/xaeros-world-map | öffentliche Modseite | 2026-11-06 |
| S4 | https://www.curseforge.com/minecraft/mc-mods/xaeros-world-map | öffentliche Modseite | 2026-11-06 |
| O1 | `options.txt` der Prism-Instanz `26.3`, Zeilen `key_gui.xaero_*` | vom Spiel geschriebene Tastenbelegung | 2026-11-06 |
| O2 | `config/xaero/{minimap,world-map,lib}/**/*.cfg`, `default_radar_categories_*.json`, `info_display_config` der Prism-Instanz | vom Spiel beim ersten Start geschriebene Benutzerkonfiguration (Optionsnamen und Standardwerte) | 2026-11-06 |
| O3 | Beobachtung im laufenden Spiel: Bildschirmaufnahmen der Prism-Instanz (Minimap, Wegpunkt in Welt und Minimap, B-Editor, U-Liste, Y-Einstellungen, M-Weltkarte) | eigene Beobachtung; Aufnahmen nur lokal, nicht im Repo | 2026-11-06 |
| SA | `DUzzL/seed-atlas-xaero` (lokal, Commit `df4e212`), `config/seedatlas_xaero.json` | eigener Code des Auftraggebers | 2026-11-06 |

**Zu O2:** Diese Dateien sind keine Jar-Inhalte, sondern Laufzeitausgabe des Spiels im Konfigurationsordner des Nutzers. Sie sind dafür gedacht, dass der Nutzer sie liest und bearbeitet. Verwendet werden nur Optionsnamen, Standardwerte und die Struktur der Radar-Kategorien, um den Funktionsumfang festzustellen. Meridian verwendet ein eigenes Dateiformat (TOML/JSON) und überwiegend eigene Schlüsselnamen. 15 Schlüssel sind schlichte beschreibende Namen, die zufällig oder bewusst wie die O2-Optionsnamen heißen (z. B. `deathpoints`, `footsteps`, `lighting`, `block_colors`, `max_loaded_regions`); sie stammen aus einer erlaubten Quelle (O2) und sind kein übernommener Code.

**Nicht verwendet:** die Jars `xaerominimap-*.jar` und `xaeroworldmap-*.jar` und deren Inhalte. `mods/.index` wurde nicht geöffnet; gelesen wurden nur die Dateinamen in `mods/`.

## Feature-Herkunft

Die Quellenspalte in `FEATURES.md` und `KEYBINDS.md` nennt für jedes Feature die Herkunft. Zusammengefasst:

| Bereich | Abgeleitet aus |
|---|---|
| Minimap-Anzeige (MM) | S1/S2 (Formen, Größen, Zoom, Rotation, Vergrößern, Blockfarben, Schattierung), O2 (alle Detailoptionen) |
| Höhlenmodus (CV) | S1/S2 (automatisch, Dachgröße), S3/S4 (Höhlenmenü der Weltkarte), O2 |
| Infoanzeigen (INFO) | S1/S2 (Koordinaten, Biom, Licht, Zeit, Winkel, Datenschutz), O2 `info_display_config` |
| Overlays (OV) | S1/S2 (Gitter, Schleimchunks, Licht-Overlay, Claims), S3/S4 (Fußspuren), O2 |
| Entity-Radar (RAD) | S1/S2 (Farben, Darstellung, Kategorien, Icon-Überschreibung), O2 (Kategoriebaum, Kategorie-Einstellungen) |
| Wegpunkte (WP) | S1/S2 (Erstellen, Liste, Sets, global/lokal, Teilen, Teleport, Todespunkte, Unterwelten), O1, O2 |
| Weltkarte (WM) | S3/S4 (Vollbild, Zoom/Pan, Speichern, Höhlen, Dimensionen, Rechtsklick, PNG, Kartenauswahl), O2 |
| Kontexte (CTX) | S1–S4 (Serveradresse, Unterwelten, Kartenauswahl-Modi, Bestätigung) |
| Einstellungen (SET), Server-Regeln (SRV) | S1–S4, O2 (Profile, Serverprofile) |
| Import/Export (IO) | `TASK.md`, S3 (PNG) |
| Hotkeys | O1 (verbindlich); Abweichung von S1 (I/O-Zoom) zugunsten von O1 aufgelöst |
| Seed Atlas (SA) | SA |

## Protokoll

- 2026-11-06: S1–S4 über einen Web-Abruf in eigenen Worten zusammengefasst, ohne längere Zitate.
- 2026-11-06: Spiel einmal über Prism gestartet; O1 und O2 entstanden dabei und wurden gelesen.
- 2026-11-06: Die Beobachtung im Spiel (O3) per Bildschirmsteuerung ist nicht möglich. Prism startet Minecraft als `java`-Prozess ohne App-Bundle, der sich nicht freigeben lässt. Die offenen Punkte stehen in `KEYBINDS.md`.
- 2026-11-06: O3 nachgeholt. Die Testwelt wurde in die Prism-Instanz kopiert, `pauseOnLostFocus` in deren `options.txt` ausgeschaltet und das Spiel per Tastatur-Automation (AppleScript) bedient. Bildschirmaufnahmen mit `screencapture`. Daraus wurden Maße, Farben und Anordnung in eigenen Worten und Zahlen abgeleitet (DECISIONS D-019); keine Pixel oder Grafiken übernommen.
