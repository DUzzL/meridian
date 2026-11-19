// What Meridian reads from the game, through the public mod API only.
//
// - Chunk events: `client_chunk_load` and `client_chunk_unload` (on a server
//   and in single player alike), block writes from a block part on every
//   block (`on_block_state_change`, `horizon:class/block`), and a sweep of
//   the loaded chunks around the player when a level comes or events were
//   lost.
// - Chunk reads: a top-down copy of a loaded chunk through `LevelRef`
//   (`block_state`, `fluid_state`, `client_biome`, `brightness`), from the
//   heightmap down, with names instead of internal ids.
// - Block and biome looks: map colours, fluids, water colours.
// - Per-frame state: the player's biome, the world spawn, entities for the
//   radar, the roof check of the cave mode, the GUI size.
//
// The hooks write here; the adapter reads plain data, so nothing holds a
// reference into the game. Everything is off until `enable` is called.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use mod_api::{
    BlockState, ClientHost, Heightmap, HostBlockStates, LevelRef, LightLayer, MobCategory,
};

/// Main-thread time per frame for chunk reads.
pub const READ_BUDGET: Duration = Duration::from_micros(1500);
/// Chunks either side of the player looked at when the loaded set must be
/// found again (no listing of a client's chunks exists).
const SWEEP_RADIUS: i32 = 34;
/// Entities further than this (blocks, horizontally) are not read.
const ENTITY_RANGE: f64 = 160.0;
/// How long a notice stays, and how long it fades at the end.
const NOTICE_FOR: Duration = Duration::from_millis(3000);
const NOTICE_FADE: Duration = Duration::from_millis(600);

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
    /// Eased from the last tick's position to the new one, to this frame.
    pub pos: [f64; 3],
    /// Where the last tick put it, not interpolated.
    pub tick_pos: [f64; 3],
    pub invisible: bool,
    /// A player's name or a custom name.
    pub name: Option<String>,
    /// A player's skin texture under `textures/` (the default skin of their
    /// id: the API gives no downloaded skins).
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

/// What a block state is, read once per state.
#[derive(Clone, Debug)]
struct Facts {
    name: String,
    map_color: u32,
    /// 0 none, 1 water, 2 lava, 3 another fluid; `None` until read at a
    /// position (the API answers fluids by position).
    fluid: Option<u8>,
    air: bool,
}

/// Where an entity was at the last two ticks, for easing between them.
#[derive(Clone, Copy, Debug)]
struct Track {
    from: [f64; 3],
    to: [f64; 3],
    since: Instant,
    seen: u64,
}

#[derive(Default)]
pub struct Shared {
    pub enabled: bool,
    pub gui_size: [f32; 2],
    /// A short notice over the hotbar (the API has no client action bar).
    pub notice: Option<(String, Instant)>,
    /// The chunk set must be found again by a sweep.
    pub sweep: bool,
    /// The dimension chunk events and reads are for.
    pub dimension: Option<String>,
    /// Chunks to read: position and the cave ceiling (`None` = surface).
    pub requests: Vec<([i32; 2], Option<i32>)>,
    pub results: Vec<Snapshot>,
    pub changes: Vec<ChunkChange>,
    pub blocks: HashMap<String, BlockLook>,
    pub biomes: HashMap<String, BiomeLook>,
    /// The biome at the player's feet, refreshed each frame.
    pub player_biome: Option<String>,
    /// The debug overlay (F3) is open, as followed from the F3 key.
    pub debug_shown: bool,
    f3_down: bool,
    f3_combined: bool,
    /// The world spawn the server announced (the Overworld's).
    pub world_spawn: Option<[i32; 3]>,
    /// Entities are copied each frame only while someone wants them.
    pub want_entities: bool,
    /// Shared with the readers, so a frame's copy is made once.
    pub entities: std::sync::Arc<[EntitySnap]>,
    tracks: HashMap<u32, Track>,
    frame: u64,
    /// Columns either side of the player a roof must cover (cave mode), or
    /// none when nobody asks.
    pub roof_radius: Option<u8>,
    pub under_roof: bool,
    /// Blocks the map looks through: flowers, redstone parts.
    pub hide: Hide,
    facts: HashMap<BlockState, Facts>,
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
        s.sweep = true;
    });
}

// ----------------------------------------------------------------- events --

/// `client_chunk_load`: a column arrived (or arrived again).
pub fn chunk_loaded(dimension: &str, chunk: [i32; 2]) {
    with(|s| {
        if s.enabled && s.dimension.as_deref() == Some(dimension) {
            s.changes.push(ChunkChange::Loaded(chunk));
        }
    });
}

/// `client_chunk_unload`: a column left the client.
pub fn chunk_unloaded(dimension: &str, chunk: [i32; 2]) {
    with(|s| {
        if s.enabled && s.dimension.as_deref() == Some(dimension) {
            s.changes.push(ChunkChange::Unloaded(chunk));
        }
    });
}

/// A block of the client's world changed (the block part's notice).
pub fn block_changed(pos: [i32; 3]) {
    let chunk = [pos[0] >> 4, pos[2] >> 4];
    with(|s| {
        if !s.enabled {
            return;
        }
        let change = ChunkChange::Changed(chunk);
        // Writes come in runs over one chunk (a fill, a section update).
        if s.changes.last() != Some(&change) {
            if s.changes.len() >= 65_536 {
                s.changes.clear();
                s.changes.push(ChunkChange::Reset);
                s.sweep = true;
            } else {
                s.changes.push(change);
            }
        }
    });
}

/// The client's level changed (joined, a new dimension): everything is
/// found again on the next frame.
pub fn level_changed() {
    with(|s| {
        s.dimension = None;
        s.requests.clear();
        s.tracks.clear();
        s.debug_shown = false;
        s.sweep = true;
    });
}

/// Follows the F3 key as vanilla does: a press and release with no other key
/// in between opens or closes the debug overlay.
pub fn key_event(key: &str, pressed: bool) {
    with(|s| {
        if key == "key.keyboard.f3" {
            if pressed {
                s.f3_down = true;
                s.f3_combined = false;
            } else if s.f3_down {
                s.f3_down = false;
                if !s.f3_combined {
                    s.debug_shown = !s.debug_shown;
                }
            }
        } else if pressed && s.f3_down {
            s.f3_combined = true;
        }
    });
}

// ------------------------------------------------------------- the frame --

/// Once a frame, with the host: finds the chunk set when needed, answers the
/// chunk reads the map asked for and refreshes the per-frame state.
pub fn frame(host: &mut dyn ClientHost) {
    let host: &dyn ClientHost = host;
    let enabled = with(|s| {
        s.frame += 1;
        s.enabled
    });
    if !enabled {
        return;
    }
    let Some(player) = host.local_player() else {
        return;
    };
    let entity = player.entity();
    let Some(dimension) = player.dimension(host).map(|d| d.to_string()) else {
        return;
    };
    let Some(position) = entity.position(host) else {
        return;
    };
    let Some(level) = host.level(&dimension) else {
        return;
    };
    let feet = position.map(|v| v.floor() as i32);
    let sweep = with(|s| {
        if s.dimension.as_deref() != Some(dimension.as_str()) {
            s.dimension = Some(dimension.clone());
            s.sweep = true;
        }
        std::mem::take(&mut s.sweep)
    });
    if sweep {
        let found = loaded_around(host, level, feet);
        with(|s| {
            s.changes.push(ChunkChange::Reset);
            s.changes.extend(found.into_iter().map(ChunkChange::Loaded));
        });
    }
    answer_requests(host, level, &dimension);

    let biome = level.client_biome(host, feet);
    let spawn = host
        .level("minecraft:overworld")
        .and_then(|l| l.respawn_data(host))
        .map(|r| r.pos);
    let (want_entities, roof) = with(|s| {
        s.player_biome = biome;
        if spawn.is_some() && spawn != s.world_spawn {
            log::info!("meridian: world spawn {:?}", spawn);
            s.world_spawn = spawn;
        }
        (s.want_entities, s.roof_radius)
    });
    if want_entities {
        let own = entity.id();
        let list = snapshot_entities(host, level, position, own);
        with(|s| s.entities = list.into());
    }
    if let Some(r) = roof {
        let under = under_roof_check(host, level, position, r);
        with(|s| s.under_roof = under);
    }
}

/// The chunks around `feet` the client holds (a block of each is loaded).
fn loaded_around(host: &dyn ClientHost, level: LevelRef, feet: [i32; 3]) -> Vec<[i32; 2]> {
    let (cx, cz) = (feet[0] >> 4, feet[2] >> 4);
    let y = level
        .height(host)
        .map_or(feet[1], |(min, height)| feet[1].clamp(min, min + height - 1));
    let mut out = Vec::new();
    for dz in -SWEEP_RADIUS..=SWEEP_RADIUS {
        for dx in -SWEEP_RADIUS..=SWEEP_RADIUS {
            let (x, z) = (cx + dx, cz + dz);
            if level.is_loaded(host, [x * 16, y, z * 16]) {
                out.push([x, z]);
            }
        }
    }
    out
}

fn answer_requests(host: &dyn ClientHost, level: LevelRef, dimension: &str) {
    let requests = with(|s| s.requests.clone());
    if requests.is_empty() {
        return;
    }
    let started = Instant::now();
    let (bottom, top) = level
        .height(host)
        .map_or((-64, 320), |(min, height)| (min, min + height));
    let mut done = 0;
    let mut results = Vec::new();
    for &(pos, cave_from) in &requests {
        done += 1;
        if !level.is_loaded(host, [pos[0] * 16, bottom, pos[1] * 16]) {
            continue;
        }
        let Some(mut snapshot) =
            read_chunk(host, level, dimension, pos, cave_from, bottom, top)
        else {
            continue;
        };
        read_biome_border(host, level, &mut snapshot);
        results.push(snapshot);
        if started.elapsed() >= READ_BUDGET {
            break;
        }
    }
    with(|s| {
        // Requests asked since are kept; the ones answered (or not loaded) go.
        let answered: Vec<_> = requests[..done].to_vec();
        s.requests.retain(|r| !answered.contains(r));
        s.results.extend(results);
    });
}

/// What `state` is; `pos` is where it stands, for its fluid.
fn facts(host: &dyn ClientHost, level: LevelRef, state: BlockState, pos: [i32; 3]) -> Facts {
    let cached = with(|s| s.facts.get(&state).cloned());
    if let Some(f) = &cached
        && f.fluid.is_some()
    {
        return f.clone();
    }
    let mut f = cached.unwrap_or_else(|| {
        let name = state.block().id(host);
        let air = matches!(
            name.as_str(),
            "minecraft:air" | "minecraft:cave_air" | "minecraft:void_air"
        );
        Facts {
            map_color: crate::core::color::map_color(
                host.block_states().map_color(state).unwrap_or(0),
            ),
            name,
            fluid: None,
            air,
        }
    });
    f.fluid = level.fluid_state(host, pos).map(|fluid| {
        match fluid.fluid.id(host).as_str() {
            "minecraft:empty" => 0,
            "minecraft:water" | "minecraft:flowing_water" => 1,
            "minecraft:lava" | "minecraft:flowing_lava" => 2,
            _ => 3,
        }
    });
    with(|s| {
        s.facts.insert(state, f.clone());
    });
    f
}

#[allow(clippy::too_many_arguments)]
fn read_chunk(
    host: &dyn ClientHost,
    level: LevelRef,
    dimension: &str,
    pos: [i32; 2],
    cave_from: Option<i32>,
    bottom: i32,
    top: i32,
) -> Option<Snapshot> {
    let hide = with(|s| s.hide);
    let (x0, z0) = (pos[0] * 16, pos[1] * 16);
    let columns_at: Vec<(i32, i32)> = (0..256).map(|i| (x0 + (i % 16), z0 + (i / 16))).collect();
    let heights = host
        .client_surface_heights(&columns_at, Heightmap::WorldSurface)
        .ok()?;
    if heights.iter().all(Option::is_none) {
        return None;
    }

    let mut blocks: Vec<String> = Vec::new();
    let mut block_index: HashMap<String, u16> = HashMap::new();
    let mut biomes: Vec<String> = Vec::new();
    let mut biome_index: HashMap<String, u16> = HashMap::new();
    let mut looks: Vec<(String, BlockLook)> = Vec::new();
    let mut intern_block = |name: &str, look: BlockLook| -> u16 {
        if let Some(&i) = block_index.get(name) {
            return i;
        }
        looks.push((name.to_owned(), look));
        blocks.push(name.to_owned());
        let i = (blocks.len() - 1) as u16;
        block_index.insert(name.to_owned(), i);
        i
    };
    let mut new_biomes: Vec<String> = Vec::new();
    let mut intern_biome = |name: String| -> u16 {
        if let Some(&i) = biome_index.get(&name) {
            return i;
        }
        new_biomes.push(name.clone());
        biomes.push(name.clone());
        let i = (biomes.len() - 1) as u16;
        biome_index.insert(name, i);
        i
    };
    let mut hidden: HashMap<String, bool> = HashMap::new();
    let mut short: HashMap<String, bool> = HashMap::new();
    let read = |p: [i32; 3]| -> Option<Facts> {
        let state = level.block_state(host, p)?;
        Some(facts(host, level, state, p))
    };
    let mut shows = |f: &Facts| {
        let visible = f.map_color != 0 || f.fluid.unwrap_or(0) != 0;
        visible
            && !(hide.any()
                && *hidden
                    .entry(f.name.clone())
                    .or_insert_with(|| hide.hides(&f.name)))
    };
    let is_fluid = |f: &Facts| f.fluid.unwrap_or(0) != 0;
    let air = Facts {
        name: "minecraft:air".to_owned(),
        map_color: 0,
        fluid: Some(0),
        air: true,
    };

    let mut columns = Vec::with_capacity(256);
    for (i, &(x, z)) in columns_at.iter().enumerate() {
        let surface_top = heights[i].map_or(bottom, |h| (h - 1).clamp(bottom, top - 1));
        let at = |y: i32| read([x, y, z]).unwrap_or_else(|| air.clone());
        let mut y = match cave_from {
            None => surface_top,
            Some(ceiling) => {
                // Under a roof: skip the ceiling down to the first air.
                let mut y = ceiling.min(surface_top);
                while y > bottom && shows(&at(y)) {
                    y -= 1;
                }
                y
            }
        };
        while y > bottom && !shows(&at(y)) {
            y -= 1;
        }
        let mut top_facts = at(y);
        if y <= bottom && !shows(&top_facts) {
            columns.push(Column {
                empty: true,
                height: bottom as i16,
                ..Column::default()
            });
            continue;
        }
        let mut fluid = None;
        if is_fluid(&top_facts) {
            let surface = y;
            while y > bottom && is_fluid(&at(y)) {
                y -= 1;
            }
            let depth = (surface - y).clamp(1, 255) as u8;
            // The water over waterlogged blocks (kelp, seagrass, stairs): the
            // map draws the water, not the block.
            let waterlogged = top_facts.fluid == Some(1) && top_facts.name != "minecraft:water";
            let index = if waterlogged {
                intern_block(
                    "minecraft:water",
                    BlockLook {
                        map_color: crate::core::color::map_color(12),
                        fluid: 1,
                        air: false,
                    },
                )
            } else {
                intern_block(&top_facts.name, look_of(&top_facts))
            };
            fluid = Some((index, depth));
            while y > bottom && !shows(&at(y)) {
                y -= 1;
            }
            top_facts = at(y);
        }
        let light_y = fluid.map_or(y, |(_, d)| y + i32::from(d)) + 1;
        let lowered = hide.lower_short
            && fluid.is_none()
            && *short
                .entry(top_facts.name.clone())
                .or_insert_with(|| Hide::short(&top_facts.name));
        let biome = level
            .client_biome(host, [x, y, z])
            .unwrap_or_else(|| "minecraft:plains".to_owned());
        let sky = level.brightness(host, LightLayer::Sky, [x, light_y, z]).unwrap_or(15);
        let block = level.brightness(host, LightLayer::Block, [x, light_y, z]).unwrap_or(0);
        columns.push(Column {
            block: intern_block(&top_facts.name, look_of(&top_facts)),
            height: y as i16 - i16::from(lowered),
            fluid,
            biome: intern_biome(biome),
            light: ((sky.clamp(0, 15) as u8) << 4) | block.clamp(0, 15) as u8,
            empty: false,
        });
    }
    learn_biomes(host, level, &new_biomes);
    with(|s| {
        for (name, look) in looks {
            s.blocks.entry(name).or_insert(look);
        }
    });
    Some(Snapshot {
        dimension: dimension.to_owned(),
        pos,
        cave_from,
        blocks,
        biomes,
        columns,
        biome_border: Vec::new(),
    })
}

fn look_of(f: &Facts) -> BlockLook {
    BlockLook {
        map_color: f.map_color,
        fluid: f.fluid.unwrap_or(0),
        air: f.air,
    }
}

/// Keeps the tints of biomes not seen before: water from the level's biome
/// table, grass and foliage from Meridian's vanilla table (the API gives no
/// biome grass or foliage colour).
fn learn_biomes(host: &dyn ClientHost, level: LevelRef, names: &[String]) {
    use crate::core::seedatlas_look::{BiomeLooks, VanillaLooks};
    let unknown: Vec<&String> =
        with(|s| names.iter().filter(|n| !s.biomes.contains_key(*n)).collect());
    if unknown.is_empty() {
        return;
    }
    let learned: Vec<(String, BiomeLook)> = unknown
        .into_iter()
        .map(|name| {
            let short = name.strip_prefix("minecraft:").unwrap_or(name);
            let vanilla = VanillaLooks;
            let water = level
                .biome_effects(host, name)
                .map_or_else(|| vanilla.water_color(short), |e| e.water);
            (
                name.clone(),
                BiomeLook {
                    grass: vanilla.grass_color(short, 0, 0),
                    foliage: vanilla.foliage_color(short),
                    dry_foliage: 0xA0A69C,
                    water: water & 0xFF_FFFF,
                },
            )
        })
        .collect();
    with(|s| {
        for (name, look) in learned {
            s.biomes.entry(name).or_insert(look);
        }
    });
}

/// Read the tint halo at the surface height of the nearest centre column.
/// Missing neighbours fall back to the edge until their load triggers a reread.
fn read_biome_border(host: &dyn ClientHost, level: LevelRef, snapshot: &mut Snapshot) {
    let mut palette: HashMap<String, u16> = snapshot
        .biomes
        .iter()
        .enumerate()
        .map(|(i, name)| (name.clone(), i as u16))
        .collect();
    let mut new_biomes = Vec::new();
    snapshot.biome_border.reserve(20 * 20);
    for z in -2i32..18 {
        for x in -2i32..18 {
            let col = snapshot.columns[z.clamp(0, 15) as usize * 16 + x.clamp(0, 15) as usize];
            // Inside the chunk the column already carries its biome.
            if (0..16).contains(&x) && (0..16).contains(&z) {
                snapshot.biome_border.push(col.biome);
                continue;
            }
            let p = [
                snapshot.pos[0] * 16 + x,
                i32::from(col.height),
                snapshot.pos[1] * 16 + z,
            ];
            let id = match level.client_biome(host, p) {
                Some(name) => match palette.get(&name) {
                    Some(&i) => i,
                    None => {
                        snapshot.biomes.push(name.clone());
                        new_biomes.push(name.clone());
                        let i = (snapshot.biomes.len() - 1) as u16;
                        palette.insert(name, i);
                        i
                    }
                },
                None => col.biome,
            };
            snapshot.biome_border.push(id);
        }
    }
    learn_biomes(host, level, &new_biomes);
}

/// The entity's custom name: the name it shows when that is not its kind's.
fn custom_name(host: &dyn ClientHost, kind: &str, name: Option<String>) -> Option<String> {
    let name = name?;
    let key = format!("entity.{}", kind.replace(':', "."));
    match host.translate(&key) {
        Some(default) if default == name => None,
        _ => Some(name),
    }
}

fn snapshot_entities(
    host: &dyn ClientHost,
    level: LevelRef,
    at: [f64; 3],
    own: u32,
) -> Vec<EntitySnap> {
    let r = ENTITY_RANGE;
    let Some(list) = level.entities_in(host, [at[0] - r, at[1] - r, at[2] - r], [
        at[0] + r,
        at[1] + r,
        at[2] + r,
    ]) else {
        return Vec::new();
    };
    // Players' ids for their default skins, by name.
    let uuids: HashMap<String, u128> = host
        .client_session()
        .client_tab_list()
        .map(|t| t.entries.into_iter().map(|e| (e.name, e.uuid)).collect())
        .unwrap_or_default();
    let (frame, now) = (with(|s| s.frame), Instant::now());
    let mut out = Vec::new();
    for e in list {
        let id = e.id();
        if id == own {
            continue;
        }
        let Some(kind) = e.kind(host).map(|k| k.id(host)) else {
            continue;
        };
        let Some(tick_pos) = e.position(host) else {
            continue;
        };
        let short = kind.strip_prefix("minecraft:").unwrap_or(&kind);
        let vehicle = short.ends_with("boat") || short.ends_with("raft") || short.contains("minecart");
        let class = if kind == "minecraft:player" {
            EntityClass::Player
        } else if kind == "minecraft:item" {
            EntityClass::Item
        } else if vehicle || short == "armor_stand" {
            // Armor stands are no living things to the radar: "other" entities.
            EntityClass::Other
        } else {
            match e.classification(host) {
                Some(MobCategory::Monster) => EntityClass::Hostile,
                // Projectiles, frames, orbs and the like are not shown.
                Some(MobCategory::Misc) | None => continue,
                Some(_) => EntityClass::Friendly,
            }
        };
        let name = e.name(host);
        let (name, skin) = match class {
            EntityClass::Player => {
                let skin = name
                    .as_ref()
                    .and_then(|n| uuids.get(n))
                    .map(|&u| crate::core::radar::default_skin(u).to_owned());
                (name, skin)
            }
            EntityClass::Item | EntityClass::Other => (None, None),
            _ => (custom_name(host, &kind, name), None),
        };
        let living = matches!(class, EntityClass::Friendly | EntityClass::Hostile);
        let pos = with(|s| {
            let t = s.tracks.entry(id).or_insert(Track {
                from: tick_pos,
                to: tick_pos,
                since: now,
                seen: frame,
            });
            if t.to != tick_pos {
                // A new tick: ease from where it is drawn now.
                let shown = ease(t, now);
                *t = Track {
                    from: shown,
                    to: tick_pos,
                    since: now,
                    seen: frame,
                };
            }
            t.seen = frame;
            ease(t, now)
        });
        out.push(EntitySnap {
            id,
            kind: kind.clone(),
            class,
            pos,
            tick_pos,
            invisible: e.is_invisible(host).unwrap_or(false),
            name,
            skin,
            tamed: living
                && host
                    .living(e)
                    .and_then(|l| l.is_tamed(host))
                    .unwrap_or(false),
        });
    }
    with(|s| s.tracks.retain(|_, t| t.seen == frame));
    out
}

/// Where a track is drawn at `now`: from its last shown place to its tick
/// position over one tick.
fn ease(t: &Track, now: Instant) -> [f64; 3] {
    let k = (now.duration_since(t.since).as_secs_f64() / 0.05).clamp(0.0, 1.0);
    [0, 1, 2].map(|i| t.from[i] + (t.to[i] - t.from[i]) * k)
}

/// Every column within `r` of the player's has something over the player's
/// eyes that hides the sky.
fn under_roof_check(host: &dyn ClientHost, level: LevelRef, p: [f64; 3], r: u8) -> bool {
    let (px, pz) = (p[0].floor() as i32, p[2].floor() as i32);
    let eye = (p[1] + 1.62).floor() as i32;
    let r = i32::from(r);
    for dx in -r..=r {
        for dz in -r..=r {
            if level.can_see_sky(host, [px + dx, eye + 1, pz + dz]) != Some(false) {
                return false;
            }
        }
    }
    true
}

// ---------------------------------------------------------------- readers --

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
            s.tracks.clear();
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

/// The world spawn the server announced, once it has.
pub fn world_spawn() -> Option<[i32; 3]> {
    with(|s| s.world_spawn)
}

/// Whether the debug overlay (F3) is open.
pub fn debug_shown() -> bool {
    with(|s| s.debug_shown)
}

/// Forgets cached looks (resources reloaded).
pub fn clear_looks() {
    with(|s| {
        s.blocks.clear();
        s.biomes.clear();
        s.facts.clear();
    });
}

pub fn gui_size() -> [f32; 2] {
    with(|s| s.gui_size)
}

/// Shows `text` over the hotbar for a few seconds.
pub fn notice(text: String) {
    with(|s| s.notice = Some((text, Instant::now())));
}

/// The notice to draw now and its opacity (0..=1).
pub fn current_notice() -> Option<(String, f32)> {
    with(|s| {
        let (text, at) = s.notice.as_ref()?;
        let age = at.elapsed();
        if age >= NOTICE_FOR {
            s.notice = None;
            return None;
        }
        let left = NOTICE_FOR - age;
        let alpha = if left < NOTICE_FADE {
            left.as_secs_f32() / NOTICE_FADE.as_secs_f32()
        } else {
            1.0
        };
        Some((text.clone(), alpha))
    })
}

/// The GUI's size in GUI pixels, as the last draw call or resize told it.
pub fn set_gui_size(size: [f32; 2]) {
    if size[0] > 0.0 && size[1] > 0.0 {
        with(|s| s.gui_size = size);
    }
}
