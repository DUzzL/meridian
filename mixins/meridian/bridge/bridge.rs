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
pub const VERSION: u32 = 1;
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
        let snapshot = read_chunk(chunk, facts, biomes, &dimension, pos, cave_from, &mut s.blocks, &mut s.biomes);
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
    let shows = |state: u32| {
        let f = facts.get(BlockStateId(state));
        f.map_color != 0 || f.fluid != Fluid::None
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
            columns.push(Column {
                block: intern_block(state),
                height: y as i16,
                fluid,
                biome: intern_biome(chunk.biome(x, y, z)),
                light: chunk.light(x, light_y, z),
                empty: false,
            });
        }
    }
    Snapshot { dimension: dimension.to_owned(), pos, blocks, biomes, columns }
}
