//! Meridian's adapter to Horizon: the trait mod the launcher links into the
//! game. It translates Horizon's hooks into [`HostEvent`]s, answers the
//! core's [`MapHost`] questions and draws what the core lays out.
//!
//! Chunk events and chunk reads (`docs/HORIZON_API.md` E1/E2) come from the
//! bundle's transitional bridge, `horizon_client::game::meridian_bridge`,
//! which horizon-mixins adds to the game (`mixins/meridian/bridge/`).
#![forbid(unsafe_code)]

mod gpu;
mod gpu_map;
mod icons;
mod terrain_sampler;

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use horizon_client::game::meridian_bridge as bridge;
use horizon_client::mod_api::{
    ClientChatReceivedEvent, ClientDisconnectingEvent, ClientLevelLoadEvent,
    ClientLevelUnloadEvent, ClientResourcesReloadedEvent, ClientStoppingEvent, CommandEvent,
    CommandsEvent, Flow, FrameEvent, Hooks, Host, HudBatch, HudEvent, KeyEvent, ModsEvent,
    NativeMod, RenderHooks, ScreenCharTypedPreEvent, ScreenClosingEvent,
    ScreenKeyPressedPreEvent, ScreenMouseButtonPressedPreEvent, ScreenMouseButtonReleasedPreEvent,
    ScreenMouseDraggedPreEvent, ScreenMouseScrolledPreEvent, GuiLayerPreEvent, RenderHandEvent,
    MenuBackgroundEvent, ScreenRenderPostEvent, Text, TintSource, WorldLifecycleEvent, WorldPhase, WorldPrimitivesEvent,
};
use meridian_core::MapEngine;
use meridian_core::config::{ConfigFile, ConfigStore, MinimapSettings};
use meridian_core::engine::DEFAULT_MAP;
use meridian_core::geom::RegionPos;
use meridian_core::keybinds::{KEY_BINDINGS, bindings_for_key};
use meridian_core::minimap::{self, Align, Controls, DrawOp};
use meridian_core::screens::{Action, Editor, Env, List, Screen, SettingsScreen};
use meridian_core::persist::MapPaths;
use meridian_core::radar;
use meridian_core::waypoint::{self, Kind, Waypoint, WaypointBook};
use meridian_core::worldmap::{MapAction, MenuItem, Overlay, WorldMap};
use meridian_host::{
    BiomeAppearance, BlockAppearance, ChunkPos, ChunkSnapshot, ColumnSample, DimensionId,
    HostEvent, MapHost, PlayerState, ReadMode, Tint, WorldContext, WorldSource,
};
use parking_lot::Mutex;

/// The mod id: its settings folder, its keys' lines, its data folder.
pub const MOD_ID: &str = "meridian";
const VERSION: &str = env!("CARGO_PKG_VERSION");
/// The bridge shape this adapter was written for.
const BRIDGE_VERSION: u32 = 6;

/// The singleplayer world's folder name, set by the integrated server's
/// thread (the window's client only knows the world's display name).
static SERVER_WORLD: Mutex<Option<String>> = Mutex::new(None);
/// The singleplayer world's seed and whether it has Large Biomes, read on
/// the integrated server's thread when the world opens.
static SERVER_SEED: Mutex<Option<(i64, bool)>> = Mutex::new(None);
/// Blocks within which a waypoint's beam is left out (it would fill the view).
const BEAM_HIDE_DISTANCE: f64 = 8.0;

/// Mean colours of the block textures, by texture file, and which file each
/// sprite of Horizon's block atlas comes from.
#[derive(Default)]
struct Textures {
    files: std::collections::HashMap<(u32, u32), PathBuf>,
    means: std::collections::HashMap<PathBuf, Option<u32>>,
    indexed: bool,
}

static TEXTURES: Mutex<Option<Textures>> = Mutex::new(None);

impl Textures {
    /// Maps every block texture file to its sprite once (`texture_sprites`).
    fn index(&mut self, host: &dyn Host) {
        if self.indexed {
            return;
        }
        self.indexed = true;
        let Some(root) = bridge::resources_dir() else { return };
        let dir = root.join("assets/minecraft/textures/block");
        let Ok(entries) = std::fs::read_dir(&dir) else { return };
        let files: Vec<PathBuf> =
            entries.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "png")).collect();
        let names: Vec<String> = files
            .iter()
            .filter_map(|p| p.file_stem().map(|s| format!("minecraft:block/{}", s.to_string_lossy())))
            .collect();
        let refs: Vec<&str> = names.iter().map(String::as_str).collect();
        for (sprite, file) in host.texture_sprites(&refs).into_iter().zip(files) {
            if let Some(s) = sprite {
                self.files.entry((s.array, s.layer)).or_insert(file);
            }
        }
    }

    fn mean(&mut self, sprite: (u32, u32)) -> Option<u32> {
        let file = self.files.get(&sprite)?.clone();
        *self.means.entry(file.clone()).or_insert_with(|| {
            std::fs::read(&file).ok().and_then(|b| meridian_core::texture::mean_color(&b))
        })
    }
}

/// Whether numbers use a decimal comma here (the system's locale), as the
/// game's Java side would print them.
fn decimal_comma() -> bool {
    static COMMA: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *COMMA.get_or_init(|| {
        let mut locale = ["LC_ALL", "LC_NUMERIC", "LANG"]
            .iter()
            .find_map(|k| std::env::var(k).ok().filter(|v| !v.is_empty() && v != "C"))
            .unwrap_or_default();
        if locale.is_empty() && cfg!(target_os = "macos") {
            locale = std::process::Command::new("defaults")
                .args(["read", "-g", "AppleLocale"])
                .output()
                .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
                .unwrap_or_default();
        }
        let lang = locale.split(['_', '-', '.']).next().unwrap_or("").to_ascii_lowercase();
        matches!(
            lang.as_str(),
            "de" | "fr" | "es" | "it" | "nl" | "pt" | "ru" | "pl" | "sv" | "da" | "fi" | "nb" | "no" | "cs" | "sk"
                | "tr" | "uk" | "hu" | "ro" | "el" | "bg" | "hr" | "sl" | "lt" | "lv" | "et" | "id" | "vi"
        )
    })
}

pub fn hooks() -> Box<dyn Hooks> {
    Box::new(Meridian {
        state: Mutex::new(None),
        renderer: gpu::MapRenderer::default(),
    })
}

struct State {
    config: ConfigFile,
    store: ConfigStore,
    engine: MapEngine,
    controls: Controls,
    /// The dimension the client shows, from `client_level_load`.
    dimension: Option<String>,
    /// The waypoints of the open world, and which world they are.
    /// The waypoints of the open world's map: its source and map name.
    book: Option<((WorldSource, String), WaypointBook)>,
    /// The maps of the server played on (several worlds behind one address).
    server_maps: Option<(WorldSource, meridian_core::subworld::ServerMaps)>,
    last_book_save: Instant,
    /// The world map's last frame, for its animation.
    map_frame: Instant,
    /// Colour for the next quick waypoint, cycling.
    next_color: u8,
    /// Feedback to show on the action bar when a hook can.
    notice: Option<String>,
    /// The minimap's composed image and what it was composed from.
    map_image: Option<(MapImageKey, std::sync::Arc<gpu::MapImage>)>,
    /// The open screen of ours, if any.
    screen: Option<Screen>,
    /// A screen to open from the next hook that may (`key`).
    open_screen: Option<Screen>,
    /// What screen hooks (which only read the game) leave for `frame`.
    pending: Vec<Pending>,
    /// The current death has its deathpoint (until the player plays again).
    death_recorded: bool,
    /// Cave mode: automatic under a roof, or manual.
    cave: meridian_core::cave::CaveControl,
    /// The world map's view (kept between openings) and its footsteps.
    map: WorldMap,
    /// The world map is the screen showing.
    map_open: bool,
    /// Open the world map from the next hook that may (`key`).
    open_map: bool,
    /// A screen opened from the map goes back to it when it closes.
    return_to_map: bool,
    /// Another dimension's map the world map shows, read from disk only.
    viewer: Option<(String, MapEngine)>,
    /// Where PNG exports go.
    export_dir: PathBuf,
    /// Client levels loaded and not yet unloaded: a dimension change loads
    /// the new level before it unloads the old one.
    levels: u32,
    /// Seeds entered per world or server.
    seeds: meridian_core::seedatlas::SeedBook,
    /// Seed Atlas tiles and markers for the current seed.
    seed_svc: meridian_core::seedatlas::SeedService,
    /// Completed structures (Seed Atlas markers).
    completed: meridian_core::completed::CompletedStore,
    /// The world or server the seeds were last resolved for.
    seed_key: Option<String>,
    /// The Seed Atlas marker under the cursor on the map, for its menu.
    hovered_marker: Option<HoveredMarker>,
    /// Whether the game's biome looks were handed to Seed Atlas.
    looks_ready: bool,
    /// The marker the open map menu is for.
    menu_marker: Option<HoveredMarker>,
    /// The other player the map menu was opened on.
    menu_player: Option<String>,
    /// Where the minimap was drawn last (the position editor drags it).
    minimap_place: Option<meridian_core::screens::MinimapPlace>,
    /// The nearest search's options, kept for the session.
    nearest_opts: meridian_core::nearest::NearestOptions,
}

/// The last finished export's message, for the action bar.
static EXPORT_NOTICE: Mutex<Option<String>> = Mutex::new(None);

/// Dimensions the map can switch between, in order.
const MAP_DIMENSIONS: [&str; 3] = ["minecraft:overworld", "minecraft:the_nether", "minecraft:the_end"];

/// Blocks of the overworld per block of a dimension (the Nether's 8).
fn dimension_scale(dimension: &str) -> f64 {
    if dimension == "minecraft:the_nether" { 8.0 } else { 1.0 }
}

/// The game seen by a map that only reads its files: no player, no chunks.
struct ViewerHost<'a>(HorizonHost<'a>);

impl MapHost for ViewerHost<'_> {
    fn context(&self) -> Option<WorldContext> {
        None
    }
    fn player(&self) -> Option<PlayerState> {
        None
    }
    fn request_chunk(&self, _chunk: ChunkPos, _mode: ReadMode) {}
    fn take_chunks(&self) -> Vec<ChunkSnapshot> {
        Vec::new()
    }
    fn block_appearance(&self, block: &str) -> BlockAppearance {
        self.0.block_appearance(block)
    }
    fn biome_appearance(&self, biome: &str) -> BiomeAppearance {
        self.0.biome_appearance(biome)
    }
    fn day_time(&self) -> Option<i64> {
        self.0.day_time()
    }
    fn data_dir(&self) -> PathBuf {
        self.0.data_dir()
    }
    fn config_dir(&self) -> PathBuf {
        self.0.config_dir()
    }
}

/// Work a screen asked for that needs the game's write access.
#[derive(Debug)]
enum Pending {
    CloseScreen,
    Chat(String),
    /// A line for the action bar.
    Notice(String),
}

/// What a minimap image depends on: its origin and size, and the regions'
/// revisions (any redraw changes them).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct MapImageKey {
    origin: [i32; 2],
    size: u32,
    revisions: u64,
    overlay: Option<[u8; 2]>,
    /// The seed slime chunks are marked from (Overworld, seed known).
    slime: Option<i64>,
}

struct Meridian {
    state: Mutex<Option<State>>,
    renderer: gpu::MapRenderer,
}

/// Horizon's `Host` seen as a [`MapHost`].
struct HorizonHost<'a> {
    host: &'a dyn Host,
    dimension: Option<&'a str>,
}

fn data_dir(host: &dyn Host) -> PathBuf {
    host.instance_dir()
        .join("horizon-mods")
        .join(".horizon-mod-data")
        .join(MOD_ID)
}

fn config_dir(host: &dyn Host) -> PathBuf {
    host.config_dir().unwrap_or_else(|e| {
        log::warn!("meridian: no config folder ({e}), using the data folder");
        data_dir(host)
    })
}

fn tint_of(source: Option<TintSource>) -> Tint {
    match source {
        None => Tint::None,
        Some(TintSource::Grass) => Tint::Grass,
        Some(TintSource::Foliage) => Tint::Foliage,
        Some(TintSource::DryFoliage) => Tint::DryFoliage,
        Some(TintSource::Water) => Tint::Water,
        Some(TintSource::Fixed(c)) => Tint::Fixed(c & 0x00FF_FFFF),
    }
}

fn snapshot(s: bridge::Snapshot) -> ChunkSnapshot {
    ChunkSnapshot {
        dimension: DimensionId::new(s.dimension),
        pos: ChunkPos::new(s.pos[0], s.pos[1]),
        blocks: s.blocks,
        biomes: s.biomes,
        columns: s
            .columns
            .into_iter()
            .map(|c| ColumnSample {
                // Out of the palette reads as air: nothing to draw.
                block: if c.empty { u16::MAX } else { c.block },
                height: c.height,
                overlay: None,
                fluid: c.fluid,
                biome: c.biome,
                block_light: c.light & 0x0F,
                sky_light: c.light >> 4,
            })
            .collect(),
    }
}

impl MapHost for HorizonHost<'_> {
    fn context(&self) -> Option<WorldContext> {
        let dimension = DimensionId::new(self.dimension?);
        // Singleplayer also plays through a connection (address "local"):
        // the integrated server's open world is what tells it apart.
        let address = self.host.client_state().and_then(|s| s.server_address);
        let local = address.as_deref().is_none_or(|a| a == "local");
        let source = match (local, SERVER_WORLD.lock().clone()) {
            (true, Some(world)) => WorldSource::Singleplayer { world },
            _ => WorldSource::Multiplayer { address: address? },
        };
        Some(WorldContext { source, dimension })
    }

    fn player(&self) -> Option<PlayerState> {
        let p = self.host.player()?;
        Some(PlayerState {
            position: [p.x, p.y, p.z],
            yaw: p.yaw,
            pitch: p.pitch,
            dimension: DimensionId::new(p.dimension),
        })
    }

    fn request_chunk(&self, chunk: ChunkPos, mode: ReadMode) {
        let cave_from = match mode {
            ReadMode::Surface => None,
            ReadMode::CaveFrom(y) => Some(y),
        };
        bridge::request([chunk.x, chunk.z], cave_from);
    }

    fn take_chunks(&self) -> Vec<ChunkSnapshot> {
        bridge::take_results().into_iter().map(snapshot).collect()
    }

    fn block_appearance(&self, block: &str) -> BlockAppearance {
        let look = bridge::block_look(block).unwrap_or_default();
        // The top face's tint, from Horizon's model library (API 380).
        let sprites = self.host.block_sprites(&[block]).into_iter().next().flatten();
        // The top face, else the model's particle sprite (crossed plants).
        let top = sprites.as_ref().and_then(|s| s.sides[1].first());
        let tint = top.map_or(Tint::None, |f| tint_of(f.tint));
        let tint = if look.fluid == 1 { Tint::Water } else { tint };
        let sprite = top.map(|f| f.sprite).or_else(|| sprites.as_ref().and_then(|s| s.particle));
        let mean = sprite.and_then(|sp| {
            let mut guard = TEXTURES.lock();
            let textures = guard.get_or_insert_with(Textures::default);
            textures.index(self.host);
            textures.mean((sp.array, sp.layer))
        });
        // Without a texture, tinted blocks use a mid grey under their tint
        // and the rest their map colour.
        let texture_color = mean.unwrap_or(if tint == Tint::None { look.map_color } else { 0x9A_9A9A });
        BlockAppearance {
            map_color: look.map_color,
            texture_color,
            tint,
            translucent: look.fluid == 1,
            invisible: look.air || (look.map_color == 0 && look.fluid == 0),
        }
    }

    fn biome_appearance(&self, biome: &str) -> BiomeAppearance {
        let b = bridge::biome_look(biome).unwrap_or_default();
        BiomeAppearance {
            grass: b.grass,
            foliage: b.foliage,
            dry_foliage: b.dry_foliage,
            water: b.water,
        }
    }

    fn day_time(&self) -> Option<i64> {
        self.host.clock().map(|c| c.day_time)
    }

    fn data_dir(&self) -> PathBuf {
        data_dir(self.host)
    }

    fn config_dir(&self) -> PathBuf {
        config_dir(self.host)
    }
}

impl State {
    fn save_config(&self) {
        if let Err(e) = self.store.save(&self.config) {
            log::warn!("meridian: cannot save settings: {e}");
        }
    }

    fn save_book(&mut self) {
        if let Some((_, book)) = &mut self.book
            && book.dirty
            && let Err(e) = book.save()
        {
            log::warn!("meridian: cannot save waypoints: {e}");
        }
        self.last_book_save = Instant::now();
    }

    /// Opens the waypoints of `context`'s world (kept across its dimensions).
    fn open_book(&mut self, data_dir: &std::path::Path, context: &WorldContext) {
        let map = self.engine.map().to_owned();
        if self
            .book
            .as_ref()
            .is_some_and(|((source, m), _)| *source == context.source && *m == map)
        {
            return;
        }
        self.save_book();
        let paths = MapPaths::new(data_dir, &context.source, &map, &context.dimension);
        self.book = Some(((context.source.clone(), map), WaypointBook::load(paths.map_dir())));
    }

    /// Picks the map of the server played on (by world spawn, or as chosen)
    /// and switches the map and its waypoints when it changed.
    fn update_map(&mut self, data_dir: &std::path::Path) {
        let Some(context) = self.engine.context().cloned() else { return };
        let name = self.map_for(data_dir, &context.source, bridge::world_spawn());
        if self.engine.map() != name {
            log::info!("meridian: showing map {name}");
            self.pending.push(Pending::Notice(format!("Meridian: map {name}")));
            self.engine.set_map(&name);
            self.viewer = None;
            self.open_book(data_dir, &context);
        }
    }

    /// The map of a world or server to show now.
    fn map_for(&mut self, data_dir: &std::path::Path, source: &WorldSource, spawn: Option<[i32; 3]>) -> String {
        match source {
            WorldSource::Singleplayer { .. } => DEFAULT_MAP.to_owned(),
            WorldSource::Multiplayer { .. } => {
                if self.server_maps.as_ref().is_none_or(|(src, _)| *src != *source) {
                    let dir = meridian_core::persist::server_dir(data_dir, source);
                    self.server_maps = Some((source.clone(), meridian_core::subworld::ServerMaps::load(&dir)));
                }
                let mode = self.config.current().world_map.multiplayer_maps;
                let (_, maps) = self.server_maps.as_mut().expect("loaded above");
                let (name, changed) = maps.choose(mode, spawn);
                if changed && let Err(e) = maps.save() {
                    log::warn!("meridian: saving the server's map list failed: {e}");
                }
                name
            }
        }
    }

    /// `/meridian map [list | use <name> | new <name>]`: the maps of the
    /// server played on.
    fn map_command(&mut self, args: &[&str]) -> String {
        let Some(WorldSource::Multiplayer { .. }) = self.engine.context().map(|c| &c.source) else {
            return "Worlds in singleplayer have one map".to_owned();
        };
        let Some((_, maps)) = self.server_maps.as_mut() else {
            return "No server map list yet".to_owned();
        };
        let name = args.get(1..).map(|r| r.join(" ")).unwrap_or_default();
        match args.first().copied() {
            None | Some("list") => format!("Maps: {} (shown: {})", maps.names().join(", "), self.engine.map()),
            Some(op @ ("use" | "new")) => {
                if !maps.select(&name, op == "new") {
                    return format!("No map named {name}");
                }
                if let Err(e) = maps.save() {
                    log::warn!("meridian: saving the server's map list failed: {e}");
                }
                let mode = &mut self.config.current_mut().world_map.multiplayer_maps;
                let note = if *mode == meridian_core::config::MapSelection::Manual {
                    String::new()
                } else {
                    *mode = meridian_core::config::MapSelection::Manual;
                    " (map selection set to manual)".to_owned()
                };
                self.engine.set_settings(self.config.current().clone());
                self.save_config();
                format!("Showing map {name}{note}")
            }
            Some("rename") => {
                let Some((old, new)) = name.split_once(' ').map(|(a, b)| (a.trim().to_owned(), b.trim().to_owned())) else {
                    return "/meridian map rename <old> <new>".to_owned();
                };
                if old == DEFAULT_MAP || !maps.maps.contains_key(&old) || maps.maps.contains_key(&new) || new.is_empty() || new.contains(' ') {
                    return format!("Cannot rename {old} to {new}");
                }
                let shown = self.engine.map() == old;
                if shown {
                    return "Switch to another map first (/meridian map use <name>)".to_owned();
                }
                // The files move with the name: maps live in one folder each.
                let Some(context) = self.engine.context() else { return "No world open".to_owned() };
                let Some(data_dir) = self.export_dir.parent() else { return "No data folder".to_owned() };
                let dir = meridian_core::persist::server_dir(data_dir, &context.source);
                let from = dir.join(meridian_core::persist::escape_component(&old));
                let to = dir.join(meridian_core::persist::escape_component(&new));
                if from.exists() && let Err(e) = std::fs::rename(&from, &to) {
                    return format!("Renaming the map's folder failed: {e}");
                }
                let entry = maps.maps.remove(&old).unwrap_or_default();
                maps.maps.insert(new.clone(), entry);
                if maps.current == old {
                    maps.current = new.clone();
                }
                if let Err(e) = maps.save() {
                    log::warn!("meridian: saving the server's map list failed: {e}");
                }
                format!("Renamed map {old} to {new}")
            }
            _ => "/meridian map [list | use <name> | new <name> | rename <old> <new>]".to_owned(),
        }
    }

    /// `/meridian radar hide|show <entity id>`, `/meridian radar hidden`:
    /// entity kinds the radar never shows.
    fn radar_command(&mut self, args: &[&str]) -> String {
        let id = args.get(1).map(|a| if a.contains(':') { (*a).to_owned() } else { format!("minecraft:{a}") });
        let excluded = &mut self.config.current_mut().radar.excluded;
        let reply = match (args.first().copied(), id) {
            (Some("hide"), Some(id)) => {
                excluded.insert(id.clone());
                format!("Radar hides {id}")
            }
            (Some("show"), Some(id)) => {
                if !excluded.remove(&id) {
                    return format!("{id} was not hidden");
                }
                format!("Radar shows {id} again")
            }
            (Some("hidden") | None, _) => {
                return if excluded.is_empty() {
                    "The radar hides no entity kinds".to_owned()
                } else {
                    format!("Hidden: {}", excluded.iter().cloned().collect::<Vec<_>>().join(", "))
                };
            }
            _ => return "/meridian radar hide|show <entity id>, /meridian radar hidden".to_owned(),
        };
        self.engine.set_settings(self.config.current().clone());
        self.save_config();
        reply
    }

    fn close_book(&mut self) {
        self.save_book();
        self.book = None;
    }

    /// The open book and the current dimension's id.
    fn book_here(&mut self) -> Option<(&mut WaypointBook, String)> {
        let dim = self.dimension.clone()?;
        self.book.as_mut().map(|(_, b)| (b, dim))
    }

    /// Adds a waypoint at the player; returns its name.
    fn quick_waypoint(&mut self, player: &PlayerState) -> Option<String> {
        let color = self.next_color;
        self.next_color = (self.next_color + 1) % 16;
        let to_bottom = false;
        let (book, dim) = self.book_here()?;
        let n = book.dimension(&dim).map_or(0, |d| d.current().len()) + 1;
        let name = format!("Waypoint {n}");
        let [x, y, z] = player.position;
        let mut w = Waypoint::new(
            name.clone(),
            [x.floor() as i32, y.floor() as i32, z.floor() as i32],
            color,
        );
        // Skip black and dark grey, which vanish on most terrain.
        if matches!(w.color, 0 | 8) {
            w.color = 12;
        }
        book.add(&dim, w, to_bottom, waypoint::now());
        Some(name)
    }
}

impl Meridian {
    /// Runs `f` with the state, creating it on first use (the folders are
    /// only known once the game calls a hook).
    fn with_state<R>(&self, host: &dyn Host, f: impl FnOnce(&mut State) -> R) -> R {
        let mut guard = self.state.lock();
        let state = guard.get_or_insert_with(|| {
            let store = ConfigStore::new(&config_dir(host));
            let mut config = store.load();
            meridian_core::seedatlas_state::normalize(&mut config.current_mut().seed_atlas);
            let engine = MapEngine::new(config.current().clone(), data_dir(host));
            if bridge::VERSION == BRIDGE_VERSION {
                bridge::enable();
            } else {
                log::error!(
                    "meridian: bridge version {} (expected {BRIDGE_VERSION}): no map data",
                    bridge::VERSION
                );
            }
            log::info!("meridian {VERSION}: settings in {}", store.path().display());
            load_icons();
            State {
                config,
                store,
                engine,
                controls: Controls::default(),
                dimension: None,
                book: None,
                server_maps: None,
                last_book_save: Instant::now(),
                map_frame: Instant::now(),
                next_color: 12,
                notice: None,
                map_image: None,
                screen: None,
                open_screen: None,
                pending: Vec::new(),
                cave: Default::default(),
                death_recorded: false,
                map: WorldMap::new([0.0; 3]),
                map_open: false,
                open_map: false,
                return_to_map: false,
                viewer: None,
                export_dir: data_dir(host).join("exports"),
                levels: 0,
                seeds: meridian_core::seedatlas::SeedBook::load(&data_dir(host)),
                seed_svc: Default::default(),
                completed: meridian_core::completed::CompletedStore::load(&data_dir(host)),
                seed_key: None,
                hovered_marker: None,
                looks_ready: false,
                menu_marker: None,
                menu_player: None,
                minimap_place: None,
                nearest_opts: Default::default(),
            }
        });
        f(state)
    }

    fn send(&self, host: &dyn Host, event: HostEvent) {
        self.with_state(host, |s| {
            let map_host = HorizonHost {
                host,
                dimension: s.dimension.as_deref(),
            };
            s.engine.handle(&map_host, event);
        });
    }
}

/// Draws the core's operations into Horizon's HUD batch. Text at another
/// scale than the GUI's goes through a pushed pose (`gui_paint`).
fn paint(host: &dyn Host, batch: &mut HudBatch, ops: &[DrawOp]) {
    use horizon_client::hud_layout::PoseOp;
    use horizon_client::mod_api::GuiDraw;
    for op in ops {
        match op {
            DrawOp::Fill { rect, argb } => host.fill(batch, *rect, *argb),
            DrawOp::Quad { points, argb } => host.quad(batch, *points, *argb),
            DrawOp::NineSlice { sprite, rect, argb } => host.gui_nine_slice(batch, sprite, *rect, *argb),
            DrawOp::Icon { id, rect, dim } => {
                load_icons();
                let Some((_, info)) = icon(id) else { continue };
                // Fitted into `rect` like `IconLayout` (longer side 18 at
                // the default), each source pixel row as runs of fills
                // under a scaled pose so the edges stay crisp.
                let (w, h) = (info.width.max(1) as f32, info.height.max(1) as f32);
                let k = (rect[2] / w).min(rect[3] / h);
                let (x0, y0) = (rect[0] + (rect[2] - w * k) / 2.0, rect[1] + (rect[3] - h * k) / 2.0);
                let mut draws = vec![GuiDraw::Push(vec![PoseOp::Translate(x0.round(), y0.round()), PoseOp::Scale(k, k)])];
                for y in 0..info.height {
                    let mut x = 0;
                    while x < info.width {
                        let at = ((y * info.width + x) * 4) as usize;
                        let px = &info.rgba[at..at + 4];
                        let color = u32::from(px[3]) << 24 | u32::from(px[0]) << 16 | u32::from(px[1]) << 8 | u32::from(px[2]);
                        let mut end = x + 1;
                        while end < info.width && info.rgba[((y * info.width + end) * 4) as usize..][..4] == *px {
                            end += 1;
                        }
                        if px[3] != 0 {
                            draws.push(GuiDraw::Fill { x0: x as i32, y0: y as i32, x1: end as i32, y1: y as i32 + 1, color });
                        }
                        x = end;
                    }
                }
                if *dim {
                    draws.push(GuiDraw::Fill { x0: 0, y0: 0, x1: info.width as i32, y1: info.height as i32, color: 0x9030_3030 });
                }
                draws.push(GuiDraw::Pop);
                host.gui_paint(batch, &draws);
            }
            DrawOp::Paragraph { x, y, width, text, argb } => {
                // Word-wrapped to `width` with the game's font metrics.
                let mut line = String::new();
                let mut row = 0.0;
                for word in text.split_whitespace() {
                    let candidate = if line.is_empty() { word.to_owned() } else { format!("{line} {word}") };
                    if !line.is_empty() && host.text_width(&candidate) > *width {
                        host.text(batch, &line, [x.round(), (y + row).round()], argb & 0x00FF_FFFF, false);
                        row += 9.0;
                        line = word.to_owned();
                    } else {
                        line = candidate;
                    }
                }
                if !line.is_empty() {
                    host.text(batch, &line, [x.round(), (y + row).round()], argb & 0x00FF_FFFF, false);
                }
            }
            DrawOp::Blit { texture, rect, uv, argb } => {
                // A missing texture (another resource pack) draws nothing.
                let Some(tex) = texture_size(texture) else { continue };
                // `uv` is in texels of a 64-wide texture; higher resolutions scale it.
                let k = (tex[0] / 64).max(1);
                let [u, v, w, h] = uv.map(|c| c as i32 * k);
                const SIDE: i32 = 16;
                host.gui_paint(
                    batch,
                    &[
                        GuiDraw::Push(vec![PoseOp::Translate(rect[0], rect[1]), PoseOp::Scale(rect[2] / SIDE as f32, rect[3] / SIDE as f32)]),
                        GuiDraw::Blit {
                            texture: texture.clone().into(),
                            blend: horizon_client::hud_layout::Blend::Textured,
                            x: 0,
                            y: 0,
                            w: SIDE,
                            h: SIDE,
                            uv: [u as f32, v as f32],
                            src: [w, h],
                            tex,
                            color: *argb,
                        },
                        GuiDraw::Pop,
                    ],
                );
            }
            DrawOp::Plate { x, y, text, align, scale, pad, argb } => {
                let w = host.text_width(text) * scale;
                let x = match align {
                    Align::Left => *x,
                    Align::Center => x - w / 2.0,
                    Align::Right => x - w,
                };
                host.fill(batch, [x - pad, y - pad, w + 2.0 * pad, 8.0 * scale + 2.0 * pad], *argb);
            }
            DrawOp::Text { x, y, text, argb, align, scale } => {
                let w = host.text_width(text) * scale;
                let x = match align {
                    Align::Left => *x,
                    Align::Center => x - w / 2.0,
                    Align::Right => x - w,
                };
                if (scale - 1.0).abs() < 1e-3 {
                    host.text(batch, text, [x.round(), y.round()], argb & 0x00FF_FFFF, true);
                } else {
                    host.gui_paint(
                        batch,
                        &[
                            GuiDraw::Push(vec![PoseOp::Translate(x, *y), PoseOp::Scale(*scale, *scale)]),
                            GuiDraw::Text { text: Text::literal(text.clone()), x: 0, y: 0, color: *argb, shadow: true },
                            GuiDraw::Pop,
                        ],
                    );
                }
            }
        }
    }
}


impl Meridian {
    /// A key of ours went down or up; returns feedback for the action bar.
    fn on_key(&self, host: &dyn Host, key: &str, pressed: bool) -> Option<String> {
        let chosen = |id: &str, default: &str| host.mod_key(MOD_ID, id, default);
        let hits: Vec<&'static str> = bindings_for_key(key, chosen).map(|b| b.id).collect();
        if hits.is_empty() {
            return None;
        }
        self.with_state(host, |s| {
            let mut changed = false;
            for id in &hits {
                changed |= s
                    .controls
                    .key(id, pressed, &mut s.config.current_mut().minimap);
            }
            let player = HorizonHost {
                host,
                dimension: s.dimension.as_deref(),
            }
            .player();
            if pressed {
                for id in &hits {
                    match *id {
                        // The editor screen comes later; both add at the player for now.
                        "waypoint.instant" => {
                            if let Some(p) = &player
                                && let Some(name) = s.quick_waypoint(p)
                            {
                                s.notice = Some(format!("Added {name}"));
                            }
                        }
                        "waypoint.new" => {
                            if let Some((book, dim)) = s.book_here() {
                                let editor = Editor::new(book.dimension_mut(&dim), waypoint::now(), false);
                                s.open_screen = Some(Screen::Editor(editor));
                            }
                        }
                        "waypoint.menu" => s.open_screen = Some(Screen::List(List::default())),
                        "worldmap.open" if s.config.current().world_map.enabled => {
                            let animate = s.config.current().world_map.opening_animation;
                            if let Some(p) = &player {
                                s.map.open(p.position, animate);
                            }
                            s.map.menu = None;
                            s.open_map = true;
                        }
                        "worldmap.settings" => s.open_screen = Some(Screen::Settings(SettingsScreen::world_map())),
                        "settings" => s.open_screen = Some(Screen::Settings(SettingsScreen::default())),
                        "waypoint.next_set" => {
                            if let Some((book, dim)) = s.book_here() {
                                let name = book.dimension_mut(&dim).next_set().to_owned();
                                book.dirty = true;
                                s.notice = Some(format!("Waypoint set: {name}"));
                            }
                        }
                        "waypoint.all_sets" => {
                            let w = &mut s.config.current_mut().waypoints;
                            w.all_sets = !w.all_sets;
                            changed = true;
                        }
                        "radar.toggle" => {
                            let r = &mut s.config.current_mut().radar;
                            r.enabled = !r.enabled;
                            changed = true;
                        }
                        "cave.toggle_manual" => {
                            if let Some(p) = &player {
                                s.cave.toggle_manual(p.position[1] + 1.62);
                                s.notice = Some(if s.cave.manual() { "Cave mode: manual" } else { "Cave mode: auto" }.to_owned());
                            }
                        }
                        "radar.reverse" => {
                            let r = &mut s.config.current_mut().radar;
                            r.reversed = !r.reversed;
                            changed = true;
                        }
                        "waypoint.toggle_world" => {
                            let w = &mut s.config.current_mut().waypoints;
                            w.in_world = !w.in_world;
                            changed = true;
                        }
                        "overlay.grid" | "overlay.slime" | "overlay.light" => {
                            let m = &mut s.config.current_mut().minimap;
                            let (flag, name) = match *id {
                                "overlay.grid" => (&mut m.chunk_grid, "Chunk grid"),
                                "overlay.slime" => (&mut m.slime_chunks, "Slime chunks"),
                                _ => (&mut m.light_overlay, "Light overlay"),
                            };
                            *flag = !*flag;
                            s.notice = Some(format!("{name}: {}", if *flag { "on" } else { "off" }));
                            changed = true;
                        }
                        "waypoint.toggle_minimap" => {
                            let w = &mut s.config.current_mut().waypoints;
                            w.on_minimap = !w.on_minimap;
                            changed = true;
                        }
                        _ => {}
                    }
                }
            }
            if changed {
                s.engine.set_settings(s.config.current().clone());
                s.save_config();
            }
            let map_host = HorizonHost {
                host,
                dimension: s.dimension.as_deref(),
            };
            for id in hits {
                s.engine.handle(&map_host, HostEvent::Key { id, pressed });
            }
            s.notice.take()
        })
    }

    /// `/meridian ...`; answers what to tell the player and a chat line to
    /// send (a teleport or a shared waypoint).
    fn on_command(&self, host: &dyn Host, args: &[&str]) -> (String, Option<String>) {
        const USAGE: &str = "/meridian waypoint add|remove|tp|share|list [name], /meridian waypoint export, /meridian waypoint import <file or folder>";
        self.with_state(host, |s| {
            let export_dir = s.export_dir.clone();
            let world_name = context_labels(s).0.trim_end_matches(" (auto)").to_owned();
            let player = HorizonHost {
                host,
                dimension: s.dimension.as_deref(),
            }
            .player();
            let tp = s.config.current().waypoints.teleport_format.clone();
            let Some((book, dim)) = s.book_here() else {
                return ("Meridian: no world open".to_owned(), None);
            };
            if args.get(1) == Some(&"map") {
                return (s.map_command(&args[2..]), None);
            }
            if args.get(1) == Some(&"radar") {
                return (s.radar_command(&args[2..]), None);
            }
            // /meridian waypoint <action> [name...]
            if args.get(1) != Some(&"waypoint") {
                return (USAGE.to_owned(), None);
            }
            let name = args.get(3..).map(|r| r.join(" ")).unwrap_or_default();
            let find = |book: &WaypointBook| {
                book.dimension(&dim).and_then(|d| {
                    d.current()
                        .iter()
                        .find(|w| w.name.eq_ignore_ascii_case(&name))
                        .cloned()
                })
            };
            match args.get(2).copied() {
                Some("add") => {
                    let Some(p) = player else {
                        return ("Meridian: no player".to_owned(), None);
                    };
                    let label = if name.is_empty() {
                        "Waypoint".to_owned()
                    } else {
                        name.clone()
                    };
                    let [x, y, z] = p.position;
                    let w = Waypoint::new(
                        label.clone(),
                        [x.floor() as i32, y.floor() as i32, z.floor() as i32],
                        12,
                    );
                    book.add(&dim, w, false, waypoint::now());
                    (format!("Added waypoint {label}"), None)
                }
                Some("remove") => match find(book) {
                    Some(w) => {
                        book.dimension_mut(&dim).remove(w.id);
                        book.dirty = true;
                        (format!("Removed waypoint {}", w.name), None)
                    }
                    None => (format!("No waypoint named {name}"), None),
                },
                Some("tp") => match find(book) {
                    Some(w) => (
                        format!("Teleporting to {}", w.name),
                        Some(waypoint::command(&tp, &w, &dim)),
                    ),
                    None => (format!("No waypoint named {name}"), None),
                },
                Some("share") => match find(book) {
                    Some(w) => ("Shared".to_owned(), Some(waypoint::share_text(&w, &dim))),
                    None => (format!("No waypoint named {name}"), None),
                },
                Some("export") => {
                    let safe: String = world_name
                        .chars()
                        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
                        .collect();
                    let path = export_dir.join(format!("waypoints-{safe}.json"));
                    let written = std::fs::create_dir_all(&export_dir)
                        .and_then(|_| std::fs::write(&path, meridian_core::waypoint_io::export_json(book)));
                    match written {
                        Ok(()) => (format!("Waypoints exported to {}", path.display()), None),
                        Err(e) => (format!("Export failed: {e}"), None),
                    }
                }
                Some("import") => {
                    if name.is_empty() {
                        return ("/meridian waypoint import <file.json | waypoint folder>".to_owned(), None);
                    }
                    let given = std::path::PathBuf::from(&name);
                    let path = if given.is_absolute() { given } else { export_dir.join(given) };
                    let now = waypoint::now();
                    let added = if path.is_dir() {
                        let found = meridian_core::waypoint_io::read_text_folder(&path);
                        if found.is_empty() {
                            return (format!("No waypoint files in {}", path.display()), None);
                        }
                        found.into_iter().map(|(d, ws)| meridian_core::waypoint_io::merge(book, &d, ws, now)).sum()
                    } else {
                        let text = match std::fs::read_to_string(&path) {
                            Ok(t) => t,
                            Err(e) => return (format!("Cannot read {}: {e}", path.display()), None),
                        };
                        if path.extension().is_some_and(|e| e == "txt") {
                            meridian_core::waypoint_io::merge(book, &dim, meridian_core::waypoint_io::parse_text(&text), now)
                        } else {
                            match meridian_core::waypoint_io::import_json(&text) {
                                Ok(dims) => meridian_core::waypoint_io::merge_export(book, dims, now),
                                Err(e) => return (format!("Not a waypoint export: {e}"), None),
                            }
                        }
                    };
                    (format!("Imported {added} waypoints"), None)
                }
                Some("list") => {
                    let names: Vec<String> = book
                        .dimension(&dim)
                        .map(|d| d.current().iter().map(|w| w.name.clone()).collect())
                        .unwrap_or_default();
                    (
                        if names.is_empty() {
                            "No waypoints".to_owned()
                        } else {
                            names.join(", ")
                        },
                        None,
                    )
                }
                _ => (USAGE.to_owned(), None),
            }
        })
    }
}

impl Hooks for Meridian {
    fn render(&self) -> Option<&dyn RenderHooks> {
        Some(&self.renderer)
    }

    fn screen_render_post(&self, host: &dyn Host, event: &mut ScreenRenderPostEvent<'_>) {
        if event.screen.id == "mod" {
            if self.map_open(host) {
                self.draw_map(host, event.batch, false);
            } else {
                // A menu opened from the map keeps the map behind it, as the
                // Fabric version's menus do (`SeedAtlasScreen`).
                if self.map_behind(host) {
                    self.draw_map(host, event.batch, true);
                }
                self.draw_screen(host, event.batch);
            }
        }
        if event.screen.id == "dead" {
            self.record_death(host);
        }
    }

    fn screen_mouse_button_pressed_pre(&self, host: &dyn Host, event: &mut ScreenMouseButtonPressedPreEvent<'_>) -> Flow {
        if host.mod_screen() != Some(MOD_ID) || event.button > 1 {
            return Flow::Continue;
        }
        let at = host.cursor();
        let right = event.button == 1;
        if self.map_open(host) {
            let (layer_button, settings_button, map_settings_button) = seed_atlas_buttons(Self::map_view(host).gui);
            let on = self.with_state(host, |s| s.config.current().seed_atlas.enabled);
            if on && !right && inside(layer_button, at) {
                self.with_state(host, |s| {
                    let sa = &mut s.config.current_mut().seed_atlas;
                    sa.biome_layer = !sa.biome_layer;
                    s.engine.set_settings(s.config.current().clone());
                    s.save_config();
                });
                return Flow::Cancel;
            }
            if on && !right && inside(settings_button, at) {
                self.with_state(host, |s| s.open_from_map(Screen::SeedAtlasSettings(meridian_core::screens_sa::SeedAtlasSettingsScreen::new())));
                return Flow::Cancel;
            }
            if on && !right && inside(map_settings_button, at) {
                self.with_state(host, |s| s.open_from_map(Screen::Settings(SettingsScreen::world_map())));
                return Flow::Cancel;
            }
            self.map_input(host, |s, view, map_data| {
                if right {
                    let w = s.map.to_world(at, view.gui, view.unit);
                    let height = s.engine.sampler().column(w[0].floor() as i32, w[1].floor() as i32).map(|c| c.height);
                    let waypoints = map_data.waypoints.iter().collect::<Vec<_>>();
                    s.map.right_click(at, view.gui, view.unit, height, &waypoints);
                    s.menu_marker = s.hovered_marker.clone();
                    // Another player under the cursor (the map of the player's own dimension).
                    s.menu_player = None;
                    if s.viewer.is_none() {
                        let reach = 6.0 * view.unit;
                        s.menu_player = bridge::entities()
                            .into_iter()
                            .filter(|e| e.class == bridge::EntityClass::Player)
                            .filter_map(|e| {
                                let p = s.map.to_screen([e.pos[0], e.pos[2]], view.gui, view.unit);
                                let d = (p[0] - at[0]).hypot(p[1] - at[1]);
                                (d <= reach).then_some((d, e.name?))
                            })
                            .min_by(|a, b| a.0.total_cmp(&b.0))
                            .map(|(_, name)| name);
                        if let (Some(name), Some(menu)) = (&s.menu_player, &mut s.map.menu) {
                            let at = menu.items.len() - 1;
                            menu.items.insert(at, MenuItem::Extra(MENU_TELEPORT_PLAYER, format!("Teleport To {name}")));
                        }
                    }
                    if let (Some(m), Some(menu)) = (&s.menu_marker, &mut s.map.menu) {
                        let done = m.key.as_ref().is_some_and(|k| s.completed.is_completed(k));
                        let at = menu.items.len() - 1;
                        menu.items.insert(at, MenuItem::Extra(MENU_TEMPORARY_WAYPOINT, "Create temporary waypoint".into()));
                        menu.items.insert(at, MenuItem::Extra(MENU_STRUCTURE_WAYPOINT, "Create waypoint".into()));
                        if m.key.is_some() {
                            let label = if done { "Mark as incomplete" } else { "Mark as completed" };
                            menu.items.insert(at, MenuItem::Extra(MENU_COMPLETE, label.into()));
                        }
                    }
                } else {
                    s.map.press(at);
                }
                MapAction::None
            });
            return Flow::Cancel;
        }
        self.with_screen(host, |screen, env| ((), screen.click(at, right, env)));
        Flow::Cancel
    }

    fn screen_mouse_button_released_pre(&self, host: &dyn Host, event: &mut ScreenMouseButtonReleasedPreEvent<'_>) -> Flow {
        if host.mod_screen() != Some(MOD_ID) {
            return Flow::Continue;
        }
        if !self.map_open(host) {
            if event.button == 0 {
                self.with_screen(host, |screen, _| {
                    screen.release();
                    ((), Action::None)
                });
            }
            return Flow::Continue;
        }
        let at = host.cursor();
        if event.button == 0 {
            self.map_input(host, |s, view, _| s.map.release(at, view.gui, view.unit));
        }
        Flow::Cancel
    }

    fn screen_mouse_dragged_pre(&self, host: &dyn Host, event: &mut ScreenMouseDraggedPreEvent<'_>) -> Flow {
        if host.mod_screen() != Some(MOD_ID) {
            return Flow::Continue;
        }
        if !self.map_open(host) {
            if event.button == 0 {
                let at = host.cursor();
                self.with_screen(host, |screen, env| {
                    screen.drag(at, env);
                    ((), Action::None)
                });
            }
            return Flow::Continue;
        }
        if event.button == 0 {
            let delta = event.delta;
            self.map_input(host, |s, view, _| {
                if s.map.press.is_some() {
                    s.map.drag(delta, view.unit);
                }
                MapAction::None
            });
        }
        Flow::Cancel
    }

    fn screen_char_typed_pre(&self, host: &dyn Host, event: &mut ScreenCharTypedPreEvent<'_>) -> Flow {
        if host.mod_screen() != Some(MOD_ID) {
            return Flow::Continue;
        }
        let c = event.c;
        self.with_screen(host, |screen, env| {
            screen.typed(c, env);
            ((), Action::None)
        });
        Flow::Cancel
    }

    fn screen_key_pressed_pre(&self, host: &dyn Host, event: &mut ScreenKeyPressedPreEvent<'_>) -> Flow {
        // Escape still closes the screen as the game does.
        if host.mod_screen() != Some(MOD_ID) || event.key == "key.keyboard.escape" {
            return Flow::Continue;
        }
        let key = event.key.to_owned();
        if self.map_open(host) {
            self.map_key(host, &key);
            return Flow::Cancel;
        }
        self.with_screen(host, |screen, env| ((), screen.key(&key, env)));
        Flow::Cancel
    }

    fn screen_mouse_scrolled_pre(&self, host: &dyn Host, event: &mut ScreenMouseScrolledPreEvent<'_>) -> Flow {
        if host.mod_screen() != Some(MOD_ID) {
            return Flow::Continue;
        }
        let lines = event.scroll[1] as f32;
        if self.map_open(host) {
            let at = host.cursor();
            self.map_input(host, |s, view, _| {
                s.map.scroll(lines as f64, at, view.gui, view.unit);
                MapAction::None
            });
            return Flow::Cancel;
        }
        self.with_screen(host, |screen, env| {
            screen.scroll(lines, env);
            ((), Action::None)
        });
        Flow::Cancel
    }

    fn gui_layer_pre(&self, host: &mut dyn Host, _event: &mut GuiLayerPreEvent<'_>) -> Flow {
        // The world map covers the HUD, as an opaque screen would.
        if host.mod_screen() == Some(MOD_ID) && (self.map_open(&*host) || self.map_behind(&*host)) { Flow::Cancel } else { Flow::Continue }
    }

    fn render_hand(&self, host: &dyn Host, _event: &mut RenderHandEvent<'_>) -> Flow {
        if host.mod_screen() == Some(MOD_ID) && (self.map_open(host) || self.map_behind(host)) { Flow::Cancel } else { Flow::Continue }
    }

    fn menu_background(&self, host: &dyn Host, event: &mut MenuBackgroundEvent<'_>) {
        // Our screens blur the world behind them as the game's menus do;
        // the position editor keeps the minimap it moves sharp.
        let positioning = || self.with_state(host, |s| matches!(s.screen, Some(Screen::Position(_))));
        if host.mod_screen() == Some(MOD_ID) && event.radius.is_none() && !self.map_open(host) && !positioning() && !self.map_behind(host) {
            let blur = host.client_state().map_or(5.0, |c| c.menu_blur);
            *event.radius = Some(blur.max(1.0));
        }
    }

    fn screen_closing(&self, host: &dyn Host, event: &mut ScreenClosingEvent<'_>) {
        if event.screen.id == "mod" && host.mod_screen() == Some(MOD_ID) {
            self.with_state(host, |s| {
                s.screen = None;
                s.map_open = false;
                s.return_to_map = false;
                s.map.press = None;
                if !s.map_open {
                    s.viewer = None;
                }
            });
            gpu_map::set_frame(None);
        }
    }

    fn mods(&self, _host: &mut dyn Host, event: &mut ModsEvent<'_>) {
        event.mods.push(NativeMod {
            id: MOD_ID.to_owned(),
            title: format!("Meridian {VERSION}"),
            description: "Minimap, world map, waypoints and Seed Atlas.".to_owned(),
            keys: KEY_BINDINGS
                .iter()
                .map(|b| [b.id.to_owned(), b.label.to_owned(), b.default.to_owned()])
                .collect(),
        });
    }

    fn key(&self, host: &mut dyn Host, event: &mut KeyEvent<'_>) {
        if !host.playing() {
            return;
        }
        let notice = self.on_key(&*host, event.key, event.pressed);
        if let Some(text) = notice {
            host.client_action_bar(Text::literal(text), false);
        }
        let (open, open_map) = self.with_state(&*host, |s| (s.open_screen.take(), std::mem::take(&mut s.open_map)));
        if let Some(screen) = open
            && host.mod_screen_open(MOD_ID)
        {
            self.with_state(&*host, |s| s.screen = Some(screen));
        } else if open_map && host.mod_screen_open(MOD_ID) {
            self.with_state(&*host, |s| {
                s.screen = None;
                s.map_open = true;
            });
        }
    }

    fn commands(&self, _host: &mut dyn Host, event: &mut CommandsEvent<'_>) {
        event.commands.push("meridian");
        event.commands.push("seedatlas");
    }

    fn command(&self, host: &mut dyn Host, event: &mut CommandEvent<'_>) -> Flow {
        let args: Vec<&str> = event.command.split_whitespace().collect();
        if args.first() == Some(&"seedatlas") {
            let rest = event.command.trim_start().strip_prefix("seedatlas").unwrap_or("").trim().to_owned();
            let (reply, open) = self.on_seedatlas(&*host, &rest);
            if !reply.is_empty() {
                host.client_action_bar(Text::literal(reply), false);
            }
            if open && host.mod_screen_open(MOD_ID) {
                self.with_state(&*host, |s| {
                    s.map_open = false;
                    s.screen = Some(Screen::SeedAtlasSettings(meridian_core::screens_sa::SeedAtlasSettingsScreen::new()));
                });
            }
            return Flow::Cancel;
        }
        if args.first() != Some(&"meridian") {
            return Flow::Continue;
        }
        let (reply, chat) = self.on_command(&*host, &args);
        host.client_action_bar(Text::literal(reply), false);
        if let Some(line) = chat
            && let Err(e) = host.send_chat(&line)
        {
            host.client_action_bar(Text::literal(format!("Meridian: {e}")), false);
        }
        Flow::Cancel
    }

    fn client_chat_received(
        &self,
        host: &mut dyn Host,
        event: &mut ClientChatReceivedEvent<'_>,
    ) -> Flow {
        let Some((name, pos, shared_dim)) = waypoint::parse_shared(&event.message.plain()) else {
            return Flow::Continue;
        };
        let added = self.with_state(&*host, |s| {
            let (book, dim) = s.book_here()?;
            if shared_dim.as_deref().is_some_and(|d| d != dim) {
                return None;
            }
            let exists = book
                .dimension(&dim)
                .is_some_and(|d| d.current().iter().any(|w| w.pos == pos));
            if exists {
                return None;
            }
            let mut w = Waypoint::new(name.clone(), pos, 11);
            w.kind = Kind::Temporary;
            book.add(&dim, w, false, waypoint::now());
            Some(name)
        });
        if let Some(name) = added {
            host.client_action_bar(
                Text::literal(format!("Shared waypoint added: {name} (temporary)")),
                false,
            );
        }
        Flow::Continue
    }

    fn world_lifecycle(&self, host: &mut dyn Host, event: &mut WorldLifecycleEvent<'_>) {
        // Runs on the integrated server's thread in singleplayer.
        match event.phase {
            WorldPhase::WorldOpenReady => {
                let name = host
                    .world_dir()
                    .and_then(|d| d.file_name().map(|n| n.to_string_lossy().into_owned()));
                *SERVER_WORLD.lock() = name;
                *SERVER_SEED.lock() = host.world_dir().and_then(|d| {
                    let level = horizon_client::save::Level::read(&d)?;
                    Some((level.seed, horizon_client::save::WorldGen::preset(&d) == "minecraft:large_biomes"))
                });
            }
            WorldPhase::WorldCloseFinal | WorldPhase::WorldOpenFailed => {
                *SERVER_WORLD.lock() = None;
                *SERVER_SEED.lock() = None;
            }
            _ => {}
        }
    }

    fn client_level_load(&self, host: &mut dyn Host, event: &mut ClientLevelLoadEvent<'_>) {
        let host: &dyn Host = host;
        self.with_state(host, |s| {
            let dimension = event.dimension.to_owned();
            s.levels += 1;
            s.viewer = None;
            s.dimension = Some(dimension.clone());
            let map_host = HorizonHost {
                host,
                dimension: Some(&dimension),
            };
            match map_host.context() {
                Some(context) => {
                    // The spawn may still be the last server's: the frame
                    // corrects the map once the new one is announced.
                    let map = s.map_for(&data_dir(host), &context.source, None);
                    s.engine.prepare_map(&map);
                    s.open_book(&data_dir(host), &context);
                    s.engine.handle(&map_host, HostEvent::LevelLoaded(context));
                }
                None => log::warn!(
                    "meridian: cannot tell which world {} belongs to",
                    event.dimension
                ),
            }
        });
    }

    fn client_level_unload(&self, host: &mut dyn Host, _event: &mut ClientLevelUnloadEvent<'_>) {
        // Only the last level's unload closes the map: on a dimension change
        // the new level is already open.
        let last = self.with_state(&*host, |s| {
            s.levels = s.levels.saturating_sub(1);
            s.levels == 0
        });
        if last {
            self.send(host, HostEvent::LevelUnloaded);
        }
    }

    fn client_disconnecting(&self, host: &dyn Host, _event: &mut ClientDisconnectingEvent) {
        self.with_state(host, |s| {
            s.close_book();
            s.levels = 0;
            s.viewer = None;
        });
        self.send(host, HostEvent::Disconnected);
    }

    fn client_stopping(&self, host: &dyn Host, _event: &mut ClientStoppingEvent) {
        self.with_state(host, |s| {
            s.engine.flush();
            s.save_config();
            s.save_book();
        });
    }

    fn client_resources_reloaded(
        &self,
        host: &mut dyn Host,
        _event: &mut ClientResourcesReloadedEvent,
    ) {
        bridge::clear_looks();
        *TEXTURES.lock() = None;
        self.send(host, HostEvent::ResourcesReloaded);
    }

    fn frame(&self, host: &mut dyn Host, _event: &mut FrameEvent) {
        if let Some(text) = EXPORT_NOTICE.lock().take() {
            host.client_action_bar(Text::literal(text), false);
        }
        let pending = self.with_state(&*host, |s| std::mem::take(&mut s.pending));
        for p in pending {
            match p {
                Pending::CloseScreen => {
                    let back = self.with_state(&*host, |s| std::mem::take(&mut s.return_to_map));
                    if back {
                        self.with_state(&*host, |s| {
                            s.screen = None;
                            s.map_open = true;
                        });
                    } else {
                        host.mod_screen_close(MOD_ID);
                        self.with_state(&*host, |s| {
                            s.screen = None;
                            s.map_open = false;
                        });
                        gpu_map::set_frame(None);
                    }
                }
                Pending::Notice(line) => host.client_action_bar(Text::literal(line), false),
                Pending::Chat(line) => {
                    if let Err(e) = host.send_chat(&line) {
                        host.client_action_bar(Text::literal(format!("Meridian: {e}")), false);
                    }
                }
            }
        }
        let host: &dyn Host = host;
        self.with_state(host, |s| {
            s.update_map(&data_dir(host));
            s.update_seeds();
            s.seed_svc.poll();
            s.poll_nearest();
            let map_host = HorizonHost {
                host,
                dimension: s.dimension.as_deref(),
            };
            for change in bridge::take_changes() {
                let event = match change {
                    bridge::ChunkChange::Loaded([x, z]) => {
                        HostEvent::ChunkLoaded(ChunkPos::new(x, z))
                    }
                    bridge::ChunkChange::Changed([x, z]) => {
                        HostEvent::ChunkChanged(ChunkPos::new(x, z))
                    }
                    bridge::ChunkChange::Unloaded([x, z]) => {
                        HostEvent::ChunkUnloaded(ChunkPos::new(x, z))
                    }
                    bridge::ChunkChange::Reset => HostEvent::ChunksReset,
                };
                s.engine.handle(&map_host, event);
            }
            let player = map_host.player();
            if let Some(p) = &player {
                s.map.step(p.position);
                // Keep the regions around the player in memory.
                let here = RegionPos::of_block(p.position[0] as i32, p.position[2] as i32);
                s.engine
                    .request_area(here.offset(-1, -1), here.offset(1, 1));
            }
            // Cave mode: the roof check runs in the bridge, the decision here.
            let mut cave = s.config.current().minimap.cave.clone();
            if s.config.current().minimap.fair_play {
                cave.allowed = false;
            }
            bridge::set_roof_probe(meridian_core::cave::roof_radius(cave.auto));
            let terrain = &s.config.current().world_map.terrain;
            bridge::set_hidden(bridge::Hide {
                flowers: !terrain.show_flowers,
                redstone: !terrain.show_redstone,
                stained_glass: !terrain.stained_glass,
                lower_short: terrain.adjust_short_blocks,
            });
            if let Some(p) = &player {
                let dim = s.dimension.as_deref().unwrap_or("");
                let allowed = cave.allowed_dimensions.is_empty() || cave.allowed_dimensions.iter().any(|d| d == dim);
                let layer = s.cave.update(&cave, allowed, bridge::under_roof(), p.position[1] + 1.62, Instant::now());
                s.engine.set_layer(layer);
            }
            let budget =
                Duration::from_micros(s.config.current().performance.frame_budget_us as u64);
            s.engine.tick(&map_host, budget);
            if let Some((_, viewer)) = &mut s.viewer {
                viewer.tick(&ViewerHost(HorizonHost { host, dimension: None }), budget / 2);
            }
            if let Some(p) = player
                && s.config.current().waypoints.delete_reached_deathpoints
                && let Some((book, dim)) = s.book_here()
            {
                book.remove_reached_deathpoints(&dim, p.position);
            }
            if s.last_book_save.elapsed() >= Duration::from_secs(2) {
                s.save_book();
            }
        });
    }

    fn world_primitives(&self, host: &dyn Host, event: &mut WorldPrimitivesEvent<'_>) {
        self.with_state(host, |s| {
            let settings = s.config.current().waypoints.clone();
            if !settings.in_world {
                return;
            }
            let camera = event.camera;
            let Some(dim) = s.dimension.clone() else {
                return;
            };
            let Some((_, book)) = &s.book else { return };
            let Some(waypoints) = book.dimension(&dim) else {
                return;
            };
            let alpha = ((settings.opacity_in_world.min(100) as u32 * 255) / 100) << 24;
            for w in waypoints.visible(settings.all_sets) {
                let distance = w.distance(camera);
                if !w.global && settings.max_distance > 0 && distance > settings.max_distance as f64
                {
                    continue;
                }
                let color = alpha | w.rgb();
                let base = [
                    w.pos[0] as f64 + 0.5,
                    w.pos[1] as f64,
                    w.pos[2] as f64 + 0.5,
                ];
                let flat = ((base[0] - camera[0]).powi(2) + (base[2] - camera[2]).powi(2)).sqrt();
                if settings.beam && flat > BEAM_HIDE_DISTANCE {
                    event.primitives.beam(base, 256, color);
                }
            }
        });
    }

    fn hud(&self, host: &mut dyn Host, event: &mut HudEvent<'_>) {
        let host: &dyn Host = host;
        let Some(client) = host.client_state() else {
            gpu::set_frame(None);
            return;
        };
        if client.gui_hidden {
            gpu::set_frame(None);
            return;
        }
        let gui = client.gui_size;
        let gui_scale = bridge::window_size().map_or(2.0, |w| w[1] as f32 / gui[1].max(1.0));
        self.with_state(host, |s| {
            if host.playing() {
                // Alive again: the next death gets its own deathpoint.
                s.death_recorded = false;
            }
            if s.map_open {
                // The world map covers the screen; nothing of the HUD shows.
                gpu::set_frame(None);
                return;
            }
            let map_host = HorizonHost { host, dimension: s.dimension.as_deref() };
            let Some(player) = map_host.player() else { return };
            let mut ops = Vec::new();
            world_waypoints(host, s, gui, gui_scale, &mut ops);
            let settings = &s.config.current().minimap;
            let under_debug = settings.hide_under_debug && bridge::debug_shown();
            // The position editor shows the minimap it moves.
            let positioning = matches!(s.screen, Some(Screen::Position(_)));
            if positioning || (settings.enabled && !under_debug && !(settings.hide_under_screens && !host.playing())) {
                minimap_ops(host, s, &player, gui, gui_scale, &mut ops);
            } else {
                gpu::set_frame(None);
            }
            paint(host, event.batch, &ops);
        });
    }
}

/// Waypoints in the world, drawn over it: a mark in the waypoint's colour,
/// its name on a dark plate and its distance under it.
fn world_waypoints(host: &dyn Host, s: &State, gui: [f32; 2], gui_scale: f32, ops: &mut Vec<DrawOp>) {
    let settings = &s.config.current().waypoints;
    if !settings.in_world {
        return;
    }
    let (Some(camera), Some(dim), Some((_, book))) = (host.camera(), &s.dimension, &s.book) else { return };
    let Some(waypoints) = book.dimension(dim) else { return };
    let alpha = |opacity: u8, share: f32| (((opacity.min(100) as f32 / 100.0) * share * 255.0) as u32) << 24;
    let u = minimap::unit(gui[1], gui_scale);
    let mark = 9.0 * u;
    for w in waypoints.visible(settings.all_sets) {
        let distance = w.distance(camera.position);
        if !w.global && settings.max_distance > 0 && distance > settings.max_distance as f64 {
            continue;
        }
        let at = [w.pos[0] as f64 + 0.5, w.pos[1] as f64 + 1.0, w.pos[2] as f64 + 0.5];
        let Some([x, y]) = minimap::world_to_screen(camera.position, &camera.view, &camera.projection, at, gui)
        else {
            continue;
        };
        if x < -50.0 || y < -50.0 || x > gui[0] + 50.0 || y > gui[1] + 50.0 {
            continue;
        }
        let initial: String = w.initials.chars().take(2).collect();
        ops.extend(minimap::waypoint_mark([x, y], mark, alpha(settings.opacity_in_world, 0.6) | w.rgb(), &initial, u));
        let name_scale = 0.64 * u;
        let name_y = y + mark / 2.0 + 2.0 * u;
        let name_w = host.text_width(&w.name) * name_scale;
        ops.push(DrawOp::Fill {
            rect: [x - name_w / 2.0 - name_scale, name_y - name_scale, name_w + 2.0 * name_scale, 9.0 * name_scale],
            argb: 0x6000_0000,
        });
        ops.push(DrawOp::Text { x, y: name_y, text: w.name.clone(), argb: 0xFFFF_FFFF, align: Align::Center, scale: name_scale });
        let dist = minimap::distance_text(distance, settings.distance_in_km_from, decimal_comma());
        ops.push(DrawOp::Text { x, y: name_y + 10.0 * name_scale, text: dist, argb: 0xFFFF_FFFF, align: Align::Center, scale: u });
    }
}

/// The minimap: terrain, frame, waypoints, compass, the player's mark and
/// the info lines.
fn minimap_ops(
    host: &dyn Host,
    s: &mut State,
    player: &PlayerState,
    gui: [f32; 2],
    gui_scale: f32,
    ops: &mut Vec<DrawOp>,
) {
    let settings = &s.config.current().minimap;
    let [px, py, pz] = player.position;
    let feet = [px.floor() as i32, py.floor() as i32, pz.floor() as i32];
    let now = {
        use chrono::Timelike;
        let t = chrono::Local::now();
        (t.hour(), t.minute())
    };
    let input = meridian_core::info::InfoInput {
        pos: player.position,
        yaw: player.yaw,
        pitch: player.pitch,
        dimension: player.dimension.as_str().to_owned(),
        biome: if settings.show_biome { bridge::player_biome() } else { None },
        weather: if settings.info.weather { host.weather() } else { (0.0, 0.0) },
        light: if settings.info.light != meridian_core::config::LightInfo::Off { host.light(feet) } else { (0, 0) },
        day_time: host.clock().map(|c| c.day_time),
        real_time: Some(now),
        manual_cave: s.cave.manual(),
    };
    let lines: Vec<(String, u32)> =
        meridian_core::info::lines(settings, &input).into_iter().map(|l| (l, 0xFFFF_FFFF)).collect();
    // In cave mode the minimap zooms in by the extra steps asked for.
    let zoomed;
    let settings = if s.engine.layer().is_some() && settings.cave.extra_zoom > 0 {
        zoomed = MinimapSettings { zoom: settings.zoom.saturating_sub(settings.cave.extra_zoom), ..settings.clone() };
        &zoomed
    } else {
        settings
    };
    let layout = minimap::layout(settings, gui, gui_scale, s.controls.enlarged(), lines.len());
    s.minimap_place = Some(meridian_core::screens::MinimapPlace {
        rect: [layout.x, layout.y, layout.size, layout.size],
        margin: 5.0 * layout.unit + layout.frame_width(),
    });
    compose_map(s, &layout, player);
    let settings = &s.config.current().minimap;
    let axes = minimap::Axes::new(layout.rotate, player.yaw);
    if let Some((key, image)) = &s.map_image {
        let undiscovered = settings.undiscovered_opacity.min(100) as f32 / 100.0;
        gpu::set_frame(Some(gpu::MapFrame {
            rect: [layout.x, layout.y, layout.size, layout.size],
            gui,
            right: [axes.right[0] as f32, axes.right[1] as f32],
            forward: [axes.forward[0] as f32, axes.forward[1] as f32],
            center: [(px - key.origin[0] as f64) as f32, (pz - key.origin[1] as f64) as f32],
            blocks_per_gui_px: layout.blocks_per_pixel,
            round: layout.round,
            opacity: settings.opacity.min(100) as f32 / 100.0,
            backdrop: [0.06, 0.06, 0.06, undiscovered],
            image: image.clone(),
            at: Instant::now(),
            grid: settings.chunk_grid.then_some(layout.blocks_per_pixel / gui_scale.max(0.25)),
            origin: key.origin,
        }));
    }
    let radar = &s.config.current().radar;
    let radar_on = radar.enabled && !s.config.current().minimap.fair_play;
    bridge::want_entities(radar_on);
    if radar_on {
        let entities = radar_entities();
        ops.extend(radar::dots(&layout, player.position, player.yaw, &entities, radar));
    }
    let wp = &s.config.current().waypoints;
    if wp.on_minimap
        && let (Some(dim), Some((_, book))) = (&s.dimension, &s.book)
        && let Some(waypoints) = book.dimension(dim)
    {
        let alpha = ((wp.opacity_on_minimap.min(100) as u32 * 255) / 100) << 24;
        let size = 9.0 * layout.unit;
        for w in waypoints.visible(wp.all_sets) {
            let at = [w.pos[0] as f64 + 0.5, w.pos[2] as f64 + 0.5];
            let (p, _) = minimap::project(&layout, [px, pz], player.yaw, at, size / 2.0);
            let initial: String = w.initials.chars().take(1).collect();
            ops.extend(minimap::waypoint_mark(p, size, alpha | w.rgb(), &initial, layout.unit));
        }
    }
    ops.extend(minimap::frame(&layout));
    ops.extend(minimap::compass(&layout, player.yaw));
    ops.extend(minimap::player_mark(&layout, player.yaw, settings.arrow_scale));
    ops.extend(minimap::info(&layout, &lines));
}

/// Composes the north-up image the GPU turns: one texel per block, wide
/// enough for the map's diagonal, centred on the player's block. Only when
/// the player crossed a block or a region under it was redrawn.
fn compose_map(s: &mut State, layout: &minimap::Layout, player: &PlayerState) {
    let side_blocks = layout.size * layout.blocks_per_pixel;
    let size = (((side_blocks * std::f32::consts::SQRT_2) as u32 + 4).div_ceil(16) * 16).clamp(16, 2048);
    let (bx, bz) = (player.position[0].floor() as i32, player.position[2].floor() as i32);
    let origin = [bx - size as i32 / 2, bz - size as i32 / 2];
    let (rmin, rmax) = (
        RegionPos::of_block(origin[0], origin[1]),
        RegionPos::of_block(origin[0] + size as i32, origin[1] + size as i32),
    );
    let mut revisions = 0u64;
    for x in rmin.x..=rmax.x {
        for z in rmin.z..=rmax.z {
            if let Some(r) = s.engine.region(RegionPos::new(x, z)) {
                revisions = revisions.wrapping_mul(31).wrapping_add(r.revision + 1);
            }
        }
    }
    let minimap = &s.config.current().minimap;
    let overlay = minimap.light_overlay.then_some(minimap.light_overlay_range);
    let slime = if minimap.slime_chunks && s.dimension.as_deref() == Some("minecraft:overworld") {
        s.seed_svc.atlas().map(|a| a.seed)
    } else {
        None
    };
    let key = MapImageKey { origin, size, revisions, overlay, slime };
    if s.map_image.as_ref().is_some_and(|(k, _)| *k == key) {
        return;
    }
    let sampler = s.engine.sampler();
    let mut pixels = Vec::with_capacity((size * size) as usize);
    for z in 0..size as i32 {
        for x in 0..size as i32 {
            let (bx, bz) = (origin[0] + x, origin[1] + z);
            let mut p = sampler.pixel(bx, bz);
            if let Some([lo, hi]) = overlay
                && p >> 24 != 0
                && let Some(col) = sampler.column(bx, bz)
                && col.water_depth == 0
                && (lo..=hi).contains(&col.block_light())
            {
                // Spawnable ground: tinted light purple, as the reference's overlay.
                p = 0xFF00_0000 | meridian_core::color::blend(p & 0x00FF_FFFF, 0xFF55FF, 0.45);
            }
            if let Some(seed) = slime
                && meridian_seedatlas::structures::is_slime_chunk(seed as u64, bx >> 4, bz >> 4)
            {
                // Slime chunks: tinted green; unexplored ones half see-through.
                p = if p >> 24 == 0 {
                    0x8022_DD22
                } else {
                    0xFF00_0000 | meridian_core::color::blend(p & 0x00FF_FFFF, 0x22DD22, 0.4)
                };
            }
            pixels.push(p);
        }
    }
    let revision = s.map_image.as_ref().map_or(0, |(_, i)| i.revision + 1);
    s.map_image = Some((key, std::sync::Arc::new(gpu::MapImage { size, pixels, revision })));
}

/// "MeridianTest (auto)" / "overworld (auto)" for the screens' selectors.
fn context_labels(s: &State) -> (String, String) {
    let world = match s.engine.context().map(|c| &c.source) {
        Some(WorldSource::Singleplayer { world }) => world.clone(),
        Some(WorldSource::Multiplayer { address }) => address.clone(),
        None => "-".to_owned(),
    };
    let dim = s.dimension.as_deref().unwrap_or("-");
    let dim = dim.rsplit(':').next().unwrap_or(dim);
    (format!("{world} (auto)"), format!("{dim} (auto)"))
}

impl Meridian {
    /// Runs `f` on our open screen with its environment, then applies what
    /// it changed and what it asks for. Nothing when our screen isn't open.
    fn with_screen<R: Default>(
        &self,
        host: &dyn Host,
        f: impl FnOnce(&mut Screen, &mut Env<'_>) -> (R, Action),
    ) -> R {
        if host.mod_screen() != Some(MOD_ID) {
            return R::default();
        }
        let gui = host.client_state().map_or([0.0, 0.0], |c| c.gui_size);
        let player_block = HorizonHost { host, dimension: None }
            .player()
            .map_or([0, 0, 0], |p| p.position.map(|v| v.floor() as i32));
        self.with_state(host, |s| {
            let Some(mut screen) = s.screen.take() else { return R::default() };
            let (world_label, dim_label) = context_labels(s);
            let seed_status = s.seed_status();
            let Some(dim) = s.dimension.clone() else {
                s.screen = Some(screen);
                return R::default();
            };
            let Some((_, book)) = &mut s.book else {
                s.screen = Some(screen);
                return R::default();
            };
            let mut settings = s.config.current().clone();
            let sa_dim = sa::dimension_of(&dim).unwrap_or(meridian_seedatlas::Dimension::Overworld);
            let sa_env = meridian_core::screens_seedatlas::SeedAtlasEnv {
                dimension: sa_dim,
                has_seed: s.seed_svc.atlas_for(sa_dim).is_some(),
                completed_count: s.seed_key.as_ref().map(|k| s.completed.count(k, None)),
                nearest: s.nearest_opts,
                completed_by_kind: s
                    .seed_key
                    .as_ref()
                    .map(|k| sa::MARKERS.iter().map(|m| (m.id.to_owned(), s.completed.count(k, Some(m.id)))).collect())
                    .unwrap_or_default(),
                seed_status,
            };
            let profiles: Vec<String> = s.config.profiles.keys().cloned().collect();
            let profile = s.config.current_profile.clone();
            let (result, action, waypoints_changed, settings_changed) = {
                let waypoints = book.dimension_mut(&dim);
                let mut env = Env {
                    gui,
                    mouse: host.cursor(),
                    world_label: &world_label,
                    dimension_label: &dim_label,
                    player_block,
                    waypoints,
                    settings: &mut settings,
                    waypoints_changed: false,
                    settings_changed: false,
                    seed_atlas: sa_env,
                    profiles: profiles.clone(),
                    profile: profile.clone(),
                    minimap: s.minimap_place,
                };
                let (result, action) = f(&mut screen, &mut env);
                (result, action, env.waypoints_changed, env.settings_changed)
            };
            if waypoints_changed {
                book.dirty = true;
            }
            if settings_changed {
                *s.config.current_mut() = settings;
                s.engine.set_settings(s.config.current().clone());
                s.save_config();
            }
            if let Screen::Nearest(n) = &screen {
                s.nearest_opts = n.options;
            }
            s.screen = Some(screen);
            match action {
                Action::None => {}
                Action::Close => s.pending.push(Pending::CloseScreen),
                Action::Open(next) => s.screen = Some(next),
                Action::Save { edit, set, waypoint } => {
                    if let Some((book, _)) = s.book.as_mut().map(|(_, b)| (b, ())) {
                        book.save_edit(&dim, edit, &set, waypoint, waypoint::now());
                    }
                    if let Some(Screen::Editor(editor)) = &s.screen {
                        match editor.after_save() {
                            Action::Open(next) => s.screen = Some(next),
                            _ => s.pending.push(Pending::CloseScreen),
                        }
                    }
                }
                Action::Teleport(id) => {
                    let tp = s.config.current().waypoints.teleport_format.clone();
                    if let Some(w) = s.book.as_ref().and_then(|(_, b)| b.dimension(&dim)?.find(id).cloned()) {
                        s.pending.push(Pending::CloseScreen);
                        s.pending.push(Pending::Chat(waypoint::command(&tp, &w, &dim)));
                    }
                }
                Action::Share(id) => {
                    if let Some(w) = s.book.as_ref().and_then(|(_, b)| b.dimension(&dim)?.find(id).cloned()) {
                        s.pending.push(Pending::Chat(waypoint::share_text(&w, &dim)));
                    }
                }
                Action::FindNearest { kind, waypoint, skip_completed } => {
                    s.nearest_opts = meridian_core::nearest::NearestOptions { waypoint, skip_completed };
                    s.find_nearest(kind, waypoint, skip_completed, sa_dim, player_block);
                }
                Action::Profile(op) => {
                    use meridian_core::screens::ProfileOp;
                    let c = &mut s.config;
                    let changed = match op {
                        ProfileOp::Select(name) => c.select_profile(&name),
                        ProfileOp::Create(name) => c.create_profile(&name) && c.select_profile(&name),
                        ProfileOp::Rename(name) => c.rename_profile(&name),
                        ProfileOp::Delete(name) => c.delete_profile(&name),
                    };
                    if changed {
                        s.engine.set_settings(s.config.current().clone());
                        s.save_config();
                    }
                }
                Action::ResetCompleted(kind) => {
                    let name = kind.as_deref().and_then(|id| sa::MARKERS.iter().find(|m| m.id == id)).map_or("", |m| m.name);
                    let which = kind.as_deref().map(|id| meridian_core::completed::ResetKind { id, name });
                    if let Some(key) = s.seed_key.clone()
                        && let Some(prompt) = s.completed.reset_prompt(&key, &world_label, which)
                    {
                        s.completed.confirm_reset(&prompt);
                    }
                    s.screen = Some(Screen::SeedAtlasMarkers(Default::default()));
                }
            }
            result
        })
    }
}

/// Our screens' drawing and input (Horizon's screen hooks, over every
/// screen: each checks that ours is the one open).
impl Meridian {
    fn draw_screen(&self, host: &dyn Host, batch: &mut HudBatch) {
        let mouse = host.cursor();
        let behind = self.map_behind(host);
        let (mut ops, tip, gui): (Vec<DrawOp>, Option<String>, [f32; 2]) = self.with_screen(host, |screen, env| {
            let widgets = screen.widgets(env);
            // Over the map: its darkening keeps text readable over bright
            // terrain; elsewhere the game's in-world menu darkening.
            let shade = if behind { 0x8800_0000 } else { 0x5000_0000 };
            let mut ops = vec![DrawOp::Fill { rect: [0.0, 0.0, env.gui[0], env.gui[1]], argb: shade }];
            ops.extend(meridian_core::ui::draw(&widgets, mouse));
            ((ops, screen.tooltip(mouse, env), env.gui), Action::None)
        });
        if let Some(text) = tip {
            // Wrapped at the game's tooltip width (170 GUI pixels).
            let mut lines = Vec::new();
            let mut line = String::new();
            for word in text.split_whitespace() {
                let candidate = if line.is_empty() { word.to_owned() } else { format!("{line} {word}") };
                if !line.is_empty() && host.text_width(&candidate) > 170.0 {
                    lines.push(std::mem::take(&mut line));
                    line = word.to_owned();
                } else {
                    line = candidate;
                }
            }
            if !line.is_empty() {
                lines.push(line);
            }
            tooltip(&mut ops, mouse, gui, &lines);
        }
        paint(host, batch, &ops);
    }
}

impl Meridian {
    /// The death screen is up: one deathpoint where the player fell, in
    /// singleplayer and on servers alike.
    fn record_death(&self, host: &dyn Host) {
        let pos = HorizonHost { host, dimension: None }.player().map(|p| p.position.map(|v| v.floor() as i32));
        self.with_state(host, |s| {
            if s.death_recorded {
                return;
            }
            s.death_recorded = true;
            let settings = s.config.current().waypoints.clone();
            let (Some(pos), true) = (pos, settings.deathpoints) else { return };
            if let Some((book, dim)) = s.book_here() {
                book.record_death(&dim, pos, settings.keep_old_deathpoints, waypoint::now());
            }
            s.save_book();
        });
    }
}

/// The bridge's entities as the radar's.
fn radar_entities() -> Vec<radar::Entity> {
    bridge::entities()
        .into_iter()
        .map(|e| radar::Entity {
            id: e.id,
            class: match e.class {
                bridge::EntityClass::Player => radar::Class::Player,
                bridge::EntityClass::Hostile => radar::Class::Hostile,
                bridge::EntityClass::Friendly => radar::Class::Friendly,
                bridge::EntityClass::Item => radar::Class::Item,
                bridge::EntityClass::Other => radar::Class::Other,
            },
            pos: e.pos,
            invisible: e.invisible,
            kind: e.kind,
            name: e.name,
        })
        .collect()
}

/// A texture's size in texels, read once from its PNG header in the game's
/// resources (`entity/zombie/zombie`); none when it is missing.
fn texture_size(texture: &str) -> Option<[i32; 2]> {
    static SIZES: Mutex<Option<HashMap<String, Option<[i32; 2]>>>> = Mutex::new(None);
    let mut sizes = SIZES.lock();
    let sizes = sizes.get_or_insert_with(HashMap::new);
    if let Some(size) = sizes.get(texture) {
        return *size;
    }
    let root = bridge::resources_dir()?;
    let path = root.join("assets/minecraft/textures").join(format!("{texture}.png"));
    let size = std::fs::File::open(path).ok().and_then(|f| {
        let reader = png::Decoder::new(std::io::BufReader::new(f)).read_info().ok()?;
        let info = reader.info();
        Some([info.width as i32, info.height as i32])
    });
    sizes.insert(texture.to_owned(), size);
    size
}

/// The map screen's size: GUI pixels and GUI pixels per minimap unit.
#[derive(Clone, Copy, Debug)]
struct MapView {
    gui: [f32; 2],
    unit: f32,
}

/// What map input needs from the state besides the view: the current
/// dimension's waypoints (copied, so the view can change meanwhile).
struct MapData {
    waypoints: Vec<Waypoint>,
}

/// Regions requested around the map's centre at most (either way); the
/// rest of a far zoomed-out view shows from the GPU's cache.
const MAP_REQUEST_RADIUS: i32 = 7;
/// Regions held whole around the centre when zoomed out.
const MAP_REQUEST_RADIUS_FAR: i32 = 2;
/// Below this many GUI pixels per region the map draws overviews outside
/// the middle.
const MAP_OVERVIEW_BELOW: f32 = 96.0;
/// Regions drawn either way of the centre at most.
const MAP_DRAW_RADIUS: i32 = 48;

impl Meridian {
    fn map_open(&self, host: &dyn Host) -> bool {
        self.with_state(host, |s| s.map_open)
    }

    fn map_view(host: &dyn Host) -> MapView {
        let gui = host.client_state().map_or([0.0, 0.0], |c| c.gui_size);
        let gui_scale = bridge::window_size().map_or(2.0, |w| w[1] as f32 / gui[1].max(1.0));
        MapView { gui, unit: minimap::unit(gui[1], gui_scale) }
    }

    /// Runs map input with the state, then carries out what it asks for.
    fn map_input(&self, host: &dyn Host, f: impl FnOnce(&mut State, MapView, &MapData) -> MapAction) {
        let view = Self::map_view(host);
        self.with_state(host, |s| {
            let waypoints = match (&s.dimension, &s.book) {
                (Some(dim), Some((_, book))) => book
                    .dimension(dim)
                    .map(|d| d.visible(s.config.current().waypoints.all_sets).into_iter().cloned().collect())
                    .unwrap_or_default(),
                _ => Vec::new(),
            };
            let data = MapData { waypoints };
            let action = f(s, view, &data);
            s.map_action(action);
        });
    }

    /// A key on the map screen: our bindings first, then panning.
    fn map_key(&self, host: &dyn Host, key: &str) {
        let chosen = |id: &str, default: &str| host.mod_key(MOD_ID, id, default);
        let hits: Vec<&'static str> = bindings_for_key(key, chosen).map(|b| b.id).collect();
        let at = host.cursor();
        let player = HorizonHost { host, dimension: None }.player();
        self.with_state(host, |s| {
            let view = Self::map_view(host);
            let center = [view.gui[0] / 2.0, view.gui[1] / 2.0];
            for id in &hits {
                match *id {
                    "worldmap.open" => s.pending.push(Pending::CloseScreen),
                    "worldmap.toggle_dimension" => s.next_map_dimension(host),
                    "worldmap.settings" => s.open_from_map(Screen::Settings(SettingsScreen::world_map())),
                    "worldmap.zoom_in" | "minimap.zoom_in" => s.map.scroll(1.0, center, view.gui, view.unit),
                    "worldmap.zoom_out" | "minimap.zoom_out" => s.map.scroll(-1.0, center, view.gui, view.unit),
                    "waypoint.new" => {
                        let w = s.map.to_world(at, view.gui, view.unit);
                        let (x, z) = (w[0].floor() as i32, w[1].floor() as i32);
                        let y = s.engine.sampler().column(x, z).map_or(64, |c| c.height as i32 + 1);
                        s.map_action(MapAction::Menu(MenuItem::CreateWaypoint([x, y, z])));
                    }
                    _ => {}
                }
            }
            if hits.is_empty() {
                let step = 64.0 / s.map.scale(view.unit) as f32;
                let delta = match key {
                    "key.keyboard.left" | "key.keyboard.a" => Some([step, 0.0]),
                    "key.keyboard.right" | "key.keyboard.d" => Some([-step, 0.0]),
                    "key.keyboard.up" | "key.keyboard.w" => Some([0.0, step]),
                    "key.keyboard.down" | "key.keyboard.s" => Some([0.0, -step]),
                    _ => None,
                };
                if let Some([dx, dz]) = delta {
                    s.map.center[0] -= dx as f64;
                    s.map.center[1] -= dz as f64;
                }
                if key == "key.keyboard.space"
                    && let Some(p) = &player
                {
                    s.map.recenter(p.position);
                }
            }
        });
    }

    /// The map screen: terrain on the GPU, everything else over it.
    /// A menu over the map: the map shows behind it.
    fn map_behind(&self, host: &dyn Host) -> bool {
        host.mod_screen() == Some(MOD_ID) && self.with_state(host, |s| s.return_to_map && s.screen.is_some())
    }

    fn draw_map(&self, host: &dyn Host, batch: &mut HudBatch, behind: bool) {
        let view = Self::map_view(host);
        if view.gui[0] <= 0.0 {
            return;
        }
        // Behind a menu the cursor stays off the map, so nothing reacts.
        let mouse = if behind { [-1.0, -1.0] } else { host.cursor() };
        let player = HorizonHost { host, dimension: None }.player();
        let ops = self.with_state(host, |s| {
            let Some(player) = player else { return Vec::new() };
            let (gui, u) = (view.gui, view.unit);
            let now = Instant::now();
            let dt = now.duration_since(s.map_frame).as_secs_f32().min(0.05);
            s.map_frame = now;
            let wm = &s.config.current().world_map;
            s.map.set_unlimited(wm.unlimited_zoom_in, wm.unlimited_zoom_out);
            s.map.tick(dt);
            let here = s.dimension.clone().unwrap_or_default();
            let (shown, engine) = match &mut s.viewer {
                Some((dim, engine)) => (dim.clone(), engine),
                None => (here.clone(), &mut s.engine),
            };
            let factor = dimension_scale(&here) / dimension_scale(&shown);
            let player_pos = [player.position[0] * factor, player.position[1], player.position[2] * factor];
            let (min, max) = s.map.visible_regions(gui, u);
            let mid = RegionPos::of_block(s.map.center[0].floor() as i32, s.map.center[1].floor() as i32);
            let clamp = |p: RegionPos, r: i32| {
                RegionPos::new(p.x.clamp(mid.x - r, mid.x + r), p.z.clamp(mid.z - r, mid.z + r))
            };
            // Up close every visible region is held whole; zoomed out only
            // the middle is, the rest comes from overviews (a quarter of the
            // resolution, which is more than the screen shows there).
            let region_px = 512.0 * s.map.scale(u) as f32;
            let whole_radius = if region_px < MAP_OVERVIEW_BELOW { MAP_REQUEST_RADIUS_FAR } else { MAP_REQUEST_RADIUS };
            let near = (clamp(min, whole_radius), clamp(max, whole_radius));
            engine.request_area(near.0, near.1);
            let (dmin, dmax) = (clamp(min, MAP_DRAW_RADIUS), clamp(max, MAP_DRAW_RADIUS));
            engine.request_overviews(dmin, dmax, near);
            let mut regions = Vec::new();
            for x in dmin.x..=dmax.x {
                for z in dmin.z..=dmax.z {
                    let pos = RegionPos::new(x, z);
                    let (bx, bz) = pos.min_block();
                    let a = s.map.to_screen([bx as f64, bz as f64], gui, u);
                    let b = s.map.to_screen([(bx + 512) as f64, (bz + 512) as f64], gui, u);
                    let quad = |pixels, revision, size| gpu_map::RegionQuad {
                        pos: [x, z],
                        rect: [a[0], a[1], b[0], b[1]],
                        pixels,
                        revision,
                        size,
                        alpha: 255,
                        layer: 0,
                    };
                    match (engine.region(pos), engine.overview(pos)) {
                        (Some(r), _) => regions.push(quad(r.shared_pixels(), r.revision, 512)),
                        (None, Some(o)) => regions.push(quad(Some(o.pixels.clone()), o.revision, meridian_core::engine::OVERVIEW_SIDE as u32)),
                        // Never explored, or not loaded yet: the GPU may still
                        // hold it up close, nothing to draw far out.
                        (None, None) if (x - mid.x).abs() <= whole_radius && (z - mid.z).abs() <= whole_radius => {
                            regions.push(quad(None, 0, 512))
                        }
                        (None, None) => {}
                    }
                }
            }
            let world = {
                use std::hash::{Hash, Hasher};
                let mut h = std::collections::hash_map::DefaultHasher::new();
                format!("{:?}", engine.context()).hash(&mut h);
                engine.layer().hash(&mut h);
                h.finish()
            };
            // Seed Atlas: the biome layer under the terrain and the markers.
            let sa_settings = s.config.current().seed_atlas.clone();
            let sa_dim = sa::dimension_of(&shown);
            let gui_scale = bridge::window_size().map_or(2.0, |w| w[1] as f32 / gui[1].max(1.0));
            let mut background = Vec::new();
            let mut icons = Vec::new();
            let mut marker_hover: Option<MarkerHover> = None;
            if let (Some(dim), true) = (sa_dim, sa_settings.enabled && s.seed_svc.active()) {
                let region_side = (512.0 * s.map.scale(u) as f32) * gui_scale;
                let step = sa::step_for(region_side, sa_settings.biome_resolution);
                let y = sa::sample_y(&sa_settings, dim);
                let mut order: Vec<RegionPos> = (dmin.x..=dmax.x).flat_map(|x| (dmin.z..=dmax.z).map(move |z| RegionPos::new(x, z))).collect();
                order.sort_by_key(|r| (r.x - mid.x).abs().max((r.z - mid.z).abs()));
                if sa_settings.biome_layer {
                    // Tiles of 128 samples at the chosen step, nearest first;
                    // a coarser tile already computed shows under one that is
                    // not ready yet.
                    let tile_quad = |s: &WorldMap, t: RegionPos, step: i32, tile: Option<sa::Tile>| {
                        let span = sa::tile_span(step);
                        let (bx, bz) = (t.x as f64 * span as f64, t.z as f64 * span as f64);
                        let a = s.to_screen([bx, bz], gui, u);
                        let b = s.to_screen([bx + span as f64, bz + span as f64], gui, u);
                        gpu_map::RegionQuad {
                            pos: [t.x, t.z],
                            rect: [a[0], a[1], b[0], b[1]],
                            size: tile.as_ref().map_or(sa::TILE_SAMPLES as u32, |t| t.size),
                            revision: tile.as_ref().map_or(0, |t| t.revision),
                            pixels: tile.map(|t| t.pixels),
                            alpha: sa_settings.biome_opacity,
                            layer: step as u32,
                        }
                    };
                    let span = sa::tile_span(step);
                    let w0 = s.map.to_world([0.0, 0.0], gui, u);
                    let w1 = s.map.to_world(gui, gui, u);
                    let t0 = RegionPos::new((w0[0] / span as f64).floor() as i32, (w0[1] / span as f64).floor() as i32);
                    let t1 = RegionPos::new((w1[0] / span as f64).floor() as i32, (w1[1] / span as f64).floor() as i32);
                    let centre = s.map.center;
                    let mut tiles: Vec<RegionPos> = (t0.x..=t1.x).flat_map(|x| (t0.z..=t1.z).map(move |z| RegionPos::new(x, z))).collect();
                    tiles.sort_by(|a, b| {
                        let d = |t: &RegionPos| {
                            let (cx, cz) = ((t.x as f64 + 0.5) * span as f64 - centre[0], (t.z as f64 + 0.5) * span as f64 - centre[1]);
                            cx * cx + cz * cz
                        };
                        d(a).total_cmp(&d(b))
                    });
                    tiles.truncate(600);
                    let mut fine = Vec::with_capacity(tiles.len());
                    let mut coarse: Vec<gpu_map::RegionQuad> = Vec::new();
                    for t in &tiles {
                        let job = sa::Job::Tile { dimension: dim, region: *t, step, y, refine: false };
                        let tile = s.seed_svc.tile(job);
                        if tile.is_none() {
                            // Meanwhile what is already computed: the four
                            // tiles one step finer (zooming out), else a
                            // coarser one (zooming in).
                            let finer: Vec<_> = if step > 1 {
                                let fstep = step / 2;
                                (0..4)
                                    .map(|i| RegionPos::new(t.x * 2 + i % 2, t.z * 2 + i / 2))
                                    .map(|c| (c, s.seed_svc.cached_tile(sa::Job::Tile { dimension: dim, region: c, step: fstep, y, refine: false })))
                                    .collect()
                            } else {
                                Vec::new()
                            };
                            if !finer.is_empty() && finer.iter().all(|(_, t)| t.is_some()) {
                                for (c, ft) in finer {
                                    coarse.push(tile_quad(&s.map, c, step / 2, ft));
                                }
                            } else {
                                let mut cstep = step * 2;
                                while cstep <= (step * 8).min(1024) {
                                    let f = sa::tile_span(cstep) / span;
                                    let c = RegionPos::new(t.x.div_euclid(f), t.z.div_euclid(f));
                                    if coarse.iter().any(|q| q.pos == [c.x, c.z] && q.layer == cstep as u32) {
                                        break;
                                    }
                                    if let Some(ct) = s.seed_svc.cached_tile(sa::Job::Tile { dimension: dim, region: c, step: cstep, y, refine: false }) {
                                        coarse.push(tile_quad(&s.map, c, cstep, Some(ct)));
                                        break;
                                    }
                                    cstep *= 2;
                                }
                            }
                        }
                        fine.push(tile_quad(&s.map, *t, step, tile));
                    }
                    background.extend(coarse);
                    background.extend(fine);
                }
                s.hovered_marker = None;
                if sa_settings.structures {
                    load_icons();
                    // `MarkerSize.iconScale`: two blocks wide with the map,
                    // never below the sprite size, at most a quarter of the view.
                    let setting = sa_settings.marker_size_factor().clamp(0.25, 4.0);
                    let floor = 20.0 * u * 0.45 * setting;
                    let with_map = 2.0 * setting * s.map.scale(u) as f32;
                    let ceiling = floor.max(gui[0].min(gui[1]) * 0.25);
                    let units = floor.max(with_map).clamp(floor, ceiling);
                    let px = units / icons::DISPLAY_SIZE;
                    let completed_alpha = sa_settings.completed_opacity_pct() as f32 / 100.0;
                    // The whole view at once, as the Fabric version scans it:
                    // dense kinds only when zoomed in far enough.
                    // `request`: the window's pixels over the map's scale (its
                    // zoom, as the reference map shows it), at least 1/512.
                    let zoom = s.map.zoom;
                    let window = bridge::window_size().map_or([gui[0] as f64 * 2.0, gui[1] as f64 * 2.0], |w| [w[0] as f64, w[1] as f64]);
                    let safe = zoom.max(1.0 / 512.0);
                    let (bbox, radius) = sa::view_box(s.map.center, [window[0] / safe, window[1] / safe]);
                    let shows = |k: i32| sa::marker_for(k).is_some_and(|m| sa::visible_at_zoom(m.id, zoom, radius));
                    let hits = s.seed_svc.view_markers(dim, bbox, shows, Instant::now());
                    {
                        for h in hits.iter().filter(|h| shows(h.kind)) {
                            let Some(m) = sa::marker_for(h.kind) else { continue };
                            let special = h.kind == meridian_seedatlas::structures::kind::ABANDONED_CAMP
                                && h.detail >= 0
                                && h.detail & meridian_seedatlas::atlas::CAMP_SPECIAL_LOOT != 0;
                            let icon_id = if special {
                                "camp_special"
                            } else if h.kind == meridian_seedatlas::atlas::marker::ORE_VEIN_COPPER {
                                "orevein_copper"
                            } else {
                                m.id
                            };
                            let Some((icon_index, info)) = icon(icon_id) else { continue };
                            let (dw, dh) = info.display();
                            let (w, hgt) = (dw * px, dh * px);
                            let c = s.map.to_screen([h.x as f64 + 0.5, h.z as f64 + 0.5], gui, u);
                            let rect = [c[0] - (dw / 2.0).floor() * px, c[1] - (dh / 2.0).floor() * px, w, hgt];
                            if rect[0] > gui[0] || rect[1] > gui[1] || rect[0] + w < 0.0 || rect[1] + hgt < 0.0 {
                                continue;
                            }
                            let key = structure_key(s.seed_key.as_deref(), &s.seed_svc, m.id, dim, h.x, h.z);
                            let completed = key.as_ref().is_some_and(|k| s.completed.is_completed(k));
                            if completed && completed_alpha <= 0.0 {
                                continue;
                            }
                            let alpha = if completed { completed_alpha } else { 1.0 };
                            if info.block
                                && let Some((frame, _)) = icon("frame")
                            {
                                icons.push(gpu_map::IconQuad { icon: frame, rect: [rect[0] - px, rect[1] - px, w + 2.0 * px, hgt + 2.0 * px], alpha });
                            }
                            icons.push(gpu_map::IconQuad { icon: icon_index, rect, alpha });
                            if completed && let Some((badge, _)) = icon("completed") {
                                // `CompletedBadge`: 10 icon pixels in the lower right, 3 overhanging.
                                let b = meridian_core::completed::BADGE_DISPLAY_SIZE as f32 * px;
                                icons.push(gpu_map::IconQuad { icon: badge, rect: [rect[0] + w - b + 3.0 * px, rect[1] + hgt - b + 3.0 * px, b, b], alpha });
                            }
                            if inside(rect, mouse) {
                                s.hovered_marker = Some(HoveredMarker { name: m.name, x: h.x, z: h.z, key: key.clone() });
                                let mut first = m.name.to_owned();
                                if h.flags & meridian_seedatlas::atlas::RESULT_APPROXIMATE != 0 {
                                    first.push_str(" (approx.)");
                                }
                                let mut lines = vec![first];
                                if completed {
                                    lines.push("Completed".into());
                                }
                                if special {
                                    lines.push("Special loot".into());
                                }
                                let y = h.y.map_or(String::new(), |y| format!(" · Y={y}"));
                                lines.push(format!("X={}{y} · Z={}", h.x, h.z));
                                marker_hover = Some(MarkerHover { lines });
                            }
                        }
                    }
                }
            }
            gpu_map::set_frame(Some(gpu_map::WorldMapFrame { gui, world, regions, background, icons, at: Instant::now() }));
            let hovered = s.map.to_world(mouse, gui, u);
            let (hx, hz) = (hovered[0].floor() as i32, hovered[1].floor() as i32);
            let sampler = engine.sampler();
            let hovered_height = sampler.column(hx, hz).map(|c| c.height);
            let hovered_biome = sampler.biome(hx, hz);
            let cave_layer = engine.layer();
            let settings = s.config.current();
            let radar_on = settings.radar.enabled && !settings.minimap.fair_play;
            bridge::want_entities(radar_on);
            let entities: Vec<radar::Entity> = if radar_on {
                radar_entities().into_iter().filter(|e| radar::shown(e, &settings.radar)).collect()
            } else {
                Vec::new()
            };
            let waypoints: Vec<&Waypoint> = match &s.book {
                Some((_, book)) if settings.waypoints.on_minimap => {
                    book.dimension(&shown).map(|d| d.visible(settings.waypoints.all_sets)).unwrap_or_default()
                }
                _ => Vec::new(),
            };
            let entities = if shown == here { entities } else { Vec::new() };
            let wm = &settings.world_map;
            let mut ops = s.map.overlay(&Overlay {
                gui,
                unit: u,
                mouse,
                player: player_pos,
                yaw: player.yaw,
                hovered_height,
                hovered_biome,
                waypoints: &waypoints,
                entities: &entities,
                show_coordinates: wm.show_coordinates,
                show_zoom: wm.show_zoom,
                show_biome: wm.show_hovered_biome,
                show_footsteps: wm.footsteps,
                player_as_arrow: wm.player_as_arrow,
                cave_layer,
                other_dimension: (shown != here).then(|| shown.rsplit(':').next().unwrap_or(&shown).to_owned()),
            });
            // Seed Atlas: the switch and settings buttons, and tooltips.
            if sa_settings.enabled {
                let (layer_button, settings_button, map_settings_button) = seed_atlas_buttons(gui);
                for (rect, label, color, tip) in [
                    (
                        layer_button,
                        "SA",
                        if sa_settings.biome_layer { 0xFF55_FF55 } else { 0xFFAA_AAAA },
                        format!("Seed map layer: {}", if sa_settings.biome_layer { "On" } else { "Off" }),
                    ),
                    (map_settings_button, "M", 0xFFFF_FFFF, "Map Settings".to_owned()),
                    (settings_button, "SA", 0xFFFF_FFFF, "Seed Atlas Settings".to_owned()),
                ] {
                    let hover = inside(rect, mouse);
                    let sprite = if hover { "gui/sprites/widget/button_highlighted" } else { "gui/sprites/widget/button" };
                    ops.push(DrawOp::NineSlice { sprite, rect, argb: 0xFFFF_FFFF });
                    ops.push(DrawOp::Text { x: rect[0] + rect[2] / 2.0, y: rect[1] + 6.0, text: label.into(), argb: color, align: Align::Center, scale: 1.0 });
                    if hover {
                        tooltip(&mut ops, mouse, gui, &[tip]);
                    }
                }
                if let Some(h) = &marker_hover {
                    tooltip(&mut ops, mouse, gui, &h.lines);
                } else if sa_settings.biome_layer
                    && let Some(dim) = sa_dim
                    && !inside(layer_button, mouse)
                    && !inside(settings_button, mouse)
                    && !inside(map_settings_button, mouse)
                    && meridian_core::worldmap::button_at(mouse, gui, u).is_none()
                    && s.map.menu.is_none()
                {
                    let y = sa::sample_y(&sa_settings, dim);
                    if let Some(id) = s.seed_svc.biome_at(dim, hx, y, hz)
                        && let Some(name) = meridian_seedatlas::biomes::name(id)
                    {
                        let line = format!("{} · Y={y} · X={hx} · Z={hz}", meridian_core::info::title_case(name));
                        tooltip(&mut ops, mouse, gui, &[line]);
                    }
                }
            }
            ops
        });
        paint(host, batch, &ops);
    }
}

impl State {
    /// Carries out what the map asked for.
    fn map_action(&mut self, action: MapAction) {
        let MapAction::Menu(item) = action else { return };
        if item == MenuItem::ExportPng {
            self.export_png();
            return;
        }
        if let MenuItem::Extra(id, _) = item {
            if id == MENU_TELEPORT_PLAYER {
                if let Some(name) = self.menu_player.take() {
                    let format = &self.config.current().world_map.player_teleport_format;
                    self.pending.push(Pending::CloseScreen);
                    self.pending.push(Pending::Chat(format.replace("{name}", &name)));
                }
                return;
            }
            self.marker_action(id);
            return;
        }
        let other = self.viewer.as_ref().map(|(d, _)| d.clone());
        let wm = &self.config.current().world_map;
        let tp = if other.is_some() { wm.map_teleport_dimension_format.clone() } else { wm.map_teleport_format.clone() };
        let wp_tp = self.config.current().waypoints.teleport_format.clone();
        let Some(dim) = other.or_else(|| self.dimension.clone()) else { return };
        let Some((_, book)) = self.book.as_mut() else { return };
        let find = |book: &WaypointBook, id| book.dimension(&dim).and_then(|d| d.find(id).cloned());
        match item {
            MenuItem::Header(_) => {}
            MenuItem::CreateWaypoint(pos) => {
                let mut editor = Editor::new(book.dimension_mut(&dim), waypoint::now(), false);
                editor.x = pos[0].to_string();
                editor.y = pos[1].to_string();
                editor.z = pos[2].to_string();
                self.open_from_map(Screen::Editor(editor));
            }
            MenuItem::TeleportHere(pos) => {
                let w = Waypoint::new("Here", pos, 0);
                self.pending.push(Pending::CloseScreen);
                self.pending.push(Pending::Chat(waypoint::command(&tp, &w, &dim)));
            }
            MenuItem::ShareLocation(pos) => {
                let w = Waypoint::new("Location", pos, 0);
                self.pending.push(Pending::Chat(waypoint::share_text(&w, &dim)));
            }
            MenuItem::EditWaypoint(id) => {
                if let Some(w) = find(book, id) {
                    let set = book.dimension(&dim).map_or_else(String::new, |d| d.current_set.clone());
                    self.open_from_map(Screen::Editor(Editor::edit(&w, &set, false)));
                }
            }
            MenuItem::TeleportToWaypoint(id) => {
                if let Some(w) = find(book, id) {
                    let template = if self.viewer.is_some() { &tp } else { &wp_tp };
                    self.pending.push(Pending::CloseScreen);
                    self.pending.push(Pending::Chat(waypoint::command(template, &w, &dim)));
                }
            }
            MenuItem::DeleteWaypoint(id) => {
                book.dimension_mut(&dim).remove(id);
                book.dirty = true;
            }
            MenuItem::ExportPng | MenuItem::Extra(..) => {}
        }
    }

    /// The map shows the next dimension: the player's own through a
    /// read-only map of the others' saved files.
    fn next_map_dimension(&mut self, host: &dyn Host) {
        let Some(here) = self.dimension.clone() else { return };
        let Some(context) = self.engine.context().cloned() else { return };
        let shown = self.viewer.as_ref().map_or(here.clone(), |(d, _)| d.clone());
        let mut order: Vec<String> = MAP_DIMENSIONS.iter().map(|d| d.to_string()).collect();
        if !order.contains(&here) {
            order.push(here.clone());
        }
        let i = order.iter().position(|d| *d == shown).unwrap_or(0);
        let next = order[(i + 1) % order.len()].clone();
        let factor = dimension_scale(&shown) / dimension_scale(&next);
        self.map.center = [self.map.center[0] * factor, self.map.center[1] * factor];
        if next == here {
            self.viewer = None;
            return;
        }
        let mut engine = MapEngine::new(self.config.current().clone(), data_dir(host));
        engine.prepare_map(self.engine.map());
        let viewer_host = ViewerHost(HorizonHost { host, dimension: None });
        let context = WorldContext { source: context.source, dimension: DimensionId::new(&next) };
        engine.handle(&viewer_host, HostEvent::LevelLoaded(context));
        // A dimension under a roof shows its most recently mapped cave layer.
        if let Some(surface) = engine.regions_dir()
            && let Some(layer) = newest_cave_layer(surface.parent().unwrap_or(&surface))
        {
            engine.set_layer(Some(layer));
        }
        self.viewer = Some((next, engine));
    }

    /// Writes the shown map to a PNG in the background.
    fn export_png(&mut self) {
        self.engine.flush();
        let engine = self.viewer.as_ref().map_or(&self.engine, |(_, e)| e);
        let Some(regions) = engine.regions_dir() else { return };
        let shading = meridian_core::color::Shading::new(&self.config.current().world_map.terrain, None)
            .for_cave(engine.layer().is_some(), self.config.current().minimap.cave.legible_caves);
        let name = {
            let (world, dim) = context_labels(self);
            let dim = self.viewer.as_ref().map_or(dim, |(d, _)| d.clone());
            let clean = |t: &str| t.replace(" (auto)", "").replace([':', '/', '\\', ' '], "_");
            format!("{}_{}_{}.png", clean(&world), clean(&dim), chrono::Local::now().format("%Y-%m-%d_%H.%M.%S"))
        };
        let out = self.export_dir.join(name);
        self.notice = Some("Exporting the map...".to_owned());
        *EXPORT_NOTICE.lock() = Some("Exporting the map...".to_owned());
        std::thread::spawn(move || {
            // Let the region saves just started land first.
            std::thread::sleep(Duration::from_millis(500));
            let text = match meridian_core::export::export_png(&regions, &out, &shading, 16384) {
                Ok(r) => format!("Map exported: {} ({}x{})", r.path.display(), r.width, r.height),
                Err(e) => format!("Map export failed: {e}"),
            };
            log::info!("meridian: {text}");
            *EXPORT_NOTICE.lock() = Some(text);
        });
    }

    /// Shows `screen` in place of the map; closing it comes back.
    fn open_from_map(&mut self, screen: Screen) {
        self.screen = Some(screen);
        self.map_open = false;
        self.return_to_map = true;
    }
}

/// The cave layer (`caves/<y>/`) of a dimension's map folder written last,
/// when its surface is older or missing (a dimension under a roof).
fn newest_cave_layer(dimension_dir: &std::path::Path) -> Option<i32> {
    let modified = |p: &std::path::Path| std::fs::metadata(p).and_then(|m| m.modified()).ok();
    let surface = modified(&dimension_dir.join("regions"));
    let entries = std::fs::read_dir(dimension_dir.join("caves")).ok()?;
    let (time, layer) = entries
        .flatten()
        .filter_map(|e| {
            let y = e.file_name().to_str()?.parse::<i32>().ok()?;
            Some((modified(&e.path().join("regions"))?, y))
        })
        .max()?;
    surface.is_none_or(|s| time > s).then_some(layer)
}

// ------------------------------------------------------------ Seed Atlas --

use meridian_core::seedatlas as sa;

/// The marker icons by id, and their order in the renderer's icon list.
static ICON_INDEX: std::sync::OnceLock<(HashMap<&'static str, usize>, Vec<icons::Icon>)> = std::sync::OnceLock::new();

/// Builds the marker icons (Minecraft's textures need the game's resources,
/// so this waits until they are known) and hands them to the renderer.
fn load_icons() {
    if ICON_INDEX.get().is_some() {
        return;
    }
    let Some(root) = bridge::resources_dir() else { return };
    let set = icons::load(Some(&root));
    let mut index = HashMap::new();
    let mut list = Vec::new();
    let mut gpu = Vec::new();
    for (id, icon) in set {
        index.insert(id, list.len());
        gpu.push((icon.width, icon.height, icon.rgba.clone()));
        list.push(icon);
    }
    let _ = gpu_map::ICONS.set(gpu);
    let _ = ICON_INDEX.set((index, list));
}

fn icon(id: &str) -> Option<(usize, &'static icons::Icon)> {
    let (index, list) = ICON_INDEX.get()?;
    let i = *index.get(id)?;
    Some((i, &list[i]))
}

/// The marker under the cursor on the map, if any.
#[derive(Clone, Debug)]
struct MarkerHover {
    lines: Vec<String>,
}

impl State {
}

impl Meridian {
    /// `/seedatlas ...` (the Fabric version's commands); answers the reply
    /// and whether to open the settings.
    fn on_seedatlas(&self, host: &dyn Host, args: &str) -> (String, bool) {
        self.with_state(host, |s| {
            let map = s.engine.map().to_owned();
            let context = s.engine.context().map(|c| meridian_core::seedatlas_state::SeedContext::of_map(&c.source, &map));
            let detected = s.detected_seed();
            let mut settings = s.config.current().seed_atlas.clone();
            let outcome = {
                let mut ctx = meridian_core::seedatlas_commands::CommandCtx { book: &mut s.seeds, settings: &mut settings, context, detected };
                meridian_core::seedatlas_commands::seedatlas_command(args, &mut ctx)
            };
            if outcome.settings_changed {
                s.config.current_mut().seed_atlas = settings;
                s.engine.set_settings(s.config.current().clone());
                s.save_config();
            }
            if outcome.seeds_changed || outcome.settings_changed {
                s.seed_key = None;
                s.update_seeds();
            }
            (outcome.lines.iter().map(|l| l.text.as_str()).collect::<Vec<_>>().join(" · "), outcome.open_settings)
        })
    }
}

/// A Seed Atlas marker the cursor is over.
#[derive(Clone, Debug)]
struct HoveredMarker {
    name: &'static str,
    x: i32,
    z: i32,
    key: Option<meridian_core::completed::StructureKey>,
}

const MENU_COMPLETE: u32 = 1;
const MENU_STRUCTURE_WAYPOINT: u32 = 2;
const MENU_TEMPORARY_WAYPOINT: u32 = 3;
const MENU_TELEPORT_PLAYER: u32 = 4;

/// The game's biome tints for Seed Atlas's terrain look, by biome name.
struct GameLooks {
    table: std::collections::HashMap<String, bridge::BiomeLook>,
    fallback: meridian_core::seedatlas_look::VanillaLooks,
}

impl meridian_core::seedatlas_look::BiomeLooks for GameLooks {
    fn known(&self, name: &str) -> bool {
        self.table.contains_key(name) || self.fallback.known(name)
    }
    fn water_color(&self, name: &str) -> u32 {
        self.table.get(name).map_or_else(|| self.fallback.water_color(name), |l| l.water & 0xFF_FFFF)
    }
    fn grass_color(&self, name: &str, x: i32, z: i32) -> u32 {
        self.table.get(name).map_or_else(|| self.fallback.grass_color(name, x, z), |l| l.grass & 0xFF_FFFF)
    }
    fn foliage_color(&self, name: &str) -> u32 {
        self.table.get(name).map_or_else(|| self.fallback.foliage_color(name), |l| l.foliage & 0xFF_FFFF)
    }
    fn cold_enough_to_snow(&self, name: &str, x: i32, y: i32, z: i32, sea_level: i32) -> bool {
        self.fallback.cold_enough_to_snow(name, x, y, z, sea_level)
    }
}

impl State {
    /// A Seed Atlas entry of the map menu.
    fn marker_action(&mut self, id: u32) {
        let Some(m) = self.menu_marker.take() else { return };
        match id {
            MENU_COMPLETE => {
                if let Some(k) = &m.key
                    && !self.completed.unmark(k)
                {
                    self.completed.mark(k);
                }
            }
            MENU_STRUCTURE_WAYPOINT | MENU_TEMPORARY_WAYPOINT => {
                let Some((book, dim)) = self.book_here() else { return };
                let mut w = Waypoint::new(m.name, [m.x, 64, m.z], 11);
                if id == MENU_TEMPORARY_WAYPOINT {
                    w.kind = Kind::Temporary;
                }
                book.add(&dim, w, false, waypoint::now());
            }
            _ => {}
        }
    }

    /// "Active seed: …" as the Fabric version's settings show it
    /// (`seedStatus`): the base seed, then the Nether's and End's own.
    fn seed_status(&self) -> String {
        let Some(key) = self.seed_key.as_deref() else {
            return "No seed configured for this world/server".into();
        };
        let own: Vec<(&str, i64)> = [(meridian_seedatlas::Dimension::Nether, "Nether"), (meridian_seedatlas::Dimension::End, "End")]
            .into_iter()
            .filter_map(|(d, name)| self.seeds.dimension_seed(key, d).map(|v| (name, v.seed)))
            .collect();
        let base = self.seed_svc.spec_for(meridian_seedatlas::Dimension::Overworld).map(|s| s.seed);
        if base.is_none() && own.is_empty() {
            return "No seed configured for this world/server".into();
        }
        let mut status = base.map_or_else(|| "No base seed".to_owned(), |b| format!("Active seed: {b}"));
        for (name, seed) in own {
            status.push_str(&format!(" · {name}: {seed}"));
        }
        status
    }

    /// The singleplayer world's own seed, when this is singleplayer.
    fn detected_seed(&self) -> Option<(i64, bool)> {
        match self.engine.context().map(|c| &c.source) {
            Some(WorldSource::Singleplayer { .. }) => *SERVER_SEED.lock(),
            _ => None,
        }
    }

    /// Resolves the seed of every dimension for the open world and hands
    /// them to the Seed Atlas service.
    fn update_seeds(&mut self) {
        let sa = self.config.current().seed_atlas.clone();
        let map = self.engine.map().to_owned();
        let context = self.engine.context().map(|c| meridian_core::seedatlas_state::SeedContext::of_map(&c.source, &map));
        if let Some(c) = &context
            && self.seed_key.as_deref() != Some(c.key.as_str())
        {
            self.seeds.context_changed(&c.key);
            self.config.current_mut().seed_atlas.context_key = c.key.clone();
            self.seed_key = Some(c.key.clone());
        }
        if !self.looks_ready {
            let mut table = std::collections::HashMap::new();
            for id in 0..256 {
                if let Some(name) = meridian_seedatlas::biomes::name(id)
                    && let Some(look) = bridge::biome_look(&format!("minecraft:{name}"))
                {
                    table.insert(name.to_owned(), look);
                }
            }
            if !table.is_empty() {
                self.seed_svc.set_looks(std::sync::Arc::new(GameLooks { table, fallback: meridian_core::seedatlas_look::VanillaLooks }));
                self.seed_svc.set_terrain_sources(
                    std::sync::Arc::new(|seed, large, ox, oz, step, w, h| {
                        terrain_sampler::sample_overworld(seed, large, ox, oz, step, w, h, &|| true).map(|g| {
                            meridian_core::seedatlas_look::SurfaceGrid { heights: g.heights, floors: g.floors, colors: g.colors }
                        })
                    }),
                    std::sync::Arc::new(terrain_sampler::nether_cave_mask),
                );
                self.looks_ready = true;
            }
        }
        let specs = match (&context, sa.enabled) {
            (Some(c), true) => meridian_core::seedatlas_state::resolve_all(&self.seeds, &sa, &c.key, self.detected_seed()),
            _ => [None, None, None],
        };
        self.seed_svc.configure_dimensions(specs, &sa);
    }

}

/// The completed-structure key of a marker in the open world.
fn structure_key(
    seed_key: Option<&str>,
    svc: &meridian_core::seedatlas::SeedService,
    id: &str,
    dimension: meridian_seedatlas::Dimension,
    x: i32,
    z: i32,
) -> Option<meridian_core::completed::StructureKey> {
    {
        let world = seed_key?;
        let atlas = svc.atlas_for(dimension)?;
        let dim_id = match dimension {
            meridian_seedatlas::Dimension::Overworld => "minecraft:overworld",
            meridian_seedatlas::Dimension::Nether => "minecraft:the_nether",
            meridian_seedatlas::Dimension::End => "minecraft:the_end",
        };
        meridian_core::completed::StructureKey::new(world, atlas.seed, atlas.large_biomes, dim_id, id, x, z)
    }
}

/// The Seed Atlas buttons on the map, where the Fabric version 2.0 puts
/// them: the layer switch at the bottom left; the map settings ("M") and
/// the Seed Atlas settings ("SA") compact in the top left corner.
fn seed_atlas_buttons(gui: [f32; 2]) -> ([f32; 4], [f32; 4], [f32; 4]) {
    ([0.0, (gui[1] - 20.0).max(0.0), 20.0, 20.0], [22.0, 0.0, 20.0, 20.0], [0.0, 0.0, 20.0, 20.0])
}

fn inside(r: [f32; 4], p: [f32; 2]) -> bool {
    p[0] >= r[0] && p[0] < r[0] + r[2] && p[1] >= r[1] && p[1] < r[1] + r[3]
}

/// A tooltip in the game's style next to the cursor (kept on screen).
fn tooltip(ops: &mut Vec<DrawOp>, mouse: [f32; 2], gui: [f32; 2], lines: &[String]) {
    // Width estimated from the font's average advance; the plate fits the
    // text the host measures.
    let longest = lines.iter().map(|l| l.chars().count()).max().unwrap_or(0) as f32 * 6.0;
    let h = lines.len() as f32 * 10.0 + 6.0;
    let mut x = mouse[0] + 12.0;
    if x + longest + 8.0 > gui[0] {
        x = (mouse[0] - 16.0 - longest).max(4.0);
    }
    let y = (mouse[1] - 12.0).clamp(4.0, (gui[1] - h).max(4.0));
    for (i, line) in lines.iter().enumerate() {
        ops.push(DrawOp::Plate {
            x,
            y: y + 3.0 + i as f32 * 10.0,
            text: line.clone(),
            align: Align::Left,
            scale: 1.0,
            pad: 3.0,
            argb: 0xF010_0010,
        });
    }
    for (i, line) in lines.iter().enumerate() {
        ops.push(DrawOp::Text { x, y: y + 3.0 + i as f32 * 10.0, text: line.clone(), argb: 0xFFFF_FFFF, align: Align::Left, scale: 1.0 });
    }
}

/// A nearest search's result: where (if found), the name and the waypoint
/// wanted.
type NearestDone = (Option<meridian_core::nearest::NearestResult>, String, meridian_core::nearest::WaypointMode);
static NEAREST_DONE: Mutex<Option<NearestDone>> = Mutex::new(None);

impl State {
    /// Starts a nearest biome or structure search on a thread.
    fn find_nearest(
        &mut self,
        kind: meridian_core::screens_seedatlas::NearestKind,
        mode: meridian_core::nearest::WaypointMode,
        skip_completed: bool,
        dim: meridian_seedatlas::Dimension,
        from: [i32; 3],
    ) {
        use meridian_core::screens_seedatlas::NearestKind;
        let Some(atlas) = self.seed_svc.atlas_for(dim).cloned() else { return };
        let y = sa::sample_y(&self.config.current().seed_atlas, dim);
        let dim_id = match dim {
            meridian_seedatlas::Dimension::Overworld => "minecraft:overworld",
            meridian_seedatlas::Dimension::Nether => "minecraft:the_nether",
            meridian_seedatlas::Dimension::End => "minecraft:the_end",
        };
        let world = self.seed_key.clone().unwrap_or_default();
        let done: Vec<(i32, i32)> = match &kind {
            NearestKind::Structure { marker, .. } if skip_completed => self
                .completed
                .keys()
                .filter(|k| k.world == world && k.dimension == dim_id && k.kind == *marker && k.seed == atlas.seed)
                .map(|k| (k.x, k.z))
                .collect(),
            _ => Vec::new(),
        };
        std::thread::spawn(move || {
            let tolerance = meridian_core::seedatlas_state::COMPLETED_TOLERANCE;
            let (found, name) = match kind {
                NearestKind::Biome { id, name } => (meridian_core::nearest::nearest_biome(&atlas, dim, id, y, from[0], from[2]), name),
                NearestKind::Structure { marker, name } => {
                    let found = sa::MARKERS.iter().find(|m| m.id == marker).and_then(|m| {
                        let skip = |x: i32, z: i32| done.iter().any(|&(cx, cz)| (cx - x).abs() <= tolerance && (cz - z).abs() <= tolerance);
                        meridian_core::nearest::nearest_structure(&atlas, dim, m, from[0], from[2], &skip)
                    });
                    (found, name)
                }
            };
            *NEAREST_DONE.lock() = Some((found, name, mode));
        });
    }

    /// Carries out a finished nearest search: the map opens there with a
    /// waypoint, or the screen says nothing was found.
    fn poll_nearest(&mut self) {
        use meridian_core::nearest::WaypointMode;
        use meridian_core::screens_seedatlas::NearestOutcome;
        let Some((found, name, mode)) = NEAREST_DONE.lock().take() else { return };
        let outcome = match found {
            Some(r) => {
                if let Some((book, dim)) = self.book_here() {
                    let mut w = Waypoint::new(name.clone(), [r.x, 64, r.z], 11);
                    match mode {
                        WaypointMode::Temporary => w.kind = Kind::Temporary,
                        WaypointMode::Enabled => {}
                        WaypointMode::Disabled => w.disabled = true,
                    }
                    book.add(&dim, w, false, waypoint::now());
                }
                self.map.center = [r.x as f64 + 0.5, r.z as f64 + 0.5];
                self.screen = None;
                self.map_open = true;
                NearestOutcome::Found
            }
            None => NearestOutcome::NotFound,
        };
        if let Some(Screen::Nearest(n)) = &mut self.screen {
            n.finish(outcome);
        }
    }
}
