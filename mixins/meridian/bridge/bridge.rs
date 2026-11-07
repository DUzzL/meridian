// Meridian's transitional bridge to the client world (docs/HORIZON_API.md,
// E1 and E2), added to the game as `game::meridian_bridge` by
// horizon-mixins. It stands in for hooks Horizon does not have yet:
//
// - client chunk events: loaded and unloaded from the chunk map's
//   membership deltas, changed from `world::meridian_changes`;
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
use std::time::{Duration, Instant};

/// Bumped when the shapes below change, so a trait mod can refuse a bridge
/// it doesn't understand.
pub const VERSION: u32 = 6;
/// Main-thread time per frame for chunk reads.
const READ_BUDGET: Duration = Duration::from_micros(1500);

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
    pub blocks: Vec<String>,
    pub biomes: Vec<String>,
    /// 256 columns, x fastest.
    pub columns: Vec<Column>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BlockLook {
    /// `MapColor.col` (0xRRGGBB), 0 for none.
    pub map_color: u32,
    /// 0 none, 1 water, 2 lava.
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

/// One entity near the player, interpolated to this frame.
#[derive(Clone, Debug, PartialEq)]
pub struct EntitySnap {
    pub id: u32,
    /// `minecraft:zombie`, `minecraft:item`...
    pub kind: String,
    pub class: EntityClass,
    pub pos: [f64; 3],
    pub invisible: bool,
    /// A player's name or a custom name.
    pub name: Option<String>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BiomeLook {
    pub grass: u32,
    pub foliage: u32,
    pub dry_foliage: u32,
    pub water: u32,
}

#[derive(Default)]
struct Shared {
    enabled: bool,
    /// The chunk map's membership revision last reported, 0 for none.
    membership: u64,
    /// Chunks to read: position and the cave ceiling (`None` = surface).
    requests: Vec<([i32; 2], Option<i32>)>,
    results: Vec<Snapshot>,
    changes: Vec<ChunkChange>,
    blocks: HashMap<String, BlockLook>,
    biomes: HashMap<String, BiomeLook>,
    /// The biome at the player's feet, refreshed each frame.
    player_biome: Option<String>,
    /// The window in physical pixels, refreshed each frame.
    window: Option<[u32; 2]>,
    /// The debug overlay (F3) is open.
    debug_shown: bool,
    /// The world spawn the server announced (the Overworld's).
    world_spawn: Option<[i32; 3]>,
    /// The prepared game resources (`assets/minecraft/textures/...`).
    resources: Option<std::path::PathBuf>,
    /// Entities are copied each frame only while someone wants them.
    want_entities: bool,
    entities: Vec<EntitySnap>,
    /// Columns either side of the player a roof must cover (cave mode), or
    /// none when nobody asks.
    roof_radius: Option<u8>,
    under_roof: bool,
    /// Blocks the map looks through: flowers, redstone parts.
    hide: Hide,
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
    fn any(self) -> bool {
        self.flowers || self.redstone || self.stained_glass
    }

    /// Whether a block is less than half a block tall (by its id).
    pub fn short(name: &str) -> bool {
        let name = name.strip_prefix("minecraft:").unwrap_or(name);
        name.ends_with("_carpet")
            || name.ends_with("_pressure_plate")
            || name.ends_with("rail")
            || matches!(name, "snow" | "lily_pad" | "repeater" | "comparator" | "daylight_detector" | "redstone_wire" | "pink_petals" | "leaf_litter")
    }

    /// Whether the map looks through this block (by its id).
    pub fn hides(self, name: &str) -> bool {
        let name = name.strip_prefix("minecraft:").unwrap_or(name);
        const FLOWERS: &[&str] = &[
            "dandelion", "poppy", "blue_orchid", "allium", "azure_bluet", "oxeye_daisy",
            "cornflower", "lily_of_the_valley", "wither_rose", "torchflower", "sunflower",
            "lilac", "rose_bush", "peony", "pitcher_plant", "pink_petals", "wildflowers",
            "open_eyeblossom", "closed_eyeblossom", "spore_blossom", "flowering_azalea",
        ];
        const REDSTONE: &[&str] = &[
            "redstone_wire", "repeater", "comparator", "redstone_torch", "redstone_wall_torch",
            "lever", "tripwire", "tripwire_hook",
        ];
        (self.stained_glass && name.contains("stained_glass"))
            || (self.flowers && (name.ends_with("_tulip") || FLOWERS.contains(&name)))
            || (self.redstone && (REDSTONE.contains(&name) || name.ends_with("_pressure_plate") || name.ends_with("_button")))
    }
}

/// Which small blocks chunk reads look through from now on.
pub fn set_hidden(hide: Hide) {
    with(|s| s.hide = hide);
}

static SHARED: Mutex<Option<Shared>> = Mutex::new(None);

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
    crate::world::meridian_changes::set_enabled(true);
}

/// Asks for a chunk; the snapshot arrives after the next frame, or never
/// when the chunk is not loaded then.
pub fn request(pos: [i32; 2], cave_from: Option<i32>) {
    with(|s| {
        if !s.requests.iter().any(|r| r.0 == pos) {
            s.requests.push((pos, cave_from));
        }
    });
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
            s.entities.clear();
        }
    });
}

/// The entities of the last frame (empty unless asked for).
pub fn entities() -> Vec<EntitySnap> {
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

/// The frame injection's entry.
pub(super) fn frame(game: &mut super::Game<'_>) {
    let mut guard = SHARED.lock().unwrap_or_else(|e| e.into_inner());
    let Some(s) = guard.as_mut().filter(|s| s.enabled) else {
        return;
    };
    report_membership(game, s);
    let (written, overflowed) = crate::world::meridian_changes::take();
    if overflowed {
        s.membership = 0;
        report_membership(game, s);
    }
    for (cx, cz) in written {
        let c = ChunkChange::Changed([cx, cz]);
        if s.changes.last() != Some(&c) {
            s.changes.push(c);
        }
    }
    answer_requests(game, s);
    s.window = game.native_physical_window_size();
    s.debug_shown = game.debug;
    s.world_spawn = game.spawn.map(|p| [p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32]);
    if s.want_entities {
        s.entities = snapshot_entities(game);
    }
    if let Some(r) = s.roof_radius {
        s.under_roof = under_roof_check(game, r);
    }
    if s.resources.is_none() {
        s.resources = Some(game.options.resources.clone());
    }
    s.player_biome = game.native_player().and_then(|p| {
        let (x, y, z) = (p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32);
        let chunk = game.world.chunks.get(&(x >> 4, z >> 4))?;
        Some(game.res.biomes().get(chunk.biome(x & 15, y, z & 15)).name.clone())
    });
}

fn report_membership(game: &super::Game<'_>, s: &mut Shared) {
    let chunks = &game.world.chunks;
    let now = chunks.membership_revision();
    if now == s.membership {
        return;
    }
    let deltas = (s.membership != 0)
        .then(|| chunks.deltas_since(s.membership))
        .flatten();
    match deltas {
        Some(deltas) => {
            use horizon_client::chunk_map::ChunkMembershipDelta as D;
            for (_, delta) in deltas {
                s.changes.push(match delta {
                    D::Added(k) => ChunkChange::Loaded([k.0, k.1]),
                    D::Removed(k) => ChunkChange::Unloaded([k.0, k.1]),
                    D::Reset => ChunkChange::Reset,
                });
            }
        }
        None => {
            s.changes.push(ChunkChange::Reset);
            for key in chunks.keys() {
                s.changes.push(ChunkChange::Loaded([key.0, key.1]));
            }
        }
    }
    s.membership = now;
}

fn answer_requests(game: &super::Game<'_>, s: &mut Shared) {
    if s.requests.is_empty() {
        return;
    }
    let started = Instant::now();
    let dimension = game
        .native_player()
        .map(|p| p.dimension.to_owned())
        .unwrap_or_default();
    let facts = game.res.facts();
    let biomes = game.res.biomes();
    let mut done = 0;
    for &(pos, cave_from) in &s.requests {
        done += 1;
        let Some(chunk) = game.world.chunks.get(&(pos[0], pos[1])) else {
            continue;
        };
        let snapshot = read_chunk(chunk, facts, biomes, &dimension, pos, cave_from, s.hide, &mut s.blocks, &mut s.biomes);
        s.results.push(snapshot);
        if started.elapsed() >= READ_BUDGET {
            break;
        }
    }
    s.requests.drain(..done);
}

#[allow(clippy::too_many_arguments)]
fn read_chunk(
    chunk: &horizon_client::chunk::Chunk,
    facts: &horizon_client::blocks::BlockFacts,
    biome_table: &horizon_client::biome::Biomes,
    dimension: &str,
    pos: [i32; 2],
    cave_from: Option<i32>,
    hide: Hide,
    block_looks: &mut HashMap<String, BlockLook>,
    biome_looks: &mut HashMap<String, BiomeLook>,
) -> Snapshot {
    use horizon_client::blocks::{Fluid, MAP_COLORS};
    use horizon_world::registry::BlockStateId;

    let mut blocks: Vec<String> = Vec::new();
    let mut block_index: HashMap<u32, u16> = HashMap::new();
    let mut biomes: Vec<String> = Vec::new();
    let mut biome_index: HashMap<u16, u16> = HashMap::new();
    let mut intern_block = |state: u32| -> u16 {
        *block_index.entry(state).or_insert_with(|| {
            let f = facts.get(BlockStateId(state));
            let name = f.name.to_string();
            block_looks.entry(name.clone()).or_insert_with(|| BlockLook {
                map_color: MAP_COLORS.get(f.map_color as usize).copied().unwrap_or(0),
                fluid: match f.fluid {
                    Fluid::None => 0,
                    Fluid::Water => 1,
                    Fluid::Lava => 2,
                },
                air: facts.is_air(BlockStateId(state)),
            });
            blocks.push(name);
            (blocks.len() - 1) as u16
        })
    };
    let mut intern_biome = |id: u16| -> u16 {
        *biome_index.entry(id).or_insert_with(|| {
            let b = biome_table.get(id);
            biome_looks.entry(b.name.clone()).or_insert(BiomeLook {
                grass: b.grass,
                foliage: b.foliage,
                dry_foliage: b.dry_foliage,
                water: b.water,
            });
            biomes.push(b.name.clone());
            (biomes.len() - 1) as u16
        })
    };

    let air = facts.air.0;
    let bottom = chunk.min_y();
    // The highest section that holds anything but air.
    let surface_top = (0..chunk.sections.len())
        .rev()
        .find(|&i| chunk.sections.get(i).is_some_and(|s| !s.is_uniform(air)))
        .map_or(bottom, |i| bottom + i as i32 * 16 + 15);
    let hidden: std::cell::RefCell<HashMap<u32, bool>> = Default::default();
    let short: std::cell::RefCell<HashMap<u32, bool>> = Default::default();
    let shows = |state: u32| {
        let f = facts.get(BlockStateId(state));
        let visible = f.map_color != 0 || f.fluid != Fluid::None;
        visible
            && !(hide.any()
                && *hidden
                    .borrow_mut()
                    .entry(state)
                    .or_insert_with(|| hide.hides(&f.name.to_string())))
    };
    let is_fluid = |state: u32| facts.get(BlockStateId(state)).fluid != Fluid::None;

    let mut columns = Vec::with_capacity(256);
    for z in 0..16 {
        for x in 0..16 {
            let mut y = match cave_from {
                None => surface_top,
                Some(ceiling) => {
                    // Under a roof: skip the ceiling down to the first air.
                    let mut y = ceiling.min(surface_top);
                    while y > bottom && shows(chunk.get(x, y, z)) {
                        y -= 1;
                    }
                    y
                }
            };
            while y > bottom && !shows(chunk.get(x, y, z)) {
                y -= 1;
            }
            if y <= bottom && !shows(chunk.get(x, y, z)) {
                columns.push(Column { empty: true, height: bottom as i16, ..Column::default() });
                continue;
            }
            let mut fluid = None;
            let top_state = chunk.get(x, y, z);
            if is_fluid(top_state) {
                let surface = y;
                while y > bottom && is_fluid(chunk.get(x, y, z)) {
                    y -= 1;
                }
                let depth = (surface - y).clamp(1, 255) as u8;
                fluid = Some((intern_block(top_state), depth));
                while y > bottom && !shows(chunk.get(x, y, z)) {
                    y -= 1;
                }
            }
            let state = chunk.get(x, y, z);
            let light_y = fluid.map_or(y, |(_, d)| y + d as i32) + 1;
            let lowered = hide.lower_short
                && fluid.is_none()
                && *short
                    .borrow_mut()
                    .entry(state)
                    .or_insert_with(|| Hide::short(&facts.get(BlockStateId(state)).name.to_string()));
            columns.push(Column {
                block: intern_block(state),
                height: y as i16 - i16::from(lowered),
                fluid,
                biome: intern_biome(chunk.biome(x, y, z)),
                light: chunk.light(x, light_y, z),
                empty: false,
            });
        }
    }
    Snapshot { dimension: dimension.to_owned(), pos, blocks, biomes, columns }
}

fn snapshot_entities(game: &super::Game<'_>) -> Vec<EntitySnap> {
    let t = game.clock.fraction();
    let lerp = |a: f64, b: f64| a + (b - a) * t;
    let mut out = Vec::new();
    for m in game.mobs.iter() {
        if m.removed || !m.alive() || m.kind.id.ends_with("armor_stand") {
            continue;
        }
        let player = game.remote_player_uuid(m.id).is_some();
        let class = if player {
            EntityClass::Player
        } else if m.hostile() {
            EntityClass::Hostile
        } else {
            EntityClass::Friendly
        };
        let name = if player {
            game.remote_player_name(m.id).map(str::to_owned)
        } else {
            m.custom_name.clone()
        };
        out.push(EntitySnap {
            id: m.id,
            kind: m.kind.id.clone(),
            class,
            pos: [
                lerp(m.previous.x, m.position.x),
                lerp(m.previous.y, m.position.y),
                lerp(m.previous.z, m.position.z),
            ],
            invisible: m.invisible || m.has_effect("minecraft:invisibility"),
            name,
        });
    }
    for item in game.dropped.iter() {
        out.push(EntitySnap {
            id: item.id,
            kind: "minecraft:item".to_owned(),
            class: EntityClass::Item,
            pos: [
                lerp(item.previous.x, item.position.x),
                lerp(item.previous.y, item.position.y),
                lerp(item.previous.z, item.position.z),
            ],
            invisible: false,
            name: None,
        });
    }
    // Vehicles: the radar's "other" entities.
    let vehicles = game
        .boats
        .iter()
        .map(|b| (b.id, "minecraft:boat", b.previous, b.position))
        .chain(game.carts.iter().map(|c| (c.id, "minecraft:minecart", c.previous, c.position)));
    for (id, kind, previous, position) in vehicles {
        out.push(EntitySnap {
            id,
            kind: kind.to_owned(),
            class: EntityClass::Other,
            pos: [lerp(previous.x, position.x), lerp(previous.y, position.y), lerp(previous.z, position.z)],
            invisible: false,
            name: None,
        });
    }
    out
}

/// Every column within `r` of the player's has a block that blocks light
/// above the player's eyes (`can_occlude`), up to the top of its chunk.
fn under_roof_check(game: &super::Game<'_>, r: u8) -> bool {
    use horizon_world::registry::BlockStateId;
    let Some(p) = game.native_player() else { return false };
    let (px, pz) = (p.x.floor() as i32, p.z.floor() as i32);
    let eye = (p.y + 1.62).floor() as i32;
    let facts = game.res.facts();
    let r = r as i32;
    for dx in -r..=r {
        for dz in -r..=r {
            let (x, z) = (px + dx, pz + dz);
            let Some(chunk) = game.world.chunks.get(&(x >> 4, z >> 4)) else { return false };
            let top = chunk.max_y();
            let covered = (eye + 1..top).any(|y| facts.get(BlockStateId(chunk.get(x & 15, y, z & 15))).can_occlude);
            if !covered {
                return false;
            }
        }
    }
    true
}
