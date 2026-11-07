# Audit-Korrekturen: Build und Spielprüfung

Geprüft am 2026-11-07, Code `30ac262` (D-039). `audit-fixes` wurde per Fast-forward in `main` übernommen. Native-Build: `native-66f3981-337ba39a8b1a`, Horizon `66f3981f522b24c250b21523a70080965b940424`. Der Build ist für die Launcher-Instanz `horizon` ausgewählt. Nichts wurde gepusht.

Das ursprüngliche Audit beschreibt weiterhin den alten Commit `92c0fd5`. Diese Datei hält die anschließende Prüfung fest; sie ersetzt keinen vollständigen neuen Feature-Abgleich.

## Build und Tests

- `cargo fmt --all --check`: bestanden.
- `cargo clippy --workspace --all-targets --locked -- -D warnings`: bestanden.
- `cargo test --workspace --locked`: 258 Kern-Tests, 2 Host-Tests, 8 Engine-Tests und der Paritätstest aller 38 Versionen bestanden. Die Parität brauchte 591,27 s; 3 Kern-Messungen und der separate Performance-Test bleiben wie vorgesehen ignoriert.
- Launcher-Native-Build: bestanden, einschließlich Prüfung der Mod-Crates gegen Horizons Lockfile. Gegenüber dem vorherigen Adapter-Check wurde damit auch das ausführbare Spiel gebaut und gestartet.

## Testumgebung

Eigene Testwelt `AuditFixValidation`, Seed 12345, eigener Spiel-, Home- und Steuerungsordner. Die bestehenden Welten der Launcher-Instanz wurden nicht geöffnet. Fenster 3024 × 1720 physische Pixel, Metal auf Apple M6, Sichtweite 8 Chunks, Limit 120 fps. Bedienung und Messungen mit dem `horizon-ctl` dieses Builds; Screenshots wurden visuell geprüft.

Die vergrößerte Minimap wurde im Zuschauermodus je 5 s vorwärts und rückwärts bewegt, mit gedrückter Sprinttaste. Dabei wurden etwa 49 beziehungsweise 44 Blöcke zurückgelegt. Das ist eine Prüfung bei schneller Bewegung, kein identischer Wiederholungslauf der Sprintstrecke des ursprünglichen Audits.

Horizons Standardeinstellung `inactivityFpsLimit=afk` begrenzte spätere frühe Messungen auf etwa 30 fps. Für die unten aufgeführten Wiederholungen stand sie auf `minimized`; `pauseOnLostFocus` war aus. Die gedrosselten Messungen werden nicht als Leistungsregression gewertet. Die beiden abschließenden Erzader-Messungen liefen nach Ende des Paritätstests.

## Im Spiel verifiziert

| Punkt | Ablauf und Ergebnis |
|---|---|
| AUD-001: Todespunkt | Am eigenen Respawnpunkt gestorben. Nach Respawn blieb „Death“ erhalten, ebenso 10 Blöcke entfernt. Erst nach der Rückkehr wurde er entfernt. `delete_reached_deathpoints=true`. |
| AUD-002: Erzadern | „Structures“ und „Ore Vein“ eingeschaltet, Karte offen gelassen. Eine volle Minute in zwei aufeinanderfolgenden 30-s-Messungen ohne zunehmenden CPU-Aufwand oder die frühere Bildratenkollaps-Reproduktion. Eisen- und Kupfermarker waren sichtbar. Die Zahl der internen Marker wurde im Spiel nicht ausgelesen; dafür besteht der Stabilitäts-Unit-Test. |
| AUD-003: neuere Region | Bei geschlossenem Spiel `0_-1.mrg` im Testspiel auf Format 3 gesetzt. Neu gestartet, mit Speicherintervall 2 s weiterlaufen lassen und regulär beendet. SHA-256 und Änderungszeit blieben unverändert; Format blieb 3. |
| AUD-003/AUD-015: kaputte Region | Eine zweite Testregion durch ungültige Bytes ersetzt. Beim Neustart entstand `0_0.mrg.broken.<Zeit>` mit genau diesen Bytes. Die frisch erzeugte Region hat Format 2. Die anderen Ablagen und I/O-Fehler sind durch die Unit-Tests abgedeckt, nicht durch diesen Spielversuch. |
| AUD-004: Dimensionsteleport | Im Nether Wegpunkt bei `[12,110,14]` angelegt, in die Oberwelt zurückgekehrt. In der Wegpunktliste `the_nether` ausgewählt und den Teleportknopf geklickt. Danach Dimension `minecraft:the_nether`, Position `[12.5,110,14.5]`. |
| AUD-005: Sets | Set per Befehl angelegt, aktuelles Set mit `/meridian waypoint set rename RenamedSet` umbenannt und mit `set delete RenamedSet` gelöscht. Danach wieder Set `default`; vorhandene Wegpunkte blieben gespeichert. `rename` nimmt den neuen Namen des aktuellen Sets, keinen alten und neuen Namen. |
| AUD-006: vergrößerte Minimap | Beide Bewegungsrichtungen hielten das 120-fps-Limit; die Karte und ihre Größe wurden auf Screenshots geprüft. Messwerte unten. |
| AUD-007: unbekannte Höhe | Auf der Karte weit außerhalb des Erkundeten zeigte das Kontextmenü `Y: ~`. „Teleport Here“ brachte den Spieler nach `[-5230.5,149,1825.5]`; seine vorherige Höhe 149 blieb erhalten. |
| AUD-008: geteilter Wegpunkt | Eine Test-Chatzeile bekam den grünen Link „[Add waypoint]“. Vor dem Klick war der Wegpunkt weder in der Liste noch auf Platte. Der tatsächliche Klick erzeugte ihn als temporär. Nach Speichern, Beenden und Neustart fehlte er auf Platte und im geladenen Buch; die normalen Wegpunkte blieben. |
| AUD-020: Markerbereiche | Bei etwa 0,3x Erzadern sichtbar, bei etwa 0,2x verschwunden, während mittlere Arten sichtbar blieben. Bei etwa 0,094x waren auch diese verborgen; Monumente, antike Städte und Strongholds blieben. Stichproben der Oberwelt, kein Spieltest jeder Art und jeder Radiusgrenze. |
| Weltversion | Override mit `/seedatlas version 1.20` gesetzt. Nach Neustart meldete `/seedatlas status` weiterhin 1.20–1.20.6; `seeds.toml` enthielt `1.20.6`. |

## Messwerte

Die CPU-Zeit umfasst Horizons gesamte Arbeit pro Frame, einschließlich Mod-Hooks. Frame-Zeit enthält das Warten auf das Bildratenlimit. Rohwerte und Dateihashes: [validation/audit-2026-11-07.json](validation/audit-2026-11-07.json).

| Zustand | Frames | Ø fps | CPU Ø / p95 (ms) | Frame p95 / p99 (ms) |
|---|---:|---:|---:|---:|
| Vergrößerte Minimap, vorwärts, 5 s | 604 | 119,95 | 1,79 / 3,23 | 8,48 / 8,52 |
| Vergrößerte Minimap, rückwärts, 5 s | 605 | 119,99 | 2,00 / 3,44 | 8,51 / 8,58 |
| Erzadern, erste 30 s | 3594 | 119,79 | 2,07 / 3,24 | 8,49 / 8,55 |
| Erzadern, nächste 30 s | 3599 | 119,96 | 2,10 / 3,27 | 8,48 / 8,52 |

## Grenzen und Endzustand

Die bekannten Einschränkungen aus D-039 bleiben: keine lokale Chat-Ausgabe oder Tab-Vervollständigung für `/seedatlas` über Horizons API, keine geklärte Dimensionsskalierung der maximalen Wegpunktentfernung, Biom-Blending nur innerhalb eines Chunks, Dateisperre nur für Regionen und Spawn-Erkennung mit 5-s-Wartefenster. Die fehlenden Beobachtungen der Referenz aus D-012/KEYBINDS wurden in dieser Horizon-Prüfung nicht nachgeholt.

Nicht im Spiel geprüft: Spawnwechsel in einem Servernetz, konkurrierende Spielprozesse, alle Dateiablagen und Fehlerarten, jede Marker-Radiusgrenze sowie physische Tastatur-Releases unter fremden Screens. Die jeweiligen Unit-/Adapter-Prüfungen sind kein Ersatz für diese Beobachtungen.

Beide Testläufe regulär mit `quit` beendet; kein Testspiel läuft weiter. Der Launcher behält den neuen Build für `horizon`. Der unveränderte ursprüngliche Auditbericht bleibt die zuvor vorhandene, unversionierte Datei.
