# Standard-Hotkeys

Meridian übernimmt die Standardbelegung der beiden Referenz-Mods 1:1, damit Umsteiger sofort zurechtfinden. Tasten in Minecraft-Notation, wie sie in `options.txt` stehen und wie Horizons Hook `mods` sie erwartet (`key.keyboard.b`).

## Verifikation

- **Quelle O1:** `options.txt` der Prism-Instanz `26.3` (Fabric Loader 0.20.1), geschrieben vom Spiel am 2026-11-06 20:38 nach dem ersten Start mit:
  - `xaerominimap-fabric-26.3-26.6.2.jar`
  - `xaeroworldmap-fabric-26.3-1.47.2.jar`
  - `seedatlas-xaero-2.0.0-mc26.3-fabric.jar` (legt keine Tasten an)
- Gelesen wurden nur die Zeilen `key_gui.xaero_*`, keine Jar-Inhalte.
- Die Zuordnung zur Mod (Minimap oder Weltkarte) ergibt sich aus der Reihenfolge in `options.txt` (die Minimap-Zeilen 177–199, die Weltkarten-Zeilen 200–206) und aus dem Tastennamen. Ein Abgleich mit den Kategorien der Steuerungsliste im Spiel steht noch aus (siehe Ende der Datei).
- Die öffentlichen Modseiten (S1–S4, siehe `CLEAN_ROOM.md`) nennen für Zoom die Tasten **I/O**. In den installierten Versionen sind Zoom rein und raus jedoch **ungebunden**. Maßgeblich ist O1. Meridian übernimmt deshalb „ungebunden“.

## Minimap

| Meridian-ID | Funktion | Standard | Referenz-Schlüssel (O1) |
|---|---|---|---|
| `meridian.minimap.zoom_in` | Minimap hineinzoomen | ungebunden | `gui.xaero_zoom_in` |
| `meridian.minimap.zoom_out` | Minimap herauszoomen | ungebunden | `gui.xaero_zoom_out` |
| `meridian.waypoint.new` | Neuer Wegpunkt (Editor öffnet sich an der Spielerposition) | **B** `key.keyboard.b` | `gui.xaero_new_waypoint` |
| `meridian.waypoint.menu` | Wegpunktliste | **U** `key.keyboard.u` | `gui.xaero_waypoints_key` |
| `meridian.minimap.enlarge` | Minimap vergrößern (halten oder umschalten, je nach Einstellung) | **Z** `key.keyboard.z` | `gui.xaero_enlarge_map` |
| `meridian.minimap.toggle` | Minimap ein/aus | ungebunden | `gui.xaero_toggle_map` |
| `meridian.waypoint.toggle_world` | Wegpunkte in der Welt ein/aus | ungebunden | `gui.xaero_toggle_waypoints` |
| `meridian.waypoint.toggle_minimap` | Wegpunkte auf der Minimap ein/aus | ungebunden | `gui.xaero_toggle_map_waypoints` |
| `meridian.overlay.slime` | Schleimchunks ein/aus | ungebunden | `gui.xaero_toggle_slime` |
| `meridian.overlay.grid` | Chunkgitter ein/aus | ungebunden | `gui.xaero_toggle_grid` |
| `meridian.waypoint.instant` | Sofort-Wegpunkt ohne Editor | **Num +** `key.keyboard.keypad.add` | `gui.xaero_instant_waypoint` |
| `meridian.waypoint.next_set` | Wegpunkt-Set wechseln | ungebunden | `gui.xaero_switch_waypoint_set` |
| `meridian.waypoint.all_sets` | Alle Sets anzeigen ein/aus | ungebunden | `gui.xaero_display_all_sets` |
| `meridian.overlay.light` | Licht-Overlay ein/aus | ungebunden | `gui.xaero_toggle_light_overlay` |
| `meridian.radar.toggle` | Entity-Radar ein/aus | ungebunden | `gui.xaero_toggle_entity_radar` |
| `meridian.radar.reverse` | Radar-Filter umkehren | ungebunden | `gui.xaero_reverse_entity_radar` |
| `meridian.radar.faces` | Gesichter von Mobs, Tieren und Spielern auf Minimap und Weltkarte, solange gehalten | **Tab** `key.keyboard.tab` | — (eigene Taste; Standard wie die Spielerliste) |
| `meridian.cave.toggle_manual` | Manuellen Höhlenmodus umschalten | ungebunden | `gui.xaero_toggle_manual_cave_mode` |
| `meridian.players.alt_list` | Alternative Spielerliste | ungebunden | `gui.xaero_alternative_list_players` |
| `meridian.players.tracked_map` | Verfolgte Spieler auf der Karte ein/aus | ungebunden | `gui.xaero_toggle_tracked_players_on_map` |
| `meridian.players.tracked_world` | Verfolgte Spieler in der Welt ein/aus | ungebunden | `gui.xaero_toggle_tracked_players_in_world` |
| `meridian.overlay.claims` | Claims eines Claim-Mods ein/aus (nur mit Integration, siehe FEATURES OV-06) | ungebunden | `gui.xaero_toggle_pac_chunk_claims` |
| `meridian.settings` | Einstellungen Minimap | **Y** `key.keyboard.y` | `gui.xaero_minimap_settings` |
| `meridian.server_settings` | Server-Einstellungen (Profil erzwingen) | ungebunden | `gui.xaero_minimap_server_profiles` |

## Weltkarte

| Meridian-ID | Funktion | Standard | Referenz-Schlüssel (O1) |
|---|---|---|---|
| `meridian.worldmap.open` | Weltkarte öffnen/schließen | **M** `key.keyboard.m` | `gui.xaero_open_map` |
| `meridian.worldmap.settings` | Einstellungen Weltkarte | **]** `key.keyboard.right.bracket` | `gui.xaero_open_settings` |
| `meridian.worldmap.server_settings` | Server-Einstellungen Weltkarte | ungebunden | `gui.xaero_world_map_server_settings` |
| `meridian.worldmap.zoom_in` | Karte hineinzoomen (Tastatur-Alternative zum Mausrad) | ungebunden | `gui.xaero_map_zoom_in` |
| `meridian.worldmap.zoom_out` | Karte herauszoomen | ungebunden | `gui.xaero_map_zoom_out` |
| `meridian.quick_confirm` | Schnellbestätigung in Dialogen (z. B. Kartenauswahl) | **Rechte Umschalttaste** `key.keyboard.right.shift` | `gui.xaero_quick_confirm` |
| `meridian.worldmap.toggle_dimension` | Dimension in der Karte wechseln | ungebunden | `gui.xaero_toggle_dimension` |

## Seed Atlas

Keine Tasten (O1: `seedatlas-xaero` legt keine `key_`-Zeilen an). Bedienung über `/seedatlas …` und das Einstellungsmenü.

## Maus und feste Tasten in Screens

Diese Bedienung steht nicht in der Steuerungsliste. Laut S3/S4 zeigt die Vollbildkarte sie in einem eigenen Tooltip an.

| Ort | Bedienung | Quelle | Status |
|---|---|---|---|
| Weltkarte | Ziehen mit linker Maustaste verschiebt die Karte | S3, S4 | übernommen |
| Weltkarte | Mausrad zoomt | S3, S4 | übernommen |
| Weltkarte | Rechtsklick öffnet ein Kontextmenü (Wegpunkt hier, Teleport, auf Wegpunkt/Spieler: bearbeiten, teleportieren …) | S3, S4 | übernommen; die genauen Einträge sind noch im Spiel zu beobachten |
| Weltkarte | Symbol unten links: Kartenauswahl und Höhlenmodus; Symbol unten rechts: Wegpunktmenü | S3, S4 | übernommen |
| Weltkarte | Tasten der Steuerungsliste wirken auch in der Karte (z. B. **B** setzt den Wegpunkt am Cursor, **M** schließt) | — | **im Spiel zu beobachten** |
| Wegpunktliste | Tasten im Menü | — | **im Spiel zu beobachten** |

## Offene Beobachtungen im Spiel

Zu klären vor Phase 4 (in `PROGRESS.md` verfolgt):

1. Kategorien der Steuerungsliste (Minimap oder Weltkarte) gegen die Zuordnung oben abgleichen.
2. Den Steuerungs-Tooltip der Vollbildkarte vollständig abschreiben (Text in eigenen Worten).
3. Die Einträge im Rechtsklickmenü der Karte (auf leerer Fläche, auf Wegpunkt, auf Spieler).
4. Tasten in der Wegpunktliste und im Wegpunkt-Editor.
5. Ob **Z** gehalten oder umgeschaltet wird (Einstellung `minimap_toggled_enlarged` deutet auf beides hin).
