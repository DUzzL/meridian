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

use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Condvar, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use parking_lot::Mutex;

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
/// never half of one: a temporary file beside it (unique per write, so
/// threads and processes never share one), flushed, then renamed; on Unix
/// the folder is flushed too, so the rename survives a power cut.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), PersistError> {
    require_writable(path)?;
    static WRITES: AtomicU64 = AtomicU64::new(0);
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(dir).map_err(|e| PersistError::io(dir, e))?;
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("file");
    let n = WRITES.fetch_add(1, Ordering::Relaxed);
    let tmp = dir.join(format!(".{name}.{}-{n}.tmp", std::process::id()));
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
    #[cfg(unix)]
    if let Ok(d) = fs::File::open(dir) {
        // Some file systems cannot flush a folder; the file itself is safe.
        let _ = d.sync_all();
    }
    Ok(())
}

/// Why a stored file could not be used.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoadError {
    /// Damaged or not ours: it is set aside and the store starts fresh.
    Broken(String),
    /// Written by a newer Meridian: it stays as it is and is never written
    /// over by this one.
    Newer(String),
}

/// What [`load_or_set_aside`] found.
#[derive(Debug, PartialEq, Eq)]
pub enum Loaded<T> {
    /// No file yet.
    Missing,
    Read(T),
    /// The file was broken and is now beside it as `.broken.<time>`.
    SetAside,
    /// The file stays untouched and must not be saved over this session:
    /// it is newer than this Meridian, could not be read, or could not be
    /// moved aside.
    ReadOnly,
}

impl<T> Loaded<T> {
    /// The value read, if any, and whether the file may be written.
    pub fn split(self) -> (Option<T>, bool) {
        match self {
            Loaded::Read(v) => (Some(v), true),
            Loaded::Missing | Loaded::SetAside => (None, true),
            Loaded::ReadOnly => (None, false),
        }
    }
}

/// Reads of a file that fail for other reasons than its absence.
const READ_ATTEMPTS: u32 = 3;
const READ_RETRY: Duration = Duration::from_millis(50);

/// Reads and parses a stored file without ever losing it: a missing file is
/// [`Loaded::Missing`]; a broken one is renamed to `<name>.broken.<unix
/// time>`; a newer one, or one that stays unreadable after a few tries
/// (a sharing violation on Windows, a permission), is left alone and
/// reported [`Loaded::ReadOnly`]. Logs what it did once; `what` names the
/// file for that ("waypoints").
pub fn load_or_set_aside<T>(
    path: &Path,
    what: &str,
    parse: impl FnOnce(&[u8]) -> Result<T, LoadError>,
) -> Loaded<T> {
    let mut attempt = 1;
    let bytes = loop {
        match fs::read(path) {
            Ok(bytes) => break bytes,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Loaded::Missing,
            Err(e) if attempt >= READ_ATTEMPTS => {
                log::warn!(
                    "meridian: cannot read {what} {} ({e}); it is not saved this session",
                    path.display()
                );
                return Loaded::ReadOnly;
            }
            Err(_) => {
                attempt += 1;
                std::thread::sleep(READ_RETRY);
            }
        }
    };
    match parse(&bytes) {
        Ok(v) => Loaded::Read(v),
        Err(LoadError::Newer(reason)) => {
            log::warn!(
                "meridian: {what} {} is from a newer Meridian ({reason}); kept as it is and not saved this session",
                path.display()
            );
            Loaded::ReadOnly
        }
        Err(LoadError::Broken(reason)) if !path_writable(path) => {
            log::warn!(
                "meridian: {what} {} is broken ({reason}); another process owns it, left untouched",
                path.display()
            );
            Loaded::ReadOnly
        }
        Err(LoadError::Broken(reason)) => match set_aside(path) {
            Ok(aside) => {
                log::warn!(
                    "meridian: {what} {} is unreadable ({reason}); moved to {} and started fresh",
                    path.display(),
                    aside.display()
                );
                Loaded::SetAside
            }
            Err(e) => {
                log::warn!(
                    "meridian: {what} {} is unreadable ({reason}) and cannot be moved aside ({e}); it is not saved this session",
                    path.display()
                );
                Loaded::ReadOnly
            }
        },
    }
}

/// The `version` field of a JSON store, so a file from a newer Meridian is
/// told from a broken one before it is read whole.
pub fn json_version(bytes: &[u8]) -> Option<u32> {
    #[derive(serde::Deserialize)]
    struct Version {
        version: u32,
    }
    serde_json::from_slice::<Version>(bytes)
        .ok()
        .map(|v| v.version)
}

/// The top-level version of TOML stores, before decoding version-specific fields.
pub fn toml_version(bytes: &[u8]) -> Option<u32> {
    let value: toml::Value = toml::from_str(std::str::from_utf8(bytes).ok()?).ok()?;
    value.get("version")?.as_integer()?.try_into().ok()
}

/// Renames a file to `<name>.broken.<unix time>` beside it (with a counter
/// when that is taken); the new path.
pub fn set_aside(path: &Path) -> std::io::Result<PathBuf> {
    require_writable(path).map_err(std::io::Error::other)?;
    let name = path
        .file_name()
        .map_or_else(|| "file".into(), |n| n.to_string_lossy().into_owned());
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let aside = (0..1000)
        .map(|i| match i {
            0 => path.with_file_name(format!("{name}.broken.{now}")),
            i => path.with_file_name(format!("{name}.broken.{now}-{i}")),
        })
        .find(|p| !p.exists())
        .ok_or_else(|| std::io::Error::other("no free name to set it aside"))?;
    fs::rename(path, &aside)?;
    Ok(aside)
}

/// The order of background saves and loads per file: a save waits for the
/// saves of the same file started before it, a load for all of them, so a
/// region read right after it was saved (a world closed and opened again,
/// another dimension's map) never sees the old file. Shared by every map
/// engine of the process.
#[derive(Debug, Default)]
pub struct FileOrder {
    /// Per file: saves started, saves finished.
    files: std::sync::Mutex<HashMap<PathBuf, (u64, u64)>>,
    changed: Condvar,
}

/// The longest a load or save waits for earlier saves (a stuck disk must not
/// stop the map for good).
const ORDER_WAIT: Duration = Duration::from_secs(30);

impl FileOrder {
    /// The process's one instance.
    pub fn global() -> &'static FileOrder {
        static ORDER: OnceLock<FileOrder> = OnceLock::new();
        ORDER.get_or_init(FileOrder::default)
    }

    /// A save of `path` starts (on the thread that hands it to a worker):
    /// its turn.
    pub fn save_started(&self, path: &Path) -> u64 {
        let mut files = self.files.lock().unwrap_or_else(|e| e.into_inner());
        let entry = files.entry(path.to_owned()).or_default();
        entry.0 += 1;
        entry.0 - 1
    }

    /// Saves of `path` started so far: a load started now waits for them.
    pub fn saves_started(&self, path: &Path) -> u64 {
        let files = self.files.lock().unwrap_or_else(|e| e.into_inner());
        files.get(path).map_or(0, |e| e.0)
    }

    /// Waits (on a worker) until `count` saves of `path` have finished.
    pub fn wait_finished(&self, path: &Path, count: u64) {
        let files = self.files.lock().unwrap_or_else(|e| e.into_inner());
        let (files, timeout) = self
            .changed
            .wait_timeout_while(files, ORDER_WAIT, |files| {
                files.get(path).is_some_and(|e| e.1 < count)
            })
            .unwrap_or_else(|e| e.into_inner());
        drop(files);
        if timeout.timed_out() {
            log::warn!(
                "meridian: earlier saves of {} take too long; going on",
                path.display()
            );
        }
    }

    /// A save of `path` is done (written or failed). A file with no save
    /// left running is forgotten (waiters take a missing file as done), so
    /// the table only holds files being saved.
    pub fn save_finished(&self, path: &Path) {
        let mut files = self.files.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(e) = files.get_mut(path) {
            e.1 += 1;
            if e.1 >= e.0 {
                files.remove(path);
            }
        }
        self.changed.notify_all();
    }
}

/// Per folder: whether it is this process's, and the locked file.
type Claims = HashMap<PathBuf, (bool, Option<fs::File>)>;
static CLAIMS: OnceLock<Mutex<Claims>> = OnceLock::new();

/// Whether a registered data/config root containing this path is writable.
/// All matching roots must be owned, including aliases through symlinks.
pub fn path_writable(path: &Path) -> bool {
    let path = resolved_path(path);
    CLAIMS
        .get_or_init(Default::default)
        .lock()
        .iter()
        .all(|(root, (owned, _))| *owned || !path.starts_with(root))
}

fn resolved_path(path: &Path) -> PathBuf {
    if let Ok(path) = path.canonicalize() {
        return path;
    }
    match (path.parent(), path.file_name()) {
        (Some(parent), Some(name)) if parent != path => resolved_path(parent).join(name),
        _ => path.to_owned(),
    }
}

/// Enforces ownership for non-atomic mutations too (map rename, exports).
pub fn require_writable(path: &Path) -> Result<(), PersistError> {
    if path_writable(path) {
        return Ok(());
    }
    Err(PersistError::io(
        path,
        std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "another Meridian process owns this folder",
        ),
    ))
}

/// Marks this process as the one that writes under `data_dir`, with a
/// lock on `meridian.lock` there that the system releases when the process
/// ends (also after a crash). False when another process holds it (a second
/// game on the same folder); asking again from this process gives the same
/// answer. A lock that cannot be taken at all counts as
/// held elsewhere, so nothing is written.
pub fn claim_data_dir(data_dir: &Path) -> bool {
    let mut claims = CLAIMS.get_or_init(Default::default).lock();
    let (claimed, _) = claims.entry(resolved_path(data_dir)).or_insert_with(|| {
        let path = data_dir.join("meridian.lock");
        let file = fs::create_dir_all(data_dir).and_then(|_| {
            fs::OpenOptions::new()
                .create(true)
                .truncate(false)
                .write(true)
                .open(&path)
        });
        match file.map(|f| (f.try_lock(), f)) {
            Ok((Ok(()), f)) => (true, Some(f)),
            Ok((Err(fs::TryLockError::WouldBlock), _)) => {
                log::warn!(
                    "meridian: {} is held by another game; nothing there is saved",
                    path.display()
                );
                (false, None)
            }
            // Fail closed: an unavailable lock is not proof of exclusive ownership.
            Ok((Err(fs::TryLockError::Error(e)), _)) | Err(e) => {
                log::warn!("meridian: cannot lock {}: {e}", path.display());
                (false, None)
            }
        }
    });
    *claimed
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

/// The folder of a world or server, which holds its maps.
pub fn server_dir(data_dir: &Path, source: &WorldSource) -> PathBuf {
    data_dir.join("worlds").join(source_key(source))
}

/// Where one map of one dimension keeps its files.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MapPaths {
    root: PathBuf,
    /// A cave layer (`caves/<y>/`) instead of the surface.
    layer: Option<i32>,
}

impl MapPaths {
    pub fn new(data_dir: &Path, source: &WorldSource, map: &str, dimension: &DimensionId) -> Self {
        let root = server_dir(data_dir, source)
            .join(escape_component(map))
            .join(escape_component(dimension.as_str()));
        Self { root, layer: None }
    }

    /// The same map's cave layer starting at `y` (or its surface).
    pub fn with_layer(&self, layer: Option<i32>) -> Self {
        Self {
            root: self.root.clone(),
            layer,
        }
    }

    pub fn layer(&self) -> Option<i32> {
        self.layer
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
        match self.layer {
            None => self.root.join("regions"),
            Some(y) => self.root.join("caves").join(y.to_string()).join("regions"),
        }
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
    fn cave_layers_have_their_own_folders() {
        let p = MapPaths::new(
            Path::new("/d"),
            &WorldSource::Singleplayer { world: "w".into() },
            "default",
            &DimensionId::new("minecraft:overworld"),
        );
        let cave = p.with_layer(Some(-16));
        assert_eq!(
            cave.regions_dir(),
            Path::new("/d/worlds/sp-w/default/minecraft%3Aoverworld/caves/-16/regions")
        );
        assert_eq!(cave.map_dir(), p.map_dir());
        assert_eq!(cave.with_layer(None), p);
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

    fn parse_number(bytes: &[u8]) -> Result<u32, LoadError> {
        let text = std::str::from_utf8(bytes).map_err(|e| LoadError::Broken(e.to_string()))?;
        match text.strip_prefix("v9:") {
            Some(_) => Err(LoadError::Newer("version 9".into())),
            None => text.parse().map_err(|_| LoadError::Broken(text.into())),
        }
    }

    fn broken_copies(dir: &Path) -> Vec<String> {
        fs::read_dir(dir)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.contains(".broken."))
            .collect()
    }

    #[test]
    fn loading_never_loses_a_file() {
        let dir = test_dir("load_aside");
        let file = dir.join("store.txt");
        assert_eq!(
            load_or_set_aside(&file, "test", parse_number),
            Loaded::Missing
        );
        fs::write(&file, "42").unwrap();
        assert_eq!(
            load_or_set_aside(&file, "test", parse_number),
            Loaded::Read(42)
        );

        // Broken: moved aside whole, the store starts fresh and may save.
        fs::write(&file, "garbage").unwrap();
        let loaded = load_or_set_aside(&file, "test", parse_number);
        assert_eq!(loaded, Loaded::SetAside);
        assert_eq!(loaded.split(), (None, true));
        assert!(!file.exists());
        let copies = broken_copies(&dir);
        assert_eq!(copies.len(), 1, "{copies:?}");
        assert!(copies[0].starts_with("store.txt.broken."));
        assert_eq!(fs::read(dir.join(&copies[0])).unwrap(), b"garbage");
        // A second broken file the same second keeps the first copy.
        fs::write(&file, "more garbage").unwrap();
        load_or_set_aside(&file, "test", parse_number);
        assert_eq!(broken_copies(&dir).len(), 2);

        // Newer: stays where it is, untouched, and must not be saved over.
        fs::write(&file, "v9:data").unwrap();
        let loaded = load_or_set_aside(&file, "test", parse_number);
        assert_eq!(loaded.split(), (None, false));
        assert_eq!(fs::read(&file).unwrap(), b"v9:data");

        // Unreadable (a folder in its place): left alone, read-only.
        let folder = dir.join("folder.txt");
        fs::create_dir_all(&folder).unwrap();
        assert_eq!(
            load_or_set_aside(&folder, "test", parse_number),
            Loaded::ReadOnly
        );
        assert!(folder.is_dir());
    }

    #[test]
    fn saves_and_loads_of_one_file_keep_their_order() {
        let order = FileOrder::default();
        let path = Path::new("/x/1_1.mrg");
        assert_eq!(order.saves_started(path), 0);
        let first = order.save_started(path);
        let second = order.save_started(path);
        assert_eq!((first, second), (0, 1));
        let wanted = order.saves_started(path);
        let order = std::sync::Arc::new(order);
        let done = std::sync::Arc::new(AtomicU64::new(0));
        let loader = {
            let (order, done) = (order.clone(), done.clone());
            std::thread::spawn(move || {
                order.wait_finished(Path::new("/x/1_1.mrg"), wanted);
                done.load(Ordering::SeqCst)
            })
        };
        // The second save waits for the first; the load for both.
        order.wait_finished(path, first);
        std::thread::sleep(Duration::from_millis(20));
        done.fetch_add(1, Ordering::SeqCst);
        order.save_finished(path);
        order.wait_finished(path, second);
        std::thread::sleep(Duration::from_millis(20));
        done.fetch_add(1, Ordering::SeqCst);
        order.save_finished(path);
        assert_eq!(loader.join().unwrap(), 2, "the load ran after both saves");
        assert!(
            order.files.lock().unwrap().is_empty(),
            "a file is forgotten once no save of it runs"
        );
        // Other files never wait.
        order.wait_finished(Path::new("/x/2_2.mrg"), 0);
    }

    #[test]
    fn data_dir_claim_is_kept_for_the_process() {
        let dir = test_dir("claim");
        assert!(claim_data_dir(&dir));
        assert!(claim_data_dir(&dir), "the same process asks again");
        assert!(dir.join("meridian.lock").exists());
        // Another holder of the lock (as a second game would be).
        let other = test_dir("claim_other");
        let lock = fs::File::create(other.join("meridian.lock")).unwrap();
        lock.try_lock().unwrap();
        assert!(!claim_data_dir(&other));
    }
}
