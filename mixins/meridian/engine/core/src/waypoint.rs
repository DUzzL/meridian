//! Waypoints: the model, sets, deathpoints, storage per world, commands
//! built from them, and sharing in chat.
//!
//! Waypoints belong to a map (`persist::MapPaths`'s source and map) and are
//! kept per dimension in one JSON file per map:
//! `worlds/<source>/<map>/waypoints.json`.

use std::collections::{BTreeMap, VecDeque};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::persist::{self, PersistError};

/// The set every map starts with.
pub const DEFAULT_SET: &str = "default";
/// The waypoint file inside a map's folder.
pub const FILE: &str = "waypoints.json";
/// Blocks from a deathpoint that count as reaching it.
pub const REACHED_DISTANCE: f64 = 4.0;
/// Most temporary waypoints a dimension keeps; the oldest go first.
pub const MAX_TEMPORARY: usize = 16;
/// Longest name a waypoint shared in chat keeps, in characters.
pub const MAX_SHARED_NAME: usize = 32;
/// Shared waypoints remembered for accepting.
pub const SHARED_KEPT: usize = 8;

/// Sixteen colours, Minecraft's chat palette (0xRRGGBB).
pub const COLORS: [u32; 16] = [
    0x000000, 0x0000AA, 0x00AA00, 0x00AAAA, 0xAA0000, 0xAA00AA, 0xFFAA00, 0xAAAAAA, 0x555555,
    0x5555FF, 0x55FF55, 0x55FFFF, 0xFF5555, 0xFF55FF, 0xFFFF55, 0xFFFFFF,
];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    #[default]
    Normal,
    /// The latest death.
    Death,
    /// An earlier death kept as a waypoint.
    OldDeath,
    /// Gone when the map closes (chat links, quick marks).
    Temporary,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Waypoint {
    pub id: u64,
    pub name: String,
    /// One or two characters shown on the map.
    pub initials: String,
    pub pos: [i32; 3],
    /// The height is unknown (unexplored ground, `~` in a share or an
    /// import): `pos[1]` is only a guess, and teleports keep the player's
    /// own height (`~`).
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub y_unknown: bool,
    /// Index into [`COLORS`].
    pub color: u8,
    pub kind: Kind,
    /// Global waypoints ignore the maximum draw distance.
    pub global: bool,
    pub disabled: bool,
    /// Yaw to face after teleporting, if any.
    pub yaw: Option<f32>,
    /// Unix seconds.
    pub created: u64,
}

impl Default for Waypoint {
    fn default() -> Self {
        Self {
            id: 0,
            name: String::new(),
            initials: String::new(),
            pos: [0, 64, 0],
            y_unknown: false,
            color: 12,
            kind: Kind::Normal,
            global: false,
            disabled: false,
            yaw: None,
            created: 0,
        }
    }
}

/// Default initials: the first letter of the first two words, or the first
/// two characters of a single word, upper case.
pub fn initials_of(name: &str) -> String {
    let words: Vec<&str> = name.split_whitespace().collect();
    let pick: String = match words.as_slice() {
        [] => "?".to_owned(),
        [one] => one.chars().take(1).collect(),
        [a, b, ..] => a.chars().take(1).chain(b.chars().take(1)).collect(),
    };
    pick.to_uppercase()
}

impl Waypoint {
    pub fn new(name: impl Into<String>, pos: [i32; 3], color: u8) -> Self {
        let name = name.into();
        Self {
            initials: initials_of(&name),
            name,
            pos,
            color: color % 16,
            ..Self::default()
        }
    }

    /// A waypoint at a spot whose height may be unknown.
    pub fn at(name: impl Into<String>, spot: Spot, color: u8) -> Self {
        Self::new(name, [spot.x, spot.y.unwrap_or(64), spot.z], color).with_height(spot.y)
    }

    /// Sets the height; `None` keeps `pos[1]` as a guess and marks it
    /// unknown.
    pub fn with_height(mut self, y: Option<i32>) -> Self {
        if let Some(y) = y {
            self.pos[1] = y;
        }
        self.y_unknown = y.is_none();
        self
    }

    /// The height, when it is known.
    pub fn height(&self) -> Option<i32> {
        (!self.y_unknown).then_some(self.pos[1])
    }

    /// `y` as commands and shares write it: `~` when unknown.
    pub fn height_text(&self) -> String {
        self.height().map_or("~".to_owned(), |y| y.to_string())
    }

    pub fn rgb(&self) -> u32 {
        COLORS[(self.color % 16) as usize]
    }

    /// Distance in blocks from a point, centred on the waypoint's block;
    /// across the ground only when its height is unknown.
    pub fn distance(&self, from: [f64; 3]) -> f64 {
        let d = |a: i32, b: f64| a as f64 + 0.5 - b;
        let dy = if self.y_unknown {
            0.0
        } else {
            d(self.pos[1], from[1])
        };
        let (dx, dz) = (d(self.pos[0], from[0]), d(self.pos[2], from[2]));
        (dx * dx + dy * dy + dz * dz).sqrt()
    }
}

/// A block position whose height may be unknown (unexplored, or the
/// Nether's bedrock roof, which says nothing about the ground below).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Spot {
    pub x: i32,
    pub y: Option<i32>,
    pub z: i32,
}

/// "12m", "1.2km" from `km_from` blocks on, with `precision` decimals.
pub fn distance_label(blocks: f64, km_from: u32, precision: usize) -> String {
    if km_from > 0 && blocks >= km_from as f64 {
        format!("{:.*}km", precision, blocks / 1000.0)
    } else {
        format!("{}m", blocks.round() as i64)
    }
}

/// Fills a teleport command template: `{x}`, `{y}` (`~` when unknown),
/// `{z}`, `{yaw}`, `{d}` (dimension) and `{name}`.
pub fn command(template: &str, w: &Waypoint, dimension: &str) -> String {
    template
        .replace("{x}", &w.pos[0].to_string())
        .replace("{y}", &w.height_text())
        .replace("{z}", &w.pos[2].to_string())
        .replace(
            "{yaw}",
            &w.yaw.map_or("~".to_owned(), |y| format!("{y:.0}")),
        )
        .replace("{d}", dimension)
        .replace("{name}", &w.name)
}

/// The teleport command templates of the settings.
#[derive(Clone, Copy, Debug)]
pub struct TeleportFormats<'a> {
    /// `/tp @s {x} {y} {z}`.
    pub plain: &'a str,
    /// The same, turning the player to the waypoint's yaw.
    pub rotation: &'a str,
    /// Into another dimension (`/execute ... in {d} run tp ...`).
    pub dimension: &'a str,
}

impl<'a> TeleportFormats<'a> {
    pub fn of(s: &'a crate::config::Settings) -> Self {
        Self {
            plain: &s.waypoints.teleport_format,
            rotation: &s.waypoints.teleport_rotation_format,
            dimension: &s.world_map.map_teleport_dimension_format,
        }
    }
}

/// The command that teleports to `w`, a waypoint of `dimension`, from
/// `here`: the dimension format when they differ (the waypoint's own
/// coordinates in its own dimension), else the rotation format when the
/// waypoint has a yaw, else the plain one.
pub fn teleport_command(
    w: &Waypoint,
    dimension: &str,
    here: &str,
    f: TeleportFormats<'_>,
) -> String {
    let template = if dimension != here {
        f.dimension
    } else if w.yaw.is_some() {
        f.rotation
    } else {
        f.plain
    };
    command(template, w, dimension)
}

/// The chat line that shares a waypoint, readable by people without the mod
/// and parsed back by [`parse_shared`].
pub fn share_text(w: &Waypoint, dimension: &str) -> String {
    format!(
        "[Meridian] waypoint \"{}\" at {} {} {} in {}",
        w.name.replace('"', "'"),
        w.pos[0],
        w.height_text(),
        w.pos[2],
        dimension
    )
}

/// A waypoint shared in chat: Meridian's own line
/// (`[Meridian] waypoint "<name>" at <x> <y or ~> <z> in <dimension>`,
/// anywhere in the message), as a temporary waypoint with its name cut to
/// [`MAX_SHARED_NAME`] characters, and the dimension it names.
pub fn parse_shared(message: &str) -> Option<(Waypoint, Option<String>)> {
    const START: &str = "[Meridian] waypoint \"";
    let start = message.find(START)?;
    let rest = &message[start + START.len()..];
    let end = rest.find('"')?;
    let name: String = rest[..end].chars().take(MAX_SHARED_NAME).collect();
    let rest = rest[end + 1..].trim_start().strip_prefix("at ")?;
    let mut parts = rest.split_whitespace();
    let x = parts.next()?.parse::<i32>().ok()?;
    let y = match parts.next()? {
        "~" => None,
        y => Some(y.parse::<i32>().ok()?),
    };
    let z = parts.next()?.parse::<i32>().ok()?;
    let dimension = rest.split(" in ").nth(1).map(|d| d.trim().to_owned());
    let name = if name.trim().is_empty() {
        "Shared".to_owned()
    } else {
        name
    };
    let mut w = Waypoint::at(name, Spot { x, y, z }, 11);
    w.kind = Kind::Temporary;
    w.global = true;
    Some((w, dimension))
}

/// A waypoint received in chat, kept until the player accepts it.
#[derive(Clone, Debug, PartialEq)]
pub struct Shared {
    /// What `/meridian waypoint accept <number>` names it by.
    pub number: u32,
    pub waypoint: Waypoint,
    /// The dimension the line names, if any.
    pub dimension: Option<String>,
}

/// The last [`SHARED_KEPT`] waypoints received in chat. Nothing is added to
/// the book until the player accepts one (a click on the line, or the
/// command), so other players cannot fill the list.
#[derive(Clone, Debug, Default)]
pub struct SharedInbox {
    next: u32,
    items: VecDeque<Shared>,
}

impl SharedInbox {
    /// Remembers a shared waypoint; returns its number. The same waypoint
    /// shared again keeps its number.
    pub fn receive(&mut self, waypoint: Waypoint, dimension: Option<String>) -> u32 {
        if let Some(s) = self.items.iter().find(|s| {
            s.waypoint.name == waypoint.name
                && s.waypoint.pos == waypoint.pos
                && s.dimension == dimension
        }) {
            return s.number;
        }
        self.next += 1;
        if self.items.len() >= SHARED_KEPT {
            self.items.pop_front();
        }
        self.items.push_back(Shared {
            number: self.next,
            waypoint,
            dimension,
        });
        self.next
    }

    /// Takes waypoint `number`, or the latest one.
    pub fn take(&mut self, number: Option<u32>) -> Option<Shared> {
        let i = match number {
            Some(n) => self.items.iter().position(|s| s.number == n)?,
            None => self.items.len().checked_sub(1)?,
        };
        self.items.remove(i)
    }

    /// The waypoints waiting, oldest first.
    pub fn waiting(&self) -> impl Iterator<Item = &Shared> {
        self.items.iter()
    }

    pub fn clear(&mut self) {
        self.items.clear();
    }
}

/// The dimension whose waypoints the map of `dimension` also shows, scaled:
/// the Overworld's on the Nether's and back.
pub fn paired_dimension(dimension: &str) -> Option<&'static str> {
    match dimension {
        "minecraft:overworld" => Some("minecraft:the_nether"),
        "minecraft:the_nether" => Some("minecraft:overworld"),
        _ => None,
    }
}

/// Blocks of the Overworld per block of a dimension (the Nether's 8).
pub fn dimension_scale(dimension: &str) -> f64 {
    if dimension == "minecraft:the_nether" {
        8.0
    } else {
        1.0
    }
}

/// Waypoints of `from` placed on the map of `to`: x and z scaled by the
/// two dimensions' scales (an Overworld base shows at an eighth in the
/// Nether).
pub fn scaled_to(waypoints: &[&Waypoint], from: &str, to: &str) -> Vec<Waypoint> {
    let factor = dimension_scale(from) / dimension_scale(to);
    waypoints
        .iter()
        .map(|w| {
            let mut w = (*w).clone();
            let scale = |v: i32| ((v as f64 + 0.5) * factor).floor() as i32;
            w.pos[0] = scale(w.pos[0]);
            w.pos[2] = scale(w.pos[2]);
            w
        })
        .collect()
}

/// The waypoints of one dimension of a map, in sets.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DimensionWaypoints {
    pub current_set: String,
    pub sets: BTreeMap<String, Vec<Waypoint>>,
}

impl Default for DimensionWaypoints {
    fn default() -> Self {
        let mut sets = BTreeMap::new();
        sets.insert(DEFAULT_SET.to_owned(), Vec::new());
        Self {
            current_set: DEFAULT_SET.to_owned(),
            sets,
        }
    }
}

impl DimensionWaypoints {
    pub fn current(&self) -> &[Waypoint] {
        self.sets.get(&self.current_set).map_or(&[], Vec::as_slice)
    }

    /// Waypoints to draw: the current set's, or every set's.
    pub fn visible(&self, all_sets: bool) -> Vec<&Waypoint> {
        let iter: Box<dyn Iterator<Item = &Waypoint>> = if all_sets {
            Box::new(self.sets.values().flatten())
        } else {
            Box::new(self.current().iter())
        };
        iter.filter(|w| !w.disabled).collect()
    }

    /// Makes the sets usable whatever a file held: at least the default
    /// set, and a current set that exists.
    pub fn repair(&mut self) {
        if self.sets.is_empty() {
            self.sets.insert(DEFAULT_SET.to_owned(), Vec::new());
        }
        if !self.sets.contains_key(&self.current_set) {
            self.current_set = if self.sets.contains_key(DEFAULT_SET) {
                DEFAULT_SET.to_owned()
            } else {
                self.sets.keys().next().cloned().unwrap_or_default()
            };
        }
    }

    /// Switches to the next set (in name order), wrapping; returns its name.
    pub fn next_set(&mut self) -> &str {
        self.step_set(1)
    }

    /// Switches to the previous set (in name order), wrapping.
    pub fn previous_set(&mut self) -> &str {
        self.step_set(-1)
    }

    fn step_set(&mut self, by: isize) -> &str {
        self.repair();
        let names: Vec<&String> = self.sets.keys().collect();
        let n = names.len() as isize;
        let i = names
            .iter()
            .position(|n| **n == self.current_set)
            .unwrap_or(0) as isize;
        self.current_set = names[(i + by).rem_euclid(n) as usize].clone();
        &self.current_set
    }

    pub fn create_set(&mut self, name: &str) -> bool {
        let name = name.trim();
        if name.is_empty() || self.sets.contains_key(name) {
            return false;
        }
        self.sets.insert(name.to_owned(), Vec::new());
        true
    }

    /// Deletes a set and its waypoints. The current set falls back to the
    /// default one, or the first; deleting the last set leaves an empty
    /// default set.
    pub fn delete_set(&mut self, name: &str) -> bool {
        if self.sets.remove(name).is_none() {
            return false;
        }
        self.repair();
        true
    }

    /// Renames a set (to a name not taken yet), keeping it current if it
    /// was.
    pub fn rename_set(&mut self, old: &str, new: &str) -> bool {
        let new = new.trim();
        if new.is_empty() || self.sets.contains_key(new) {
            return false;
        }
        let Some(list) = self.sets.remove(old) else {
            return false;
        };
        self.sets.insert(new.to_owned(), list);
        if self.current_set == old {
            self.current_set = new.to_owned();
        }
        true
    }

    /// The set holding waypoint `id`.
    pub fn set_of(&self, id: u64) -> Option<&str> {
        self.sets
            .iter()
            .find(|(_, list)| list.iter().any(|w| w.id == id))
            .map(|(name, _)| name.as_str())
    }

    pub fn find(&self, id: u64) -> Option<&Waypoint> {
        self.sets.values().flatten().find(|w| w.id == id)
    }

    pub fn find_mut(&mut self, id: u64) -> Option<&mut Waypoint> {
        self.sets.values_mut().flatten().find(|w| w.id == id)
    }

    pub fn remove(&mut self, id: u64) -> Option<Waypoint> {
        for list in self.sets.values_mut() {
            if let Some(i) = list.iter().position(|w| w.id == id) {
                return Some(list.remove(i));
            }
        }
        None
    }
}

/// All waypoints of one map, every dimension, with its file.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct WaypointBook {
    pub version: u32,
    pub next_id: u64,
    pub dimensions: BTreeMap<String, DimensionWaypoints>,
    #[serde(skip)]
    path: Option<PathBuf>,
    #[serde(skip)]
    pub dirty: bool,
    /// "Delete reached deathpoints" may act: the player has been away from
    /// the deathpoints since the last death (see
    /// [`WaypointBook::reach_deathpoints`]).
    #[serde(skip)]
    deathpoints_armed: bool,
}

/// A world or server map that has waypoints: its folder and a label
/// (`MyWorld`, `play.example.net`, `play.example.net / map-2`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BookPlace {
    pub dir: PathBuf,
    pub label: String,
}

/// Undoes `persist::escape_component` for display.
fn unescape(name: &str) -> String {
    let bytes = name.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        // Two hex digits after `%`, read from the bytes: a slice of the
        // `str` could end inside a multi-byte character.
        let hex = |b: u8| (b as char).to_digit(16);
        if bytes[i] == b'%'
            && let (Some(hi), Some(lo)) = (
                bytes.get(i + 1).copied().and_then(hex),
                bytes.get(i + 2).copied().and_then(hex),
            )
        {
            out.push((hi * 16 + lo) as u8);
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Every map with a waypoint file under `data_dir/worlds`, sorted by label.
pub fn book_places(data_dir: &Path) -> Vec<BookPlace> {
    let mut out = Vec::new();
    let Ok(sources) = std::fs::read_dir(data_dir.join("worlds")) else {
        return out;
    };
    for source in sources.flatten() {
        let folder = source.file_name().to_string_lossy().into_owned();
        let name = folder
            .strip_prefix("sp-")
            .or_else(|| folder.strip_prefix("mp-"))
            .unwrap_or(&folder);
        let name = unescape(name.trim_start_matches('_'));
        let Ok(maps) = std::fs::read_dir(source.path()) else {
            continue;
        };
        for map in maps.flatten() {
            if !map.path().join(FILE).is_file() {
                continue;
            }
            let map_name = unescape(&map.file_name().to_string_lossy());
            let label = if map_name == crate::engine::DEFAULT_MAP {
                name.clone()
            } else {
                format!("{name} / {map_name}")
            };
            out.push(BookPlace {
                dir: map.path(),
                label,
            });
        }
    }
    out.sort_by_key(|p| p.label.to_lowercase());
    out
}

impl Default for WaypointBook {
    fn default() -> Self {
        Self {
            version: 1,
            next_id: 1,
            dimensions: BTreeMap::new(),
            path: None,
            dirty: false,
            deathpoints_armed: false,
        }
    }
}

impl WaypointBook {
    /// Reads the book in a map folder (empty when there is none yet). A
    /// broken file is moved aside, never overwritten silently.
    pub fn load(map_dir: &Path) -> Self {
        let path = map_dir.join(FILE);
        let mut book = match std::fs::read(&path) {
            Ok(bytes) => serde_json::from_slice::<WaypointBook>(&bytes).unwrap_or_else(|e| {
                log::warn!(
                    "meridian: {} is not valid ({e}); keeping it as .broken",
                    path.display()
                );
                let _ = std::fs::rename(&path, path.with_extension("json.broken"));
                WaypointBook::default()
            }),
            Err(_) => WaypointBook::default(),
        };
        // Temporary waypoints never outlive a session.
        for dim in book.dimensions.values_mut() {
            // A file may hold no sets (`"sets": {}`) or name a missing one.
            dim.repair();
            for list in dim.sets.values_mut() {
                list.retain(|w| w.kind != Kind::Temporary);
            }
        }
        let max = book
            .dimensions
            .values()
            .flat_map(|d| d.sets.values().flatten())
            .map(|w| w.id)
            .max();
        book.next_id = book.next_id.max(max.unwrap_or(0) + 1);
        book.path = Some(path);
        book
    }

    pub fn save(&mut self) -> Result<(), PersistError> {
        let Some(path) = &self.path else {
            return Ok(());
        };
        // Temporary waypoints stay in memory only.
        let mut lasting = self.clone();
        for dim in lasting.dimensions.values_mut() {
            for list in dim.sets.values_mut() {
                list.retain(|w| w.kind != Kind::Temporary);
            }
        }
        let json =
            serde_json::to_vec_pretty(&lasting).map_err(|e| PersistError::Encode(e.to_string()))?;
        persist::write_atomic(path, &json)?;
        self.dirty = false;
        Ok(())
    }

    pub fn dimension(&self, dimension: &str) -> Option<&DimensionWaypoints> {
        self.dimensions.get(dimension)
    }

    pub fn dimension_mut(&mut self, dimension: &str) -> &mut DimensionWaypoints {
        self.dimensions.entry(dimension.to_owned()).or_default()
    }

    /// Saves what an editor produced: replaces waypoint `edit` (keeping its
    /// id, kind and creation time; in its place, or on top of `set` when it
    /// moves there) or adds a new one on top of `set`. Returns the
    /// waypoint's id.
    pub fn save_edit(
        &mut self,
        dimension: &str,
        edit: Option<u64>,
        set: &str,
        mut w: Waypoint,
        now: u64,
    ) -> u64 {
        let dim = self.dimension_mut(dimension);
        if let Some(id) = edit
            && let Some(old) = dim.find(id).cloned()
        {
            w.id = old.id;
            w.created = old.created;
            w.kind = old.kind;
            let place = dim
                .sets
                .get(set)
                .and_then(|l| l.iter().position(|x| x.id == id));
            match place {
                Some(i) => dim.sets.get_mut(set).expect("found above")[i] = w,
                None => {
                    dim.remove(id);
                    dim.sets.entry(set.to_owned()).or_default().insert(0, w);
                }
            }
            self.dirty = true;
            return id;
        }
        self.insert(dimension, set, w, false, now)
    }

    /// Adds a waypoint to `set` of a dimension, on top or at the bottom;
    /// returns its id.
    pub fn insert(
        &mut self,
        dimension: &str,
        set: &str,
        mut w: Waypoint,
        to_bottom: bool,
        now: u64,
    ) -> u64 {
        w.id = self.next_id;
        self.next_id += 1;
        if w.created == 0 {
            w.created = now;
        }
        if w.initials.is_empty() {
            w.initials = initials_of(&w.name);
        }
        let id = w.id;
        let list = self
            .dimension_mut(dimension)
            .sets
            .entry(set.to_owned())
            .or_default();
        if to_bottom {
            list.push(w)
        } else {
            list.insert(0, w)
        }
        self.dirty = true;
        id
    }

    /// Adds a waypoint to the dimension's current set; returns its id.
    pub fn add(&mut self, dimension: &str, w: Waypoint, to_bottom: bool, now: u64) -> u64 {
        let set = self.dimension_mut(dimension).current_set.clone();
        self.insert(dimension, &set, w, to_bottom, now)
    }

    /// Adds a temporary waypoint (global, as the reference's are by
    /// default) to the dimension's current set, unless one with the same
    /// position is already there. The oldest temporary ones go beyond
    /// [`MAX_TEMPORARY`]. Returns the new id.
    pub fn add_temporary(&mut self, dimension: &str, mut w: Waypoint, now: u64) -> Option<u64> {
        let dim = self.dimension_mut(dimension);
        if dim.current().iter().any(|x| x.pos == w.pos) {
            return None;
        }
        let mut temporary: Vec<u64> = dim
            .sets
            .values()
            .flatten()
            .filter(|x| x.kind == Kind::Temporary)
            .map(|x| x.id)
            .collect();
        temporary.sort_unstable();
        let excess = (temporary.len() + 1).saturating_sub(MAX_TEMPORARY);
        for id in &temporary[..excess] {
            dim.remove(*id);
        }
        w.kind = Kind::Temporary;
        w.global = true;
        Some(self.add(dimension, w, false, now))
    }

    /// A death at `pos`: the latest deathpoint becomes an old one (kept as a
    /// waypoint) or goes, and a new one is added. Returns the new id.
    pub fn record_death(
        &mut self,
        dimension: &str,
        pos: [i32; 3],
        keep_old: bool,
        now: u64,
    ) -> u64 {
        for dim in self.dimensions.values_mut() {
            for list in dim.sets.values_mut() {
                if keep_old {
                    for w in list.iter_mut().filter(|w| w.kind == Kind::Death) {
                        w.kind = Kind::OldDeath;
                        w.name = format!("Death ({})", w.name.trim_start_matches("Death").trim());
                        w.name = w.name.replace(" ()", "");
                    }
                } else {
                    list.retain(|w| w.kind != Kind::Death);
                }
            }
        }
        let mut w = Waypoint::new("Death", pos, 0);
        w.kind = Kind::Death;
        w.initials = "X".to_owned();
        w.global = true;
        // The player is at the new deathpoint: not reached until they have
        // been away from it.
        self.deathpoints_armed = false;
        self.add(dimension, w, false, now)
    }

    /// "Delete reached deathpoints" for one frame: removes the dimension's
    /// deathpoints the player stands at, but only while `playing` (not on
    /// the death screen, which shows right at the deathpoint) and once the
    /// player has been further than [`REACHED_DISTANCE`] from every
    /// deathpoint of their dimension since the last death or load.
    pub fn reach_deathpoints(&mut self, dimension: &str, player: [f64; 3], playing: bool) -> usize {
        if !playing {
            return 0;
        }
        if !self.deathpoints_armed {
            let near = self.dimension(dimension).is_some_and(|d| {
                d.sets
                    .values()
                    .flatten()
                    .any(|w| w.kind == Kind::Death && w.distance(player) <= REACHED_DISTANCE)
            });
            self.deathpoints_armed = !near;
            return 0;
        }
        self.remove_reached_deathpoints(dimension, player)
    }

    /// Removes deathpoints the player stands at; returns how many.
    pub fn remove_reached_deathpoints(&mut self, dimension: &str, player: [f64; 3]) -> usize {
        let Some(dim) = self.dimensions.get_mut(dimension) else {
            return 0;
        };
        let mut removed = 0;
        for list in dim.sets.values_mut() {
            let before = list.len();
            list.retain(|w| w.kind != Kind::Death || w.distance(player) > REACHED_DISTANCE);
            removed += before - list.len();
        }
        if removed > 0 {
            self.dirty = true;
        }
        removed
    }
}

/// Seconds since 1970, for `created`.
pub fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn waypoint_places_on_disk() {
        let dir = std::env::temp_dir().join(format!("meridian-places-{}", std::process::id()));
        for (source, map) in [
            ("sp-my%20world", "default"),
            ("mp-play.example.net", "map-2"),
            ("sp-empty", "default"),
        ] {
            std::fs::create_dir_all(dir.join("worlds").join(source).join(map)).unwrap();
        }
        for (source, map) in [
            ("sp-my%20world", "default"),
            ("mp-play.example.net", "map-2"),
        ] {
            std::fs::write(dir.join("worlds").join(source).join(map).join(FILE), "{}").unwrap();
        }
        let labels: Vec<String> = book_places(&dir).into_iter().map(|p| p.label).collect();
        assert_eq!(
            labels,
            ["my world", "play.example.net / map-2"],
            "maps without waypoints are left out"
        );
        std::fs::remove_dir_all(&dir).ok();
    }
    use crate::persist::test_dir;

    const OW: &str = "minecraft:overworld";

    #[test]
    fn initials() {
        assert_eq!(initials_of("home"), "H");
        assert_eq!(initials_of("Iron Farm"), "IF");
        assert_eq!(initials_of("  "), "?");
        assert_eq!(initials_of("ünter welt"), "ÜW");
    }

    #[test]
    fn distances_and_labels() {
        let w = Waypoint::new("a", [10, 64, 0], 1);
        assert!((w.distance([10.5, 64.5, 0.5])).abs() < 1e-9);
        assert_eq!(distance_label(123.4, 10_000, 1), "123m");
        assert_eq!(distance_label(12_345.0, 10_000, 1), "12.3km");
        assert_eq!(distance_label(12_345.0, 0, 1), "12345m");
    }

    #[test]
    fn teleport_commands() {
        let mut w = Waypoint::new("Base", [1, -2, 3], 0);
        assert_eq!(command("/tp @s {x} {y} {z}", &w, OW), "/tp @s 1 -2 3");
        assert_eq!(
            command("/tp @s {x} {y} {z} {yaw} ~", &w, OW),
            "/tp @s 1 -2 3 ~ ~"
        );
        w.yaw = Some(90.4);
        assert_eq!(
            command("/execute in {d} run tp @s {x} {y} {z} {yaw} ~", &w, OW),
            "/execute in minecraft:overworld run tp @s 1 -2 3 90 ~"
        );
    }

    #[test]
    fn share_round_trip() {
        let w = Waypoint::new("My \"Base\"", [-100, 70, 2000], 3);
        let text = share_text(&w, OW);
        assert_eq!(
            text,
            "[Meridian] waypoint \"My 'Base'\" at -100 70 2000 in minecraft:overworld"
        );
        let (back, dim) = parse_shared(&format!("<Steve> {text}")).unwrap();
        assert_eq!(
            (back.name.as_str(), back.pos, dim.as_deref()),
            ("My 'Base'", [-100, 70, 2000], Some(OW))
        );
        assert_eq!(back.kind, Kind::Temporary);
        assert!(back.global, "temporary waypoints are global by default");
        // An unknown height travels as `~`; long names are cut.
        let far = Waypoint::new("x".repeat(100), [5, 64, 6], 0).with_height(None);
        let text = share_text(&far, OW);
        assert!(text.contains(" at 5 ~ 6 in "), "{text}");
        let (back, _) = parse_shared(&text).unwrap();
        assert_eq!(back.height(), None);
        assert_eq!(back.name.chars().count(), MAX_SHARED_NAME);
        assert!(parse_shared("hello at 1 2 3").is_none());
    }

    #[test]
    fn sets() {
        let mut d = DimensionWaypoints::default();
        assert!(d.create_set("nether hub"));
        assert!(!d.create_set("nether hub"));
        assert_eq!(d.next_set(), "nether hub");
        assert_eq!(d.next_set(), DEFAULT_SET);
    }

    #[test]
    fn add_order_visibility_and_removal() {
        let mut book = WaypointBook::default();
        let a = book.add(OW, Waypoint::new("a", [0, 0, 0], 0), false, 5);
        let b = book.add(OW, Waypoint::new("b", [0, 0, 0], 0), false, 6);
        let c = book.add(OW, Waypoint::new("c", [0, 0, 0], 0), true, 7);
        let names: Vec<_> = book
            .dimension(OW)
            .unwrap()
            .current()
            .iter()
            .map(|w| w.name.as_str())
            .collect();
        assert_eq!(
            names,
            ["b", "a", "c"],
            "new ones on top unless asked for the bottom"
        );
        book.dimension_mut(OW).find_mut(a).unwrap().disabled = true;
        assert_eq!(book.dimension(OW).unwrap().visible(false).len(), 2);
        assert_eq!(book.dimension_mut(OW).remove(b).unwrap().name, "b");
        assert!(book.dimension(OW).unwrap().find(c).is_some());
        assert!(book.dirty);
    }

    #[test]
    fn deathpoints() {
        let mut book = WaypointBook::default();
        let first = book.record_death(OW, [1, 2, 3], true, 1);
        let second = book.record_death(OW, [4, 5, 6], true, 2);
        let dim = book.dimension(OW).unwrap();
        assert_eq!(dim.find(first).unwrap().kind, Kind::OldDeath);
        assert_eq!(dim.find(second).unwrap().kind, Kind::Death);
        book.record_death(OW, [7, 8, 9], false, 3);
        let deaths: Vec<_> = book
            .dimension(OW)
            .unwrap()
            .current()
            .iter()
            .filter(|w| w.kind == Kind::Death)
            .collect();
        assert_eq!(
            deaths.len(),
            1,
            "without keeping, the previous deathpoint goes"
        );
        assert_eq!(book.remove_reached_deathpoints(OW, [7.5, 8.0, 9.5]), 1);
        assert_eq!(book.remove_reached_deathpoints(OW, [7.5, 8.0, 9.5]), 0);
    }

    #[test]
    fn unknown_heights_teleport_with_tilde() {
        let w = Waypoint::at(
            "Here",
            Spot {
                x: 3,
                y: None,
                z: -4,
            },
            0,
        );
        assert_eq!(command("/tp @s {x} {y} {z}", &w, OW), "/tp @s 3 ~ -4");
        assert_eq!(w.height(), None);
        assert!(
            (w.distance([3.5, 200.0, -3.5])).abs() < 1e-9,
            "along the ground only"
        );
        let known = Waypoint::at(
            "Here",
            Spot {
                x: 3,
                y: Some(70),
                z: -4,
            },
            0,
        );
        assert_eq!(command("/tp @s {x} {y} {z}", &known, OW), "/tp @s 3 70 -4");
        // The flag is only written when set, and reads back.
        let json = serde_json::to_string(&known).unwrap();
        assert!(!json.contains("y_unknown"));
        let back: Waypoint = serde_json::from_str(&serde_json::to_string(&w).unwrap()).unwrap();
        assert!(back.y_unknown);
    }

    #[test]
    fn teleports_pick_the_dimension_and_rotation_formats() {
        const NETHER: &str = "minecraft:the_nether";
        let settings = crate::config::Settings::default();
        let f = TeleportFormats::of(&settings);
        let mut w = Waypoint::new("Fort", [10, 70, -20], 0);
        assert_eq!(teleport_command(&w, OW, OW, f), "/tp @s 10 70 -20");
        assert_eq!(
            teleport_command(&w, NETHER, OW, f),
            "/execute as @s in minecraft:the_nether run tp 10 70 -20",
            "another dimension: its own coordinates, in it"
        );
        w.yaw = Some(-90.0);
        assert_eq!(teleport_command(&w, OW, OW, f), "/tp @s 10 70 -20 -90 ~");
    }

    #[test]
    fn deathpoint_survives_until_reached_again() {
        let mut book = WaypointBook::default();
        let spot = [100.5, 64.0, 100.5];
        // The death screen: the deathpoint is made where the player lies.
        let id = book.record_death(OW, [100, 64, 100], true, 1);
        for _ in 0..10 {
            assert_eq!(book.reach_deathpoints(OW, spot, false), 0, "dead: never");
        }
        // Respawned next to it (a bed close by): still not reached.
        for _ in 0..10 {
            assert_eq!(book.reach_deathpoints(OW, [102.0, 64.0, 101.0], true), 0);
        }
        assert!(book.dimension(OW).unwrap().find(id).is_some());
        // Away, then back: reached.
        assert_eq!(book.reach_deathpoints(OW, [0.0, 64.0, 0.0], true), 0);
        assert_eq!(
            book.reach_deathpoints(OW, spot, false),
            0,
            "not while a screen is open"
        );
        assert_eq!(book.reach_deathpoints(OW, spot, true), 1);
        assert!(book.dimension(OW).unwrap().find(id).is_none());
        // A new death disarms again; a respawn far away arms at once.
        let id = book.record_death(OW, [100, 64, 100], true, 2);
        assert_eq!(
            book.reach_deathpoints(OW, spot, true),
            0,
            "at it right after dying"
        );
        assert!(book.dimension(OW).unwrap().find(id).is_some());
        assert_eq!(book.reach_deathpoints(OW, [-500.0, 64.0, 0.0], true), 0);
        assert_eq!(book.reach_deathpoints(OW, spot, true), 1);
    }

    #[test]
    fn sets_are_created_renamed_and_deleted() {
        let mut d = DimensionWaypoints::default();
        assert!(d.create_set("mine"));
        assert!(d.rename_set(DEFAULT_SET, "home"));
        assert_eq!(d.current_set, "home", "the current set keeps being current");
        assert!(!d.rename_set("home", "mine"), "names stay unique");
        assert_eq!(d.set_of(1), None);
        d.sets.get_mut("mine").unwrap().push(Waypoint {
            id: 1,
            ..Waypoint::default()
        });
        assert_eq!(d.set_of(1), Some("mine"));
        assert!(d.delete_set("home"));
        assert_eq!(d.current_set, "mine");
        assert!(d.delete_set("mine"));
        assert_eq!(
            d.current_set, DEFAULT_SET,
            "the last set leaves a default one"
        );
        assert!(d.sets[DEFAULT_SET].is_empty());
        assert!(!d.delete_set("nope"));
        // An empty set map (a hand-edited file) does not break switching.
        d.sets.clear();
        assert_eq!(d.next_set(), DEFAULT_SET);
        d.sets.clear();
        assert_eq!(d.previous_set(), DEFAULT_SET);
    }

    #[test]
    fn empty_sets_in_a_file_get_the_default_set() {
        let dir = test_dir("waypoints_no_sets");
        std::fs::write(
            dir.join(FILE),
            r#"{"dimensions":{"minecraft:overworld":{"current_set":"gone","sets":{}}}}"#,
        )
        .unwrap();
        let mut book = WaypointBook::load(&dir);
        let dim = book.dimension_mut(OW);
        assert_eq!(dim.current_set, DEFAULT_SET);
        assert_eq!(dim.next_set(), DEFAULT_SET);
    }

    #[test]
    fn folder_names_unescape_on_character_boundaries() {
        assert_eq!(unescape("my%20world"), "my world");
        assert_eq!(unescape("%e2%82%ac"), "€");
        // A `%` before a multi-byte character, or at the end, stays.
        assert_eq!(unescape("%a€"), "%a€");
        assert_eq!(unescape("a%€b"), "a%€b");
        assert_eq!(unescape("50%"), "50%");
        assert_eq!(unescape("%4"), "%4");
    }

    #[test]
    fn temporary_waypoints_are_capped_and_not_saved() {
        let dir = test_dir("waypoints_temporary");
        let mut book = WaypointBook::load(&dir);
        for i in 0..(MAX_TEMPORARY as i32 + 5) {
            book.add_temporary(OW, Waypoint::new(format!("t{i}"), [i, 64, 0], 0), 1)
                .unwrap();
        }
        let dim = book.dimension(OW).unwrap();
        let names: Vec<&str> = dim.current().iter().map(|w| w.name.as_str()).collect();
        assert_eq!(names.len(), MAX_TEMPORARY);
        assert_eq!(
            names[0],
            format!("t{}", MAX_TEMPORARY + 4),
            "the newest stay"
        );
        assert!(!names.contains(&"t0"), "the oldest went");
        assert!(
            dim.current()
                .iter()
                .all(|w| w.kind == Kind::Temporary && w.global)
        );
        assert!(
            book.add_temporary(OW, Waypoint::new("dup", [20, 64, 0], 0), 2)
                .is_none(),
            "one at the same place is not added twice"
        );
        book.add(OW, Waypoint::new("keep", [1, 2, 3], 0), false, 3);
        book.save().unwrap();
        let file = std::fs::read_to_string(dir.join(FILE)).unwrap();
        assert!(!file.contains("temporary"), "{file}");
        assert_eq!(
            book.dimension(OW).unwrap().current().len(),
            MAX_TEMPORARY + 1,
            "kept in memory"
        );
    }

    #[test]
    fn shared_waypoints_wait_to_be_accepted() {
        let mut inbox = SharedInbox::default();
        let w = |n: &str| {
            parse_shared(&format!("[Meridian] waypoint \"{n}\" at 1 2 3 in {OW}")).unwrap()
        };
        let (a, dim) = w("a");
        let first = inbox.receive(a.clone(), dim.clone());
        assert_eq!(
            inbox.receive(a, dim),
            first,
            "the same share keeps its number"
        );
        for i in 0..SHARED_KEPT {
            let (x, d) = w(&format!("n{i}"));
            inbox.receive(x, d);
        }
        assert_eq!(inbox.waiting().count(), SHARED_KEPT);
        assert!(inbox.take(Some(first)).is_none(), "the oldest was dropped");
        let latest = inbox.take(None).unwrap();
        assert_eq!(latest.waypoint.name, format!("n{}", SHARED_KEPT - 1));
        assert_eq!(latest.dimension.as_deref(), Some(OW));
        let n = inbox.waiting().next().unwrap().number;
        assert!(inbox.take(Some(n)).is_some());
        assert!(inbox.take(Some(n)).is_none());
    }

    #[test]
    fn paired_dimension_waypoints_scale_by_eight() {
        assert_eq!(paired_dimension(OW), Some("minecraft:the_nether"));
        assert_eq!(paired_dimension("minecraft:the_end"), None);
        let base = Waypoint::new("Base", [800, 70, -805], 0);
        let shown = scaled_to(&[&base], OW, "minecraft:the_nether");
        assert_eq!(shown[0].pos, [100, 70, -101]);
        let portal = Waypoint::new("Portal", [-3, 70, 12], 0);
        assert_eq!(
            scaled_to(&[&portal], "minecraft:the_nether", OW)[0].pos,
            [-20, 70, 100]
        );
    }

    #[test]
    fn file_round_trip_drops_temporary() {
        let dir = test_dir("waypoints");
        let mut book = WaypointBook::load(&dir);
        let keep = book.add(OW, Waypoint::new("keep", [1, 2, 3], 4), false, 1);
        let mut temp = Waypoint::new("temp", [0, 0, 0], 0);
        temp.kind = Kind::Temporary;
        book.add("minecraft:the_nether", temp, false, 1);
        book.save().unwrap();
        assert!(!book.dirty);
        let back = WaypointBook::load(&dir);
        assert_eq!(back.dimension(OW).unwrap().find(keep).unwrap().name, "keep");
        assert!(
            back.dimension("minecraft:the_nether")
                .unwrap()
                .current()
                .is_empty()
        );
        assert!(back.next_id > keep);
    }

    #[test]
    fn broken_file_is_kept_aside() {
        let dir = test_dir("waypoints_broken");
        std::fs::write(dir.join(FILE), b"{ nope").unwrap();
        let book = WaypointBook::load(&dir);
        assert!(book.dimensions.is_empty());
        assert!(dir.join("waypoints.json.broken").exists());
    }
}

#[cfg(test)]
mod save_tests {
    use super::*;

    #[test]
    fn save_adds_and_replaces() {
        let mut book = WaypointBook::default();
        let id = book.save_edit("d", None, "mine", Waypoint::new("A", [1, 2, 3], 4), 10);
        let dim = book.dimension("d").unwrap();
        assert_eq!(dim.sets["mine"][0].name, "A");
        assert_eq!(dim.sets["mine"][0].created, 10);
        // An edit in the same set keeps the waypoint's place.
        let other = book.save_edit("d", None, "mine", Waypoint::new("Z", [0, 0, 0], 4), 11);
        book.save_edit("d", Some(id), "mine", Waypoint::new("A2", [1, 2, 3], 4), 12);
        let names: Vec<&str> = book.dimension("d").unwrap().sets["mine"]
            .iter()
            .map(|w| w.name.as_str())
            .collect();
        assert_eq!(names, ["Z", "A2"]);
        book.dimension_mut("d").remove(other);
        let mut changed = Waypoint::new("B", [0, 0, 0], 1);
        changed.created = 99;
        assert_eq!(book.save_edit("d", Some(id), DEFAULT_SET, changed, 20), id);
        let dim = book.dimension("d").unwrap();
        assert!(dim.sets["mine"].is_empty(), "moved to another set");
        let w = dim.find(id).unwrap();
        assert_eq!(
            (w.name.as_str(), w.created),
            ("B", 10),
            "keeps its creation time"
        );
    }
}
