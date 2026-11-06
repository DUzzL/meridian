# Feature-Inventar

Alle Features von Minimap- und Weltkarten-Referenz, die sich aus erlaubten Quellen ableiten lassen. Meridian muss sie gleichwertig abdecken; die Optik darf abweichen.

**Quellen** (Details in `CLEAN_ROOM.md`):

- S1/S2: öffentliche Modseite Minimap (Modrinth/CurseForge)
- S3/S4: öffentliche Modseite Weltkarte
- O1: `options.txt` der Prism-Instanz
- O2: vom Spiel erzeugte Konfigdateien unter `config/xaero/` (Einstellungsnamen und Standardwerte)
- O3: Beobachtung im Spiel (offen)
- T: `TASK.md`

**Priorität:**

- **P0** Kern; ohne das kein Umstieg.
- **P1** gehört zur Parität, kann nach P0 kommen.
- **P2** Komfort oder seltene Optionen.
- **P3** Integration mit Mods, die es in Horizon (noch) nicht gibt; nur Schnittstelle vorsehen.

Die Spalte **API** nennt die Horizon-Lücke aus `HORIZON_API.md` (E1–E6), die das Feature braucht.

## MM: Minimap-Anzeige

| ID | Feature | Prio | Akzeptanzkriterium | Quelle | API |
|---|---|---|---|---|---|
| MM-01 | Minimap im HUD, ein/ausschaltbar (auch per Taste) | P0 | Taste schaltet sichtbar um; Zustand bleibt nach Neustart | S1, O2 `display_minimap`, O1 | – |
| MM-02 | Formen: quadratisch und rund | P0 | Beide Formen ohne Artefakte am Rand; runde Form schneidet sauber kreisförmig | S1, O2 `minimap_shape` | E3 |
| MM-03 | Größen inkl. „automatisch“ (an GUI-Skala/Fenster angepasst) | P0 | Mindestens 4 Stufen + Auto; Auto skaliert bei Fenstergrößenwechsel | S1, O2 `minimap_size` | – |
| MM-04 | Position frei wählbar (pixelgenau verschiebbar, Positionseditor) | P1 | Editor-Screen; Position wird pro Spieler gespeichert | S1 | – |
| MM-05 | Zoomstufen, Zoom per Taste; „weiter herauszoomen“-Option | P0 | ≥ 5 Stufen; mit `further_zoomout` zusätzliche Stufen | S1, O2 `minimap_zoom`, `minimap_further_zoomout` | – |
| MM-06 | Rotation mit Spieler (Standard) oder Nord-Lock mit Pfeil | P0 | Standard dreht; Nord-Lock zeigt Spieler als Pfeil, Karte fest | S1, O2 `minimap_north_locked` | E3 |
| MM-07 | Vergrößern (Z): halten oder umschalten, zentriert optional, eigene Zoomstufe | P0 | Beide Modi; Option „zentriert“ und „vergrößert entsperrt“ | S1, O1, O2 `minimap_enlarged_zoom`, `minimap_centered_when_enlarged`, `keep_enlarged_minimap_unlocked`, `minimap_toggled_enlarged` | – |
| MM-08 | Blockfarben: „Vanilla“ (Kartenfarben) und „genau“ (Textur- und Biomfarben); Biomfarben auch im Vanilla-Modus | P0 | Umschaltbar; „genau“ nutzt die Texturen des aktiven Ressourcenpakets | S1, O2 `minimap_block_colors`, `minimap_biomes_in_vanilla_colors` | E2 |
| MM-09 | Geländetiefe und Hangschattierung (mehrere Stufen) | P0 | Höhenunterschiede sichtbar; 3 Hangstufen | S1, O2 `minimap_terrain_depth`, `minimap_terrain_slopes` | E2 |
| MM-10 | Biom-Überblendung | P1 | Weiche Biomübergänge bei Gras/Laub/Wasser | O2 `minimap_biome_blending` | E2 |
| MM-11 | Beleuchtung (Tag/Nacht, Blocklicht) | P0 | Nachts dunkler, Fackeln hell; abschaltbar | O2 `minimap_lighting`, T | E2 |
| MM-12 | Darstellungsdetails: Redstone, Blumen, Blocktransparenz, gefärbtes Glas, Höhenanpassung kurzer Blöcke | P1 | Je Option sichtbarer Unterschied | O2 `minimap_display_redstone`, `_flowers`, `_block_transparency`, `_stained_glass`, `_adjust_height_for_short_blocks` | E2 |
| MM-13 | Kantenglättung | P2 | Rotierte Karte ohne Treppchen | O2 `minimap_anti_aliasing` | E3 |
| MM-14 | Deckkraft, Rahmen (Stil, Farbe), UI-Skala, Pfeilgröße/-farbe/-deckkraft | P1 | Einstellbar, sofort wirksam | O2 `minimap_opacity`, `minimap_frame`, `_frame_color`, `minimap_ui_scale`, `minimap_arrow_*` | – |
| MM-15 | Kompass (N/O/S/W) am Rand, Position, Skala, Schattenfarben, „über allem“ | P1 | Himmelsrichtungen drehen korrekt mit | O2 `minimap_compass_*` | – |
| MM-16 | Ausblenden unter Screens und unter F3 | P0 | Standard: verborgen, wenn ein Screen offen ist oder F3 aktiv | O2 `hide_minimap_under_screen`, `hide_minimap_under_f3` | – |
| MM-17 | Bossleiste/Effekte verschieben, wenn Minimap im Weg | P2 | Keine Überlappung mit Boss-HP und Effektsymbolen | O2 `boss_health_push_box`, `potion_effect_push_box` | – |
| MM-18 | Unerkundete Bereiche mit eigener Deckkraft | P1 | Noch nicht geladene Chunks als halbtransparente Fläche | O2 `minimap_undiscovered_opacity` | – |
| MM-19 | Minimap nutzt die Texturen der Weltkarte (keine Doppelarbeit) | P0 | Ein gemeinsamer Tile-Cache; CPU-Zeit wird nicht doppelt verbraucht | S1, O2 `minimap_display_world_map_chunks` | – |
| MM-20 | Sicherer Modus (Kompatibilität, reduziertes Rendering) | P2 | Option schaltet aufwendige Effekte ab | O2 `minimap_safe_mode` | – |
| MM-21 | Minimap an Gegenstand koppeln (z. B. Kompass im Inventar) | P2 | Ohne Gegenstand keine Minimap | S1, O2 `minimap_item` | – |
| MM-22 | Render-Ebene des HUD wählbar | P2 | Minimap über/unter anderen HUD-Elementen | O2 `hud_render_layer` | – |

## CV: Höhlenmodus

| ID | Feature | Prio | Akzeptanzkriterium | Quelle | API |
|---|---|---|---|---|---|
| CV-01 | Automatischer Höhlenmodus unter festem Dach; Dachgröße wählbar (z. B. ≥3×3, um Baumfällen nicht auszulösen) | P0 | Unter Tage schaltet um; unter einzelnen Stämmen nicht; Umschaltverzögerung | S1, O2 `minimap_auto_cave_mode`, `minimap_cave_mode_toggle_timer` | E2 |
| CV-02 | Manueller Höhlenmodus per Taste mit Start-Y | P1 | Taste umschalten; Start-Y einstellbar | O1, O2 `minimap_manual_cave_mode_start` | E2 |
| CV-03 | Höhlentiefe, lesbare Höhlenkarten, Extra-Zoom im Höhlenmodus | P1 | Einstellungen wirken sichtbar | O2 `minimap_cave_mode_depth`, `minimap_legible_cave_maps`, `minimap_cave_mode_extra_zoom` | E2 |
| CV-04 | Höhlenmodus pro Dimension erlauben/verbieten | P1 | Liste erlaubter Dimensionen | O2 `minimap_cave_mode_allowed(_dimensions)` | – |
| CV-05 | Höhlenkarten in der Weltkarte (Höhlenmodus-Typ aus/voll, Top-Y, Standard-Typ, Anzeige Start-Y, Ober-/Unterseite des Netherdachs) | P1 | Höhlenmenü über Symbol unten links; Einstellung pro Dimension | S3, O2 Weltkarte `cave_mode_*`, `default_cave_mode_type`, `display_cave_mode_start` | E2 |

## INFO: Infoanzeigen unter der Minimap

Die Reihenfolge ist frei sortierbar. Jede Zeile hat Farbe und Sichtbarkeit (O2 `info_display_config`).

| ID | Feature | Prio | Akzeptanzkriterium | Quelle | API |
|---|---|---|---|---|---|
| INFO-01 | Koordinaten (Standard an) | P0 | X Y Z aktuell | S1, O2 | – |
| INFO-02 | Overworld-Koordinaten (im Nether ×8) | P1 | Im Nether korrekt umgerechnet | O2 `overworld_coords` | – |
| INFO-03 | Chunk-Koordinaten | P2 | Chunk und Position im Chunk | O2 `chunk_coords` | – |
| INFO-04 | Blickwinkel | P2 | Yaw/Pitch | S1, O2 `angles` | – |
| INFO-05 | Dimension | P2 | Name der Dimension | O2 `dimension` | – |
| INFO-06 | Biom | P0 | Biomname an der Spielerposition | S1, O2 `biome` | E5 |
| INFO-07 | Wetter | P2 | Klar/Regen/Gewitter | O2 `weather` | – |
| INFO-08 | Lichtlevel (Modi) | P1 | Block- und/oder Himmelslicht | S1, O2 `light_level` | – |
| INFO-09 | Spielzeit (12/24 h) und Echtzeit | P1 | Uhrzeit korrekt | S1, O2 `time`, `real_time` | – |
| INFO-10 | Hervorhebungen, Licht-Overlay-Hinweis, Höhlenmodus-Hinweis, Unterwelt-Name | P1 | Anzeigen erscheinen nur, wenn zutreffend | O2 `highlights`, `light_overlay_indicator`, `manual_cave_mode_indicator`, `custom_sub_world` | – |
| INFO-11 | Hintergrund-Deckkraft und Ausrichtung | P2 | Einstellbar | O2 `minimap_info_display_background_opacity`, `_alignment` | – |
| INFO-12 | Datenschutz: Wegpunktkoordinaten und Serveradressen ausblenden (ganz/teilweise) | P1 | Für Streams: keine Koordinaten bzw. gekürzte Adressen in allen Screens | S1, O2 `hide_waypoint_coordinates`, `hide_world_names` | – |

## OV: Overlays

| ID | Feature | Prio | Akzeptanzkriterium | Quelle | API |
|---|---|---|---|---|---|
| OV-01 | Chunkgitter (Linienbreite, Farbe) | P1 | Linien exakt an Chunkgrenzen | S1, O1, O2 `minimap_chunk_grid*` | – |
| OV-02 | Schleimchunks (nur mit bekanntem Seed; eigener Screen zur Seed-Eingabe im MP) | P1 | Grün markiert, identisch zu Vanilla-Berechnung | S1, O1, O2 `minimap_slime_chunks`, `_open_slime_chunks_screen` | – (Seed Atlas) |
| OV-03 | Licht-Overlay: Blöcke mit Licht im Bereich min–max (Standard 0–7), Farbe, Typ | P1 | Spawnbare Flächen markiert | S1, O1, O2 `minimap_light_overlay_*` | E2 |
| OV-04 | Fußspuren / zuletzt gelaufener Weg | P1 | Spur auf der Weltkarte | S3, O2 Weltkarte `footsteps` | – |
| OV-05 | Leitlinie zum verfolgten Wegpunkt | P1 | Linie/Pfeil zeigt Richtung zum Ziel | T | – |
| OV-06 | Claims anderer Mods (Füllung/Rand, eigener Claim) | P3 | Schnittstelle für Claim-Anbieter; Standard-Taste wie O1 | S1, O1, O2 `minimap_display_opac_*` | – |

## RAD: Entity-Radar

| ID | Feature | Prio | Akzeptanzkriterium | Quelle | API |
|---|---|---|---|---|---|
| RAD-01 | Radar ein/aus (auch per Taste), Filter umkehren | P0 | Taste schaltet; Umkehren invertiert Filter | S1, O1, O2 `display_radar` | E4 |
| RAD-02 | Kategoriebaum (Wurzel › Lebewesen › Spieler/feindlich/friedlich …, Items, Sonstige) mit vererbbaren Einstellungen | P0 | Jede Kategorie: anzeigen, Farbe, Punktgröße, Symbol, Namen, Höhenlimit, Höhenausblendung, Max-Anzahl, Render-Reihenfolge | S1, O2 `default_radar_categories_*` | E4 |
| RAD-03 | Darstellung: Punkte, Symbole, Köpfe; Fallback auf Punkt | P0 | Spieler als Kopf; Mobs mit eigenen Symbolen (selbst erstellt, keine Fremd-Assets) | S1, O2 `radar_dots_style` | E4 |
| RAD-04 | Standardfarben: Mobs gelb, Spieler weiß, Items rot, Sonstige lila; feindlich/friedlich getrennt | P0 | Farben wie beschrieben | S1 | E4 |
| RAD-05 | Namen anzeigen (Skala, immer/nie) | P1 | Namen unter Symbolen | S1, O2 `radar_name_scale` | E4 |
| RAD-06 | Unsichtbare ausblenden, weiche Bewegung | P1 | Standard: unsichtbare nicht zeigen; Punkte interpoliert | O2 `radar_hide_invisible_entities`, `radar_smooth_dots` | E4 |
| RAD-07 | Benutzerdefinierte Kategorien (Ein-/Ausschlusslisten) | P1 | Eigene Kategorie mit Entity-IDs | S1, O2 | E4 |
| RAD-08 | Symbole per Ressourcenpaket überschreibbar | P2 | Eigener Asset-Pfad `assets/meridian/…` | S1 | – |
| RAD-09 | Spielerverfolgung auf Karte/in der Welt (Symbol- und Namensgröße) | P1 | Verfolgte Spieler auch außerhalb der Sichtweite, sofern bekannt | O1, O2 `tracked_players_*` | E4 |

## WP: Wegpunkte

| ID | Feature | Prio | Akzeptanzkriterium | Quelle | API |
|---|---|---|---|---|---|
| WP-01 | Erstellen (B) mit Editor: Name, Kürzel (1–2 Zeichen), Farbe, Symbol, Koordinaten, Set, global/lokal, deaktiviert | P0 | Alle Felder; Standard = Spielerposition | S1, O1, T | – |
| WP-02 | Sofort-Wegpunkt (Num +) | P0 | Ohne Dialog, automatischer Name | O1 | – |
| WP-03 | Liste (U): sortieren, Mehrfachauswahl, bearbeiten, löschen, aktivieren | P0 | Sortierung nach Name/Distanz/Erstellung/Kürzel | S1, O1 | – |
| WP-04 | Farben (Palette) und Symbole | P0 | ≥ 16 Farben; Kürzel oder Symbol | T | – |
| WP-05 | Sets: anlegen, wechseln (Taste), alle anzeigen | P0 | Set-Wechsel per Taste; „alle Sets“ umschaltbar | S1, O1, O2 `waypoints_all_sets` | – |
| WP-06 | Global vs. lokal; maximale Zeichendistanz (mit Dimensionsskalierung) | P1 | Lokale verschwinden jenseits der Distanz | S1, O2 `waypoint_max_distance*` | – |
| WP-07 | Pro Welt/Dimension/Server bzw. Unterwelt gespeichert | P0 | Getrennte Speicherung; Wechsel lädt richtige Liste | S1, T | – |
| WP-08 | Todespunkte: automatisch, alte behalten, beim Erreichen löschen, Wegpunkt-Set nach Tod wechseln | P0 | Bei Tod entsteht Totenkopf-Wegpunkt; Optionen wirken | S1, O2 `deathpoints`, `old_deathpoints`, `delete_reached_deathpoints`, `switch_auto_waypoints_on_death` | E6* |
| WP-09 | Temporäre Wegpunkte (z. B. aus Chat, Kartenklick), global oder lokal | P1 | Verschwinden bei Weltwechsel | O2 `temporary_waypoints_global`, T | – |
| WP-10 | Teilen im Chat und Übernehmen geteilter Wegpunkte | P0 | Senden über `send_chat`; empfangene Nachricht ist anklickbar und erzeugt Wegpunkt | S1, T | – |
| WP-11 | Teleport per Befehl (Format global und pro Server, mit/ohne Rotation, dimensionsübergreifend, Teil-Y) | P0 | Standard `/tp @s {x} {y} {z}`; Button deaktiviert, wenn nicht erlaubt | S1, O2 `default_waypoint_teleport_*`, `waypoint_teleport_cross_dimension`, `waypoint_partial_y_teleport` | – |
| WP-12 | Anzeige in der Welt: Symbol, Name, Distanz (km ab Schwelle, Genauigkeit), Skalen, Deckkraft, Blickwinkel-Hervorhebung | P0 | Durch Wände sichtbar, Größe distanzunabhängig | S1, O2 `waypoint_*_in_world`, `waypoint_*_pointing_angle`, `waypoint_convert_distance_to_km_at` | – |
| WP-13 | Beam (Leuchtfeuer-Strahl) am Wegpunkt | P1 | Abschaltbar, Farbe = Wegpunktfarbe | T | – |
| WP-14 | Anzeige auf Minimap (Deckkraft, Symbolgröße), am Rand geklemmt | P0 | Wegpunkte außerhalb zeigen am Rand Richtung | O2 `waypoints_on_minimap`, `waypoint_opacity_on_minimap` | – |
| WP-15 | Neue Wegpunkte oben/unten einsortieren | P2 | Option wirkt | O2 `new_waypoints_to_bottom` | – |
| WP-16 | Weltkarte: Wegpunktmenü (Filter, deaktivierte zeigen, alle Sets, Set wechseln), Hover hebt hervor | P0 | Symbol unten rechts öffnet Liste | S3, S4 | – |
| WP-17 | Gemeinsames Bearbeiten derselben Wegpunkte in Minimap und Weltkarte | P0 | Eine Datenquelle | O2 `waypoint_mutual_edit` | – |

## WM: Weltkarte

| ID | Feature | Prio | Akzeptanzkriterium | Quelle | API |
|---|---|---|---|---|---|
| WM-01 | Vollbildkarte (M), Öffnungsanimation | P0 | Öffnet < 100 ms bei 300 geladenen Regionen | S3, O1, O2 `opening_animation` | E3 |
| WM-02 | Zoom (Mausrad, Tasten), Pan (Ziehen), unbegrenzter Zoom optional | P0 | Flüssig, 60 fps, keine Lücken beim Pan | S3, O2 `unlimited_zoom_*`, `zoom_buttons` | E3 |
| WM-03 | Speichern des Erkundeten (regionsweise, inkrementell), neue Chunks laden/aktualisieren abschaltbar | P0 | Nach Neustart identisch; Schreiben im Hintergrund | S3, O2 `load_new_chunks`, `update_chunks` | E1, E2 |
| WM-04 | Cache mit Limit geladener Regionen; Neu-Laden betrachteter Regionen | P0 | Speicher begrenzt (Standard 300 Regionen) | O2 `max_loaded_regions`, `reload_viewed` | – |
| WM-05 | Singleplayer: Karte aus Weltspeicher erzeugen (vorgenerierte Welt) | P1 | Bereits generierte Regionen erscheinen ohne Erkunden | S3 | E2 |
| WM-06 | Weltspeicher in Mehrspieler-Karteninstanz konvertieren | P2 | Konvertierung erzeugt Instanz | S3 | – |
| WM-07 | Dimensionen, Wechsel in der Karte, Nether-Skalierung (1:8) für Koordinaten/Wegpunkte | P0 | Overworld-Wegpunkte im Nether richtig skaliert | S3, O1, T | – |
| WM-08 | Rechtsklickmenü: Wegpunkt hier, Teleport hierher, Koordinaten; auf Wegpunkt: bearbeiten, teleportieren, löschen; auf Spieler: teleportieren | P0 | Alle Einträge funktionieren | S3, S4, O3 | – |
| WM-09 | Teleportbefehle: Format, mit Dimension, Spieler-Teleport, Teil-Y, mehrdeutiges Y erkennen | P1 | Standardformate wie O2 | O2 `default_map_teleport_*`, `default_player_teleport_command_format`, `detect_ambiguous_y` | – |
| WM-10 | Koordinaten unter Cursor, Zoomanzeige, Biom unter Cursor | P0 | Live beim Bewegen | O2 `display_coordinates`, `display_zoom`, `display_hovered_biome` | E5 |
| WM-11 | Spieler als Pfeil (Farbe), Radar-Entities und verfolgte Spieler auf der Karte | P1 | Optionen wirken | O2 `display_player_as_arrow`, `arrow_color`, `display_minimap_radar`, `display_tracked_players` | E4 |
| WM-12 | Wegpunkte auf der Karte (Hintergrund, Skala, Mindestzoom lokale, deaktivierte zeigen, nur aktuelle Karte) | P0 | Optionen wirken | O2 `waypoints`, `render_waypoints`, `waypoint_backgrounds`, `waypoint_scale`, `minimum_zoom_for_local_waypoints` | – |
| WM-13 | Distanzen messen/anzeigen (km ab Schwelle) | P2 | Distanz zum Cursor | O2 `display_map_distances` | – |
| WM-14 | Vanilla-Kartenstil optional | P2 | Papierkarten-Look | S3 | – |
| WM-15 | Ressourcenpaket-Wechsel regeneriert Farben | P1 | Nach Wechsel neue Farben | S3 | E2 |
| WM-16 | PNG-Export (ganze Karte oder geteilt, Nacht, Verkleinerung, Hervorhebungen) | P1 | Exportiert PNG ohne Speicherexplosion | S3, O2 `png_export_*` | – |
| WM-17 | Server-Chunk-Radius anzeigen | P2 | Rahmen um geladene Server-Chunks | O2 `display_server_chunk_radius` | E1 |
| WM-18 | Schnelles Kartenschreiben erzwingen, Schreibdistanz | P2 | Option wirkt | O2 `force_fast_map_writing`, `map_writing_distance` | – |
| WM-19 | Steuerungs-Tooltip in der Karte | P1 | Listet alle Bedienungen | S3 | – |
| WM-20 | Dateisperren / Mehrfachinstanzen | P2 | Zwei Spiele zerstören keine Karte | O2 `ignore_map_file_locks` | – |

## CTX: Multiplayer-Kontexte

| ID | Feature | Prio | Akzeptanzkriterium | Quelle | API |
|---|---|---|---|---|---|
| CTX-01 | Server-Erkennung über Adresse (abschaltbar) | P0 | Gleiche Adresse → gleiche Daten | S1, O2 `differentiate_by_server_address` | – |
| CTX-02 | Mehrere Karten/Unterwelten pro Adresse; Kartenauswahl-Modi (manuell, Weltspawn, einzeln, Server) | P0 | Lobbies überschreiben Karten nicht | S3, S4 | – |
| CTX-03 | Bestätigungsdialog im manuellen Modus (Schnellbestätigung per rechter Umschalttaste) | P1 | Dialog bei Dimensions-/Weltwechsel | S3, O1 | – |
| CTX-04 | Unterwelt „auto“ setzen; Welt/Server auto setzen; Daten zwischen Unterwelten verschieben | P1 | Optionen im Wegpunktmenü | S1 | – |
| CTX-05 | Wiederherstellung bei geänderter Adresse/Dimension-ID | P2 | Alte Daten auswählbar | S1, S3 | – |

## SET: Einstellungen

| ID | Feature | Prio | Akzeptanzkriterium | Quelle | API |
|---|---|---|---|---|---|
| SET-01 | Einstellungsmenüs Minimap (Y) und Weltkarte (]) | P0 | Alle Optionen dieser Liste erreichbar | S1, O1 | – |
| SET-02 | Profile (benannte Einstellungssätze, wechselbar) | P1 | Profil anlegen, wechseln | O2 `profiles/`, `current_profile` | – |
| SET-03 | Server-erzwungene Profile, Ignorieren bei Bearbeitungsrecht | P2 | Server kann Optionen sperren | S1, S3, O2 `server_profiles/`, `ignore_enforcement_if_edit_permission` | – |
| SET-04 | Update-Hinweise | P3 | Entfällt (Horizon-Registry übernimmt) | O2 | – |

## IO: Import/Export

| ID | Feature | Prio | Akzeptanzkriterium | Quelle | API |
|---|---|---|---|---|---|
| IO-01 | Wegpunkte exportieren/importieren (eigenes JSON-Format) | P1 | Rundreise verlustfrei | T | – |
| IO-02 | Import von Wegpunkten aus der Referenz-Mod (öffentlich dokumentiertes Textformat der Wegpunktdateien, nur Datenformat) | P2 | Umsteiger übernehmen Wegpunkte | T | – |
| IO-03 | PNG-Export der Karte | P1 | siehe WM-16 | S3 | – |
| IO-04 | Einstellungen exportieren/importieren | P2 | Profil als Datei | T | – |

## SRV: Server-Regeln

| ID | Feature | Prio | Akzeptanzkriterium | Quelle | API |
|---|---|---|---|---|---|
| SRV-01 | Server kann Minimap, Radar, Wegpunkte, Höhlenkarten, Weltkarte sperren (eigener Mechanismus über Mod-Nachrichten/Statuseffekte) | P2 | Gesperrte Features verschwinden clientseitig | S1, S3 | – |
| SRV-02 | Fair-Play-Modus (kein Radar, keine Höhlenkarten) | P2 | Option bzw. Serversignal | S1 | – |
| SRV-03 | Kompatible Erkennung bestehender Fair-Play-Chatsignale von Servern | P3 | Server mit solchen Signalen werden respektiert | S1 | – |

## SA: Seed Atlas (standardmäßig an, abschaltbar)

| ID | Feature | Prio | Akzeptanzkriterium | Quelle | API |
|---|---|---|---|---|---|
| SA-01 | Biom-Layer aus Seed (Weltkarte, optional Minimap), Deckkraft, Auflösung, Terrainstil, Erkundetes überlagern | P0 | Biome identisch zur Fabric-Version (Paritätstests) | Seed-Atlas-Konfig, T | – |
| SA-02 | Strukturmarker (alle Typen der Fabric-Version), Größe, Filter pro Typ, erledigte Strukturen | P0 | Positionen identisch (Paritätstests) | Seed-Atlas-Konfig, T | – |
| SA-03 | Befehle `/seedatlas seed|clear|status|settings` | P0 | Gleiches Verhalten wie Fabric | T | – |
| SA-04 | Auto-Erkennung des Seeds im Singleplayer | P0 | Ohne Eingabe aktiv | T | E6 |
| SA-05 | Seed pro Server/Karten-Kontext, verlässt nie den Client | P0 | Kein Netzwerkverkehr mit Seed; Speicherung lokal | T | – |
| SA-06 | Normal und Large Biomes | P0 | Beide Welttypen korrekt | T | – |
| SA-07 | Biom-Hervorhebung, Wegpunkt aus Struktur erzeugen | P1 | wie Fabric-Version | Seed-Atlas-Konfig | – |
| SA-08 | Performance: Worker-Threads, Prefetch-Radius | P1 | UI ruckelt nicht | Seed-Atlas-Konfig | – |

## Bewusst nicht übernommen

- Inhalte und Assets der Referenz-Mods (Symbole, Texturen, Sprachdateien).
- Update-Prüfung gegen deren Server.
- Herstellerspezifische Signaturen und Namen. Statuseffekte und Befehle von Meridian tragen den Namensraum `meridian:`.
