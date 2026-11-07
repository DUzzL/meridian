//! Several maps behind one server address (lobbies, networks of worlds):
//! which map of a server is shown, and the list of a server's maps.
//!
//! A server keeps a `maps.toml` next to its map folders. In the "world
//! spawn" mode each world spawn the server announces gets its own map; the
//! first spawn seen takes over the default map, so a server with one world
//! keeps a single map. "Manual" keeps the map chosen with `/meridian map`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

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
        let mut maps = std::fs::read_to_string(&path)
            .ok()
            .and_then(|t| toml::from_str::<ServerMaps>(&t).ok())
            .unwrap_or_default();
        maps.maps.entry(DEFAULT_MAP.to_owned()).or_default();
        if !maps.maps.contains_key(&maps.current) {
            maps.current = DEFAULT_MAP.to_owned();
        }
        maps.path = Some(path);
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

    /// Switches to a map by name, creating it if `create`; false when there
    /// is no such map (or the name is not usable).
    pub fn select(&mut self, name: &str, create: bool) -> bool {
        let name = name.trim();
        if name.is_empty() || name.len() > 40 {
            return false;
        }
        if !self.maps.contains_key(name) {
            if !create {
                return false;
            }
            self.maps.insert(name.to_owned(), MapEntry::default());
        }
        self.current = name.to_owned();
        true
    }

    pub fn names(&self) -> Vec<String> {
        self.maps.keys().cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
