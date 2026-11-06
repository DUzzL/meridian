# Aufgabe: Map-Mod für Horizon + Port von Seed Atlas

## Kontext
- **Horizon** (`horizon-rust/horizon`) ist eine Portierung von Minecraft Java nach Rust. Lokal installiert, darf genutzt werden.
- **Seed Atlas** (`https://github.com/DUzzL/seed-atlas-xaero`) ist meine bestehende Fabric-Mod (MC 26.2, client-seitig). Sie rendert aus dem Weltseed deterministisch Biome und Strukturmarker in Xaero's World Map. Befehle: `/seedatlas seed <zahl-oder-text>`, `/seedatlas clear`, `/seedatlas status`, `/seedatlas settings`. Singleplayer-Seed wird automatisch erkannt, Multiplayer-Seeds pro Server/Map-Kontext gespeichert, der Seed verlässt nie den Client. Unterstützt nur Vanilla Normal + Large Biomes.
- **Prism Launcher** mit einer Fabric-Instanz ist installiert und darf als Referenz genutzt werden (Original-Verhalten beobachten, Seed-Atlas-Ausgaben vergleichen).
- System: Windows-PC.

## Ziel
Ein **eigenes neues Repo** (nicht im Horizon-Fork), das enthält:
1. **Eine Map-Mod für Horizon**, die alle Features von *Xaero's Minimap* und *Xaero's World Map* abdeckt. Optik darf abweichen, **Qualität, Funktionsumfang und Standard-Hotkeys müssen gleichwertig sein**, damit Umsteiger sich sofort zurechtfinden.
2. **Seed Atlas, standardmäßig integriert** (kein separates Addon, per Einstellung abschaltbar).

## Harte Regeln: Clean Room
- **Kein dekompilierter, disassemblierter oder aus Jars extrahierter Code von Xaero.** Keine Xaero-`.class`/`.jar`-Inhalte öffnen, keine Assets (Texturen, Icons, Sounds, Sprachdateien) übernehmen.
- Erlaubte Quellen für Xaero-Verhalten: öffentliche Mod-Seiten (Modrinth/CurseForge), Wiki, Changelogs, Videos, und **Beobachtung im laufenden Spiel** (Einstellungsmenüs, Keybind-Liste, `options.txt` der Prism-Instanz).
- Dokumentiere in `docs/CLEAN_ROOM.md`, aus welchen Quellen jedes Feature abgeleitet wurde.
- Kein Hinweis auf Zugehörigkeit zu Xaero; Name der Mod eigenständig (Vorschlag machen).
- Seed Atlas ist mein eigener Code und darf frei portiert werden.

## Git-Regeln
- Trage dich **niemals** als Co-Autor/Contributor ein: keine `Co-Authored-By`-Zeilen, keine Claude-Hinweise in Commits, PRs oder README.
- Kleine, sinnvolle Commits pro Meilenstein.

## Vorgehen in Phasen
Arbeite Phase für Phase. Jede Phase endet mit: grünem Build, bestandenen Tests, kurzem Statusbericht in `docs/PROGRESS.md` (was fertig ist, was offen ist, bekannte Probleme). Dann weiter zur nächsten Phase.

### Phase 0: Recherche Horizon
- Untersuche Horizon gründlich: Gibt es eine **Client-Mod-/Plugin-API** (Rendering, GUI/Screens, HUD-Overlays, Keybinds, Chat-Befehle, Weltdaten/Chunk-Zugriff, Persistenz)?
- Ergebnis in `docs/HORIZON_API.md`: was existiert, was fehlt, wie Mods geladen werden.
- **Falls nötige APIs fehlen:** Plane minimale, saubere Erweiterungen als separaten Vorschlag (für spätere PRs an Horizon), dokumentiere sie, und baue die Mod gegen eine klar abgegrenzte Schnittstelle, damit sie später ohne Umbau läuft. Ist Horizon (noch) rein serverseitig, stopp hier und berichte, bevor du weitermachst.

### Phase 1: Feature- und Hotkey-Inventar
- Erstelle `docs/FEATURES.md`: vollständige Liste aller Features von Xaero's Minimap und World Map (aus erlaubten Quellen), jeweils mit Priorität und Akzeptanzkriterium.
- Erstelle `docs/KEYBINDS.md`: alle **Standard-Hotkeys** beider Mods, verifiziert über die Keybind-Liste bzw. `options.txt` in der Prism-Instanz. Übernimm dieselben Defaults.
- Mindestens abdecken: Minimap (Formen, Größen, Zoom, Rotation, Nord-Lock, Höhen-/Höhlenmodus, Tag/Nacht-Beleuchtung, Koordinaten/Biom/Uhr-Anzeigen, Entity-Radar mit Filtern, Spieler/Mobs/Items), Wegpunkte (erstellen, bearbeiten, Farben, Symbole, Sets, pro Welt/Dimension/Server, Todespunkte, temporäre Wegpunkte, Teilen im Chat, Teleport wenn erlaubt, Beam/Ingame-Anzeige mit Distanz), World Map (Vollbild, Zoom/Pan, Speichern des Erkundeten, Caching, Höhlenkarten, Dimensionen inkl. Nether-Skalierung, Rechtsklick-Menü, Wegpunkte auf der Karte, Spuren/Leitlinien), Multiplayer-Kontexte (Server-/Welt-Erkennung, Unter-Welten), Einstellungsmenüs, Import/Export.

### Phase 2: Grundgerüst Map-Mod
- Projektstruktur, Build, CI (GitHub Actions), Konfigsystem, Keybinds, Persistenz.
- Chunk-/Regions-Datenmodell, Farbberechnung aus Blöcken/Biomen, Render-Cache. Fokus auf Performance (kein Ruckeln, inkrementelles Update, Hintergrund-Threads).

### Phase 3: Minimap + Wegpunkte

### Phase 4: World Map

### Phase 5: Seed Atlas integrieren
- Seed-basierte Biom- und Strukturberechnung aus meiner Mod portieren, als Layer in World Map (und optional Minimap).
- Gleiches Verhalten wie im Original: Befehle, Auto-Erkennung im Singleplayer, Seed pro Server-Kontext, Seed bleibt lokal.
- **Paritätstests:** Für feste Seeds und Koordinaten müssen Biome und Strukturpositionen exakt mit der Fabric-Version übereinstimmen (Referenzwerte über die Prism-Instanz erzeugen und als Testfixtures ablegen).

### Phase 6: Feinschliff
- Vollständiger Abgleich mit `FEATURES.md` und `KEYBINDS.md`, fehlende Punkte nachziehen.
- Performance-Messung (große erkundete Karten), Bugfixes, README mit Installation, Hotkeys und Umstiegs-Hinweisen für Xaero-Nutzer (ohne Xaero-Assets oder Screenshots).

## Tests
- Unit-Tests für Datenmodell, Farbberechnung, Wegpunkt-Persistenz, Seed-Atlas-Berechnung.
- Paritätstests für Seed Atlas (siehe Phase 5).
- Wo möglich automatisierte Ingame-/Render-Tests in Horizon.

## Rückfragen
Wenn eine Entscheidung den Umfang oder die Architektur deutlich ändert (z. B. fehlende Horizon-API, Lizenzfrage, Feature nicht umsetzbar), halte an und frag nach. Kleinere Entscheidungen triffst du selbst und dokumentierst sie in `docs/DECISIONS.md`.

**Starte mit Phase 0.**
