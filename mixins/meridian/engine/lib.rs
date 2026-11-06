//! Meridian's adapter to Horizon: the trait mod the launcher links into the
//! game. It translates Horizon's hooks into [`HostEvent`]s, answers the
//! core's [`MapHost`] questions and draws what the core lays out.
//!
//! Chunk events and chunk reads (`docs/HORIZON_API.md` E1/E2) come from the
//! bundle's transitional bridge, `horizon_client::game::meridian_bridge`,
//! which horizon-mixins adds to the game (`mixins/meridian/bridge/`).
#![forbid(unsafe_code)]

mod gpu;

use std::path::PathBuf;
use std::time::{Duration, Instant};

use horizon_client::game::meridian_bridge as bridge;
use horizon_client::mod_api::{
    ClientChatReceivedEvent, ClientDisconnectingEvent, ClientLevelLoadEvent,
    ClientLevelUnloadEvent, ClientResourcesReloadedEvent, ClientStoppingEvent, CommandEvent,
    CommandsEvent, Flow, FrameEvent, Hooks, Host, HudBatch, HudEvent, KeyEvent, ModsEvent,
    NativeMod, PlayerDiedEvent, RenderHooks, ScreenCharTypedPreEvent, ScreenClosingEvent,
    ScreenKeyPressedPreEvent, ScreenMouseButtonPressedPreEvent, ScreenMouseScrolledPreEvent,
    MenuBackgroundEvent, ScreenRenderPostEvent, Text, TintSource, WorldLifecycleEvent, WorldPhase, WorldPrimitivesEvent,
};
use meridian_core::MapEngine;
use meridian_core::config::{ConfigFile, ConfigStore};
use meridian_core::engine::DEFAULT_MAP;
use meridian_core::geom::RegionPos;
use meridian_core::keybinds::{KEY_BINDINGS, bindings_for_key};
use meridian_core::minimap::{self, Align, Controls, DrawOp};
use meridian_core::screens::{Action, Editor, Env, List, Screen, SettingsScreen};
use meridian_core::persist::MapPaths;
use meridian_core::radar;
use meridian_core::waypoint::{self, Kind, Waypoint, WaypointBook};
use meridian_host::{
    BiomeAppearance, BlockAppearance, ChunkPos, ChunkSnapshot, ColumnSample, DimensionId,
    HostEvent, MapHost, PlayerState, ReadMode, Tint, WorldContext, WorldSource,
};
use parking_lot::Mutex;

/// The mod id: its settings folder, its keys' lines, its data folder.
pub const MOD_ID: &str = "meridian";
const VERSION: &str = env!("CARGO_PKG_VERSION");
/// The bridge shape this adapter was written for.
const BRIDGE_VERSION: u32 = 3;

/// The singleplayer world's folder name, set by the integrated server's
/// thread (the window's client only knows the world's display name).
static SERVER_WORLD: Mutex<Option<String>> = Mutex::new(None);
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
    book: Option<(WorldSource, WaypointBook)>,
    last_book_save: Instant,
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
}

/// Work a screen asked for that needs the game's write access.
#[derive(Debug)]
enum Pending {
    CloseScreen,
    Chat(String),
}

/// What a minimap image depends on: its origin and size, and the regions'
/// revisions (any redraw changes them).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct MapImageKey {
    origin: [i32; 2],
    size: u32,
    revisions: u64,
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
        if self
            .book
            .as_ref()
            .is_some_and(|(source, _)| *source == context.source)
        {
            return;
        }
        self.save_book();
        let paths = MapPaths::new(data_dir, &context.source, DEFAULT_MAP, &context.dimension);
        self.book = Some((context.source.clone(), WaypointBook::load(paths.map_dir())));
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
            let config = store.load();
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
            State {
                config,
                store,
                engine,
                controls: Controls::default(),
                dimension: None,
                book: None,
                last_book_save: Instant::now(),
                next_color: 12,
                notice: None,
                map_image: None,
                screen: None,
                open_screen: None,
                pending: Vec::new(),
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

/// `minecraft:plains` as "Plains".
fn biome_label(id: &str) -> String {
    let name = id.rsplit(':').next().unwrap_or(id);
    let mut out = String::with_capacity(name.len());
    for (i, word) in name.split('_').enumerate() {
        if i > 0 {
            out.push(' ');
        }
        let mut chars = word.chars();
        if let Some(first) = chars.next() {
            out.extend(first.to_uppercase());
            out.push_str(chars.as_str());
        }
    }
    out
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
        const USAGE: &str = "/meridian waypoint add|remove|tp|share|list [name]";
        self.with_state(host, |s| {
            let player = HorizonHost {
                host,
                dimension: s.dimension.as_deref(),
            }
            .player();
            let tp = s.config.current().waypoints.teleport_format.clone();
            let Some((book, dim)) = s.book_here() else {
                return ("Meridian: no world open".to_owned(), None);
            };
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
            self.draw_screen(host, event.batch);
        }
    }

    fn screen_mouse_button_pressed_pre(&self, host: &dyn Host, event: &mut ScreenMouseButtonPressedPreEvent<'_>) -> Flow {
        if host.mod_screen() != Some(MOD_ID) || event.button > 1 {
            return Flow::Continue;
        }
        let at = host.cursor();
        let right = event.button == 1;
        self.with_screen(host, |screen, env| ((), screen.click(at, right, env)));
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
        self.with_screen(host, |screen, env| ((), screen.key(&key, env)));
        Flow::Cancel
    }

    fn screen_mouse_scrolled_pre(&self, host: &dyn Host, event: &mut ScreenMouseScrolledPreEvent<'_>) -> Flow {
        if host.mod_screen() != Some(MOD_ID) {
            return Flow::Continue;
        }
        let lines = event.scroll[1] as f32;
        self.with_screen(host, |screen, _| {
            screen.scroll(lines);
            ((), Action::None)
        });
        Flow::Cancel
    }

    fn menu_background(&self, host: &dyn Host, event: &mut MenuBackgroundEvent<'_>) {
        // Our screens blur the world behind them as the game's menus do.
        if host.mod_screen() == Some(MOD_ID) && event.radius.is_none() {
            let blur = host.client_state().map_or(5.0, |c| c.menu_blur);
            *event.radius = Some(blur.max(1.0));
        }
    }

    fn screen_closing(&self, host: &dyn Host, event: &mut ScreenClosingEvent<'_>) {
        if event.screen.id == "mod" && host.mod_screen() == Some(MOD_ID) {
            self.with_state(host, |s| s.screen = None);
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
        let open = self.with_state(&*host, |s| s.open_screen.take());
        if let Some(screen) = open
            && host.mod_screen_open(MOD_ID)
        {
            self.with_state(&*host, |s| s.screen = Some(screen));
        }
    }

    fn commands(&self, _host: &mut dyn Host, event: &mut CommandsEvent<'_>) {
        event.commands.push("meridian");
    }

    fn command(&self, host: &mut dyn Host, event: &mut CommandEvent<'_>) -> Flow {
        let args: Vec<&str> = event.command.split_whitespace().collect();
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
            }
            WorldPhase::WorldCloseFinal | WorldPhase::WorldOpenFailed => {
                *SERVER_WORLD.lock() = None
            }
            _ => {}
        }
    }

    fn player_died(&self, host: &mut dyn Host, _event: &mut PlayerDiedEvent<'_>) {
        // Server-side hook: in singleplayer it is the local player's death.
        let host: &dyn Host = host;
        if SERVER_WORLD.lock().is_none() {
            return;
        }
        self.with_state(host, |s| {
            let settings = s.config.current().waypoints.clone();
            if !settings.deathpoints {
                return;
            }
            let Some(p) = (HorizonHost {
                host,
                dimension: s.dimension.as_deref(),
            })
            .player() else {
                return;
            };
            let pos = [
                p.position[0].floor() as i32,
                p.position[1].floor() as i32,
                p.position[2].floor() as i32,
            ];
            if let Some((book, dim)) = s.book_here() {
                book.record_death(&dim, pos, settings.keep_old_deathpoints, waypoint::now());
            }
            s.save_book();
        });
    }

    fn client_level_load(&self, host: &mut dyn Host, event: &mut ClientLevelLoadEvent<'_>) {
        let host: &dyn Host = host;
        self.with_state(host, |s| {
            let dimension = event.dimension.to_owned();
            s.dimension = Some(dimension.clone());
            let map_host = HorizonHost {
                host,
                dimension: Some(&dimension),
            };
            match map_host.context() {
                Some(context) => {
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
        self.send(host, HostEvent::LevelUnloaded);
    }

    fn client_disconnecting(&self, host: &dyn Host, _event: &mut ClientDisconnectingEvent) {
        self.with_state(host, State::close_book);
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
        let pending = self.with_state(&*host, |s| std::mem::take(&mut s.pending));
        for p in pending {
            match p {
                Pending::CloseScreen => {
                    host.mod_screen_close(MOD_ID);
                    self.with_state(&*host, |s| s.screen = None);
                }
                Pending::Chat(line) => {
                    if let Err(e) = host.send_chat(&line) {
                        host.client_action_bar(Text::literal(format!("Meridian: {e}")), false);
                    }
                }
            }
        }
        let host: &dyn Host = host;
        self.with_state(host, |s| {
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
                // Keep the regions around the player in memory.
                let here = RegionPos::of_block(p.position[0] as i32, p.position[2] as i32);
                s.engine
                    .request_area(here.offset(-1, -1), here.offset(1, 1));
            }
            let budget =
                Duration::from_micros(s.config.current().performance.frame_budget_us as u64);
            s.engine.tick(&map_host, budget);
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
            let map_host = HorizonHost { host, dimension: s.dimension.as_deref() };
            let Some(player) = map_host.player() else { return };
            let mut ops = Vec::new();
            world_waypoints(host, s, gui, gui_scale, &mut ops);
            let settings = &s.config.current().minimap;
            if settings.enabled && !(settings.hide_under_screens && !host.playing()) {
                minimap_ops(s, &player, gui, gui_scale, &mut ops);
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
fn minimap_ops(s: &mut State, player: &PlayerState, gui: [f32; 2], gui_scale: f32, ops: &mut Vec<DrawOp>) {
    let settings = &s.config.current().minimap;
    let [px, py, pz] = player.position;
    let mut lines = Vec::new();
    if settings.show_coords {
        lines.push((format!("{}, {}, {}", px.floor(), py.floor(), pz.floor()), 0xFFFF_FFFF));
    }
    if settings.show_biome
        && let Some(biome) = bridge::player_biome()
    {
        lines.push((biome_label(&biome), 0xFFFF_FFFF));
    }
    let layout = minimap::layout(settings, gui, gui_scale, s.controls.enlarged(), lines.len());
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
        }));
    }
    let radar = &s.config.current().radar;
    bridge::want_entities(radar.enabled);
    if radar.enabled {
        let entities: Vec<radar::Entity> = bridge::entities()
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
            })
            .collect();
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
    let key = MapImageKey { origin, size, revisions };
    if s.map_image.as_ref().is_some_and(|(k, _)| *k == key) {
        return;
    }
    let sampler = s.engine.sampler();
    let mut pixels = Vec::with_capacity((size * size) as usize);
    for z in 0..size as i32 {
        for x in 0..size as i32 {
            pixels.push(sampler.pixel(origin[0] + x, origin[1] + z));
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
            let Some(dim) = s.dimension.clone() else {
                s.screen = Some(screen);
                return R::default();
            };
            let Some((_, book)) = &mut s.book else {
                s.screen = Some(screen);
                return R::default();
            };
            let mut settings = s.config.current().clone();
            let (result, action, waypoints_changed, settings_changed) = {
                let waypoints = book.dimension_mut(&dim);
                let mut env = Env {
                    gui,
                    world_label: &world_label,
                    dimension_label: &dim_label,
                    player_block,
                    waypoints,
                    settings: &mut settings,
                    waypoints_changed: false,
                    settings_changed: false,
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
        let ops: Vec<DrawOp> = self.with_screen(host, |screen, env| {
            let widgets = screen.widgets(env);
            // The game's in-world menu darkening over the blur.
            let mut ops = vec![DrawOp::Fill { rect: [0.0, 0.0, env.gui[0], env.gui[1]], argb: 0x5000_0000 }];
            ops.extend(meridian_core::ui::draw(&widgets, mouse));
            (ops, Action::None)
        });
        paint(host, batch, &ops);
    }
}
