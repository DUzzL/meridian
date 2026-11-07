//! Several maps behind one server address (lobbies, networks of worlds):
//! which map of a server is shown, and the list of a server's maps.
//!
//! A server keeps a `maps.toml` next to its map folders. In the "world
//! spawn" mode each world spawn the server announces gets its own map; the
//! first spawn seen takes over the default map, so a server with one world
//! keeps a single map. "Manual" keeps the map chosen with `/meridian map`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::config::MapSelection;
use crate::engine::DEFAULT_MAP;
use crate::persist;

pub const FILE: &str = "maps.toml";

/// One map of a server.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct MapEntry {
    /// The world spawn it belongs to (world spawn mode).
    pub spawn: Option<[i32; 3]>,
}

/// A server's maps and the one in use.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ServerMaps {
    pub current: String,
    pub maps: BTreeMap<String, MapEntry>,
    #[serde(skip)]
    path: Option<PathBuf>,
}

impl Default for ServerMaps {
    fn default() -> Self {
        let mut maps = BTreeMap::new();
        maps.insert(DEFAULT_MAP.to_owned(), MapEntry::default());
        Self {
            current: DEFAULT_MAP.to_owned(),
            maps,
            path: None,
        }
    }
}

impl ServerMaps {
    /// The list in a server's folder (`worlds/<source>/`).
    pub fn load(server_dir: &Path) -> Self {
        let path = server_dir.join(FILE);
        let (maps, writable) = persist::load_or_set_aside(&path, "map list", |bytes| {
            std::str::from_utf8(bytes)
                .map_err(|e| e.to_string())
                .and_then(|t| toml::from_str::<ServerMaps>(t).map_err(|e| e.to_string()))
                .map_err(persist::LoadError::Broken)
        })
        .split();
        let mut maps = maps.unwrap_or_default();
        maps.maps.entry(DEFAULT_MAP.to_owned()).or_default();
        if !maps.maps.contains_key(&maps.current) {
            maps.current = DEFAULT_MAP.to_owned();
        }
        // An unreadable list is kept: without a path it is never saved.
        maps.path = writable.then_some(path);
        maps
    }

    pub fn save(&self) -> Result<(), persist::PersistError> {
        let Some(path) = &self.path else {
            return Ok(());
        };
        let text =
            toml::to_string(self).map_err(|e| persist::PersistError::Encode(e.to_string()))?;
        persist::write_atomic(path, text.as_bytes())
    }

    /// The map to show now; true in the second place when the list changed.
    pub fn choose(&mut self, mode: MapSelection, spawn: Option<[i32; 3]>) -> (String, bool) {
        match mode {
            MapSelection::Single => (DEFAULT_MAP.to_owned(), false),
            MapSelection::Manual => (self.current.clone(), false),
            MapSelection::WorldSpawn => {
                let Some(spawn) = spawn else {
                    return (self.current.clone(), false);
                };
                if let Some((name, _)) = self.maps.iter().find(|(_, m)| m.spawn == Some(spawn)) {
                    let name = name.clone();
                    let changed = self.current != name;
                    self.current = name.clone();
                    return (name, changed);
                }
                // The first spawn seen takes the default map over.
                let name = if self
                    .maps
                    .get(DEFAULT_MAP)
                    .is_some_and(|m| m.spawn.is_none())
                {
                    DEFAULT_MAP.to_owned()
                } else {
                    (2..)
                        .map(|i| format!("map-{i}"))
                        .find(|n| !self.maps.contains_key(n))
                        .expect("a free name")
                };
                self.maps
                    .insert(name.clone(), MapEntry { spawn: Some(spawn) });
                self.current = name.clone();
                (name, true)
            }
        }
    }

    /// The map a name means: maps share a folder when their names differ
    /// only in case (`Default` is `default`), so they are one map.
    pub fn find(&self, name: &str) -> Option<&str> {
        let key = persist::escape_component(name.trim());
        self.maps
            .keys()
            .find(|m| persist::escape_component(m) == key)
            .map(String::as_str)
    }

    /// Switches to a map by name, creating it if `create`; false when there
    /// is no such map (or the name is not usable).
    pub fn select(&mut self, name: &str, create: bool) -> bool {
        let name = name.trim();
        if name.is_empty() || name.len() > 40 {
            return false;
        }
        let name = match self.find(name) {
            Some(existing) => existing.to_owned(),
            None if create => {
                self.maps.insert(name.to_owned(), MapEntry::default());
                name.to_owned()
            }
            None => return false,
        };
        self.current = name;
        true
    }

    /// Renames a map other than the default one (in this list only; the
    /// caller moves its folder and what else is keyed by its name): its
    /// old and new name, or why not.
    pub fn rename(&mut self, old: &str, new: &str) -> Result<(String, String), String> {
        let new = new.trim();
        let Some(old) = self.find(old).map(str::to_owned) else {
            return Err(format!("No map named {old}"));
        };
        let same_folder = persist::escape_component(&old) == persist::escape_component(new);
        if old == DEFAULT_MAP
            || new.is_empty()
            || new.len() > 40
            || new.contains(' ')
            || (self.find(new).is_some() && !same_folder)
        {
            return Err(format!("Cannot rename {old} to {new}"));
        }
        let entry = self.maps.remove(&old).unwrap_or_default();
        self.maps.insert(new.to_owned(), entry);
        if self.current == old {
            self.current = new.to_owned();
        }
        Ok((old, new.to_owned()))
    }

    pub fn names(&self) -> Vec<String> {
        self.maps.keys().cloned().collect()
    }
}

/// How long after a level loads its world spawn may still be the last
/// level's: a server that announces the same spawn again (or none) is taken
/// as it is after this.
pub const SPAWN_GRACE: Duration = Duration::from_secs(5);

/// The world spawn reported may still be the last level's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NotYet;

/// Tells a world spawn the server announced for the level now loaded from
/// one left over from the last level or server (the game keeps it across a
/// server switch behind a proxy): after a level loads, a spawn counts once
/// one other than the old is announced, or after [`SPAWN_GRACE`].
#[derive(Clone, Debug, Default)]
pub struct SpawnWatch {
    /// The spawn reported when the level loaded, and when that was; `None`
    /// once the spawn is known.
    stale: Option<(Option<[i32; 3]>, Instant)>,
}

impl SpawnWatch {
    /// A level loaded; `reported` is the spawn the game reports now.
    pub fn level_loaded(&mut self, reported: Option<[i32; 3]>, now: Instant) {
        self.stale = Some((reported, now));
    }

    /// The spawn to choose the map by, given what the game reports now;
    /// [`NotYet`] while it may still be the last level's.
    pub fn spawn(
        &mut self,
        reported: Option<[i32; 3]>,
        now: Instant,
    ) -> Result<Option<[i32; 3]>, NotYet> {
        if let Some((stale, since)) = self.stale {
            // None: not announced yet (a new connection starts without).
            let fresh = reported.is_some() && reported != stale;
            if !fresh && now.duration_since(since) < SPAWN_GRACE {
                return Err(NotYet);
            }
            self.stale = None;
        }
        Ok(reported)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_spawn_counts_once_announced_for_the_level() {
        let t = Instant::now();
        let mut w = SpawnWatch::default();
        assert_eq!(
            w.spawn(Some([1, 2, 3]), t),
            Ok(Some([1, 2, 3])),
            "no level yet"
        );
        // The last server's spawn is still reported after the switch.
        w.level_loaded(Some([1, 2, 3]), t);
        assert_eq!(
            w.spawn(Some([1, 2, 3]), t + Duration::from_secs(1)),
            Err(NotYet)
        );
        assert_eq!(
            w.spawn(Some([9, 9, 9]), t + Duration::from_secs(2)),
            Ok(Some([9, 9, 9])),
            "the new server announced its own"
        );
        assert_eq!(
            w.spawn(Some([9, 9, 9]), t + Duration::from_secs(3)),
            Ok(Some([9, 9, 9]))
        );
        // A new connection reports none until the server announces it.
        w.level_loaded(Some([9, 9, 9]), t);
        assert_eq!(w.spawn(None, t + Duration::from_secs(1)), Err(NotYet));
        assert_eq!(
            w.spawn(Some([4, 5, 6]), t + Duration::from_secs(2)),
            Ok(Some([4, 5, 6]))
        );
        // A server that announces the same spawn again, or none: taken after
        // the grace time.
        w.level_loaded(Some([9, 9, 9]), t);
        assert_eq!(
            w.spawn(Some([9, 9, 9]), t + Duration::from_secs(4)),
            Err(NotYet)
        );
        assert_eq!(
            w.spawn(Some([9, 9, 9]), t + SPAWN_GRACE),
            Ok(Some([9, 9, 9]))
        );
        w.level_loaded(None, t);
        assert_eq!(w.spawn(None, t + SPAWN_GRACE), Ok(None));
    }

    #[test]
    fn names_differing_in_case_are_one_map() {
        let mut m = ServerMaps::default();
        assert!(m.select("Default", false));
        assert_eq!(m.current, DEFAULT_MAP);
        assert!(m.select("Survival", true));
        assert!(m.select("SURVIVAL", true));
        assert_eq!(m.names(), ["Survival", "default"]);
        assert_eq!(m.current, "Survival");
        assert_eq!(m.find("survival"), Some("Survival"));
    }

    #[test]
    fn rename_keeps_the_entry_and_the_current_map() {
        let mut m = ServerMaps::default();
        m.choose(MapSelection::WorldSpawn, Some([0, 64, 0]));
        m.choose(MapSelection::WorldSpawn, Some([500, 70, -20]));
        assert_eq!(m.current, "map-2");
        assert_eq!(
            m.rename("MAP-2", "Skyblock"),
            Ok(("map-2".to_owned(), "Skyblock".to_owned()))
        );
        assert_eq!(m.current, "Skyblock");
        assert_eq!(m.maps["Skyblock"].spawn, Some([500, 70, -20]));
        assert!(
            m.rename("default", "x").is_err(),
            "the default map keeps its name"
        );
        assert!(m.rename("nope", "x").is_err());
        assert!(m.rename("Skyblock", "DEFAULT").is_err(), "taken");
        assert!(m.rename("Skyblock", "two words").is_err());
        assert!(m.rename("Skyblock", "skyblock").is_ok(), "a change of case");
    }

    #[test]
    fn a_broken_list_is_set_aside() {
        let dir = crate::persist::test_dir("maps_broken");
        std::fs::write(dir.join(FILE), "current = [").unwrap();
        let mut m = ServerMaps::load(&dir);
        assert_eq!(m.names(), ["default"]);
        assert!(!dir.join(FILE).exists());
        m.select("survival", true);
        m.save().unwrap();
        assert_eq!(ServerMaps::load(&dir).names(), ["default", "survival"]);
    }

    #[test]
    fn world_spawns_get_their_own_maps() {
        let mut m = ServerMaps::default();
        assert_eq!(
            m.choose(MapSelection::WorldSpawn, None),
            (DEFAULT_MAP.to_owned(), false),
            "spawn not known yet"
        );
        assert_eq!(
            m.choose(MapSelection::WorldSpawn, Some([0, 64, 0])),
            (DEFAULT_MAP.to_owned(), true),
            "first spawn takes the default map"
        );
        assert_eq!(
            m.choose(MapSelection::WorldSpawn, Some([0, 64, 0])),
            (DEFAULT_MAP.to_owned(), false)
        );
        assert_eq!(
            m.choose(MapSelection::WorldSpawn, Some([500, 70, -20])),
            ("map-2".to_owned(), true),
            "another world"
        );
        assert_eq!(
            m.choose(MapSelection::WorldSpawn, Some([0, 64, 0])),
            (DEFAULT_MAP.to_owned(), true),
            "back to the lobby"
        );
        assert_eq!(
            m.choose(MapSelection::Single, Some([500, 70, -20])).0,
            DEFAULT_MAP
        );
        assert_eq!(m.names(), ["default", "map-2"]);
    }

    #[test]
    fn manual_selection_and_persistence() {
        let dir = std::env::temp_dir().join(format!("meridian-maps-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut m = ServerMaps::load(&dir);
        assert!(!m.select("survival", false));
        assert!(m.select("survival", true));
        assert_eq!(
            m.choose(MapSelection::Manual, Some([1, 2, 3])).0,
            "survival"
        );
        m.save().unwrap();
        let back = ServerMaps::load(&dir);
        assert_eq!(back.current, "survival");
        assert_eq!(back.names(), ["default", "survival"]);
        std::fs::remove_dir_all(&dir).ok();
    }
}
