//! Nearest biome or structure from a position (`NearestFinder` and the
//! logic of `NearestScreen` in the Fabric version).
//!
//! The searches are blocking: call them off the render thread. They return
//! the block to show or to turn into a waypoint (see [`WaypointMode`]).

use meridian_seedatlas::atlas::{Hit, marker};
use meridian_seedatlas::{Atlas, Dimension};

use crate::seedatlas::{MarkerKind, dimension_of_kind};

/// Structure searches widen to these radii, in blocks.
const STRUCTURE_RADII: [i32; 3] = [1024, 4096, 16384];
/// `(sample step, radius)` of the biome passes. Each is at most 1024 x 1024
/// samples, within the engine's limit; small cheap passes come first, as
/// most searches end in the first one or two.
const BIOME_PASSES: [(i32, i32); 4] = [(8, 1024), (16, 4096), (32, 16384), (64, 32768)];

/// Where the nearest hit is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NearestResult {
    pub x: i32,
    pub z: i32,
    /// Blocks from the starting position, rounded.
    pub distance: i32,
}

fn distance(x: i32, z: i32, px: i32, pz: i32) -> i32 {
    let d = (f64::from(x) - f64::from(px)).hypot(f64::from(z) - f64::from(pz));
    d.round().min(f64::from(i32::MAX)) as i32
}

/// Whether a marker kind has structures in a dimension (the nearest list
/// shows only those).
pub fn applies_to(marker: &MarkerKind, dimension: Dimension) -> bool {
    marker
        .kinds
        .iter()
        .any(|&k| dimension_of_kind(k) == dimension)
}

/// The marker's hits in a block box `[min_x, min_z, max_x, max_z]` of one
/// dimension.
fn scan(atlas: &Atlas, marker: &MarkerKind, dimension: Dimension, b: [i32; 4]) -> Vec<Hit> {
    let mut hits = Vec::new();
    for &k in marker
        .kinds
        .iter()
        .filter(|&&k| dimension_of_kind(k) == dimension)
    {
        let found = match k {
            marker::STRONGHOLD => atlas.scan_strongholds(b, true),
            marker::SLIME_CHUNK => atlas.scan_slime_chunks(b),
            marker::SPAWN => {
                let (x, z) = atlas.spawn(true);
                Ok(if x >= b[0] && x <= b[2] && z >= b[1] && z <= b[3] {
                    vec![Hit {
                        kind: marker::SPAWN,
                        x,
                        y: None,
                        z,
                        flags: 0,
                        detail: 0,
                    }]
                } else {
                    Vec::new()
                })
            }
            k => atlas.scan_structures(k, b, true),
        };
        hits.extend(found.unwrap_or_default());
    }
    hits
}

/// The nearest structure of a marker kind around (`px`, `pz`), up to 16k
/// blocks away. `skip(x, z)` answers true for a structure to pass over (a
/// completed one when the player asked to skip those).
pub fn nearest_structure(
    atlas: &Atlas,
    dimension: Dimension,
    marker: &MarkerKind,
    px: i32,
    pz: i32,
    skip: &dyn Fn(i32, i32) -> bool,
) -> Option<NearestResult> {
    if !applies_to(marker, dimension) {
        return None;
    }
    for (i, &r) in STRUCTURE_RADII.iter().enumerate() {
        let b = [
            px.saturating_sub(r),
            pz.saturating_sub(r),
            px.saturating_add(r),
            pz.saturating_add(r),
        ];
        let best = scan(atlas, marker, dimension, b)
            .into_iter()
            .filter(|h| !skip(h.x, h.z))
            .map(|h| NearestResult {
                x: h.x,
                z: h.z,
                distance: distance(h.x, h.z, px, pz),
            })
            .min_by_key(|r| r.distance);
        // A hit outside the inscribed circle may lose to an unscanned
        // closer one in the corners of the next box.
        if let Some(best) = best
            && (best.distance <= r || i == STRUCTURE_RADII.len() - 1)
        {
            return Some(best);
        }
    }
    None
}

/// The nearest block of a biome around (`px`, `pz`), up to 32k blocks away,
/// sampled at height `y`.
pub fn nearest_biome(
    atlas: &Atlas,
    dimension: Dimension,
    biome_id: i32,
    y: i32,
    px: i32,
    pz: i32,
) -> Option<NearestResult> {
    for (i, &(step, r)) in BIOME_PASSES.iter().enumerate() {
        let ox = (i64::from(px) - i64::from(r)).div_euclid(i64::from(step)) * i64::from(step);
        let oz = (i64::from(pz) - i64::from(r)).div_euclid(i64::from(step)) * i64::from(step);
        let size = 2 * r / step;
        let (Ok(ox), Ok(oz)) = (i32::try_from(ox), i32::try_from(oz)) else {
            return None;
        };
        let ids = atlas
            .biome_area(dimension, ox, oz, y, step, size, size)
            .ok()?;
        let mut best: Option<NearestResult> = None;
        for (n, &id) in ids.iter().enumerate() {
            if id != biome_id {
                continue;
            }
            let (sx, sz) = ((n as i32) % size, (n as i32) / size);
            let bx = ox + sx * step + step / 2;
            let bz = oz + sz * step + step / 2;
            let d = distance(bx, bz, px, pz);
            if best.is_none_or(|b| d < b.distance) {
                best = Some(NearestResult {
                    x: bx,
                    z: bz,
                    distance: d,
                });
            }
        }
        if let Some(best) = best
            && (best.distance <= r || i == BIOME_PASSES.len() - 1)
        {
            return Some(best);
        }
    }
    None
}

/// Every jump leaves a waypoint: a temporary one, or a permanent one that
/// starts shown or hidden.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum WaypointMode {
    #[default]
    Temporary,
    Enabled,
    Disabled,
}

impl WaypointMode {
    pub const ALL: [WaypointMode; 3] = [Self::Temporary, Self::Enabled, Self::Disabled];

    /// The next mode, wrapping.
    pub fn next(self) -> Self {
        match self {
            Self::Temporary => Self::Enabled,
            Self::Enabled => Self::Disabled,
            Self::Disabled => Self::Temporary,
        }
    }

    /// "Temporary", "Permanent (Enabled)" or "Permanent (Disabled)".
    pub fn label(self) -> &'static str {
        match self {
            Self::Temporary => "Temporary",
            Self::Enabled => "Permanent (Enabled)",
            Self::Disabled => "Permanent (Disabled)",
        }
    }

    /// A permanent waypoint (shown or hidden); false for a temporary one.
    pub fn permanent(self) -> bool {
        self != Self::Temporary
    }

    /// A permanent waypoint that starts hidden.
    pub fn starts_disabled(self) -> bool {
        self == Self::Disabled
    }
}

/// The choices of the nearest screens, remembered for the session only.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NearestOptions {
    pub waypoint: WaypointMode,
    pub skip_completed: bool,
}

impl Default for NearestOptions {
    fn default() -> Self {
        Self {
            waypoint: WaypointMode::Temporary,
            skip_completed: true,
        }
    }
}

impl NearestOptions {
    /// "Waypoint: Temporary"
    pub fn waypoint_label(&self) -> String {
        format!("Waypoint: {}", self.waypoint.label())
    }

    /// "Skip completed: On"
    pub fn skip_label(&self) -> String {
        format!(
            "Skip completed: {}",
            if self.skip_completed { "On" } else { "Off" }
        )
    }
}

/// "Searching for Village..."
pub fn searching_message(name: &str) -> String {
    format!("Searching for {name}...")
}

/// The reply when nothing was found.
pub fn none_message(name: &str) -> String {
    format!("{name} was not found near you (structures up to 16k blocks away, biomes up to 32k).")
}

pub const MAP_UNAVAILABLE_MESSAGE: &str = "The map is not available.";
pub const WAYPOINT_UNAVAILABLE_MESSAGE: &str = "Waypoint creation is currently unavailable.";

#[cfg(test)]
mod tests {
    use super::*;
    use crate::seedatlas::{MARKERS, marker_for};
    use meridian_seedatlas::biomes;
    use meridian_seedatlas::structures::kind;

    const SEED: i64 = 1;

    fn village() -> &'static MarkerKind {
        marker_for(kind::VILLAGE).unwrap()
    }

    #[test]
    fn nearest_village_is_the_closest_of_a_full_scan() {
        let atlas = Atlas::new(SEED, false);
        let (px, pz) = (0, 0);
        let found = nearest_structure(&atlas, Dimension::Overworld, village(), px, pz, &|_, _| {
            false
        })
        .expect("a village within 16k blocks");

        // The brute-force answer over the same box.
        let r = 4096;
        let all = atlas
            .scan_structures(kind::VILLAGE, [-r, -r, r, r], true)
            .unwrap();
        let best = all
            .iter()
            .map(|h| distance(h.x, h.z, px, pz))
            .min()
            .unwrap();
        assert_eq!(found.distance, best);
        assert!(all.iter().any(|h| (h.x, h.z) == (found.x, found.z)));
        assert_eq!(found.distance, distance(found.x, found.z, px, pz));

        // Starting at the village finds it at distance 0.
        let there = nearest_structure(
            &atlas,
            Dimension::Overworld,
            village(),
            found.x,
            found.z,
            &|_, _| false,
        )
        .unwrap();
        assert_eq!((there.x, there.z, there.distance), (found.x, found.z, 0));
    }

    #[test]
    fn completed_structures_are_skipped() {
        let atlas = Atlas::new(SEED, false);
        let first = nearest_structure(&atlas, Dimension::Overworld, village(), 0, 0, &|_, _| false)
            .unwrap();
        let skip_first = |x: i32, z: i32| (x, z) == (first.x, first.z);
        let second =
            nearest_structure(&atlas, Dimension::Overworld, village(), 0, 0, &skip_first).unwrap();
        assert_ne!((second.x, second.z), (first.x, first.z));
        assert!(second.distance >= first.distance);
        // Skipping everything finds nothing.
        assert_eq!(
            nearest_structure(&atlas, Dimension::Overworld, village(), 0, 0, &|_, _| true),
            None
        );
    }

    #[test]
    fn structures_are_searched_in_their_own_dimension() {
        let atlas = Atlas::new(SEED, false);
        assert!(
            nearest_structure(&atlas, Dimension::Nether, village(), 0, 0, &|_, _| false).is_none()
        );
        let fortress = marker_for(kind::FORTRESS).unwrap();
        assert!(applies_to(fortress, Dimension::Nether));
        assert!(!applies_to(fortress, Dimension::Overworld));
        let portal = marker_for(kind::RUINED_PORTAL).unwrap();
        assert!(applies_to(portal, Dimension::Overworld) && applies_to(portal, Dimension::Nether));
        let found = nearest_structure(&atlas, Dimension::Nether, fortress, 0, 0, &|_, _| false);
        assert!(found.is_some());
        let applicable = MARKERS
            .iter()
            .filter(|m| applies_to(m, Dimension::End))
            .count();
        assert!(applicable >= 3, "End City, End Ship and End Gateway");
    }

    #[test]
    fn stronghold_and_spawn_are_found() {
        let atlas = Atlas::new(SEED, false);
        let stronghold = marker_for(marker::STRONGHOLD).unwrap();
        let found = nearest_structure(&atlas, Dimension::Overworld, stronghold, 0, 0, &|_, _| {
            false
        })
        .unwrap();
        let all = atlas
            .scan_strongholds([-16384, -16384, 16384, 16384], true)
            .unwrap();
        let best = all.iter().map(|h| distance(h.x, h.z, 0, 0)).min().unwrap();
        assert_eq!(found.distance, best);
        let spawn = marker_for(marker::SPAWN).unwrap();
        let (sx, sz) = atlas.spawn(true);
        let found = nearest_structure(&atlas, Dimension::Overworld, spawn, 0, 0, &|_, _| false);
        assert_eq!(found.map(|f| (f.x, f.z)), Some((sx, sz)));
    }

    #[test]
    fn nearest_biome_is_that_biome() {
        let atlas = Atlas::new(SEED, false);
        // Where the player stands is a biome; searching for it finds the
        // player's own sample.
        let here = atlas.biome_at(Dimension::Overworld, 0, 255, 0);
        let found = nearest_biome(&atlas, Dimension::Overworld, here, 255, 0, 0).unwrap();
        assert!(found.distance <= 8, "{found:?}");
        assert_eq!(
            atlas.biome_at(Dimension::Overworld, found.x, 255, found.z),
            here
        );

        // A different biome somewhere near: the answer really is that biome.
        let other = [
            biomes::DESERT,
            biomes::JUNGLE,
            biomes::SWAMP,
            biomes::SNOWY_PLAINS,
        ]
        .into_iter()
        .find(|&b| b != here)
        .unwrap();
        if let Some(f) = nearest_biome(&atlas, Dimension::Overworld, other, 255, 0, 0) {
            assert!(f.distance > 0);
        }
    }

    #[test]
    fn waypoint_and_skip_options_cycle_with_the_fabric_labels() {
        let mut o = NearestOptions::default();
        assert_eq!(o.waypoint_label(), "Waypoint: Temporary");
        assert_eq!(o.skip_label(), "Skip completed: On");
        o.waypoint = o.waypoint.next();
        assert_eq!(o.waypoint_label(), "Waypoint: Permanent (Enabled)");
        o.waypoint = o.waypoint.next();
        assert_eq!(o.waypoint_label(), "Waypoint: Permanent (Disabled)");
        assert!(o.waypoint.permanent() && o.waypoint.starts_disabled());
        o.waypoint = o.waypoint.next();
        assert_eq!(o.waypoint, WaypointMode::Temporary);
        assert!(!o.waypoint.permanent());
        o.skip_completed = false;
        assert_eq!(o.skip_label(), "Skip completed: Off");
        assert_eq!(searching_message("Village"), "Searching for Village...");
        assert_eq!(
            none_message("Village"),
            "Village was not found near you (structures up to 16k blocks away, biomes up to 32k)."
        );
    }
}
