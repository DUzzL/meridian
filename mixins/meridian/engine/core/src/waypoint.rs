//! Waypoints: the model, sets, deathpoints, storage per world, commands
//! built from them, and sharing in chat.
//!
//! Waypoints belong to a map (`persist::MapPaths`'s source and map) and are
//! kept per dimension in one JSON file per map:
//! `worlds/<source>/<map>/waypoints.json`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::persist::{self, PersistError};

/// The set every map starts with.
pub const DEFAULT_SET: &str = "default";
/// The waypoint file inside a map's folder.
pub const FILE: &str = "waypoints.json";
/// Blocks from a deathpoint that count as reaching it.
pub const REACHED_DISTANCE: f64 = 4.0;

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

    pub fn rgb(&self) -> u32 {
        COLORS[(self.color % 16) as usize]
    }

    /// Distance in blocks from a point, centred on the waypoint's block.
    pub fn distance(&self, from: [f64; 3]) -> f64 {
        let d = |a: i32, b: f64| a as f64 + 0.5 - b;
        let (dx, dy, dz) = (
            d(self.pos[0], from[0]),
            d(self.pos[1], from[1]),
            d(self.pos[2], from[2]),
        );
        (dx * dx + dy * dy + dz * dz).sqrt()
    }
}

/// "12m", "1.2km" from `km_from` blocks on, with `precision` decimals.
pub fn distance_label(blocks: f64, km_from: u32, precision: usize) -> String {
    if km_from > 0 && blocks >= km_from as f64 {
        format!("{:.*}km", precision, blocks / 1000.0)
    } else {
        format!("{}m", blocks.round() as i64)
    }
}

/// Fills a teleport command template: `{x}`, `{y}`, `{z}`, `{yaw}`, `{d}`
/// (dimension) and `{name}`.
pub fn command(template: &str, w: &Waypoint, dimension: &str) -> String {
    template
        .replace("{x}", &w.pos[0].to_string())
        .replace("{y}", &w.pos[1].to_string())
        .replace("{z}", &w.pos[2].to_string())
        .replace(
            "{yaw}",
            &w.yaw.map_or("~".to_owned(), |y| format!("{y:.0}")),
        )
        .replace("{d}", dimension)
        .replace("{name}", &w.name)
}

/// The chat line that shares a waypoint, readable by people without the mod
/// and parsed back by [`parse_shared`].
pub fn share_text(w: &Waypoint, dimension: &str) -> String {
    format!(
        "[Meridian] waypoint \"{}\" at {} {} {} in {}",
        w.name.replace('"', "'"),
        w.pos[0],
        w.pos[1],
        w.pos[2],
        dimension
    )
}

/// A waypoint shared in chat: Meridian's own line, or a bare
/// `x y z` / `x, y, z` triple after a name-like prefix.
pub fn parse_shared(message: &str) -> Option<(String, [i32; 3], Option<String>)> {
    if let Some(start) = message.find("[Meridian] waypoint \"") {
        let rest = &message[start + "[Meridian] waypoint \"".len()..];
        let end = rest.find('"')?;
        let name = rest[..end].to_owned();
        let rest = rest[end + 1..].trim_start().strip_prefix("at ")?;
        let mut parts = rest.split_whitespace();
        let mut coord = || parts.next()?.parse::<i32>().ok();
        let pos = [coord()?, coord()?, coord()?];
        let dimension = rest.split(" in ").nth(1).map(|d| d.trim().to_owned());
        return Some((name, pos, dimension));
    }
    None
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

    /// Switches to the next set (in name order), wrapping; returns its name.
    pub fn next_set(&mut self) -> &str {
        let names: Vec<&String> = self.sets.keys().collect();
        let i = names
            .iter()
            .position(|n| **n == self.current_set)
            .unwrap_or(0);
        self.current_set = names[(i + 1) % names.len()].clone();
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
}

impl Default for WaypointBook {
    fn default() -> Self {
        Self {
            version: 1,
            next_id: 1,
            dimensions: BTreeMap::new(),
            path: None,
            dirty: false,
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
        let json =
            serde_json::to_vec_pretty(self).map_err(|e| PersistError::Encode(e.to_string()))?;
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

    /// Adds a waypoint to the dimension's current set; returns its id.
    pub fn add(&mut self, dimension: &str, mut w: Waypoint, to_bottom: bool, now: u64) -> u64 {
        w.id = self.next_id;
        self.next_id += 1;
        if w.created == 0 {
            w.created = now;
        }
        if w.initials.is_empty() {
            w.initials = initials_of(&w.name);
        }
        let dim = self.dimension_mut(dimension);
        let set = dim.current_set.clone();
        let list = dim.sets.entry(set).or_default();
        if to_bottom {
            list.push(w)
        } else {
            list.insert(0, w)
        }
        self.dirty = true;
        self.next_id - 1
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
        self.add(dimension, w, false, now)
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
        let (name, pos, dim) = parse_shared(&format!("<Steve> {text}")).unwrap();
        assert_eq!(
            (name.as_str(), pos, dim.as_deref()),
            ("My 'Base'", [-100, 70, 2000], Some(OW))
        );
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
