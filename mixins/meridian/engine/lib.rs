//! Meridian's adapter to Horizon: the trait mod the launcher links into the
//! game. It translates Horizon's hooks into [`HostEvent`]s, answers the
//! core's [`MapHost`] questions and draws what the core lays out.
//!
//! Chunk events and chunk reads (`docs/HORIZON_API.md` E1/E2) come from the
//! bundle's transitional bridge, `horizon_client::game::meridian_bridge`,
//! which horizon-mixins adds to the game (`mixins/meridian/bridge/`).
#![forbid(unsafe_code)]

use std::path::PathBuf;
use std::time::{Duration, Instant};

use horizon_client::game::meridian_bridge as bridge;
use horizon_client::mod_api::{
    ClientChatReceivedEvent, ClientDisconnectingEvent, ClientLevelLoadEvent,
    ClientLevelUnloadEvent, ClientResourcesReloadedEvent, ClientStoppingEvent, CommandEvent,
    CommandsEvent, Flow, FrameEvent, Hooks, Host, HudBatch, HudEvent, KeyEvent, ModsEvent,
    NativeMod, PlayerDiedEvent, Text, TintSource, WorldLifecycleEvent, WorldPhase,
    WorldPrimitivesEvent,
};
use meridian_core::MapEngine;
use meridian_core::config::{ConfigFile, ConfigStore};
use meridian_core::engine::DEFAULT_MAP;
use meridian_core::geom::RegionPos;
use meridian_core::keybinds::{KEY_BINDINGS, bindings_for_key};
use meridian_core::minimap::{self, Align, Controls, DrawOp};
use meridian_core::persist::MapPaths;
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
const BRIDGE_VERSION: u32 = 1;

/// The singleplayer world's folder name, set by the integrated server's
/// thread (the window's client only knows the world's display name).
static SERVER_WORLD: Mutex<Option<String>> = Mutex::new(None);
/// Blocks within which a waypoint's beam is left out (it would fill the view).
const BEAM_HIDE_DISTANCE: f64 = 8.0;

pub fn hooks() -> Box<dyn Hooks> {
    Box::new(Meridian {
        state: Mutex::new(None),
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
}

struct Meridian {
    state: Mutex<Option<State>>,
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
        let tint = self
            .host
            .block_sprites(&[block])
            .into_iter()
            .next()
            .flatten()
            .and_then(|s| s.sides[1].first().map(|f| f.tint))
            .map_or(Tint::None, tint_of);
        let tint = if look.fluid == 1 { Tint::Water } else { tint };
        // Until texture means are available (Phase 4), tinted blocks use a
        // mid grey under their tint and the rest their map colour.
        let texture_color = if tint == Tint::None {
            look.map_color
        } else {
            0x9A_9A9A
        };
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

/// Draws the core's operations into Horizon's HUD batch.
fn paint(host: &dyn Host, batch: &mut HudBatch, ops: &[DrawOp]) {
    for op in ops {
        match op {
            DrawOp::Fill { rect, argb } => host.fill(batch, *rect, *argb),
            DrawOp::Quad { points, argb } => host.quad(batch, *points, *argb),
            DrawOp::Text {
                x,
                y,
                text,
                argb,
                align,
            } => {
                let w = host.text_width(text);
                let x = match align {
                    Align::Left => *x,
                    Align::Center => x - w / 2.0,
                    Align::Right => x - w,
                };
                host.text(
                    batch,
                    text,
                    [x.round(), y.round()],
                    argb & 0x00FF_FFFF,
                    true,
                );
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
                        "waypoint.instant" | "waypoint.new" => {
                            if let Some(p) = &player
                                && let Some(name) = s.quick_waypoint(p)
                            {
                                s.notice = Some(format!("Added {name}"));
                            }
                        }
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
        self.send(host, HostEvent::ResourcesReloaded);
    }

    fn frame(&self, host: &mut dyn Host, _event: &mut FrameEvent) {
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
                if flat > BEAM_HIDE_DISTANCE {
                    event.primitives.beam(base, 256, color);
                }
                let label = format!(
                    "{} ({})",
                    w.name,
                    waypoint::distance_label(distance, settings.distance_in_km_from, 1)
                );
                event
                    .primitives
                    .text(
                        [base[0], base[1] + 2.0, base[2]],
                        label,
                        0xFF00_0000 | 0xFFFFFF,
                        1.0,
                    )
                    .through_walls()
                    .scale_with_distance()
                    .background();
            }
        });
    }

    fn hud(&self, host: &mut dyn Host, event: &mut HudEvent<'_>) {
        let host: &dyn Host = host;
        let Some(client) = host.client_state() else {
            return;
        };
        if client.gui_hidden {
            return;
        }
        self.with_state(host, |s| {
            let settings = &s.config.current().minimap;
            if !settings.enabled || (settings.hide_under_screens && !host.playing()) {
                return;
            }
            let map_host = HorizonHost {
                host,
                dimension: s.dimension.as_deref(),
            };
            let Some(player) = map_host.player() else {
                return;
            };
            let [px, py, pz] = player.position;
            let mut lines = vec![(
                format!("{}, {}, {}", px.floor(), py.floor(), pz.floor()),
                0xFFFF_FFFF,
            )];
            if let Some(biome) = bridge::player_biome() {
                lines.push((biome_label(&biome), 0xFFFF_FFFF));
            }
            let enlarged = s.controls.enlarged();
            let layout = minimap::layout(settings, client.gui_size, enlarged, lines.len());
            let sampler = s.engine.sampler();
            let mut ops = minimap::terrain(
                &layout,
                [px, pz],
                player.yaw,
                settings.opacity,
                settings.undiscovered_opacity,
                |x, z| sampler.pixel(x, z),
            );
            ops.extend(minimap::frame(&layout, 0xFF20_2020));
            ops.extend(minimap::compass(&layout, player.yaw));
            let wp = &s.config.current().waypoints;
            if wp.on_minimap
                && let (Some(dim), Some((_, book))) = (&s.dimension, &s.book)
                && let Some(waypoints) = book.dimension(dim)
            {
                let alpha = ((wp.opacity_on_minimap.min(100) as u32 * 255) / 100) << 24;
                for w in waypoints.visible(wp.all_sets) {
                    let at = [w.pos[0] as f64 + 0.5, w.pos[2] as f64 + 0.5];
                    let ([x, y], _) = minimap::project(&layout, [px, pz], player.yaw, at, 5.0);
                    ops.push(DrawOp::Fill {
                        rect: [x - 4.0, y - 4.0, 9.0, 9.0],
                        argb: alpha,
                    });
                    ops.push(DrawOp::Fill {
                        rect: [x - 3.0, y - 3.0, 7.0, 7.0],
                        argb: alpha | w.rgb(),
                    });
                    let initial: String = w.initials.chars().take(1).collect();
                    ops.push(DrawOp::Text {
                        x: x + 0.5,
                        y: y - 3.0,
                        text: initial,
                        argb: 0xFFFF_FFFF,
                        align: Align::Center,
                    });
                }
            }
            ops.push(minimap::arrow(
                &layout,
                player.yaw,
                settings.arrow_scale,
                0xFFFF_FFFF,
            ));
            ops.extend(minimap::info(&layout, &lines));
            paint(host, event.batch, &ops);
        });
    }
}
