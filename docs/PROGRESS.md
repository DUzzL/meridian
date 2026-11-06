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

- Nächster Schritt: Phase 1 (Feature- und Hotkey-Inventar).
- Noch kein Code, daher weder Build noch Tests. Beides beginnt in Phase 2.

**Bekannte Probleme**

- `gh` ist nicht angemeldet. Das Repo existiert bisher nur lokal; ein GitHub-Remote und CI-Läufe brauchen `gh auth login`.
- `horizon-client` referenziert in der Doku `crate::hud::MOD_IMAGE_SIZE`, die Konstante ist in `main` @ `41f822b` aber nicht auffindbar. Die genaue Größe des `mod_image`-Slots bleibt zu klären (im Spiel messen).
