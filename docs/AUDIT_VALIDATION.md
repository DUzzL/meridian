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

## Grenzen und Endzustand (Stand 2026-11-07)

> Überholt durch den Abschnitt „Nachprüfung der Restpunkte (2026-11-08)“ unten. Der Text bleibt zum Vergleich stehen.

Die bekannten Einschränkungen aus D-039 bleiben: keine lokale Chat-Ausgabe oder Tab-Vervollständigung für `/seedatlas` über Horizons API, keine geklärte Dimensionsskalierung der maximalen Wegpunktentfernung, Biom-Blending nur innerhalb eines Chunks, Dateisperre nur für Regionen und Spawn-Erkennung mit 5-s-Wartefenster. Die fehlenden Beobachtungen der Referenz aus D-012/KEYBINDS wurden in dieser Horizon-Prüfung nicht nachgeholt.

Nicht im Spiel geprüft: Spawnwechsel in einem Servernetz, konkurrierende Spielprozesse, alle Dateiablagen und Fehlerarten, jede Marker-Radiusgrenze sowie physische Tastatur-Releases unter fremden Screens. Die jeweiligen Unit-/Adapter-Prüfungen sind kein Ersatz für diese Beobachtungen.

Beide Testläufe regulär mit `quit` beendet; kein Testspiel läuft weiter. Der Launcher behält den neuen Build für `horizon`. Der unveränderte ursprüngliche Auditbericht bleibt die zuvor vorhandene, unversionierte Datei.

## Nachprüfung der Restpunkte (2026-11-08)

Auftrag: die fünf Grenzen und die sechs ungeprüften Punkte aus dem Abschnitt oben beheben bzw. prüfen. Entscheidungen dazu: D-040. Rohdaten: [validation/followup-2026-11-08.json](validation/followup-2026-11-08.json).

### Stand und Builds

- Code: `main` mit nicht committeten Änderungen auf `0ecd305`. Bridge Version 12.
- Spielprüfungen mit `native-66f3981-f25e342242b3`. Danach zwei Builds nur für das Biom-Blending (`a86344a3637b`, dann `91d307a394a7`). `native-66f3981-91d307a394a7` ist der letzte Build und für die Instanz `horizon` ausgewählt.
- Testumgebung wie oben, aber Ordner `/tmp/meridian-followup/` (Spiel A: `game-a`, Spiel B: `home-b` mit `game-a` oder `game-b`), Seed 12345, Kreativ, Sichtweite 6, Audio aus (`HORIZON_NO_AUDIO=1`, weil der erste Start in der macOS-Audio-Geräteabfrage auf dem Hauptthread hing; in Horizons `main` mit `745b95ace` schon behoben, daher nicht gemeldet), `inactivityFpsLimit=minimized`, `pauseOnLostFocus=false`. Die Welten der Launcher-Instanz wurden nicht geöffnet.

### Build und Tests

- `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --locked -- -D warnings`: bestanden.
- `tools/check-adapter.sh --release`: „adapter OK against Horizon 66f3981… with its mixins applied“.
- `cargo test --workspace --locked --release`: Kern 261 (3 ignorierte Messungen), `storage_safety` 2 (1 ignorierter Kindprozess-Helfer), Host 2, Seed Atlas 8, Paritätstest aller 38 Versionen bestanden (59,2 s im Release-Profil).
- Gezielt laufen lassen (sonst ignoriert): `seedatlas::tests::marker_limits` scannt jede Art an ihrem größten Radius; alle 26 Arten liefern ein Ergebnis unter dem Trefferlimit (größte: Mineshaft 6775, Ruinen 6735, Monument 6639; längster Scan: End-Städte 3,9 s). Log aus dem Lauf vom 2026-11-07, 23:20, Code danach in diesem Bereich unverändert.

### Die fünf Grenzen

| Grenze | Änderung | Prüfung | Ergebnis |
|---|---|---|---|
| `/seedatlas`-Antworten nur in der Aktionsleiste | Bridge `local_chat`: Zeilen gehen als Client-Chatzeilen (`ChatSource::Client`) in den eigenen Chat, nie an den Server. Fehlerzeilen rot. | `/seedatlas status` im Spiel, dann `messages`. | 7 Zeilen im Chat („Seed Atlas context: Followup2“ … „Performance: …“), `actionbar` leer, Server-Chat leer. Screenshot `tab-seedatlas_version_1.20..png` zeigt sie. |
| Keine Tab-Vervollständigung | Neue Injektion `client-command-tree` (`bridge/commands.rs`) hängt `seed`, `dimension-seed`, `clear`, `version`, `status`, `settings` samt `nether`/`end` und allen Versionsnamen an Horizons lokalen Befehlsbaum. Ausführung bleibt beim Mod (`MOD_COMMAND`). | Chat öffnen, Text tippen, Tab, `screen`. | `/seedatlas ` → `/seedatlas clear`; `/seedatlas cl` → `clear`; `/seedatlas dimension-seed n` → `nether`; `/seedatlas version 1.20.` → `1.20.6` (Vorschlagsliste im Screenshot). Nach vollständigem Argument kein Vorschlag mehr. |
| Keine Dimensionsskalierung der Wegpunkt-Entfernung | Option „Scale WP Distance to Dimension“ (`max_distance_dimension_scale`, Standard an): Grenze geteilt durch den Koordinatenmaßstab der Dimension, im Nether also ein Achtel. Globale Wegpunkte bleiben sichtbar. Quelle: Changelog des Referenz-Autors im öffentlichen Forum-Thread (minecraftforum.net, Thread „Xaero's Minimap“, Seite 118). | Unit-Test `waypoint_draw_range_uses_dimension_coordinates` (1000 → Oberwelt 1000, Nether 125, End 1000; aus → 1000; 0 → unbegrenzt). | Bestanden. Im Spiel nicht nachgemessen; das Ablesen von Zeichengrenzen auf Screenshots wäre ungenauer als der Test. |
| Biom-Blending nur innerhalb eines Chunks | Bridge liest einen Rand von 2 Spalten aus den Nachbarchunks (20 × 20 Biome, `biome_border`); das Blending nutzt ihn. Ein Nachbar wird nur neu gelesen, wenn er ohne diesen Chunk gelesen wurde und die Biome an der gemeinsamen Kante nicht ein und dasselbe sind. Blockänderungen lösen keine Nachbar-Lesungen aus. | Unit-Tests `biome_blending_crosses_chunk_boundaries` (Farbverlauf über die Kante, exakte Werte) und `blend_rereads_a_neighbour_only_when_the_shared_edge_differs`. Im Spiel: Weltkarte bei 3x nach einem Flug (`map-after-fly.png`), A/B-Flugmessung (unten). | Tests bestanden. Auf der Karte keine Chunk-Kanten zwischen Taiga, Schnee und Wald zu sehen; ein Pixelvergleich an einer Kante wurde nicht gemacht. |
| Dateisperre nur für Regionen | `claim_data_dir` jetzt für Daten- und Einstellungsordner, vor dem ersten Laden. `write_atomic`, `set_aside`, Kartenumbenennung und Wegpunkt-Export prüfen die Sperre. Eine Sperre, die gar nicht zu bekommen ist, zählt als fremd. TOML-Ablagen mit neuerer `version` bleiben unberührt. | `tests/storage_safety.rs`: alle sechs Ablagen mit fehlender, kaputter, neuerer und nicht lesbarer Datei sowie ein echter zweiter Prozess. Im Spiel: zwei Spiele auf demselben `game-a`. | Siehe „Zwei Spielprozesse“ unten. |
| Spawn-Erkennung mit 5-s-Fenster | Kein Zeitfenster mehr. Ein Spawn gilt, wenn ein anderer gemeldet wird oder das Paket `SetDefaultSpawnPosition` ankommt (neuer Hook `client_packet`, hört auch einen gleichen Spawn). Bis dahin: Hinweis, kein Wegpunktbuch, Karten-Speichern angehalten. Schickt ein Server nie einen Spawn, bleibt es dabei; der Hinweis nennt „Manual maps“. | LAN, siehe unten. | Siehe unten. |

### Die sechs ungeprüften Punkte

**Zwei Spielprozesse (Sperre).** Spiel A (Welt Followup2) lief auf `game-a`. Spiel B startete auf demselben Ordner mit der Welt Followup, legte mit `/meridian waypoint add LockTest` einen Wegpunkt an, speicherte und beendete regulär.

- B meldete „Added waypoint LockTest“ (nur im Speicher).
- Danach war keine Meridian-Datei außer den drei Regionen von A (`sp-followup2/…/0_0.mrg`, `-1_-1.mrg`, `0_-1.mrg`) geändert; `meridian.toml` und `sp-followup` unverändert; „LockTest“ steht in keiner Datei. SHA-256 vorher/nachher in den Rohdaten.
- Gegenprobe: B allein auf `game-a` mit `ControlAlone` → `sp-followup/default/waypoints.json` wurde geschrieben, „LockTest“ fehlt darin weiterhin.
- Nicht gesehen: der Hinweis „data or settings folder is locked“ in der Aktionsleiste. Er erscheint nur kurz beim Start und war bei der ersten Abfrage (nach 3 s) schon weg. Meridian-Logzeilen tauchen in Horizons Log nicht auf, weil Horizons Logger nur Ziele mit `horizon…` durchlässt (`horizon-client-render/src/logging.rs`, `ours()`); das gilt für alle Meridian-Meldungen und ist nicht Teil dieser Änderung.

**Spawnwechsel auf einem Server.** Kein echtes Servernetz verfügbar; geprüft mit A als LAN-Host (`lan open`, nur `localhost`, danach geschlossen) und B als Gast mit eigenem `game-b`.

- Erster Beitritt: Hinweis „waiting for this world's spawn …“, dann Wegpunktbuch offen, `maps.toml` mit `default` und Spawn `[96, 145, -32]`.
- Neu verbinden mit gleichem Spawn: Buch nach 0,23–0,27 s offen (vorher galt das 5-s-Fenster).
- Spawnwechsel während der Sitzung: Ein Respawn von B brachte das Paket mit `[100, 140, -20]` (aus einer `studio record`-Aufnahme dekodiert). Meridian legte `map-2` an und zeigte sie (`Maps: default, map-2 (shown: map-2)`).
- Rückweg: Beim nächsten Beitritt mit Spawn `[96, 145, -32]` zeigte B wieder `default`; nach einem Respawn wieder `map-2`.
- Horizons LAN-Host weicht hier von Vanilla ab, nicht Meridian: `/setworldspawn` schickt Gästen kein Paket (nur Beitritt und Respawn tun das, `lan/hub.rs`), und beim Beitritt kam ein anderer Spawn an als beim Respawn (Beitritt blieb bei `[96, 145, -32]`, obwohl Weltspawn und Host-Spawn auf `[100, 140, -20]` standen). Das Beitrittspaket ließ sich nicht aufnehmen (`studio record` braucht eine laufende Sitzung). Ein Vanilla-Server schickt das Paket bei jeder Änderung; das ist hier nicht geprüft.

**Alle Ablagen und Fehlerarten.** Durch `tests/storage_safety.rs` (Release-Lauf oben) für Regionen, `waypoints.json`, `meridian.toml`, `seeds.toml`, `maps.toml`, `completed.json`: fehlend → Standard; kaputt → `.broken.<zeit>`; neuer → unberührt und nie geschrieben; nicht lesbar → unberührt; zweiter Prozess → kein Schreiben, kein Beiseitelegen. Im Spiel zusätzlich die Sperre (oben) und die Regionsfälle vom 2026-11-07.

**Jede Marker-Radiusgrenze.** Durch `marker_limits` (oben) für alle Arten an ihrem größten Radius. Im Spiel nur die Zoom-Stichproben vom 2026-11-07.

**Tasten-Releases unter fremden Screens.** Neu: Die Bridge reicht Horizons physischen Tastenzustand durch; jeder Frame setzt „Minimap vergrößern“ und „Gesichter“ zurück, wenn die Taste nicht mehr gedrückt ist. Release-Ereignisse kommen auch außerhalb des Spiels an.

- Z gedrückt → Minimap groß (`z-1-held.png`). Inventar geöffnet, Z losgelassen, Inventar geschlossen → Minimap klein (`z-2-after.png`).
- Dasselbe mit Tab (Gesichter): danach keine Gesichter (`tab-faces-after.png`).
- Grenze: `horizon-ctl key` geht durch Horizons Eingabe; ob dabei das Ereignis oder der Abgleich gegriffen hat, lässt sich von außen nicht trennen. Ein Fokusverlust des Fensters wurde nicht nachgestellt.

**Beobachtungen an der Referenz (D-012, KEYBINDS).** Weiterhin offen. Sie brauchen die Referenz-Mod laufend in Prism; Computer Use bekam am 2026-11-07 keinen Zugriff auf Prism, und in dieser Sitzung gibt es keine Bildschirmsteuerung. Ohne Beobachtung wird nach den Clean-Room-Regeln nichts geraten.

### Messwerte Biom-Blending

Flug mit `horizon-ctl fly 96 180 -32 → 1296 180 -32` in 20 s durch frisches Gelände, je eine neue Welt mit Seed 12345, Minimap an. CPU = Horizons Arbeit pro Frame (ms). Alle Läufe hielten das 120-fps-Limit (2395–2400 Frames).

| Build | Blending | Welt | CPU Ø | CPU p50 | CPU p95 |
|---|---|---|---:|---:|---:|
| `f25e342242b3` (Nachbarn bei jedem Laden neu gelesen) | an | Blend1 | 1,11 | 0,98 | 2,29 |
| | aus | Blend0 | 0,81 | 0,59 | 2,03 |
| | an | Blend2 | 1,17 | 1,06 | 2,21 |
| | aus | Blend3 | 0,84 | 0,60 | 2,20 |
| `a86344a3637b` (Rand nur außen gelesen) | an | Blend4 | 1,02 | 0,91 | 2,03 |
| | aus | Blend5 | 0,73 | 0,59 | 1,74 |
| | an | Blend6 | 1,11 | 0,99 | 2,24 |
| | aus | Blend7 | 0,79 | 0,62 | 1,92 |
| `91d307a394a7` (Nachbarn nur bei anderer Kante) | an | Blend8 | 0,85 | 0,68 | 2,04 |
| | aus | Blend9 | 0,80 | 0,63 | 1,89 |
| | an | Blend10 | 0,81 | 0,66 | 1,96 |
| | aus | Blend11 | 0,78 | 0,62 | 1,89 |

Der erste Stand kostete beim Erkunden rund 0,3 ms pro Frame mehr; der letzte liegt im Rauschen der Aus-Läufe (+0,03 bis +0,05 ms Ø).

### Was offen bleibt

- Beobachtungen an der Referenz-Mod (D-012, KEYBINDS): brauchen die Referenz im Spiel.
- ~~Spawnwechsel nur mit Horizons fehlerhaftem LAN-Host geprüft~~ → [#12128](https://github.com/horizon-rust/horizon/issues/12128) behoben, siehe Nachtest unten. Ein Vanilla- oder Proxy-Server ist weiter ungeprüft.
- ~~Der Sperr-Hinweis selbst wurde nicht gesehen~~ → steht jetzt im Log, siehe Nachtest unten (der Hinweis in der Aktionsleiste selbst wurde weiter nicht abgepasst).
- ~~Meridian-Logzeilen erscheinen nicht im Log~~ → [#12129](https://github.com/horizon-rust/horizon/issues/12129) behoben, siehe Nachtest unten.
- Keine Commits: alle Änderungen liegen ungespeichert im Arbeitsbaum von `main`.

Beide Testspiele wurden regulär mit `quit` beendet; LAN wurde vor dem Beenden geschlossen.

## Nachtest mit Horizon-Fixes (2026-11-08)

Beide gemeldeten Horizon-Bugs wurden noch am selben Tag behoben: [#12129](https://github.com/horizon-rust/horizon/issues/12129) mit PR #12265 (`f6dda11`), [#12128](https://github.com/horizon-rust/horizon/issues/12128) mit PR #12321 (`170cfb4`). Keine Nightly enthält sie bisher (neueste: 2026-11-07, `2ebf00a`).

### Änderungen

- Pin auf Horizon `main` `447e9d277d06272196ba66363d5ea8d582581c63` gehoben (`horizon-mixin.toml`, `engine/Cargo.toml`). Die Bridge brauchte keine Anpassung.
- Neue Logzeilen, damit die Spawn- und Sperrentscheidungen im Log nachlesbar sind:
  - `level … loaded, spawn reported …[, waiting for the spawn packet]` beim Laden einer Ebene;
  - `spawn announced at …` bei jedem SetDefaultSpawnPosition-Paket;
  - `showing map X (spawn …)` beim Kartenwechsel (vorher ohne Spawn);
  - Warnung `… meridian.lock is held by another game; nothing there is saved`, wenn die Sperre belegt ist (vorher still).

### Prüfung

- `tools/check-adapter.sh --release`: „adapter OK against Horizon 447e9d2… with its mixins applied“.
- `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --release -- -D warnings`: sauber.
- `cargo test --workspace --release`: 261 Kern, 2 storage_safety, 2 Host, 8 Seed Atlas, Parität (58 s), alles grün.
- Native-Build `native-447e9d2-236224f0c3f6`, für die Instanz `horizon` ausgewählt.
- Spiel: Ordner `/tmp/meridian-followup/` wie oben. Spiel A hostet LAN (Welt LanHost), Spiel B tritt als `LanTest` bei, Spiel C startet auf dem Ordner von A. Logauszüge: [validation/logs-447e9d2-2026-11-08.md](validation/logs-447e9d2-2026-11-08.md).

| Schritt | Erwartet | Log von B |
|---|---|---|
| Host: `/setworldspawn 96 145 -32`, eigener Spawnpunkt `300 120 300`; B tritt bei | Weltspawn, nicht der Spawnpunkt des Hosts | `level … loaded, spawn reported None, waiting for the spawn packet`, dann `spawn announced at … [96, 145, -32]` |
| Host: `/setworldspawn 200 130 50` | Paket an den verbundenen Gast, Kartenwechsel | `spawn announced at … [200, 130, 50]`, `showing map map-2 (spawn Some([200, 130, 50]))` |
| B stirbt und spawnt neu | Spawn bleibt, kein Kartenwechsel | zweimal `spawn announced at … [200, 130, 50]`, kein `showing map` |
| B verlässt und betritt die Welt neu | wieder `map-2` | wartet, dann `[200, 130, 50]`; `/meridian map list`: „Maps: default, map-2 (shown: map-2)“ |
| C startet auf dem Ordner von A | beide Sperren belegt, nichts gespeichert | `…/meridian/meridian.lock is held by another game`, `…/config/meridian.lock is held by another game`, `another game uses …; map changes are not saved` |

Vorher (Pin `66f3981`) kam beim Beitritt der Spawnpunkt des Hosts an, `/setworldspawn` gar nicht, und der Respawn wechselte die Karte auf `map-2`. Das war Horizons Fehler; Meridian folgte nur dem Paket.

Außer `HORIZON_NO_AUDIO` gab es in keinem der drei Logs eine Warnung oder einen Fehler. Alle drei Spiele wurden mit `quit` beendet, LAN vorher geschlossen.

### Was offen bleibt

- Beobachtungen an der Referenz-Mod (D-012, KEYBINDS).
- Spawnwechsel auf einem Vanilla- oder Proxy-Server.
- Der Pin zeigt auf einen Horizon-Stand, den noch keine Nightly enthält. Spieler mit der Nightly vom 07.11. bauen dadurch gegen `main`; sobald eine Nightly mit beiden Fixes erscheint, sollte der Pin auf deren Commit.
- Keine Commits.
