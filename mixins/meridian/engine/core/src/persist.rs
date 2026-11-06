//! Files on disk: atomic writes and where each world's data lives.
//!
//! Layout under the host's data folder:
//!
//! ```text
//! worlds/<source>/<map>/<dimension>/regions/<x>_<z>.mrg
//! ```
//!
//! `<source>` is `sp-<world folder>` or `mp-<server address>`, `<map>` names
//! one of several maps for the same source (lobbies, sub-worlds; `default`
//! at first) and `<dimension>` is the dimension id. Every part is escaped so
//! that any name maps to one safe folder name on Windows, macOS and Linux.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use meridian_host::{DimensionId, WorldSource};

use crate::geom::RegionPos;

#[derive(Debug, thiserror::Error)]
pub enum PersistError {
    #[error("i/o error on {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("cannot encode: {0}")]
    Encode(String),
    #[error("cannot decode {path}: {reason}")]
    Decode { path: PathBuf, reason: String },
}

impl PersistError {
    pub(crate) fn io(path: &Path, source: std::io::Error) -> Self {
        Self::Io {
            path: path.to_owned(),
            source,
        }
    }
}

/// Writes `bytes` to `path` so that readers see the old or the new file,
/// never half of one: a temporary file beside it, flushed, then renamed.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), PersistError> {
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(dir).map_err(|e| PersistError::io(dir, e))?;
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("file");
    let tmp = dir.join(format!(".{name}.{}.tmp", std::process::id()));
    let result = (|| {
        let mut f = fs::File::create(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
        drop(f);
        fs::rename(&tmp, path)
    })();
    if let Err(e) = result {
        let _ = fs::remove_file(&tmp);
        return Err(PersistError::io(path, e));
    }
    Ok(())
}

/// Escapes a name into one folder name: `[a-z0-9._-]` stay (upper case is
/// lowered, since macOS and Windows ignore case), everything else becomes
/// `%XX` per UTF-8 byte. Windows device names and leading dots get a `_`.
pub fn escape_component(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for byte in name.bytes() {
        match byte {
            b'a'..=b'z' | b'0'..=b'9' | b'_' | b'-' | b'.' => out.push(byte as char),
            b'A'..=b'Z' => out.push(byte.to_ascii_lowercase() as char),
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    let stem = out.split('.').next().unwrap_or("").to_ascii_uppercase();
    let reserved = matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || (stem.len() == 4
            && (stem.starts_with("COM") || stem.starts_with("LPT"))
            && stem.as_bytes()[3].is_ascii_digit());
    if out.is_empty() || out.starts_with('.') || reserved {
        out.insert(0, '_');
    }
    out
}

/// The folder name of a world source.
pub fn source_key(source: &WorldSource) -> String {
    match source {
        WorldSource::Singleplayer { world } => format!("sp-{}", escape_component(world)),
        WorldSource::Multiplayer { address } => {
            let address = address.trim().trim_end_matches('.').to_ascii_lowercase();
            let address = address.strip_suffix(":25565").unwrap_or(&address);
            format!("mp-{}", escape_component(address))
        }
    }
}

/// Where one map of one dimension keeps its files.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MapPaths {
    root: PathBuf,
}

impl MapPaths {
    pub fn new(data_dir: &Path, source: &WorldSource, map: &str, dimension: &DimensionId) -> Self {
        let root = data_dir
            .join("worlds")
            .join(source_key(source))
            .join(escape_component(map))
            .join(escape_component(dimension.as_str()));
        Self { root }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The map's folder (all dimensions): where its waypoints live.
    pub fn map_dir(&self) -> &Path {
        self.root
            .parent()
            .expect("a dimension folder inside a map folder")
    }

    pub fn regions_dir(&self) -> PathBuf {
        self.root.join("regions")
    }

    pub fn region_file(&self, region: RegionPos) -> PathBuf {
        self.regions_dir()
            .join(format!("{}_{}.mrg", region.x, region.z))
    }
}

/// A fresh, empty folder for one test.
#[cfg(test)]
pub(crate) fn test_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("meridian-test-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_unsafe_names() {
        assert_eq!(
            escape_component("minecraft:overworld"),
            "minecraft%3Aoverworld"
        );
        assert_eq!(escape_component("My World"), "my%20world");
        assert_eq!(escape_component("../etc"), "_..%2Fetc");
        assert_eq!(escape_component("CON"), "_con");
        assert_eq!(escape_component("com1.txt"), "_com1.txt");
        assert_eq!(escape_component("computer"), "computer");
        assert_eq!(escape_component(""), "_");
        assert_eq!(escape_component("Welt ü"), "welt%20%C3%BC");
    }

    #[test]
    fn different_names_stay_apart() {
        assert_ne!(escape_component("a:b"), escape_component("a_b"));
        assert_ne!(escape_component("a b"), escape_component("a%20b"));
    }

    #[test]
    fn server_keys_ignore_case_and_default_port() {
        let a = WorldSource::Multiplayer {
            address: "Play.Example.com:25565".into(),
        };
        let b = WorldSource::Multiplayer {
            address: "play.example.com".into(),
        };
        let c = WorldSource::Multiplayer {
            address: "play.example.com:25566".into(),
        };
        assert_eq!(source_key(&a), source_key(&b));
        assert_ne!(source_key(&a), source_key(&c));
        let sp = WorldSource::Singleplayer {
            world: "play.example.com".into(),
        };
        assert_ne!(
            source_key(&sp),
            source_key(&b),
            "singleplayer and servers never share data"
        );
    }

    #[test]
    fn map_paths() {
        let paths = MapPaths::new(
            Path::new("/data"),
            &WorldSource::Singleplayer {
                world: "New World".into(),
            },
            "default",
            &DimensionId::new("minecraft:the_nether"),
        );
        assert_eq!(
            paths.region_file(RegionPos::new(-1, 2)),
            Path::new(
                "/data/worlds/sp-new%20world/default/minecraft%3Athe_nether/regions/-1_2.mrg"
            )
        );
    }

    #[test]
    fn atomic_write_replaces_and_leaves_no_temp() {
        let dir = test_dir("atomic");
        let file = dir.join("sub/a.bin");
        write_atomic(&file, b"one").unwrap();
        write_atomic(&file, b"two").unwrap();
        assert_eq!(fs::read(&file).unwrap(), b"two");
        let leftovers: Vec<_> = fs::read_dir(dir.join("sub")).unwrap().collect();
        assert_eq!(leftovers.len(), 1);
    }
}
