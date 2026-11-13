// Mod-owned data shared with the pinned native client support module.
// The module declared in horizon-mod.toml samples game state and writes these
// queues. This fills the public API gaps recorded in docs/FORK_PORT.md:
//
// - client chunk events: loaded and unloaded from the chunk map's
//   membership deltas, changed from the client Game::set_block observer;
// - client chunk reads: a top-down copy of a loaded chunk, the same scan
//   Horizon's own minimap makes, with names instead of internal ids;
// - block and biome looks: map colours, fluids, biome tints.
//
// Meridian's trait mod (which links against this library) asks through the
// functions below; the frame injection answers on the game thread before
// trait mods hear `frame`. Plain data only, so the trait mod never holds a
// reference into the game. Everything is off until `enable` is called.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;

/// Main-thread time per frame for chunk reads.
pub const READ_BUDGET: Duration = Duration::from_micros(1500);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChunkChange {
    Loaded([i32; 2]),
    Changed([i32; 2]),
    Unloaded([i32; 2]),
    /// Everything may have changed (a new level, or events were lost): the
    /// loaded chunks follow as `Loaded`.
    Reset,
}

/// One column, top-down. Indexes point into the snapshot's palettes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Column {
    pub block: u16,
    pub height: i16,
    /// A fluid above `block`: its palette index and depth.
    pub fluid: Option<(u16, u8)>,
    pub biome: u16,
    /// Sky light in the high nibble, block light in the low one, above the
    /// surface.
    pub light: u8,
    /// Nothing to show (void).
    pub empty: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Snapshot {
    pub dimension: String,
    pub pos: [i32; 2],
    /// The request's cave ceiling (`None` = surface), so an answer is never
    /// taken for another layer's.
    pub cave_from: Option<i32>,
    pub blocks: Vec<String>,
    pub biomes: Vec<String>,
    /// 256 columns, x fastest.
    pub columns: Vec<Column>,
    pub biome_border: Vec<u16>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BlockLook {
    /// `MapColor.col` (0xRRGGBB), 0 for none.
    pub map_color: u32,
    /// 0 none, 1 water, 2 lava, 3 another registered fluid.
    pub fluid: u8,
    pub air: bool,
}

/// What an entity is, for the radar.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntityClass {
    Player,
    Hostile,
    /// Other living things: animals, villagers, golems, water mobs.
    Friendly,
    Item,
    /// Boats, minecarts and the like.
    Other,
}

/// One entity near the player.
#[derive(Clone, Debug, PartialEq)]
pub struct EntitySnap {
    pub id: u32,
    /// `minecraft:zombie`, `minecraft:item`...
    pub kind: String,
    pub class: EntityClass,
    /// Interpolated between the last two ticks, to this frame.
    pub pos: [f64; 3],
    /// Where the last tick put it, not interpolated.
    pub tick_pos: [f64; 3],
    pub invisible: bool,
    /// A player's name or a custom name.
    pub name: Option<String>,
    /// A player's skin texture under `textures/` (their own when the game
    /// drew it from its textures, else the default skin of their id).
    pub skin: Option<String>,
    /// A tamed animal (wolf, cat, horse, parrot...).
    pub tamed: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BiomeLook {
    pub grass: u32,
    pub foliage: u32,
    pub dry_foliage: u32,
    pub water: u32,
}

#[derive(Default)]
pub struct Shared {
    pub enabled: bool,
    pub gui_size: [f32; 2],
    pub action_bar: Option<(mod_api::Text, bool)>,
    pub local_chat: Vec<mod_api::Text>,
    pub held_keys: Vec<String>,
    /// The chunk map's membership revision last reported, 0 for none.
    pub membership: u64,
    /// Chunks to read: position and the cave ceiling (`None` = surface).
    pub requests: Vec<([i32; 2], Option<i32>)>,
    pub results: Vec<Snapshot>,
    pub changes: Vec<ChunkChange>,
    pub blocks: HashMap<String, BlockLook>,
    pub biomes: HashMap<String, BiomeLook>,
    /// The biome at the player's feet, refreshed each frame.
    pub player_biome: Option<String>,
    /// The window in physical pixels, refreshed each frame.
    pub window: Option<[u32; 2]>,
    /// The debug overlay (F3) is open.
    pub debug_shown: bool,
    /// The world spawn the server announced (the Overworld's).
    pub world_spawn: Option<[i32; 3]>,
    /// Text to put on the clipboard on the next frame.
    pub clipboard: Option<String>,
    /// The prepared game resources (`assets/minecraft/textures/...`).
    pub resources: Option<std::path::PathBuf>,
    /// Entities are copied each frame only while someone wants them.
    pub want_entities: bool,
    /// Shared with the readers, so a frame's copy is made once.
    pub entities: std::sync::Arc<[EntitySnap]>,
    /// Columns either side of the player a roof must cover (cave mode), or
    /// none when nobody asks.
    pub roof_radius: Option<u8>,
    pub under_roof: bool,
    /// Blocks the map looks through: flowers, redstone parts.
    pub hide: Hide,
}

/// Small blocks the map can look through to the ground under them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Hide {
    pub flowers: bool,
    pub redstone: bool,
    pub stained_glass: bool,
    /// Short blocks (carpets, snow layers, plates) count one block lower,
    /// so they don't shade as a step.
    pub lower_short: bool,
}

impl Hide {
    pub fn any(self) -> bool {
        self.flowers || self.redstone || self.stained_glass
    }

    /// Whether a block is less than half a block tall (by its id).
    pub fn short(name: &str) -> bool {
        let name = name.strip_prefix("minecraft:").unwrap_or(name);
        name.ends_with("_carpet")
            || name.ends_with("_pressure_plate")
            || name.ends_with("rail")
            || matches!(
                name,
                "snow"
                    | "lily_pad"
                    | "repeater"
                    | "comparator"
                    | "daylight_detector"
                    | "redstone_wire"
                    | "pink_petals"
                    | "leaf_litter"
            )
    }

    /// Whether the map looks through this block (by its id).
    pub fn hides(self, name: &str) -> bool {
        let name = name.strip_prefix("minecraft:").unwrap_or(name);
        const FLOWERS: &[&str] = &[
            "dandelion",
            "poppy",
            "blue_orchid",
            "allium",
            "azure_bluet",
            "oxeye_daisy",
            "cornflower",
            "lily_of_the_valley",
            "wither_rose",
            "torchflower",
            "sunflower",
            "lilac",
            "rose_bush",
            "peony",
            "pitcher_plant",
            "pink_petals",
            "wildflowers",
            "open_eyeblossom",
            "closed_eyeblossom",
            "spore_blossom",
            "flowering_azalea",
        ];
        const REDSTONE: &[&str] = &[
            "redstone_wire",
            "repeater",
            "comparator",
            "redstone_torch",
            "redstone_wall_torch",
            "lever",
            "tripwire",
            "tripwire_hook",
        ];
        (self.stained_glass && name.contains("stained_glass"))
            || (self.flowers && (name.ends_with("_tulip") || FLOWERS.contains(&name)))
            || (self.redstone
                && (REDSTONE.contains(&name)
                    || name.ends_with("_pressure_plate")
                    || name.ends_with("_button")))
    }
}

/// Which small blocks chunk reads look through from now on.
pub fn set_hidden(hide: Hide) {
    with(|s| s.hide = hide);
}

pub static SHARED: Mutex<Option<Shared>> = Mutex::new(None);

fn with<R>(f: impl FnOnce(&mut Shared) -> R) -> R {
    let mut guard = SHARED.lock().unwrap_or_else(|e| e.into_inner());
    f(guard.get_or_insert_with(Shared::default))
}

/// Starts the bridge (the trait mod calls it once): from the next frame on,
/// chunk events are collected and reads answered.
pub fn enable() {
    with(|s| {
        s.enabled = true;
        s.membership = 0;
    });
    changes::set_enabled(true);
}

/// Asks for a chunk; the snapshot arrives after the next frame, or never
/// when the chunk is not loaded then.
pub fn request(pos: [i32; 2], cave_from: Option<i32>) {
    with(|s| {
        if !s.requests.contains(&(pos, cave_from)) {
            s.requests.push((pos, cave_from));
        }
    });
}

/// Forgets the chunks asked for and not read yet (the map switched layers:
/// it asks again for the new one).
pub fn clear_requests() {
    with(|s| s.requests.clear());
}

pub fn take_results() -> Vec<Snapshot> {
    with(|s| std::mem::take(&mut s.results))
}

pub fn take_changes() -> Vec<ChunkChange> {
    with(|s| std::mem::take(&mut s.changes))
}

/// How a block state seen in a snapshot looks.
pub fn block_look(name: &str) -> Option<BlockLook> {
    with(|s| s.blocks.get(name).copied())
}

/// A biome's tints, for a biome seen in a snapshot.
pub fn biome_look(name: &str) -> Option<BiomeLook> {
    with(|s| s.biomes.get(name).copied())
}

/// The biome at the player's feet (`minecraft:plains`).
pub fn player_biome() -> Option<String> {
    with(|s| s.player_biome.clone())
}

/// Asks for (or stops) the per-frame entity copy.
pub fn want_entities(on: bool) {
    with(|s| {
        s.want_entities = on;
        if !on {
            s.entities = std::sync::Arc::new([]);
        }
    });
}

/// The entities of the last frame (empty unless asked for); cheap to call
/// more than once a frame.
pub fn entities() -> std::sync::Arc<[EntitySnap]> {
    with(|s| s.entities.clone())
}

/// Asks for the roof check around the player (or stops it).
pub fn set_roof_probe(radius: Option<u8>) {
    with(|s| {
        s.roof_radius = radius;
        if radius.is_none() {
            s.under_roof = false;
        }
    });
}

/// Whether the player was under a solid roof covering the asked area.
pub fn under_roof() -> bool {
    with(|s| s.under_roof)
}

/// Puts text on the system clipboard (`KeyboardHandler.setClipboard`).
/// Lowers the calling thread as the game's far terrain threads are: long
/// background work the frames and near chunks never wait behind (utility
/// quality of service on macOS). Whether the OS took it.
pub fn lower_current_thread() -> bool {
    PRIORITY.get().is_some_and(|lower| lower())
}

pub fn copy_to_clipboard(text: &str) {
    with(|s| s.clipboard = Some(text.to_owned()));
}

/// The world spawn the server announced, once it has.
pub fn world_spawn() -> Option<[i32; 3]> {
    with(|s| s.world_spawn)
}

/// Whether the debug overlay (F3) is open.
pub fn debug_shown() -> bool {
    with(|s| s.debug_shown)
}

/// The window's size in physical pixels.
pub fn window_size() -> Option<[u32; 2]> {
    with(|s| s.window)
}

/// The folder of the game's prepared resources, for texture colours.
pub fn resources_dir() -> Option<std::path::PathBuf> {
    with(|s| s.resources.clone())
}

/// Forgets cached looks (resources reloaded).
pub fn clear_looks() {
    with(|s| {
        s.blocks.clear();
        s.biomes.clear();
    });
}

/// Physical held state is independent of screens consuming key releases.
pub fn key_held(name: &str) -> bool {
    with(|s| s.held_keys.iter().any(|key| key == name))
}

/// Local-only feedback: never sent over the connection or back through chat hooks.
pub fn local_chat(lines: Vec<mod_api::Text>) {
    with(|s| s.local_chat.extend(lines));
}

pub static PRIORITY: std::sync::OnceLock<fn() -> bool> = std::sync::OnceLock::new();
pub fn gui_size() -> [f32; 2] {
    with(|s| s.gui_size)
}
pub fn action_bar(text: mod_api::Text, animate: bool) {
    with(|s| s.action_bar = Some((text, animate)));
}
pub mod changes {
    // Meridian (transitional, docs/HORIZON_API.md E1): which loaded chunks had
    // blocks written since the map last asked. Added to the client's `world`
    // module by horizon-mixins; filled by HEAD injections into `World::set` and
    // `World::edit_runs`, drained once a frame by `game::meridian_bridge`.
    // Off until the bridge turns it on, so it costs one atomic load per write
    // when Meridian is not running.

    use std::sync::Mutex;
    use std::sync::atomic::{AtomicBool, Ordering};

    /// At most this many entries wait; past it the bridge rescans everything.
    const LIMIT: usize = 65_536;

    static ON: AtomicBool = AtomicBool::new(false);
    static CHANGED: Mutex<Vec<(i32, i32)>> = Mutex::new(Vec::new());
    static OVERFLOWED: AtomicBool = AtomicBool::new(false);

    pub fn set_enabled(on: bool) {
        ON.store(on, Ordering::Relaxed);
        if !on {
            CHANGED.lock().unwrap_or_else(|e| e.into_inner()).clear();
        }
    }

    /// Chunk (`cx`, `cz`) had a block written.
    pub fn record(cx: i32, cz: i32) {
        if !ON.load(Ordering::Relaxed) {
            return;
        }
        let mut changed = CHANGED.lock().unwrap_or_else(|e| e.into_inner());
        if changed.last() == Some(&(cx, cz)) {
            return;
        }
        if changed.len() >= LIMIT {
            OVERFLOWED.store(true, Ordering::Relaxed);
            return;
        }
        changed.push((cx, cz));
    }

    /// The chunks written since the last call, and whether some were dropped.
    pub fn take() -> (Vec<(i32, i32)>, bool) {
        let list = std::mem::take(&mut *CHANGED.lock().unwrap_or_else(|e| e.into_inner()));
        (list, OVERFLOWED.swap(false, Ordering::Relaxed))
    }
}
