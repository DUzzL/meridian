//! Settings, named profiles and their file.
//!
//! Every field has a default and `#[serde(default)]`, so an older or partial
//! file keeps loading as fields are added. Defaults follow the reference
//! mods' first-start configuration (`docs/FEATURES.md`, source O2) unless
//! `docs/DECISIONS.md` says otherwise.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::persist::{self, PersistError};

/// The settings file inside the host's config folder.
pub const CONFIG_FILE: &str = "meridian.toml";
/// The profile every install starts with.
pub const DEFAULT_PROFILE: &str = "default";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MinimapShape {
    #[default]
    Square,
    Round,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MinimapSize {
    /// Follows the GUI scale and window size.
    #[default]
    Auto,
    Small,
    Medium,
    Large,
    Huge,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BlockColors {
    /// Mean texture colours, tinted by biome: what the world looks like.
    #[default]
    Accurate,
    /// Vanilla map item colours.
    Vanilla,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Corner {
    #[default]
    TopLeft,
    TopRight,
    BottomRight,
    BottomLeft,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AutoCaveMode {
    Off,
    /// Under any solid roof directly above.
    Roof1x1,
    /// Under a roof at least 3x3 wide (cutting a tree doesn't trigger it).
    #[default]
    Roof3x3,
    Roof5x5,
}

/// Which map of a server is shown (several worlds behind one address).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MapSelection {
    /// One map per server address.
    Single,
    /// One map per world spawn the server announces.
    #[default]
    WorldSpawn,
    /// The map chosen with `/meridian map`.
    Manual,
}

/// How strongly slopes are shaded.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Slopes {
    Off,
    Vanilla,
    #[default]
    Smooth,
    Strong,
}

/// How the map is drawn; shared by the minimap and the world map, each with
/// its own copy.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TerrainStyle {
    pub block_colors: BlockColors,
    pub biome_colors_in_vanilla: bool,
    pub terrain_depth: bool,
    pub slopes: Slopes,
    pub biome_blending: bool,
    pub lighting: bool,
    pub show_flowers: bool,
    pub show_redstone: bool,
    pub block_transparency: bool,
    pub stained_glass: bool,
    pub adjust_short_blocks: bool,
}

impl Default for TerrainStyle {
    fn default() -> Self {
        Self {
            block_colors: BlockColors::Accurate,
            biome_colors_in_vanilla: false,
            terrain_depth: true,
            slopes: Slopes::Smooth,
            biome_blending: true,
            lighting: true,
            show_flowers: true,
            show_redstone: true,
            block_transparency: true,
            stained_glass: true,
            adjust_short_blocks: true,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CaveSettings {
    pub allowed: bool,
    /// Dimensions where cave mode may run; empty means all.
    pub allowed_dimensions: Vec<String>,
    pub auto: AutoCaveMode,
    /// Blocks scanned below the cave ceiling.
    pub depth: u32,
    pub legible: bool,
    /// Delay before switching in or out, milliseconds.
    pub toggle_delay_ms: u32,
    /// Extra zoom steps while in cave mode.
    pub extra_zoom: u8,
}

impl Default for CaveSettings {
    fn default() -> Self {
        Self {
            allowed: true,
            allowed_dimensions: Vec::new(),
            auto: AutoCaveMode::Roof3x3,
            depth: 30,
            // Readable by default: the map's own caves were always drawn so.
            legible: true,
            toggle_delay_ms: 1000,
            extra_zoom: 2,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LightInfo {
    #[default]
    Off,
    Block,
    Sky,
    Both,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClockInfo {
    #[default]
    Off,
    H24,
    H12,
}

/// The info lines beyond coordinates and biome, off by default like the
/// reference's (O2 `info_display_config`).
#[derive(Clone, Debug, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct InfoSettings {
    pub overworld_coords: bool,
    pub chunk_coords: bool,
    pub angles: bool,
    pub dimension: bool,
    pub weather: bool,
    pub light: LightInfo,
    pub game_time: ClockInfo,
    pub real_time: ClockInfo,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MinimapSettings {
    pub enabled: bool,
    pub shape: MinimapShape,
    pub size: MinimapSize,
    pub corner: Corner,
    /// Offset from the corner in GUI pixels.
    pub offset: [i32; 2],
    /// Index into the zoom steps.
    pub zoom: u8,
    pub further_zoom_out: bool,
    pub north_locked: bool,
    /// Z toggles instead of being held.
    pub enlarge_toggles: bool,
    pub enlarged_zoom: u8,
    pub enlarged_centered: bool,
    pub opacity: u8,
    pub frame: u8,
    pub arrow_scale: f32,
    pub hide_under_screens: bool,
    pub hide_under_debug: bool,
    pub undiscovered_opacity: u8,
    /// Reuse the world map's tiles instead of building separate ones.
    pub share_world_map_tiles: bool,
    pub chunk_grid: bool,
    pub slime_chunks: bool,
    /// Mark ground whose block light is within `light_overlay_range`.
    pub light_overlay: bool,
    pub light_overlay_range: [u8; 2],
    /// Info lines under the map.
    pub show_coords: bool,
    pub show_biome: bool,
    pub info: InfoSettings,
    pub terrain: TerrainStyle,
    pub cave: CaveSettings,
}

impl Default for MinimapSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            shape: MinimapShape::Square,
            size: MinimapSize::Auto,
            corner: Corner::TopLeft,
            offset: [0, 0],
            zoom: 1,
            further_zoom_out: false,
            north_locked: false,
            enlarge_toggles: false,
            enlarged_zoom: 0,
            enlarged_centered: false,
            opacity: 100,
            frame: 0,
            arrow_scale: 1.5,
            hide_under_screens: true,
            hide_under_debug: true,
            undiscovered_opacity: 50,
            share_world_map_tiles: true,
            chunk_grid: false,
            slime_chunks: false,
            light_overlay: false,
            light_overlay_range: [0, 7],
            show_coords: true,
            show_biome: false,
            info: InfoSettings::default(),
            terrain: TerrainStyle::default(),
            cave: CaveSettings::default(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct WorldMapSettings {
    pub enabled: bool,
    /// Write newly loaded chunks into the map.
    pub load_new_chunks: bool,
    /// Rewrite chunks that changed.
    pub update_chunks: bool,
    pub footsteps: bool,
    pub show_coordinates: bool,
    pub show_zoom: bool,
    pub show_hovered_biome: bool,
    pub player_as_arrow: bool,
    pub opening_animation: bool,
    pub unlimited_zoom_in: bool,
    pub unlimited_zoom_out: bool,
    /// Regions kept in memory before the least recently used are dropped.
    pub max_loaded_regions: usize,
    pub map_teleport_format: String,
    pub map_teleport_dimension_format: String,
    pub player_teleport_format: String,
    /// Which map of a server is shown.
    pub multiplayer_maps: MapSelection,
    pub terrain: TerrainStyle,
    pub cave: CaveSettings,
}

impl Default for WorldMapSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            load_new_chunks: true,
            update_chunks: true,
            footsteps: true,
            show_coordinates: true,
            show_zoom: true,
            show_hovered_biome: true,
            player_as_arrow: true,
            opening_animation: true,
            unlimited_zoom_in: false,
            unlimited_zoom_out: false,
            max_loaded_regions: 300,
            map_teleport_format: "/tp @s {x} {y} {z}".into(),
            map_teleport_dimension_format: "/execute as @s in {d} run tp {x} {y} {z}".into(),
            player_teleport_format: "/tp @s {name}".into(),
            multiplayer_maps: MapSelection::WorldSpawn,
            terrain: TerrainStyle::default(),
            cave: CaveSettings::default(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct WaypointSettings {
    pub in_world: bool,
    pub on_minimap: bool,
    pub all_sets: bool,
    pub deathpoints: bool,
    pub keep_old_deathpoints: bool,
    pub delete_reached_deathpoints: bool,
    /// 0 means unlimited.
    pub max_distance: u32,
    pub teleport_format: String,
    pub teleport_rotation_format: String,
    pub opacity_in_world: u8,
    pub opacity_on_minimap: u8,
    pub distance_in_km_from: u32,
    /// A beacon-like beam over waypoints in the world (off by default).
    pub beam: bool,
}

impl Default for WaypointSettings {
    fn default() -> Self {
        Self {
            in_world: true,
            on_minimap: true,
            all_sets: false,
            deathpoints: true,
            keep_old_deathpoints: true,
            delete_reached_deathpoints: true,
            max_distance: 0,
            teleport_format: "/tp @s {x} {y} {z}".into(),
            teleport_rotation_format: "/tp @s {x} {y} {z} {yaw} ~".into(),
            opacity_in_world: 80,
            opacity_on_minimap: 90,
            distance_in_km_from: 10_000,
            beam: false,
        }
    }
}

/// Whose names the radar writes under their mark.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NameMode {
    Off,
    #[default]
    Players,
    All,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RadarSettings {
    /// Mobs as their faces (from the game's textures) instead of dots.
    pub faces: bool,
    pub names: NameMode,
    pub enabled: bool,
    /// Shows what the filters hide and hides what they show (the reverse key).
    pub reversed: bool,
    pub hide_invisible: bool,
    pub smooth: bool,
    pub name_scale: f32,
    pub players: bool,
    pub hostile: bool,
    pub friendly: bool,
    pub items: bool,
    pub other: bool,
    /// Entities further than this above or below are left out (players: 2050).
    pub height_limit: u32,
    /// Darken dots by height difference.
    pub height_fade: bool,
    /// Dot size in minimap units.
    pub dot_size: u8,
    pub max_entities: u32,
}

impl Default for RadarSettings {
    fn default() -> Self {
        Self {
            faces: false,
            names: NameMode::Players,
            enabled: true,
            reversed: false,
            hide_invisible: true,
            smooth: true,
            name_scale: 1.0,
            players: true,
            hostile: true,
            friendly: true,
            items: true,
            other: true,
            height_limit: 20,
            height_fade: true,
            dot_size: 2,
            max_entities: 1000,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SeedAtlasSettings {
    /// Seed Atlas ships built in and on; this switches it off.
    pub enabled: bool,
    /// The seed biome layer on the world map.
    pub biome_layer: bool,
    /// The layer's opacity, 0..=255 (the Fabric version's default 255).
    pub biome_opacity: u8,
    /// Structure markers (off by default, as in the Fabric version).
    pub structures: bool,
    /// Legacy switch of the Fabric version up to config version 7; it reads
    /// as a completed opacity of 0 (see [`SeedAtlasSettings::completed_opacity_pct`]).
    pub hide_completed: bool,
    /// Completed structure markers in percent: 0 hides them, 100 draws them
    /// like every other marker.
    pub completed_opacity: u8,
    /// Marker size in percent of the default band: 50, 75, 100, 150, 200 or 250.
    pub marker_size: u16,
    /// Draws only the highlighted biomes in focus.
    pub biome_highlight: bool,
    /// The engine biome ids to highlight (0..=255).
    pub highlighted_biomes: std::collections::BTreeSet<u8>,
    /// Also draws the seed map over explored terrain.
    pub overlay_explored: bool,
    /// Minecraft version chosen per world or server (`persist::source_key`);
    /// a missing context follows the game ("auto", 26.3 in Horizon).
    pub game_versions: std::collections::BTreeMap<String, String>,
    /// The open world or server's key, so the settings screen can show its
    /// version. Not stored; the adapter sets it when the world changes.
    #[serde(skip)]
    pub context_key: String,
    /// Blocks per biome sample at full zoom: 1, 2, 4, 8 or 16.
    pub biome_resolution: u8,
    /// Background threads; 0 picks from the CPU count.
    pub worker_threads: u8,
    /// Regions computed ahead around the view: 0 (off) to 3.
    pub prefetch_radius: u8,
    /// Large Biomes for seeds entered by hand.
    pub large_biomes: bool,
    /// The heights biomes are sampled at.
    pub overworld_y: i32,
    pub nether_y: i32,
    pub end_y: i32,
    /// Terrain look for seed-only tiles: relief shading, water depth, End
    /// islands and Nether floors instead of flat biome colours.
    pub terrain_look: bool,
    /// Marker kinds by id (`village`, `stronghold`...); missing ids use
    /// their default.
    pub markers: std::collections::BTreeMap<String, bool>,
}

impl Default for SeedAtlasSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            biome_layer: true,
            biome_opacity: 255,
            structures: false,
            hide_completed: false,
            completed_opacity: 100,
            marker_size: 150,
            biome_highlight: false,
            highlighted_biomes: std::collections::BTreeSet::new(),
            overlay_explored: false,
            game_versions: std::collections::BTreeMap::new(),
            context_key: String::new(),
            biome_resolution: 1,
            worker_threads: 0,
            prefetch_radius: 1,
            large_biomes: false,
            overworld_y: 255,
            nether_y: 64,
            end_y: 128,
            terrain_look: true,
            markers: std::collections::BTreeMap::new(),
        }
    }
}

impl SeedAtlasSettings {
    /// The completed markers' opacity in percent, 0 when the legacy hide
    /// switch is on.
    pub fn completed_opacity_pct(&self) -> u8 {
        if self.hide_completed {
            0
        } else {
            self.completed_opacity.min(100)
        }
    }

    /// Completed markers at 0 % are not drawn, hovered or clicked.
    pub fn completed_hidden(&self) -> bool {
        self.completed_opacity_pct() == 0
    }

    /// The marker size as a factor of the default band.
    pub fn marker_size_factor(&self) -> f32 {
        f32::from(self.marker_size) / 100.0
    }

    /// The version chosen for the open context (`None`: follows the game).
    pub fn game_version(&self) -> Option<&str> {
        self.game_versions
            .get(&self.context_key)
            .map(String::as_str)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PerformanceSettings {
    /// Background threads for map tiles; 0 picks from the CPU count.
    pub worker_threads: usize,
    /// Main-thread time per frame for map work, microseconds.
    pub frame_budget_us: u32,
    /// Chunks read from the game per tick at most.
    pub chunk_reads_per_tick: usize,
    /// Seconds between saves of changed regions.
    pub save_interval_s: u32,
}

impl Default for PerformanceSettings {
    fn default() -> Self {
        Self {
            worker_threads: 0,
            frame_budget_us: 2_000,
            chunk_reads_per_tick: 16,
            save_interval_s: 30,
        }
    }
}

impl PerformanceSettings {
    /// Worker threads to start: the setting, or half the CPUs (1 to 4).
    pub fn effective_workers(&self) -> usize {
        if self.worker_threads > 0 {
            return self.worker_threads.min(16);
        }
        let cpus = std::thread::available_parallelism().map_or(2, |n| n.get());
        (cpus / 2).clamp(1, 4)
    }
}

/// One profile: everything a player can set.
#[derive(Clone, Debug, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub minimap: MinimapSettings,
    pub world_map: WorldMapSettings,
    pub waypoints: WaypointSettings,
    pub radar: RadarSettings,
    pub seed_atlas: SeedAtlasSettings,
    pub performance: PerformanceSettings,
}

/// The settings file: named profiles and the one in use.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ConfigFile {
    pub version: u32,
    pub current_profile: String,
    pub profiles: BTreeMap<String, Settings>,
}

impl Default for ConfigFile {
    fn default() -> Self {
        let mut profiles = BTreeMap::new();
        profiles.insert(DEFAULT_PROFILE.to_owned(), Settings::default());
        Self {
            version: 1,
            current_profile: DEFAULT_PROFILE.to_owned(),
            profiles,
        }
    }
}

impl ConfigFile {
    /// The profile in use; repairs a dangling name by falling back to the
    /// default profile.
    pub fn current(&self) -> &Settings {
        self.profiles
            .get(&self.current_profile)
            .or_else(|| self.profiles.get(DEFAULT_PROFILE))
            .or_else(|| self.profiles.values().next())
            .expect("a config file always keeps one profile")
    }

    pub fn current_mut(&mut self) -> &mut Settings {
        self.repair();
        self.profiles
            .get_mut(&self.current_profile)
            .expect("repaired")
    }

    /// Creates a profile as a copy of the current one; false if the name is
    /// taken or empty.
    pub fn create_profile(&mut self, name: &str) -> bool {
        let name = name.trim();
        if name.is_empty() || self.profiles.contains_key(name) {
            return false;
        }
        let copy = self.current().clone();
        self.profiles.insert(name.to_owned(), copy);
        true
    }

    /// Switches to an existing profile.
    pub fn select_profile(&mut self, name: &str) -> bool {
        if self.profiles.contains_key(name) {
            self.current_profile = name.to_owned();
            true
        } else {
            false
        }
    }

    /// Renames the current profile; false when the name is empty or taken.
    pub fn rename_profile(&mut self, name: &str) -> bool {
        let name = name.trim();
        if name.is_empty() || self.profiles.contains_key(name) {
            return false;
        }
        let Some(settings) = self.profiles.remove(&self.current_profile) else {
            return false;
        };
        self.profiles.insert(name.to_owned(), settings);
        self.current_profile = name.to_owned();
        true
    }

    /// Deletes a profile other than the last one; the current profile moves
    /// to the default when it was deleted.
    pub fn delete_profile(&mut self, name: &str) -> bool {
        if self.profiles.len() <= 1 || self.profiles.remove(name).is_none() {
            return false;
        }
        self.repair();
        true
    }

    fn repair(&mut self) {
        if self.profiles.is_empty() {
            self.profiles
                .insert(DEFAULT_PROFILE.to_owned(), Settings::default());
        }
        if !self.profiles.contains_key(&self.current_profile) {
            self.current_profile = if self.profiles.contains_key(DEFAULT_PROFILE) {
                DEFAULT_PROFILE.to_owned()
            } else {
                self.profiles.keys().next().cloned().expect("not empty")
            };
        }
    }
}

/// Loads and saves [`ConfigFile`] in a folder.
#[derive(Debug, Clone)]
pub struct ConfigStore {
    path: PathBuf,
}

impl ConfigStore {
    pub fn new(config_dir: &Path) -> Self {
        Self {
            path: config_dir.join(CONFIG_FILE),
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Reads the file. A missing file gives the defaults; a broken one is
    /// moved aside to `meridian.toml.broken` and also gives the defaults, so
    /// a typo never stops the game.
    pub fn load(&self) -> ConfigFile {
        let text = match std::fs::read_to_string(&self.path) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return ConfigFile::default(),
            Err(e) => {
                log::warn!("meridian: cannot read {}: {e}", self.path.display());
                return ConfigFile::default();
            }
        };
        match toml::from_str::<ConfigFile>(&text) {
            Ok(mut file) => {
                file.repair();
                file
            }
            Err(e) => {
                log::warn!(
                    "meridian: {} is not valid, using defaults: {e}",
                    self.path.display()
                );
                let aside = self.path.with_extension("toml.broken");
                if let Err(e) = std::fs::rename(&self.path, &aside) {
                    log::warn!("meridian: cannot move it aside: {e}");
                }
                ConfigFile::default()
            }
        }
    }

    pub fn save(&self, file: &ConfigFile) -> Result<(), PersistError> {
        let text = toml::to_string_pretty(file).map_err(|e| PersistError::Encode(e.to_string()))?;
        persist::write_atomic(&self.path, text.as_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persist::test_dir;

    #[test]
    fn defaults_follow_reference_first_start() {
        let s = Settings::default();
        assert!(s.minimap.enabled);
        assert_eq!(s.minimap.shape, MinimapShape::Square);
        assert!(
            !s.minimap.north_locked,
            "the minimap turns with the player by default"
        );
        assert_eq!(s.minimap.cave.depth, 30);
        assert_eq!(s.world_map.max_loaded_regions, 300);
        assert_eq!(s.waypoints.teleport_format, "/tp @s {x} {y} {z}");
        assert!(
            s.seed_atlas.enabled,
            "Seed Atlas is built in and on by default"
        );
    }

    #[test]
    fn round_trip_through_file() {
        let dir = test_dir("config_round_trip");
        let store = ConfigStore::new(&dir);
        let mut file = ConfigFile::default();
        file.current_mut().minimap.shape = MinimapShape::Round;
        assert!(file.create_profile("pvp"));
        assert!(file.select_profile("pvp"));
        file.current_mut().radar.enabled = false;
        store.save(&file).unwrap();
        let back = store.load();
        assert_eq!(back, file);
        assert!(!back.current().radar.enabled);
        assert_eq!(
            back.current().minimap.shape,
            MinimapShape::Round,
            "copied from default"
        );
    }

    #[test]
    fn partial_and_unknown_fields_load() {
        let dir = test_dir("config_partial");
        let store = ConfigStore::new(&dir);
        std::fs::write(
            store.path(),
            "current_profile = \"default\"\n[profiles.default.minimap]\nzoom = 3\nfuture_option = 7\n",
        )
        .unwrap();
        let file = store.load();
        assert_eq!(file.current().minimap.zoom, 3);
        assert_eq!(file.current().minimap.opacity, 100);
        assert_eq!(file.current().world_map, WorldMapSettings::default());
    }

    #[test]
    fn broken_file_is_moved_aside() {
        let dir = test_dir("config_broken");
        let store = ConfigStore::new(&dir);
        std::fs::write(store.path(), "this is = = not toml").unwrap();
        assert_eq!(store.load(), ConfigFile::default());
        assert!(dir.join("meridian.toml.broken").exists());
        assert!(!store.path().exists());
    }

    #[test]
    fn missing_file_gives_defaults() {
        let dir = test_dir("config_missing");
        assert_eq!(ConfigStore::new(&dir).load(), ConfigFile::default());
    }

    #[test]
    fn profile_rules() {
        let mut file = ConfigFile::default();
        assert!(!file.create_profile(" "));
        assert!(!file.create_profile(DEFAULT_PROFILE));
        assert!(
            !file.delete_profile(DEFAULT_PROFILE),
            "the last profile stays"
        );
        assert!(file.create_profile("b"));
        assert!(file.select_profile("b"));
        assert!(file.delete_profile("b"));
        assert_eq!(file.current_profile, DEFAULT_PROFILE);
        assert!(!file.select_profile("b"));
    }

    #[test]
    fn rename_keeps_the_settings() {
        let mut f = ConfigFile::default();
        f.current_mut().minimap.zoom = 3;
        assert!(f.create_profile("pvp"));
        assert!(!f.rename_profile("pvp"), "taken");
        assert!(f.rename_profile("survival"));
        assert_eq!(f.current_profile, "survival");
        assert_eq!(f.current().minimap.zoom, 3);
        assert!(!f.profiles.contains_key(DEFAULT_PROFILE));
    }

    #[test]
    fn dangling_current_profile_is_repaired() {
        let mut file = ConfigFile {
            current_profile: "gone".into(),
            ..ConfigFile::default()
        };
        assert_eq!(file.current(), &Settings::default());
        file.current_mut().minimap.zoom = 4;
        assert_eq!(file.current_profile, DEFAULT_PROFILE);
    }
}
