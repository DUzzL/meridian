//! Seed Atlas state of the Fabric version (`SeedAtlasClientState`,
//! `SeedProfile`, `GameVersion`) as pure logic: the Minecraft versions, the
//! seed an effective dimension generates from, and the allowed values of the
//! options.
//!
//! The seeds live in the [`SeedBook`]; the version chosen per world or
//! server lives in [`SeedAtlasSettings::game_versions`]. `auto` (no entry)
//! follows the game, which is always 26.3 in Horizon.

use meridian_host::WorldSource;
use meridian_seedatlas::Dimension;
use meridian_seedatlas::mc;

use crate::config::SeedAtlasSettings;
use crate::persist;
use crate::seedatlas::{SeedBook, SeedSpec, dim_index};

/// A span of Minecraft releases with the same world generation. `name` is
/// the newest release of the span (what the engine is asked to generate),
/// `first` the oldest.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GameVersion {
    pub name: &'static str,
    pub first: &'static str,
}

const fn gv(name: &'static str, first: &'static str) -> GameVersion {
    GameVersion { name, first }
}

impl GameVersion {
    /// Newest first. Every boundary is a version where the generation changes.
    pub const ALL: [GameVersion; 25] = [
        gv("26.3", "26.3"),
        gv("26.2", "26.2"),
        gv("26.1.2", "1.21.5"),
        gv("1.21.4", "1.21.4"),
        gv("1.21.3", "1.21.2"),
        gv("1.21.1", "1.21"),
        gv("1.20.6", "1.20"),
        gv("1.19.4", "1.19.3"),
        gv("1.19.2", "1.19"),
        gv("1.18.2", "1.18"),
        gv("1.17.1", "1.17"),
        gv("1.16.5", "1.16.2"),
        gv("1.16.1", "1.16"),
        gv("1.15.2", "1.15"),
        gv("1.14.4", "1.14"),
        gv("1.13.2", "1.13"),
        gv("1.12.2", "1.11"),
        gv("1.10.2", "1.10"),
        gv("1.9.4", "1.9"),
        gv("1.8.9", "1.8"),
        gv("1.7.10", "1.7"),
        gv("1.6.4", "1.4"),
        gv("1.3.2", "1.3"),
        gv("1.2.5", "1.2"),
        gv("1.1", "1.1"),
    ];

    /// The version Horizon runs, and the engine's newest.
    pub const NEWEST: GameVersion = Self::ALL[0];

    /// "1.21.5 – 26.1.2", or the single release.
    pub fn label(&self) -> String {
        if self.first == self.name {
            self.name.to_owned()
        } else {
            format!("{} \u{2013} {}", self.first, self.name)
        }
    }

    /// The engine's version number for this span.
    pub fn mc(&self) -> i32 {
        mc::str2mc(self.name)
    }

    /// Terrain heights, Nether floors and ore veins need 1.18 and later.
    pub fn has_terrain(&self) -> bool {
        let p = parts(self.first).unwrap_or([0; 3]);
        (p[0], p[1]) >= (1, 18)
    }

    /// The span containing a release such as "26.1", "1.20.1" or a span's
    /// own name; `None` for unknown text or a release newer than 26.3.
    pub fn parse(input: &str) -> Option<GameVersion> {
        let v = parts(input.trim())?;
        if v > parts(Self::NEWEST.name)? {
            return None;
        }
        Self::ALL
            .iter()
            .find(|span| parts(span.first).is_some_and(|f| v >= f))
            .copied()
    }
}

fn parts(version: &str) -> Option<[u32; 3]> {
    let split: Vec<&str> = version.split('.').collect();
    if split.len() < 2 || split.len() > 3 {
        return None;
    }
    let mut out = [0u32; 3];
    for (i, p) in split.iter().enumerate() {
        out[i] = p.parse().ok()?;
    }
    Some(out)
}

/// The version of a context and whether it follows the game (`auto`).
pub fn game_version(settings: &SeedAtlasSettings, key: &str) -> (GameVersion, bool) {
    match settings
        .game_versions
        .get(key)
        .and_then(|v| GameVersion::parse(v))
    {
        Some(v) => (v, false),
        None => (GameVersion::NEWEST, true),
    }
}

/// "1.21.5 - 26.1.2", or "Auto (26.3)" while the context follows the game.
pub fn game_version_name(settings: &SeedAtlasSettings, key: &str) -> String {
    let (version, auto) = game_version(settings, key);
    if auto {
        format!("Auto ({})", version.name)
    } else {
        version.label()
    }
}

/// Selects the version of a context; `None` follows the game again.
/// Whether anything changed.
pub fn set_game_version(
    settings: &mut SeedAtlasSettings,
    key: &str,
    version: Option<GameVersion>,
) -> bool {
    if key.trim().is_empty() {
        return false;
    }
    match version {
        None => settings.game_versions.remove(key).is_some(),
        Some(v) => {
            settings
                .game_versions
                .insert(key.to_owned(), v.name.to_owned())
                .as_deref()
                != Some(v.name)
        }
    }
}

/// The next version in the list, `auto` first, for the world-generation
/// option (`back` for a right click).
pub fn cycle_game_version(settings: &mut SeedAtlasSettings, key: &str, back: bool) {
    let all = GameVersion::ALL;
    // Index 0 is auto, then every span from the newest.
    let now = match settings
        .game_versions
        .get(key)
        .and_then(|v| GameVersion::parse(v))
    {
        None => 0,
        Some(v) => 1 + all.iter().position(|a| *a == v).unwrap_or(0),
    };
    let n = all.len() + 1;
    let next = if back {
        (now + n - 1) % n
    } else {
        (now + 1) % n
    };
    let version = (next > 0).then(|| all[next - 1]);
    set_game_version(settings, key, version);
}

/// A world or server as the commands see it: its key in the data folder
/// and the name shown to the player.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SeedContext {
    pub key: String,
    pub label: String,
}

impl SeedContext {
    pub fn from_source(source: &WorldSource) -> Self {
        let label = match source {
            WorldSource::Singleplayer { world } => world.clone(),
            WorldSource::Multiplayer { address } => address.clone(),
        };
        Self {
            key: persist::source_key(source),
            label,
        }
    }

    /// The context of one map of a world or server: maps other than the
    /// default one (other worlds behind the same address) keep their own
    /// seeds.
    pub fn of_map(source: &WorldSource, map: &str) -> Self {
        let mut c = Self::from_source(source);
        if map != crate::engine::DEFAULT_MAP {
            c.key = format!("{}/{map}", c.key);
            c.label = format!("{} ({map})", c.label);
        }
        c
    }
}

/// The effective generation of one dimension in a context.
///
/// The base seed is the book's, else the world's own (`detected`: seed and
/// world type, e.g. read from the singleplayer world) unless suppressed by
/// `clear`. The Nether and the End use their own seed when they have one.
/// The world type is the "World type" option for seeds entered by hand and
/// the world's for its own seed; the version is the context's.
pub fn resolve_seed(
    book: &SeedBook,
    settings: &SeedAtlasSettings,
    key: &str,
    dimension: Dimension,
    detected: Option<(i64, bool)>,
) -> Option<SeedSpec> {
    let mc = game_version(settings, key).0.mc();
    let base = match book.get(key) {
        Some(e) => Some((e.seed, settings.large_biomes)),
        None if !book.detected_suppressed(key) => detected,
        None => None,
    };
    if let Some(own) = book.dimension_seed(key, dimension) {
        let large = base.map_or(settings.large_biomes, |b| b.1);
        return Some(SeedSpec {
            seed: own.seed,
            large_biomes: large,
            mc,
        });
    }
    base.map(|(seed, large_biomes)| SeedSpec {
        seed,
        large_biomes,
        mc,
    })
}

/// [`resolve_seed`] for the Overworld, Nether and End (see [`dim_index`]).
pub fn resolve_all(
    book: &SeedBook,
    settings: &SeedAtlasSettings,
    key: &str,
    detected: Option<(i64, bool)>,
) -> [Option<SeedSpec>; 3] {
    let mut out = [None; 3];
    for d in [Dimension::Overworld, Dimension::Nether, Dimension::End] {
        out[dim_index(d)] = resolve_seed(book, settings, key, d, detected);
    }
    out
}

// ---------------------------------------------------------------- options --

pub const BIOME_RESOLUTION_STEPS: [u8; 5] = [1, 2, 4, 8, 16];
pub const WORKER_THREAD_STEPS: [u8; 5] = [0, 1, 2, 4, 8];
pub const PREFETCH_RADIUS_STEPS: [u8; 4] = [0, 1, 2, 3];
pub const MARKER_SIZE_STEPS: [u16; 6] = [50, 75, 100, 150, 200, 250];
pub const MIN_SAMPLE_Y: i32 = -2048;
pub const MAX_SAMPLE_Y: i32 = 2048;
/// Blocks a completed structure may move (between versions, say) and stay
/// completed.
pub const COMPLETED_TOLERANCE: i32 = 12;

/// The next marker size; an unknown value returns to the default 150.
pub fn next_marker_size(current: u16) -> u16 {
    MARKER_SIZE_STEPS
        .iter()
        .position(|&s| s == current)
        .map_or(150, |i| {
            MARKER_SIZE_STEPS[(i + 1) % MARKER_SIZE_STEPS.len()]
        })
}

/// "Full Detail (1x)"...
pub fn resolution_label(step: u8) -> &'static str {
    match step {
        1 => "Full Detail (1x)",
        2 => "High (2x)",
        4 => "Reduced (4x)",
        8 => "Low (8x)",
        _ => "Very Low (16x)",
    }
}

pub fn prefetch_label(radius: u8) -> &'static str {
    ["Off", "Near", "Medium", "Far"][radius.min(3) as usize]
}

pub fn workers_label(threads: u8) -> String {
    if threads == 0 {
        "Auto".into()
    } else {
        threads.to_string()
    }
}

pub fn world_type_label(large_biomes: bool) -> &'static str {
    if large_biomes {
        "Large Biomes"
    } else {
        "Normal"
    }
}

/// The text of the completed-opacity option: "Hidden" or "<n>%".
pub fn completed_opacity_label(pct: u8) -> String {
    if pct == 0 {
        "Hidden".into()
    } else {
        format!("{pct}%")
    }
}

/// Puts loaded values back into what the options allow, as the Fabric
/// version's setters do.
pub fn normalize(s: &mut SeedAtlasSettings) {
    s.completed_opacity = s.completed_opacity.min(100);
    if !MARKER_SIZE_STEPS.contains(&s.marker_size) {
        s.marker_size = 150;
    }
    if !BIOME_RESOLUTION_STEPS.contains(&s.biome_resolution) {
        s.biome_resolution = 1;
    }
    if !WORKER_THREAD_STEPS.contains(&s.worker_threads) {
        s.worker_threads = 0;
    }
    if !PREFETCH_RADIUS_STEPS.contains(&s.prefetch_radius) {
        s.prefetch_radius = 1;
    }
    for y in [&mut s.overworld_y, &mut s.nether_y, &mut s.end_y] {
        *y = (*y).clamp(MIN_SAMPLE_Y, MAX_SAMPLE_Y);
    }
    s.game_versions
        .retain(|k, v| !k.trim().is_empty() && GameVersion::parse(v).is_some());
    for v in s.game_versions.values_mut() {
        if let Some(span) = GameVersion::parse(v) {
            *v = span.name.to_owned();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::seedatlas::{SeedEntry, SeedValue};

    fn book(dir: &std::path::Path) -> SeedBook {
        SeedBook::load(dir)
    }

    fn entry(seed: i64) -> SeedEntry {
        SeedEntry {
            input: seed.to_string(),
            seed,
            large_biomes: false,
        }
    }

    fn value(seed: i64) -> SeedValue {
        SeedValue {
            input: seed.to_string(),
            seed,
        }
    }

    fn temp_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("meridian-sa-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn versions_parse_to_their_span() {
        assert_eq!(GameVersion::parse("26.3"), Some(GameVersion::NEWEST));
        assert_eq!(GameVersion::parse("26.1").map(|v| v.name), Some("26.1.2"));
        assert_eq!(GameVersion::parse("1.20.1").map(|v| v.name), Some("1.20.6"));
        assert_eq!(GameVersion::parse("1.21.5").map(|v| v.name), Some("26.1.2"));
        assert_eq!(GameVersion::parse("1.1").map(|v| v.name), Some("1.1"));
        assert_eq!(GameVersion::parse("26.4"), None);
        assert_eq!(GameVersion::parse("1.0"), None);
        assert_eq!(GameVersion::parse("banana"), None);
        assert_eq!(GameVersion::parse("1"), None);
        assert_eq!(GameVersion::parse("1.x"), None);
        assert_eq!(GameVersion::ALL[2].label(), "1.21.5 \u{2013} 26.1.2");
        assert_eq!(GameVersion::NEWEST.label(), "26.3");
        assert!(GameVersion::parse("1.18").unwrap().has_terrain());
        assert!(!GameVersion::parse("1.17.1").unwrap().has_terrain());
        for v in GameVersion::ALL {
            assert_ne!(v.mc(), mc::MC_UNDEF, "{} has an engine version", v.name);
        }
        assert_eq!(GameVersion::NEWEST.mc(), mc::MC_NEWEST);
    }

    #[test]
    fn version_follows_the_game_until_chosen() {
        let mut s = SeedAtlasSettings::default();
        assert_eq!(game_version(&s, "sp-a"), (GameVersion::NEWEST, true));
        assert_eq!(game_version_name(&s, "sp-a"), "Auto (26.3)");
        assert!(set_game_version(&mut s, "sp-a", GameVersion::parse("1.20")));
        assert!(!set_game_version(
            &mut s,
            "sp-a",
            GameVersion::parse("1.20.4")
        ));
        assert_eq!(game_version_name(&s, "sp-a"), "1.20 \u{2013} 1.20.6");
        assert_eq!(game_version_name(&s, "sp-b"), "Auto (26.3)");
        assert!(set_game_version(&mut s, "sp-a", None));
        assert!(!set_game_version(&mut s, "sp-a", None));
        assert!(!set_game_version(&mut s, " ", GameVersion::parse("1.20")));
        // The option steps through auto and every span, both ways.
        cycle_game_version(&mut s, "k", false);
        assert_eq!(s.game_versions.get("k").map(String::as_str), Some("26.3"));
        cycle_game_version(&mut s, "k", true);
        assert!(!s.game_versions.contains_key("k"));
        cycle_game_version(&mut s, "k", true);
        assert_eq!(s.game_versions.get("k").map(String::as_str), Some("1.1"));
    }

    #[test]
    fn base_and_dimension_seeds_resolve() {
        let dir = temp_dir("resolve");
        let mut b = book(&dir);
        let mut s = SeedAtlasSettings::default();
        let key = "sp-world";
        let nether = Dimension::Nether;

        // Nothing configured, nothing detected.
        assert_eq!(resolve_seed(&b, &s, key, Dimension::Overworld, None), None);

        // The world's own seed is the base for all dimensions.
        let spec = resolve_seed(&b, &s, key, nether, Some((7, true))).unwrap();
        assert_eq!(
            (spec.seed, spec.large_biomes, spec.mc),
            (7, true, mc::MC_NEWEST)
        );

        // A seed entered by hand wins over it; the option sets the world type.
        b.set(key, entry(100));
        s.large_biomes = true;
        let spec = resolve_seed(&b, &s, key, Dimension::End, Some((7, false))).unwrap();
        assert_eq!((spec.seed, spec.large_biomes), (100, true));

        // The Nether has its own seed, the others keep the base.
        assert!(b.set_dimension_seed(key, nether, value(-5)));
        assert!(!b.set_dimension_seed(key, Dimension::Overworld, value(1)));
        let all = resolve_all(&b, &s, key, None);
        assert_eq!(
            all.map(|x| x.map(|x| x.seed)),
            [Some(100), Some(-5), Some(100)]
        );

        // An explicit version applies to every dimension of the context only.
        set_game_version(&mut s, key, GameVersion::parse("1.12"));
        let all = resolve_all(&b, &s, key, None);
        assert!(all.iter().all(|x| x.unwrap().mc == mc::MC_1_12_2));
        assert_eq!(
            resolve_seed(&b, &s, "sp-other", nether, None),
            None,
            "contexts are separate"
        );

        // An own seed alone: the Overworld has none.
        let mut b2 = book(&temp_dir("resolve-own"));
        b2.set_dimension_seed(key, Dimension::End, value(9));
        let all = resolve_all(&b2, &SeedAtlasSettings::default(), key, None);
        assert_eq!(all.map(|x| x.map(|x| x.seed)), [None, None, Some(9)]);

        // Clearing removes everything and ignores the world's seed until the
        // context changes.
        assert!(b.clear(key));
        b.suppress_detected(key);
        assert_eq!(
            resolve_seed(&b, &s, key, Dimension::Overworld, Some((7, false))),
            None
        );
        b.context_changed("sp-elsewhere");
        assert!(resolve_seed(&b, &s, key, Dimension::Overworld, Some((7, false))).is_some());
    }

    #[test]
    fn seeds_persist_and_old_files_still_load() {
        let dir = temp_dir("persist");
        // The format of older releases: base seeds only.
        std::fs::write(
            dir.join("seeds.toml"),
            "[seeds.sp-old]\ninput = \"hello\"\nseed = 99162322\nlarge_biomes = true\n",
        )
        .unwrap();
        let mut b = book(&dir);
        assert_eq!(b.get("sp-old").map(|e| e.seed), Some(99162322));
        assert!(b.get("sp-old").unwrap().large_biomes);
        assert!(b.dimension_seed("sp-old", Dimension::Nether).is_none());

        b.set_dimension_seed("sp-old", Dimension::Nether, value(5));
        b.set_dimension_seed("mp-x", Dimension::End, value(6));
        let again = book(&dir);
        assert_eq!(again.get("sp-old"), b.get("sp-old"));
        assert_eq!(
            again.dimension_seed("sp-old", Dimension::Nether),
            Some(&value(5))
        );
        assert_eq!(
            again.dimension_seed("mp-x", Dimension::End),
            Some(&value(6))
        );
        assert!(again.has_any("mp-x"));
        assert!(!again.has_any("nope"));

        let mut b = again;
        assert!(b.clear_dimension_seed("sp-old", Dimension::Nether));
        assert!(!b.clear_dimension_seed("sp-old", Dimension::Nether));
        assert!(b.clear("mp-x"));
        let last = book(&dir);
        assert!(last.dimension_seed("sp-old", Dimension::Nether).is_none());
        assert!(!last.has_any("mp-x"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn new_settings_have_the_fabric_defaults_and_round_trip() {
        let d = SeedAtlasSettings::default();
        assert_eq!((d.completed_opacity, d.marker_size), (100, 150));
        assert!(!d.biome_highlight && !d.overlay_explored && d.highlighted_biomes.is_empty());
        assert_eq!(d.biome_opacity, 255);
        assert!(d.game_version().is_none());

        // An older file without the new keys keeps the defaults.
        let old: SeedAtlasSettings = serde_json::from_str(r#"{"biome_opacity": 100}"#).unwrap();
        assert_eq!(
            (old.biome_opacity, old.marker_size, old.completed_opacity),
            (100, 150, 100)
        );

        // The new keys survive a save; the open context does not.
        let mut s = SeedAtlasSettings {
            marker_size: 250,
            completed_opacity: 30,
            biome_highlight: true,
            overlay_explored: true,
            context_key: "sp-w".into(),
            ..Default::default()
        };
        s.highlighted_biomes.extend([1, 4, 200]);
        s.game_versions.insert("sp-w".into(), "1.20.6".into());
        let text = serde_json::to_string(&s).unwrap();
        let back: SeedAtlasSettings = serde_json::from_str(&text).unwrap();
        assert_eq!(back.context_key, "");
        assert_eq!(back.highlighted_biomes, s.highlighted_biomes);
        assert_eq!(back.game_versions, s.game_versions);
        assert_eq!((back.marker_size, back.completed_opacity), (250, 30));
        assert!(back.biome_highlight && back.overlay_explored);
        let mut back = back;
        back.context_key = "sp-w".into();
        assert_eq!(back.game_version(), Some("1.20.6"));
        assert_eq!(
            toml::from_str::<SeedAtlasSettings>(&toml::to_string(&s).unwrap())
                .unwrap()
                .marker_size,
            250
        );
    }

    #[test]
    fn options_normalise() {
        let mut s = SeedAtlasSettings {
            marker_size: 123,
            completed_opacity: 200,
            biome_resolution: 3,
            worker_threads: 5,
            prefetch_radius: 9,
            overworld_y: 99999,
            nether_y: -99999,
            ..Default::default()
        };
        s.game_versions.insert("a".into(), "1.20.1".into());
        s.game_versions.insert("b".into(), "nonsense".into());
        normalize(&mut s);
        assert_eq!((s.marker_size, s.completed_opacity), (150, 100));
        assert_eq!(
            (s.biome_resolution, s.worker_threads, s.prefetch_radius),
            (1, 0, 1)
        );
        assert_eq!((s.overworld_y, s.nether_y), (2048, -2048));
        assert_eq!(s.game_versions.get("a").map(String::as_str), Some("1.20.6"));
        assert!(!s.game_versions.contains_key("b"));
        assert_eq!(next_marker_size(150), 200);
        assert_eq!(next_marker_size(250), 50);
        assert_eq!(next_marker_size(7), 150);
        assert_eq!(completed_opacity_label(0), "Hidden");
        assert_eq!(completed_opacity_label(40), "40%");
        assert!(SeedAtlasSettings::default().completed_opacity_pct() == 100);
        let hidden = SeedAtlasSettings {
            hide_completed: true,
            ..Default::default()
        };
        assert!(hidden.completed_hidden());
    }
}
