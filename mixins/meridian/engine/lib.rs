//! Meridian's adapter to Horizon: the trait mod the launcher links into the
//! game. It translates Horizon's hooks into [`HostEvent`]s and answers the
//! core's [`MapHost`] questions through Horizon's `Host`.
//!
//! What Horizon does not offer yet (`docs/HORIZON_API.md`):
//!
//! - E1 client chunk events and E2 client chunk reads: [`HorizonHost::read_chunk`]
//!   answers `None` and no chunk events arrive until the transitional mixins
//!   land (Phase 3). The engine runs, loads and saves maps, but writes nothing
//!   new yet.
//! - Block appearances (part of E2): answered with "invisible" until then.
#![forbid(unsafe_code)]

use std::path::PathBuf;
use std::time::Duration;

use horizon_client::mod_api::{
    ClientDisconnectingEvent, ClientLevelLoadEvent, ClientLevelUnloadEvent,
    ClientResourcesReloadedEvent, ClientStoppingEvent, FrameEvent, Hooks, Host, KeyEvent,
    ModsEvent, NativeMod,
};
use meridian_core::MapEngine;
use meridian_core::config::{ConfigFile, ConfigStore};
use meridian_core::keybinds::{KEY_BINDINGS, bindings_for_key};
use meridian_host::{
    BiomeAppearance, BlockAppearance, ChunkPos, ChunkSnapshot, DimensionId, HostEvent, MapHost,
    PlayerState, ReadMode, WorldContext, WorldSource,
};
use parking_lot::Mutex;

/// The mod id: its settings folder, its keys' lines, its data folder.
pub const MOD_ID: &str = "meridian";
const VERSION: &str = env!("CARGO_PKG_VERSION");

pub fn hooks() -> Box<dyn Hooks> {
    Box::new(Meridian { state: Mutex::new(None) })
}

struct State {
    config: ConfigFile,
    store: ConfigStore,
    engine: MapEngine,
    /// The dimension the client shows, from `client_level_load`.
    dimension: Option<String>,
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
    host.instance_dir().join("horizon-mods").join(".horizon-mod-data").join(MOD_ID)
}

fn config_dir(host: &dyn Host) -> PathBuf {
    host.config_dir().unwrap_or_else(|e| {
        log::warn!("meridian: no config folder ({e}), using the data folder");
        data_dir(host)
    })
}

impl MapHost for HorizonHost<'_> {
    fn context(&self) -> Option<WorldContext> {
        let dimension = DimensionId::new(self.dimension?);
        let address = self.host.client_state().and_then(|s| s.server_address);
        let source = match address {
            Some(address) => WorldSource::Multiplayer { address },
            None => {
                let dir = self.host.world_dir()?;
                let world = dir.file_name()?.to_string_lossy().into_owned();
                WorldSource::Singleplayer { world }
            }
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

    fn read_chunk(&self, _chunk: ChunkPos, _mode: ReadMode) -> Option<ChunkSnapshot> {
        // E2: needs the transitional chunk-read mixin (Phase 3).
        None
    }

    fn block_appearance(&self, _block: &str) -> BlockAppearance {
        // E2: map colours and texture means come with the chunk-read mixin.
        BlockAppearance { invisible: true, ..Default::default() }
    }

    fn biome_appearance(&self, _biome: &str) -> BiomeAppearance {
        BiomeAppearance::default()
    }

    fn day_time(&self) -> Option<i64> {
        self.host.playing().then(|| self.host.day_time())
    }

    fn data_dir(&self) -> PathBuf {
        data_dir(self.host)
    }

    fn config_dir(&self) -> PathBuf {
        config_dir(self.host)
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
            log::info!("meridian {VERSION}: settings in {}", store.path().display());
            State { config, store, engine, dimension: None }
        });
        f(state)
    }

    fn send(&self, host: &dyn Host, event: HostEvent) {
        self.with_state(host, |s| {
            let map_host = HorizonHost { host, dimension: s.dimension.as_deref() };
            s.engine.handle(&map_host, event);
        });
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
        let host: &dyn Host = host;
        let chosen = |id: &str, default: &str| host.mod_key(MOD_ID, id, default);
        let hits: Vec<_> = bindings_for_key(event.key, chosen).map(|b| b.id).collect();
        for id in hits {
            self.send(host, HostEvent::Key { id, pressed: event.pressed });
        }
    }

    fn client_level_load(&self, host: &mut dyn Host, event: &mut ClientLevelLoadEvent<'_>) {
        let host: &dyn Host = host;
        self.with_state(host, |s| {
            s.dimension = Some(event.dimension.to_owned());
            let map_host = HorizonHost { host, dimension: s.dimension.as_deref() };
            match map_host.context() {
                Some(context) => s.engine.handle(&map_host, HostEvent::LevelLoaded(context)),
                None => log::warn!("meridian: cannot tell which world {} belongs to", event.dimension),
            }
        });
    }

    fn client_level_unload(&self, host: &mut dyn Host, _event: &mut ClientLevelUnloadEvent<'_>) {
        self.send(host, HostEvent::LevelUnloaded);
    }

    fn client_disconnecting(&self, host: &dyn Host, _event: &mut ClientDisconnectingEvent) {
        self.send(host, HostEvent::Disconnected);
    }

    fn client_stopping(&self, host: &dyn Host, _event: &mut ClientStoppingEvent) {
        self.with_state(host, |s| {
            s.engine.flush();
            if let Err(e) = s.store.save(&s.config) {
                log::warn!("meridian: cannot save settings: {e}");
            }
        });
    }

    fn client_resources_reloaded(&self, host: &mut dyn Host, _event: &mut ClientResourcesReloadedEvent) {
        self.send(host, HostEvent::ResourcesReloaded);
    }

    fn frame(&self, host: &mut dyn Host, _event: &mut FrameEvent) {
        let host: &dyn Host = host;
        self.with_state(host, |s| {
            let budget = Duration::from_micros(s.config.current().performance.frame_budget_us as u64);
            let map_host = HorizonHost { host, dimension: s.dimension.as_deref() };
            s.engine.tick(&map_host, budget);
        });
    }
}
