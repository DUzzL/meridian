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

Wo Katalog-Hooks reichen, nutzt der Adapter die Trait-Mod-API (`"hooks": N`, kein Revision-Pin). Mixins kommen nur für Lücken zum Einsatz (siehe O-001).

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
