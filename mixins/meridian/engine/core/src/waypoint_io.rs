//! Waypoint export and import: Meridian's own JSON file (every dimension and
//! set of a map), and the plain-text waypoint files other map mods write
//! (one `waypoint:` line per waypoint, a `dim%<id>` folder per dimension).
//!
//! The text format is read only from its data, as the files describe it in
//! their own header line (`docs/CLEAN_ROOM.md`).

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::waypoint::{DEFAULT_SET, DimensionWaypoints, Kind, Waypoint, WaypointBook};

/// The export file's content.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Export {
    pub format: String,
    pub version: u32,
    /// Dimension id → its sets.
    pub dimensions: BTreeMap<String, DimensionWaypoints>,
}

pub const EXPORT_FORMAT: &str = "meridian-waypoints";

/// Every lasting waypoint of a book (temporary ones are left out).
pub fn export_json(book: &WaypointBook) -> String {
    let mut dimensions = book.dimensions.clone();
    for dim in dimensions.values_mut() {
        for list in dim.sets.values_mut() {
            list.retain(|w| w.kind != Kind::Temporary);
        }
    }
    let export = Export {
        format: EXPORT_FORMAT.into(),
        version: 1,
        dimensions,
    };
    serde_json::to_string_pretty(&export).unwrap_or_default()
}

/// Reads an export file.
pub fn import_json(text: &str) -> Result<BTreeMap<String, DimensionWaypoints>, String> {
    let export: Export = serde_json::from_str(text).map_err(|e| e.to_string())?;
    if export.format != EXPORT_FORMAT {
        return Err(format!("not a waypoint export ({})", export.format));
    }
    Ok(export.dimensions)
}

/// One waypoint read from a text file, with its set.
#[derive(Clone, Debug, PartialEq)]
pub struct TextWaypoint {
    pub set: String,
    pub waypoint: Waypoint,
}

/// The waypoints of one text file. Lines are
/// `waypoint:name:initials:x:y:z:color:disabled:type:set:rotate_on_tp:tp_yaw:...`;
/// colons inside names are written as `§§`. Unknown or broken lines are
/// skipped.
pub fn parse_text(text: &str) -> Vec<TextWaypoint> {
    let mut out = Vec::new();
    for line in text.lines() {
        let Some(rest) = line.trim().strip_prefix("waypoint:") else {
            continue;
        };
        let f: Vec<String> = rest.split(':').map(|s| s.replace("§§", ":")).collect();
        if f.len() < 9 {
            continue;
        }
        let (Ok(x), Ok(z)) = (f[2].parse::<i32>(), f[4].parse::<i32>()) else {
            continue;
        };
        // "~" (or anything else not a number) is an unknown height.
        let y = f[3].parse::<i32>().ok();
        let color = f[5].parse::<u8>().unwrap_or(12).min(15);
        let kind = match f[7].as_str() {
            "1" => Kind::OldDeath,
            _ => Kind::Normal,
        };
        let set = match f.get(8).map(String::as_str) {
            None | Some("") => DEFAULT_SET.to_owned(),
            // The default set is stored under a translation key.
            Some(s) if s.starts_with("gui.") && s.ends_with("_default") => DEFAULT_SET.to_owned(),
            Some(s) => s.to_owned(),
        };
        let rotate = f.get(9).is_some_and(|v| v == "true");
        let yaw = if rotate {
            f.get(10).and_then(|v| v.parse::<f32>().ok())
        } else {
            None
        };
        let mut initials: String = f[1].chars().take(2).collect();
        if initials.is_empty() {
            initials = crate::waypoint::initials_of(&f[0]);
        }
        out.push(TextWaypoint {
            set,
            waypoint: Waypoint {
                name: f[0].clone(),
                initials,
                pos: [x, y.unwrap_or(64), z],
                y_unknown: y.is_none(),
                color,
                kind,
                disabled: f[6] == "true",
                yaw,
                ..Waypoint::default()
            },
        });
    }
    out
}

/// The dimension a `dim%<id>` folder holds.
pub fn text_dimension(folder: &str) -> Option<String> {
    let id = folder.strip_prefix("dim%")?;
    Some(match id {
        "0" => "minecraft:overworld".into(),
        "-1" => "minecraft:the_nether".into(),
        "1" => "minecraft:the_end".into(),
        other => other.replacen('$', ":", 1),
    })
}

/// Reads a world folder of text waypoint files: every `dim%*/*.txt` in it
/// (or `folder` itself when it is a dimension folder). Dimension → waypoints.
pub fn read_text_folder(folder: &Path) -> BTreeMap<String, Vec<TextWaypoint>> {
    let mut out: BTreeMap<String, Vec<TextWaypoint>> = BTreeMap::new();
    let mut dims = Vec::new();
    let own = folder
        .file_name()
        .and_then(|n| n.to_str())
        .and_then(text_dimension);
    if let Some(dim) = own {
        dims.push((dim, folder.to_path_buf()));
    } else if let Ok(entries) = std::fs::read_dir(folder) {
        for e in entries.flatten() {
            if let Some(dim) = e.file_name().to_str().and_then(text_dimension) {
                dims.push((dim, e.path()));
            }
        }
    }
    for (dim, path) in dims {
        let Ok(files) = std::fs::read_dir(&path) else {
            continue;
        };
        for f in files.flatten() {
            if f.path().extension().is_some_and(|e| e == "txt")
                && let Ok(text) = std::fs::read_to_string(f.path())
            {
                out.entry(dim.clone())
                    .or_default()
                    .extend(parse_text(&text));
            }
        }
    }
    out
}

/// Adds waypoints to a book, each at the bottom of its set so a set keeps
/// the file's order; one with the same name and position already there is
/// skipped. Returns how many were added.
pub fn merge(
    book: &mut WaypointBook,
    dimension: &str,
    waypoints: Vec<TextWaypoint>,
    now: u64,
) -> usize {
    let mut added = 0;
    for TextWaypoint { set, waypoint } in waypoints {
        let exists = book.dimension(dimension).is_some_and(|d| {
            d.sets
                .values()
                .flatten()
                .any(|w| w.name == waypoint.name && w.pos == waypoint.pos)
        });
        if exists {
            continue;
        }
        book.insert(dimension, &set, waypoint, true, now);
        added += 1;
    }
    added
}

/// Adds an export's waypoints to a book (see [`merge`]). A dimension that
/// had no waypoints yet (a backup restored) also gets the export's current
/// set back; elsewhere the player's choice stays.
pub fn merge_export(
    book: &mut WaypointBook,
    dimensions: BTreeMap<String, DimensionWaypoints>,
    now: u64,
) -> usize {
    let mut added = 0;
    for (dim, sets) in dimensions {
        let fresh = book
            .dimension(&dim)
            .is_none_or(|d| d.sets.values().all(Vec::is_empty));
        let current = sets.current_set.clone();
        let list = sets
            .sets
            .into_iter()
            .flat_map(|(set, ws)| {
                ws.into_iter().map(move |w| TextWaypoint {
                    set: set.clone(),
                    waypoint: w,
                })
            })
            .collect();
        added += merge(book, &dim, list, now);
        if fresh
            && book
                .dimension(&dim)
                .is_some_and(|d| d.sets.contains_key(&current))
        {
            book.dimension_mut(&dim).current_set = current;
        }
    }
    added
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "#\n#waypoint:name:initials:x:y:z:color:disabled:type:set:rotate_on_tp:tp_yaw:visibility_type:destination\n#\nwaypoint:Home:H:130:133:10:7:false:0:gui.example_default:false:0:0:false\nwaypoint:Mine§§2:M2:-40:~:300:12:true:1:Caves:true:90:0:false\nbroken line\nwaypoint:short:S:1:2\n";

    #[test]
    fn reads_text_lines() {
        let ws = parse_text(SAMPLE);
        assert_eq!(ws.len(), 2);
        assert_eq!(ws[0].set, DEFAULT_SET);
        let home = &ws[0].waypoint;
        assert_eq!(
            (
                home.name.as_str(),
                home.initials.as_str(),
                home.pos,
                home.color
            ),
            ("Home", "H", [130, 133, 10], 7)
        );
        assert!(!home.disabled);
        let mine = &ws[1];
        assert_eq!(mine.set, "Caves");
        assert_eq!(mine.waypoint.name, "Mine:2");
        assert_eq!(mine.waypoint.pos, [-40, 64, 300]);
        assert_eq!(mine.waypoint.height(), None, "`~` is an unknown height");
        assert_eq!(
            crate::waypoint::command("/tp @s {x} {y} {z}", &mine.waypoint, "d"),
            "/tp @s -40 ~ 300"
        );
        assert!(mine.waypoint.disabled);
        assert_eq!(mine.waypoint.kind, Kind::OldDeath);
        assert_eq!(mine.waypoint.yaw, Some(90.0));
    }

    #[test]
    fn dimension_folders() {
        assert_eq!(
            text_dimension("dim%0").as_deref(),
            Some("minecraft:overworld")
        );
        assert_eq!(
            text_dimension("dim%-1").as_deref(),
            Some("minecraft:the_nether")
        );
        assert_eq!(
            text_dimension("dim%1").as_deref(),
            Some("minecraft:the_end")
        );
        assert_eq!(
            text_dimension("dim%mymod$moon").as_deref(),
            Some("mymod:moon")
        );
        assert!(text_dimension("config.txt").is_none());
    }

    #[test]
    fn folder_import_merges_without_duplicates() {
        let dir = std::env::temp_dir().join(format!("meridian-wpio-{}", std::process::id()));
        let world = dir.join("World");
        std::fs::create_dir_all(world.join("dim%0")).unwrap();
        std::fs::create_dir_all(world.join("dim%-1")).unwrap();
        std::fs::write(world.join("dim%0/waypoints.txt"), SAMPLE).unwrap();
        std::fs::write(
            world.join("dim%-1/waypoints.txt"),
            "waypoint:Fort:F:10:70:10:4:false:0:gui.example_default:false:0:0:false\n",
        )
        .unwrap();
        std::fs::write(world.join("config.txt"), "x").unwrap();
        let read = read_text_folder(&world);
        assert_eq!(read["minecraft:overworld"].len(), 2);
        assert_eq!(read["minecraft:the_nether"].len(), 1);
        let mut book = WaypointBook::default();
        for (dim, ws) in read.clone() {
            merge(&mut book, &dim, ws, 1);
        }
        let again: usize = read
            .into_iter()
            .map(|(d, ws)| merge(&mut book, &d, ws, 2))
            .sum();
        assert_eq!(again, 0, "nothing twice");
        let over = book.dimension("minecraft:overworld").unwrap();
        assert_eq!(over.sets[DEFAULT_SET].len(), 1);
        assert_eq!(over.sets["Caves"].len(), 1);
        // A dimension folder on its own.
        assert_eq!(
            read_text_folder(&world.join("dim%-1"))["minecraft:the_nether"].len(),
            1
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn imports_keep_order_and_the_current_set() {
        let mut book = WaypointBook::default();
        let dim = book.dimension_mut("minecraft:overworld");
        dim.create_set("trip");
        for name in ["first", "second", "third"] {
            dim.sets.get_mut("trip").unwrap().push(Waypoint::new(
                name,
                [0, 0, name.len() as i32 * 7],
                1,
            ));
        }
        dim.current_set = "trip".into();
        let text = export_json(&book);
        let mut restored = WaypointBook::default();
        merge_export(&mut restored, import_json(&text).unwrap(), 1);
        let dim = restored.dimension("minecraft:overworld").unwrap();
        let names: Vec<&str> = dim.sets["trip"].iter().map(|w| w.name.as_str()).collect();
        assert_eq!(names, ["first", "second", "third"], "the file's order");
        assert_eq!(dim.current_set, "trip", "a restored backup keeps its set");
        // Into a dimension that has waypoints, the player's set stays.
        let mut used = WaypointBook::default();
        used.add(
            "minecraft:overworld",
            Waypoint::new("mine", [9, 9, 9], 1),
            false,
            1,
        );
        merge_export(&mut used, import_json(&text).unwrap(), 2);
        assert_eq!(
            used.dimension("minecraft:overworld").unwrap().current_set,
            DEFAULT_SET
        );
    }

    #[test]
    fn json_round_trip() {
        let mut book = WaypointBook::default();
        book.add(
            "minecraft:overworld",
            Waypoint::new("Base", [1, 2, 3], 5),
            false,
            1,
        );
        let mut temp = Waypoint::new("Temp", [9, 9, 9], 5);
        temp.kind = Kind::Temporary;
        book.add("minecraft:overworld", temp, false, 1);
        let text = export_json(&book);
        let dims = import_json(&text).unwrap();
        let mut other = WaypointBook::default();
        assert_eq!(
            merge_export(&mut other, dims.clone(), 5),
            1,
            "temporary waypoints are not exported"
        );
        assert_eq!(merge_export(&mut other, dims, 6), 0);
        assert_eq!(
            other.dimension("minecraft:overworld").unwrap().current()[0].name,
            "Base"
        );
        assert!(import_json("{\"format\":\"x\",\"version\":1,\"dimensions\":{}}").is_err());
    }
}
