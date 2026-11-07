//! Completed structures: which markers the player ticked off, per world or
//! server and seed (`SeedAtlasConfig.StructureKey`, the completed groups and
//! `CompletedBadge` of the Fabric version).
//!
//! A structure counts as completed when one is stored at its position or
//! within [`COMPLETED_TOLERANCE`] blocks of it, so progress survives a
//! version or engine update that moves a structure a little. The store is
//! kept in `completed.json` in Meridian's data folder.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::persist;
use crate::seedatlas_state::COMPLETED_TOLERANCE;

/// What identifies a completed structure. Y is deliberately left out:
/// approximate structure heights can change with sampling.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct StructureKey {
    /// The world or server (`persist::source_key`).
    pub world: String,
    pub seed: i64,
    pub large_biomes: bool,
    /// The dimension id, e.g. `minecraft:the_nether`.
    pub dimension: String,
    /// The marker kind's id (`village`, `fortress`...).
    #[serde(rename = "type")]
    pub kind: String,
    pub x: i32,
    pub z: i32,
}

impl StructureKey {
    /// `None` when the world, dimension or kind is blank.
    pub fn new(
        world: &str,
        seed: i64,
        large_biomes: bool,
        dimension: &str,
        kind: &str,
        x: i32,
        z: i32,
    ) -> Option<Self> {
        if [world, dimension, kind].iter().any(|s| s.trim().is_empty()) {
            return None;
        }
        Some(Self {
            world: world.to_owned(),
            seed,
            large_biomes,
            dimension: dimension.to_owned(),
            kind: kind.to_owned(),
            x,
            z,
        })
    }

    fn group(&self) -> Group {
        (
            self.world.clone(),
            self.seed,
            self.large_biomes,
            self.dimension.clone(),
            self.kind.clone(),
        )
    }
}

/// Everything that identifies a completed structure except its position.
type Group = (String, i64, bool, String, String);

#[derive(Default, Serialize, Deserialize)]
struct CompletedFile {
    #[serde(default)]
    version: u32,
    #[serde(default)]
    completed: Vec<StructureKey>,
}

/// The completed structures of every world, saved after each change.
#[derive(Debug, Default)]
pub struct CompletedStore {
    path: PathBuf,
    /// In the order they were marked.
    order: Vec<StructureKey>,
    set: HashSet<StructureKey>,
    groups: HashMap<Group, Vec<StructureKey>>,
}

impl CompletedStore {
    /// Every completed structure, in the order it was marked.
    pub fn keys(&self) -> impl Iterator<Item = &StructureKey> {
        self.order.iter()
    }

    pub fn load(data_dir: &Path) -> Self {
        let path = data_dir.join("completed.json");
        let file = std::fs::read(&path)
            .ok()
            .and_then(|b| serde_json::from_slice::<CompletedFile>(&b).ok())
            .unwrap_or_default();
        let mut store = Self {
            path,
            ..Self::default()
        };
        for key in file.completed {
            store.insert(key);
        }
        store
    }

    /// Adds an entry exactly as given, without merging it into a nearby one.
    fn insert(&mut self, key: StructureKey) -> bool {
        if !self.set.insert(key.clone()) {
            return false;
        }
        self.groups
            .entry(key.group())
            .or_default()
            .push(key.clone());
        self.order.push(key);
        true
    }

    fn remove(&mut self, key: &StructureKey) -> bool {
        if !self.set.remove(key) {
            return false;
        }
        self.order.retain(|k| k != key);
        let group = key.group();
        if let Some(list) = self.groups.get_mut(&group) {
            list.retain(|k| k != key);
            if list.is_empty() {
                self.groups.remove(&group);
            }
        }
        true
    }

    fn near(&self, key: &StructureKey) -> Option<&StructureKey> {
        self.groups.get(&key.group())?.iter().find(|o| {
            (i64::from(o.x) - i64::from(key.x)).abs() <= i64::from(COMPLETED_TOLERANCE)
                && (i64::from(o.z) - i64::from(key.z)).abs() <= i64::from(COMPLETED_TOLERANCE)
        })
    }

    /// Completed at this position or within the tolerance of it.
    pub fn is_completed(&self, key: &StructureKey) -> bool {
        self.set.contains(key) || self.near(key).is_some()
    }

    /// Marks or unmarks a structure (unmarking also removes every entry
    /// within the tolerance); whether anything changed. Saves when it did.
    pub fn set_completed(&mut self, key: &StructureKey, completed: bool) -> bool {
        let changed = if completed {
            !self.is_completed(key) && self.insert(key.clone())
        } else {
            let mut removed = self.remove(key);
            while let Some(near) = self.near(key).cloned() {
                removed |= self.remove(&near);
            }
            removed
        };
        if changed {
            self.save();
        }
        changed
    }

    pub fn mark(&mut self, key: &StructureKey) -> bool {
        self.set_completed(key, true)
    }

    pub fn unmark(&mut self, key: &StructureKey) -> bool {
        self.set_completed(key, false)
    }

    /// Completed structures of a world or server, of one kind or (`None`)
    /// all kinds.
    pub fn count(&self, world: &str, kind: Option<&str>) -> usize {
        self.order
            .iter()
            .filter(|k| k.world == world && kind.is_none_or(|t| k.kind == t))
            .count()
    }

    /// Completed structures of a world or server: all, and per kind, in
    /// one pass (the settings screen shows both every frame).
    pub fn counts(&self, world: &str) -> (usize, std::collections::BTreeMap<String, usize>) {
        let mut by_kind = std::collections::BTreeMap::new();
        let mut all = 0;
        for k in self.order.iter().filter(|k| k.world == world) {
            all += 1;
            *by_kind.entry(k.kind.clone()).or_insert(0) += 1;
        }
        (all, by_kind)
    }

    /// Marks every completed structure of a world or server, of one kind or
    /// all, as not completed; how many were. The UI asks first, see
    /// [`CompletedStore::reset_prompt`].
    pub fn reset(&mut self, world: &str, kind: Option<&str>) -> usize {
        let matching: Vec<StructureKey> = self
            .order
            .iter()
            .filter(|k| k.world == world && kind.is_none_or(|t| k.kind == t))
            .cloned()
            .collect();
        for key in &matching {
            self.remove(key);
        }
        if !matching.is_empty() {
            self.save();
        }
        matching.len()
    }

    /// The question to ask before a reset; `None` when there is nothing to
    /// reset (the button is inactive then).
    pub fn reset_prompt(
        &self,
        world: &str,
        world_label: &str,
        kind: Option<ResetKind<'_>>,
    ) -> Option<ResetPrompt> {
        let count = self.count(world, kind.map(|k| k.id));
        if count == 0 {
            return None;
        }
        let message = match kind {
            None => format!(
                "All {count} completed structures of {world_label} become not completed again."
            ),
            Some(k) => format!(
                "{count} completed of type {} in {world_label} become not completed again.",
                k.name
            ),
        };
        Some(ResetPrompt {
            title: "Reset completed structures?".into(),
            message,
            world: world.to_owned(),
            kind: kind.map(|k| k.id.to_owned()),
        })
    }

    /// Runs a confirmed reset.
    pub fn confirm_reset(&mut self, prompt: &ResetPrompt) -> usize {
        self.reset(&prompt.world, prompt.kind.as_deref())
    }

    /// Every completed structure, oldest first.
    pub fn all(&self) -> &[StructureKey] {
        &self.order
    }

    fn save(&self) {
        let file = CompletedFile {
            version: 1,
            completed: self.order.clone(),
        };
        match serde_json::to_vec_pretty(&file) {
            Ok(bytes) => {
                if let Err(e) = persist::write_atomic(&self.path, &bytes) {
                    log::warn!("meridian: cannot save completed structures: {e}");
                }
            }
            Err(e) => log::warn!("meridian: cannot encode completed structures: {e}"),
        }
    }
}

/// The marker kind a reset is limited to: its id and shown name.
#[derive(Clone, Copy, Debug)]
pub struct ResetKind<'a> {
    pub id: &'a str,
    pub name: &'a str,
}

/// A reset waiting for the player's yes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResetPrompt {
    pub title: String,
    pub message: String,
    pub world: String,
    pub kind: Option<String>,
}

/// The label of the reset button: "Reset completed (3)".
pub fn reset_button_label(count: usize) -> String {
    format!("Reset completed ({count})")
}

/// The completion badge on a marker's lower right corner, in marker units
/// where the icon spans 18 (`CompletedBadge`). `icon` is
/// `[left, top, right, bottom]`; the result has the same shape.
pub const BADGE_SOURCE_SIZE: i32 = 12;
pub const BADGE_DISPLAY_SIZE: i32 = 10;
const BADGE_OVERHANG: i32 = 3;

pub fn badge_rect(icon: [i32; 4]) -> [i32; 4] {
    let left = icon[2] - BADGE_DISPLAY_SIZE + BADGE_OVERHANG;
    let top = icon[3] - BADGE_DISPLAY_SIZE + BADGE_OVERHANG;
    [
        left,
        top,
        left + BADGE_DISPLAY_SIZE,
        top + BADGE_DISPLAY_SIZE,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(world: &str, kind: &str, x: i32, z: i32) -> StructureKey {
        StructureKey::new(world, 42, false, "minecraft:overworld", kind, x, z).unwrap()
    }

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("meridian-done-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn keys_need_an_identity() {
        assert!(StructureKey::new("", 1, false, "minecraft:overworld", "village", 0, 0).is_none());
        assert!(StructureKey::new("w", 1, false, " ", "village", 0, 0).is_none());
        assert!(StructureKey::new("w", 1, false, "minecraft:overworld", "", 0, 0).is_none());
    }

    #[test]
    fn mark_and_unmark_with_tolerance() {
        let dir = temp_dir("mark");
        let mut s = CompletedStore::load(&dir);
        let a = key("sp-a", "village", 100, 200);
        assert!(!s.is_completed(&a));
        assert!(s.mark(&a));
        assert!(!s.mark(&a), "already completed");
        assert!(s.is_completed(&a));
        // Within 12 blocks it is the same structure; further it is not.
        assert!(s.is_completed(&key("sp-a", "village", 112, 188)));
        assert!(!s.is_completed(&key("sp-a", "village", 113, 200)));
        assert!(
            !s.mark(&key("sp-a", "village", 105, 205)),
            "merged into the stored one"
        );
        assert_eq!(s.count("sp-a", None), 1);
        // Other kind, world, seed or dimension never match.
        assert!(!s.is_completed(&key("sp-a", "igloo", 100, 200)));
        assert!(!s.is_completed(&key("sp-b", "village", 100, 200)));
        let mut other_seed = a.clone();
        other_seed.seed = 43;
        assert!(!s.is_completed(&other_seed));
        let mut nether = a.clone();
        nether.dimension = "minecraft:the_nether".into();
        assert!(!s.is_completed(&nether));
        // Unmarking at a nearby position removes it.
        assert!(s.unmark(&key("sp-a", "village", 108, 200)));
        assert!(!s.is_completed(&a));
        assert!(!s.unmark(&a));
        assert_eq!(s.count("sp-a", None), 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn store_persists_and_reloads() {
        let dir = temp_dir("persist");
        let mut s = CompletedStore::load(&dir);
        s.mark(&key("sp-a", "village", 1, 2));
        s.mark(&key("sp-a", "igloo", 300, -400));
        s.mark(&key("mp-x", "village", -50, 60));
        let again = CompletedStore::load(&dir);
        assert_eq!(again.all(), s.all());
        assert_eq!(again.count("sp-a", None), 2);
        assert_eq!(again.count("sp-a", Some("village")), 1);
        let (all, by_kind) = again.counts("sp-a");
        assert_eq!(all, 2);
        assert_eq!(
            by_kind,
            [("igloo".to_owned(), 1), ("village".to_owned(), 1)].into()
        );
        assert!(again.is_completed(&key("mp-x", "village", -50, 60)));
        // An unmark persists too.
        let mut again = again;
        again.unmark(&key("sp-a", "igloo", 300, -400));
        let last = CompletedStore::load(&dir);
        assert_eq!(last.count("sp-a", None), 1);
        // A broken file starts empty instead of failing.
        std::fs::write(dir.join("completed.json"), "{ not json").unwrap();
        assert_eq!(CompletedStore::load(&dir).all().len(), 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn reset_asks_first_and_clears() {
        let dir = temp_dir("reset");
        let mut s = CompletedStore::load(&dir);
        s.mark(&key("sp-a", "village", 1, 2));
        s.mark(&key("sp-a", "village", 500, 2));
        s.mark(&key("sp-a", "igloo", 300, 400));
        s.mark(&key("mp-x", "village", 0, 0));

        assert!(s.reset_prompt("sp-empty", "Empty", None).is_none());
        let all = s.reset_prompt("sp-a", "My World", None).unwrap();
        assert_eq!(all.title, "Reset completed structures?");
        assert_eq!(
            all.message,
            "All 3 completed structures of My World become not completed again."
        );
        let village = s
            .reset_prompt(
                "sp-a",
                "My World",
                Some(ResetKind {
                    id: "village",
                    name: "Village",
                }),
            )
            .unwrap();
        assert_eq!(
            village.message,
            "2 completed of type Village in My World become not completed again."
        );
        // Nothing happens before the yes.
        assert_eq!(s.count("sp-a", None), 3);
        assert_eq!(s.confirm_reset(&village), 2);
        assert_eq!(s.count("sp-a", None), 1);
        assert_eq!(s.count("mp-x", None), 1, "other worlds are untouched");
        assert_eq!(s.confirm_reset(&all), 1);
        assert_eq!(s.count("sp-a", None), 0);
        assert_eq!(s.reset("sp-a", None), 0);
        // The reset is saved.
        assert_eq!(CompletedStore::load(&dir).all().len(), 1);
        assert_eq!(reset_button_label(3), "Reset completed (3)");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn badge_sits_on_the_lower_right_corner() {
        assert_eq!(badge_rect([0, 0, 18, 18]), [11, 11, 21, 21]);
    }
}
