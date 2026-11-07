//! Seed-only vanilla terrain for the "terrain look" of unexplored map areas:
//! a port of Seed Atlas's `OverworldTerrainSampler` and `NetherCaveMask`
//! onto Horizon's own 26.3 world generation.
//!
//! Nothing here touches a live world. The Overworld sampler builds its own
//! generator from the seed (`VanillaTerrain`, the same binding the game uses
//! for a world, without structures) and asks one of its workers for a noise
//! column per sample (`GeneratorWorker::surface_column`): the final density at
//! block resolution through the aquifer, top down, with no chunk, carver,
//! feature or structure. The Nether sampler needs no generator at all: the
//! carvers are pure geometry from the seed, so the Nether cave carver
//! (`worldgen/carvers.json`, `minecraft:nether_cave`) is ported here. It
//! follows Horizon's port of vanilla's cave carver (`worldgen/carver.rs`,
//! MIT) step by step, because Horizon's crates are not reachable by name from
//! a native mod, only through `horizon_client`.

// Nothing in the adapter calls the samplers yet.
#![allow(dead_code)]

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::time::Duration;

use horizon_client::assets::Assets;
use horizon_client::biome::{BiomeInfo, Biomes};
use horizon_client::game::meridian_bridge as bridge;
use horizon_client::mod_api::BlockPos;
use horizon_client::streaming::ChunkGenerator;
use horizon_client::vanillagen::VanillaTerrain;

/// Samples per tile side, and the largest output side, as in the Java.
const TILE_SIZE: usize = 16;
const MAX_DIMENSION: usize = 262;
/// 16x16 samples each, about 3 KB: enough for a dozen detailed map tiles.
const MAX_CACHED_TILES: usize = 4096;
const MAX_CONTEXTS: usize = 2;
/// The Overworld's lowest block, its sea level and the first block of the top
/// slide: the density is fixed from here up, so every block is air.
const MIN_Y: i32 = -64;
const SEA_LEVEL: i32 = 63;
const TOP_SLIDE_START: i32 = 256;
/// Workers kept per generator between tiles.
const MAX_IDLE_WORKERS: usize = 4;

/// Row-major arrays of one sampled area (`OverworldTerrainSampler.SurfaceGrid`).
/// Heights are the highest non-air base block Y (fluid included), floors the
/// highest solid Y, colours 0xRRGGBB or -1 where unknown.
#[derive(Clone, Debug, PartialEq)]
pub struct SurfaceGrid {
    pub heights: Vec<f32>,
    pub floors: Vec<f32>,
    pub colors: Vec<i32>,
}

fn valid_step(step: i32) -> bool {
    matches!(step, 1 | 2 | 4 | 8 | 16)
}

fn valid_area(origin_x: i32, origin_z: i32, step: i32, width: usize, height: usize) -> bool {
    valid_step(step)
        && (1..=MAX_DIMENSION).contains(&width)
        && (1..=MAX_DIMENSION).contains(&height)
        && i64::from(origin_x) + width as i64 * i64::from(step) - 1 <= i64::from(i32::MAX)
        && i64::from(origin_z) + height as i64 * i64::from(step) - 1 <= i64::from(i32::MAX)
}

/// A map with a least-recently-used eviction (`LinkedHashMap` in access order).
struct Lru<K, V> {
    entries: HashMap<K, (V, u64)>,
    order: BTreeMap<u64, K>,
    tick: u64,
    capacity: usize,
}

impl<K: Clone + Eq + std::hash::Hash, V: Clone> Lru<K, V> {
    fn new(capacity: usize) -> Self {
        Self { entries: HashMap::new(), order: BTreeMap::new(), tick: 0, capacity }
    }

    fn get(&mut self, key: &K) -> Option<V> {
        let tick = self.tick + 1;
        let (value, used) = self.entries.get_mut(key)?;
        self.order.remove(used);
        *used = tick;
        self.order.insert(tick, key.clone());
        self.tick = tick;
        Some(value.clone())
    }

    fn insert(&mut self, key: K, value: V) {
        if let Some((_, used)) = self.entries.remove(&key) {
            self.order.remove(&used);
        }
        if self.entries.len() >= self.capacity
            && let Some((_, eldest)) = self.order.pop_first()
        {
            self.entries.remove(&eldest);
        }
        self.tick += 1;
        self.order.insert(self.tick, key.clone());
        self.entries.insert(key, (value, self.tick));
    }
}

// ---------------------------------------------------------------------------
// Overworld
// ---------------------------------------------------------------------------

/// Samples the Overworld's base terrain, row-major, at
/// `origin + index * step + (step - 1) / 2` on both axes (steps 1/2/4/8/16;
/// sides 1..=262). `None` when `still_current` turned false meanwhile, when the
/// arguments are out of range, or when Horizon's resources are not available.
///
/// Like the Java, this follows the base columns of vanilla's noise terrain:
/// no surface rules, carvers, structures, features, ice or snow layers. The
/// colours infer the surface material from the surface biome and temperature.
pub fn sample_overworld(
    seed: i64,
    large_biomes: bool,
    origin_x: i32,
    origin_z: i32,
    step: i32,
    width: usize,
    height: usize,
    still_current: &dyn Fn() -> bool,
) -> Option<SurfaceGrid> {
    if !valid_area(origin_x, origin_z, step, width, height) {
        return None;
    }
    let world = WorldKey { seed, large_biomes };
    let mut result = SurfaceGrid {
        heights: vec![0.0; width * height],
        floors: vec![0.0; width * height],
        colors: vec![0; width * height],
    };
    let span = TILE_SIZE as i64 * i64::from(step);
    let mut last: Option<(TileKey, Arc<Tile>)> = None;
    for z in 0..height {
        if !still_current() {
            return None;
        }
        let block_z = i64::from(origin_z) + z as i64 * i64::from(step) + i64::from((step - 1) / 2);
        for x in 0..width {
            let block_x =
                i64::from(origin_x) + x as i64 * i64::from(step) + i64::from((step - 1) / 2);
            let key = TileKey {
                world,
                step,
                x: block_x.div_euclid(span) as i32,
                z: block_z.div_euclid(span) as i32,
                phase_x: block_x.rem_euclid(i64::from(step)) as i32,
                phase_z: block_z.rem_euclid(i64::from(step)) as i32,
            };
            let tile = match &last {
                Some((k, tile)) if *k == key => tile.clone(),
                _ => {
                    let tile = tile(key, still_current)?;
                    last = Some((key, tile.clone()));
                    tile
                }
            };
            let source = (block_z.rem_euclid(span) / i64::from(step)) as usize * TILE_SIZE
                + (block_x.rem_euclid(span) / i64::from(step)) as usize;
            let target = z * width + x;
            result.heights[target] = tile.heights[source];
            result.floors[target] = tile.floors[source];
            result.colors[target] = tile.colors[source];
        }
    }
    Some(result)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct WorldKey {
    seed: i64,
    large_biomes: bool,
}

/// A tile of 16x16 samples `step` blocks apart, starting at
/// `x * 16 * step + phase_x`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct TileKey {
    world: WorldKey,
    step: i32,
    x: i32,
    z: i32,
    phase_x: i32,
    phase_z: i32,
}

struct Tile {
    heights: [f32; TILE_SIZE * TILE_SIZE],
    floors: [f32; TILE_SIZE * TILE_SIZE],
    colors: [i32; TILE_SIZE * TILE_SIZE],
}

struct TileState {
    cache: Lru<TileKey, Arc<Tile>>,
    /// Tiles some thread is generating: others wait for them.
    generating: HashSet<TileKey>,
}

fn tiles() -> &'static (Mutex<TileState>, Condvar) {
    static TILES: OnceLock<(Mutex<TileState>, Condvar)> = OnceLock::new();
    TILES.get_or_init(|| {
        (
            Mutex::new(TileState { cache: Lru::new(MAX_CACHED_TILES), generating: HashSet::new() }),
            Condvar::new(),
        )
    })
}

/// Gives the tile back to the waiters when generation ends, however it ends.
struct Generating(TileKey);

impl Drop for Generating {
    fn drop(&mut self) {
        let (lock, wake) = tiles();
        lock.lock().unwrap_or_else(|e| e.into_inner()).generating.remove(&self.0);
        wake.notify_all();
    }
}

fn tile(key: TileKey, still_current: &dyn Fn() -> bool) -> Option<Arc<Tile>> {
    let (lock, wake) = tiles();
    let mut state = lock.lock().unwrap_or_else(|e| e.into_inner());
    loop {
        if let Some(tile) = state.cache.get(&key) {
            return Some(tile);
        }
        if !still_current() {
            return None;
        }
        if state.generating.insert(key) {
            break;
        }
        // Another worker is generating it: wait, but keep watching for staleness.
        state = wake.wait_timeout(state, Duration::from_millis(50)).unwrap_or_else(|e| e.into_inner()).0;
    }
    drop(state);
    let guard = Generating(key);
    let generated = context(key.world)?.generate(&key, still_current);
    if let Some(tile) = &generated {
        let mut state = lock.lock().unwrap_or_else(|e| e.into_inner());
        state.cache.insert(key, tile.clone());
    }
    drop(guard);
    generated
}

/// Horizon's prepared resources, loaded once: the block registry the
/// generator is bound to and the biome table (also the source of the colours).
struct Environment {
    root: PathBuf,
    assets: &'static Assets,
    biomes: Arc<Biomes>,
}

fn resources_dir() -> Option<PathBuf> {
    bridge::resources_dir().or_else(|| std::env::var_os("MERIDIAN_RESOURCES").map(PathBuf::from))
}

fn environment() -> Option<Arc<Environment>> {
    static ENVIRONMENT: Mutex<Option<Arc<Environment>>> = Mutex::new(None);
    let root = resources_dir()?;
    let mut slot = ENVIRONMENT.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(env) = slot.as_ref().filter(|env| env.root == root) {
        return Some(env.clone());
    }
    match load_environment(&root) {
        Ok(env) => {
            let env = Arc::new(env);
            *slot = Some(env.clone());
            Some(env)
        }
        Err(e) => {
            log::warn!("Meridian: terrain sampler resources at {}: {e}", root.display());
            None
        }
    }
}

fn load_environment(root: &Path) -> Result<Environment, String> {
    // A registry is leaked per `Assets::load`, and loading one resizes the
    // chunks' global palette (as the game did once at start). Load one for
    // the process, and put the palette's width back to what the game has.
    static ASSETS: OnceLock<Option<Assets>> = OnceLock::new();
    let assets = ASSETS
        .get_or_init(|| {
            let bits = horizon_client::chunk::block_strategy().global_bits;
            let assets = Assets::load(root).ok();
            if horizon_client::chunk::block_strategy().global_bits != bits {
                horizon_client::chunk::set_block_states(1usize << bits);
            }
            assets
        })
        .as_ref()
        .ok_or("block registry")?;
    let biomes = Biomes::load(root).map_err(|e| e.to_string())?;
    Ok(Environment { root: root.to_owned(), assets, biomes: Arc::new(biomes) })
}

/// One seed's generator, with the workers sampling it.
struct Context {
    terrain: Arc<VanillaTerrain>,
    biomes: Arc<Biomes>,
    workers: Mutex<Vec<<VanillaTerrain as ChunkGenerator>::Worker>>,
}

fn context(world: WorldKey) -> Option<Arc<Context>> {
    static CONTEXTS: Mutex<Option<Lru<WorldKey, Arc<Context>>>> = Mutex::new(None);
    let mut contexts = CONTEXTS.lock().unwrap_or_else(|e| e.into_inner());
    let contexts = contexts.get_or_insert_with(|| Lru::new(MAX_CONTEXTS));
    if let Some(context) = contexts.get(&world) {
        return Some(context);
    }
    let env = environment()?;
    let preset = if world.large_biomes { "minecraft:large_biomes" } else { "minecraft:normal" };
    let terrain = VanillaTerrain::for_world(
        &env.root.join("worldgen"),
        env.assets.registry,
        &env.biomes,
        world.seed,
        preset,
        "minecraft:overworld",
        false,
    );
    let terrain = match terrain {
        Ok(terrain) => Arc::new(terrain),
        Err(e) => {
            log::warn!("Meridian: terrain sampler generator: {e}");
            return None;
        }
    };
    let context = Arc::new(Context {
        terrain,
        biomes: env.biomes.clone(),
        workers: Mutex::new(Vec::new()),
    });
    contexts.insert(world, context.clone());
    Some(context)
}

/// How a tile's columns are sampled.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ColumnSource {
    /// Every column on its own, with the full aquifer (slow: each column
    /// evaluates the density lattice around it again).
    Single,
    /// One shared density lattice for the whole tile (Horizon's far
    /// sampling at full detail): the same cell interpolation the game
    /// generates with, at a fraction of the work; aquifer pockets are left
    /// out, sea level water is kept.
    Lattice,
    /// The shared lattice, fine only in the band around the surface (found
    /// on a coarse lattice first): the surface the same, far less density
    /// work in the deep rock and the sky.
    Band,
}

const COLUMN_SOURCE: ColumnSource = ColumnSource::Band;

const fn columns_len() -> usize {
    TILE_SIZE * TILE_SIZE
}

impl Context {
    fn generate(&self, key: &TileKey, still_current: &dyn Fn() -> bool) -> Option<Arc<Tile>> {
        let pooled = self.workers.lock().unwrap_or_else(|e| e.into_inner()).pop();
        let mut worker = pooled.unwrap_or_else(|| self.terrain.worker());
        let tile = self.fill(&mut worker, key, still_current, COLUMN_SOURCE);
        let mut idle = self.workers.lock().unwrap_or_else(|e| e.into_inner());
        if idle.len() < MAX_IDLE_WORKERS {
            idle.push(worker);
        }
        tile.map(Arc::new)
    }

    fn fill(
        &self,
        worker: &mut <VanillaTerrain as ChunkGenerator>::Worker,
        key: &TileKey,
        still_current: &dyn Fn() -> bool,
        source: ColumnSource,
    ) -> Option<Tile> {
        let states = self.terrain.generator_data().block_states().clone();
        let origin_x = i64::from(key.x) * TILE_SIZE as i64 * i64::from(key.step) + i64::from(key.phase_x);
        let origin_z = i64::from(key.z) * TILE_SIZE as i64 * i64::from(key.step) + i64::from(key.phase_z);
        let columns = TILE_SIZE * TILE_SIZE;
        let mut tops = [MIN_Y - 1; TILE_SIZE * TILE_SIZE];
        let mut floors = [MIN_Y - 1; TILE_SIZE * TILE_SIZE];
        let mut fluid = [false; TILE_SIZE * TILE_SIZE];
        let mut blocks = [[0i32; 2]; TILE_SIZE * TILE_SIZE];
        if !still_current() {
            return None;
        }
        let (ox, oz, step) = (origin_x as i32, origin_z as i32, key.step);
        let spans_of: Vec<_> = match source {
            ColumnSource::Lattice | ColumnSource::Band => {
                let band = source == ColumnSource::Band;
                let columns = worker.lod_grid(ox, oz, step, TILE_SIZE, band).ok()?;
                if columns.len() != columns_len() {
                    return None;
                }
                columns.into_iter().map(|(_, spans)| spans).collect()
            }
            ColumnSource::Single => (0..columns_len())
                .map(|i| worker.surface_column(ox + (i % TILE_SIZE) as i32 * step, oz + (i / TILE_SIZE) as i32 * step).1)
                .collect(),
        };
        for (index, spans) in spans_of.into_iter().enumerate() {
            let block_x = (origin_x + (index % TILE_SIZE) as i64 * i64::from(key.step)) as i32;
            let block_z = (origin_z + (index / TILE_SIZE) as i64 * i64::from(key.step)) as i32;
            blocks[index] = [block_x, block_z];
            // The runs of non-air blocks, half-open; the top slide's air
            // above `TOP_SLIDE_START` is never looked at.
            let (mut top, mut top_fluid) = (None::<i32>, false);
            let mut floor = None::<i32>;
            for (low, high, state) in spans {
                let high = high.min(TOP_SLIDE_START);
                if low >= high {
                    continue;
                }
                let y = high - 1;
                let is_fluid = states.get(state).fluid;
                if top.is_none_or(|t| y > t) {
                    top = Some(y);
                    top_fluid = is_fluid;
                }
                if !is_fluid && floor.is_none_or(|f| y > f) {
                    floor = Some(y);
                }
            }
            if let Some(top) = top {
                tops[index] = top;
                fluid[index] = top_fluid;
            }
            if let Some(floor) = floor {
                floors[index] = floor;
            }
        }
        // `getNoiseBiome` at the surface's quart, unzoomed.
        let quarts: Vec<[i32; 3]> =
            (0..columns).map(|i| [blocks[i][0] >> 2, tops[i] >> 2, blocks[i][1] >> 2]).collect();
        let names = worker.noise_biomes(&quarts);
        let mut tile = Tile {
            heights: [0.0; TILE_SIZE * TILE_SIZE],
            floors: [0.0; TILE_SIZE * TILE_SIZE],
            colors: [-1; TILE_SIZE * TILE_SIZE],
        };
        for index in 0..columns {
            tile.heights[index] = tops[index] as f32;
            tile.floors[index] = floors[index] as f32;
            tile.colors[index] = names
                .get(index)
                .and_then(|name| name.as_deref())
                .filter(|name| self.biomes.contains(name))
                .map_or(-1, |name| {
                    let id = self.biomes.id(name);
                    surface_color(
                        &self.biomes,
                        id,
                        name,
                        blocks[index][0],
                        tops[index],
                        blocks[index][1],
                        fluid[index],
                        SEA_LEVEL,
                        key.step,
                    )
                });
        }
        Some(tile)
    }
}

// --- Surface colours (`BiomeSurfacePalette`) --------------------------------

/// Texture brightness of grass_block_top and water_still.
const GRASS_TEXTURE: f32 = 0.62;
const WATER_TEXTURE: f32 = 0.70;
const SNOW: i32 = 0xE8EFF0;
const SAND: i32 = 0xD8CF9B;
const STONE: i32 = 0x858585;
const TERRACOTTA: i32 = 0xB06B48;
const MYCELIUM: i32 = 0x7A6D72;
const MUD: i32 = 0x514B40;
const CROWN_CELL: i32 = 5;
const CROWN_SALT: i64 = 0x5EED_A71A5;

/// `FoliageColor.FOLIAGE_*` without alpha.
const FOLIAGE_EVERGREEN: i32 = 0x619961;
const FOLIAGE_BIRCH: i32 = 0x80A755;
const FOLIAGE_MANGROVE: i32 = 0x92C648;

/// Leaf tint source: a fixed colour, or the biome's own foliage colour.
struct Canopy {
    fixed: Option<i32>,
    texture: f32,
    density: f32,
}

fn canopy(name: &str) -> Option<Canopy> {
    let foliage = |texture, density| Canopy { fixed: None, texture, density };
    let fixed = |color, texture, density| Canopy { fixed: Some(color), texture, density };
    Some(match name.strip_prefix("minecraft:")? {
        "forest" => foliage(0.60, 0.55),
        "flower_forest" => foliage(0.60, 0.35),
        "dappled_forest" => foliage(0.60, 0.45),
        "windswept_forest" => foliage(0.55, 0.45),
        "dark_forest" => foliage(0.50, 0.90),
        "birch_forest" => fixed(FOLIAGE_BIRCH, 0.62, 0.55),
        "old_growth_birch_forest" => fixed(FOLIAGE_BIRCH, 0.62, 0.60),
        "taiga" => fixed(FOLIAGE_EVERGREEN, 0.42, 0.55),
        "old_growth_pine_taiga" => fixed(FOLIAGE_EVERGREEN, 0.42, 0.70),
        "old_growth_spruce_taiga" => fixed(FOLIAGE_EVERGREEN, 0.42, 0.75),
        "jungle" => foliage(0.62, 0.85),
        "bamboo_jungle" => foliage(0.62, 0.75),
        "sparse_jungle" => foliage(0.62, 0.30),
        "savanna" => foliage(0.62, 0.12),
        "savanna_plateau" => foliage(0.62, 0.15),
        "windswept_savanna" => foliage(0.62, 0.08),
        "wooded_badlands" => foliage(0.60, 0.25),
        "swamp" => foliage(0.60, 0.15),
        "mangrove_swamp" => fixed(FOLIAGE_MANGROVE, 0.55, 0.75),
        // Cherry and pale oak leaves are not tinted: texture averages.
        "cherry_grove" => fixed(0xE5ADC2, 0.95, 0.50),
        "pale_garden" => fixed(0x757A73, 1.05, 0.85),
        _ => return None,
    })
}

/// `OverworldTerrainSampler.surfaceColor`: the top colour of a base-terrain
/// column at one sample.
#[allow(clippy::too_many_arguments)]
fn surface_color(
    biomes: &Biomes,
    id: u16,
    name: &str,
    x: i32,
    y: i32,
    z: i32,
    fluid: bool,
    sea_level: i32,
    step: i32,
) -> i32 {
    let info = biomes.get(id);
    let ground = ground(biomes, info, id, name, x, y, z, fluid, sea_level);
    // Snow layers also cover leaves.
    let canopy = if fluid || ground == SNOW { None } else { canopy(name) };
    match canopy {
        None => ground,
        Some(canopy) => {
            let leaf = leaf(info, &canopy);
            crowns(ground, leaf, canopy.density, x, z, step)
        }
    }
}

/// Colour of the visible block between trees: water, snow, sand, stone or tinted grass.
#[allow(clippy::too_many_arguments)]
fn ground(
    biomes: &Biomes,
    info: &BiomeInfo,
    id: u16,
    name: &str,
    x: i32,
    y: i32,
    z: i32,
    fluid: bool,
    sea_level: i32,
) -> i32 {
    if fluid {
        return scale((info.water & 0xFF_FFFF) as i32, WATER_TEXTURE);
    }
    // `Biome.coldEnoughToSnow`: the height-adjusted temperature is below 0.15.
    if info.temperature_at(BlockPos::new(x, y, z), sea_level) < 0.15 {
        return SNOW;
    }
    match name {
        "minecraft:desert" | "minecraft:beach" => SAND,
        "minecraft:stony_shore" | "minecraft:stony_peaks" => STONE,
        "minecraft:badlands" | "minecraft:eroded_badlands" | "minecraft:wooded_badlands" => TERRACOTTA,
        "minecraft:mushroom_fields" => MYCELIUM,
        "minecraft:mangrove_swamp" => MUD,
        _ => scale((biomes.grass(id, f64::from(x), f64::from(z)) & 0xFF_FFFF) as i32, GRASS_TEXTURE),
    }
}

/// Leaf colour of a forest biome's crowns.
fn leaf(info: &BiomeInfo, canopy: &Canopy) -> i32 {
    let tint = canopy.fixed.unwrap_or((info.foliage & 0xFF_FFFF) as i32);
    scale(tint, canopy.texture)
}

/// Java's `Math.round(float)`: halves round up.
fn round(value: f32) -> i32 {
    let floor = value.floor();
    (if value - floor >= 0.5 { floor + 1.0 } else { floor }) as i32
}

fn scale(rgb: i32, factor: f32) -> i32 {
    let mut result = 0;
    for shift in [16, 8, 0] {
        let channel = round((rgb >> shift & 0xFF) as f32 * factor).clamp(0, 255);
        result |= channel << shift;
    }
    result
}

fn mix(from: i32, to: i32, amount: f32) -> i32 {
    let mut result = 0;
    for shift in [16, 8, 0] {
        let a = (from >> shift & 0xFF) as f32;
        let b = (to >> shift & 0xFF) as f32;
        result |= round(a + (b - a) * amount).clamp(0, 255) << shift;
    }
    result
}

/// `BiomeTerrainStyler.hash`.
fn hash(x: i32, z: i32, seed: i64) -> i64 {
    let mut h = seed
        ^ i64::from(x).wrapping_mul(0x9E37_79B9_7F4A_7C15_u64 as i64)
        ^ i64::from(z).wrapping_mul(0xC2B2_AE3D_27D4_EB4F_u64 as i64);
    h ^= ((h as u64) >> 32) as i64;
    h = h.wrapping_mul(0xD6E8_FEB8_6659_FD93_u64 as i64);
    h ^= ((h as u64) >> 32) as i64;
    h & i64::MAX
}

/// Tree crowns of radius 1.8..2.7 on a jittered 5-block grid, lit from the
/// north-west. Samples of 8+ blocks blend ground and leaves by coverage.
fn crowns(ground: i32, leaf: i32, density: f32, x: i32, z: i32, step: i32) -> i32 {
    if step >= 8 {
        return mix(ground, leaf, density);
    }
    let cell_x = x.div_euclid(CROWN_CELL);
    let cell_z = z.div_euclid(CROWN_CELL);
    // A crown covers about 0.6 of its cell.
    let threshold = round((density / 0.6).min(1.0) * 1000.0) as i64;
    for dz in -1..=1 {
        for dx in -1..=1 {
            let hash = hash(cell_x + dx, cell_z + dz, CROWN_SALT);
            if hash % 1000 >= threshold {
                continue;
            }
            let centre_x =
                ((cell_x + dx) * CROWN_CELL) as f32 + 0.5 + (((hash as u64) >> 12) % CROWN_CELL as u64) as f32;
            let centre_z =
                ((cell_z + dz) * CROWN_CELL) as f32 + 0.5 + (((hash as u64) >> 20) % CROWN_CELL as u64) as f32;
            let radius = 1.8 + (((hash as u64) >> 28) % 10) as f32 / 10.0;
            let offset_x = x as f32 + 0.5 - centre_x;
            let offset_z = z as f32 + 0.5 - centre_z;
            if offset_x * offset_x + offset_z * offset_z > radius * radius {
                continue;
            }
            let light = round(-(offset_x + offset_z) / radius * 9.0);
            return scale(leaf, 1.0 + light as f32 / 100.0);
        }
    }
    ground
}

// ---------------------------------------------------------------------------
// Nether cave mask
// ---------------------------------------------------------------------------

/// Two words per row-major sample (bit `y` of the 128-bit pair is set where a
/// Nether cave carver reaches that block), the geometry of the vanilla
/// carvers from the seed alone: `NetherCaveMask.area`. Steps 1/2/4 use the
/// lower block at the native fractional centre; steps 8/16 use the nearest
/// 4-block cell. An out-of-range argument gives an empty vector.
pub fn nether_cave_mask(
    seed: i64,
    origin_x: i32,
    origin_z: i32,
    step: i32,
    width: usize,
    height: usize,
) -> Vec<u64> {
    if !valid_area(origin_x, origin_z, step, width, height) {
        return Vec::new();
    }
    let mut result = vec![0u64; width * height * 2];
    let mut last: Option<((i32, i32), Arc<[u64; 512]>)> = None;
    for z in 0..height {
        let block_z = sample_coordinate(origin_z, z, step);
        for x in 0..width {
            let block_x = sample_coordinate(origin_x, x, step);
            let chunk = (block_x >> 4, block_z >> 4);
            let mask = match &last {
                Some((c, mask)) if *c == chunk => mask.clone(),
                _ => {
                    let mask = chunk_mask(seed, chunk.0, chunk.1);
                    last = Some((chunk, mask.clone()));
                    mask
                }
            };
            let source = (((block_z & 15) * 16 + (block_x & 15)) * 2) as usize;
            let target = (z * width + x) * 2;
            result[target] = mask[source];
            result[target + 1] = mask[source + 1];
        }
    }
    result
}

fn sample_coordinate(origin: i32, index: usize, step: i32) -> i32 {
    let block = i64::from(origin) + index as i64 * i64::from(step);
    (if step >= 8 { (block + i64::from(step / 2) + 2).div_euclid(4) * 4 } else { block + i64::from((step - 1) / 2) })
        as i32
}

const MAX_CACHED_CHUNKS: usize = 512;

fn chunk_mask(seed: i64, chunk_x: i32, chunk_z: i32) -> Arc<[u64; 512]> {
    static CACHE: Mutex<Option<Lru<(i64, i32, i32), Arc<[u64; 512]>>>> = Mutex::new(None);
    // Held while generating, as the Java's `synchronized`: the same chunk is
    // never carved twice at once.
    let mut cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    let cache = cache.get_or_insert_with(|| Lru::new(MAX_CACHED_CHUNKS));
    let key = (seed, chunk_x, chunk_z);
    if let Some(mask) = cache.get(&key) {
        return mask;
    }
    let mask = Arc::new(carve_chunk(seed, chunk_x, chunk_z));
    cache.insert(key, mask.clone());
    mask
}

/// The union of the Nether cave carver from the 17x17 chunks around the target.
fn carve_chunk(seed: i64, chunk_x: i32, chunk_z: i32) -> [u64; 512] {
    let mut out = ChunkMask { words: [0; 512] };
    for dx in -8..=8i32 {
        for dz in -8..=8i32 {
            let origin = (chunk_x.wrapping_add(dx), chunk_z.wrapping_add(dz));
            // The Nether's biomes all carry exactly one carver, at index 0.
            let mut random = large_feature_random(seed, origin);
            if random.next_float() <= nether_cave::PROBABILITY {
                nether_cave::carve(&mut random, (chunk_x, chunk_z), origin, &mut out);
            }
        }
    }
    out.words
}

/// Carved blocks of one chunk: two words per column, bit `y & 63` of word `y >> 6`.
struct ChunkMask {
    words: [u64; 512],
}

impl ChunkMask {
    /// The lowest and highest block a carver may reach (`CarverOutput`).
    const MIN_Y: i32 = 1;
    const MAX_Y: i32 = 120;

    fn carve(&mut self, local_x: i32, y: i32, local_z: i32) {
        let index = (((local_z & 15) * 16 + (local_x & 15)) * 2 + (y >> 6)) as usize;
        self.words[index] |= 1u64 << (y & 63);
    }
}

/// `WorldgenRandom.setLargeFeatureSeed` over a legacy random source.
fn large_feature_random(seed: i64, origin: (i32, i32)) -> LegacyRandom {
    let mut random = LegacyRandom::new(seed);
    let a = random.next_long();
    let b = random.next_long();
    random.set_seed(i64::from(origin.0).wrapping_mul(a) ^ i64::from(origin.1).wrapping_mul(b) ^ seed);
    random
}

/// `java.util.Random` as vanilla's `LegacyRandomSource`.
#[derive(Clone, Debug)]
struct LegacyRandom {
    state: u64,
}

impl LegacyRandom {
    const MULTIPLIER: u64 = 0x5_DEEC_E66D;
    const MASK: u64 = (1 << 48) - 1;

    fn new(seed: i64) -> Self {
        Self { state: (seed as u64 ^ Self::MULTIPLIER) & Self::MASK }
    }

    fn set_seed(&mut self, seed: i64) {
        *self = Self::new(seed);
    }

    fn bits(&mut self, count: u32) -> i32 {
        self.state = self.state.wrapping_mul(Self::MULTIPLIER).wrapping_add(11) & Self::MASK;
        (self.state >> (48 - count)) as i32
    }

    /// A bound that is not positive never happens with the carver's providers.
    fn next_int_bound(&mut self, bound: i32) -> i32 {
        if bound <= 0 {
            return 0;
        }
        if bound & (bound - 1) == 0 {
            return ((i64::from(bound) * i64::from(self.bits(31))) >> 31) as i32;
        }
        loop {
            let bits = self.bits(31);
            let value = bits % bound;
            // Java's signed overflow is part of the rejection rule.
            if bits.wrapping_sub(value).wrapping_add(bound - 1) >= 0 {
                return value;
            }
        }
    }

    fn next_long(&mut self) -> i64 {
        let high = i64::from(self.bits(32)) << 32;
        // Java sign-extends the low int before adding.
        high.wrapping_add(i64::from(self.bits(32)))
    }

    fn next_float(&mut self) -> f32 {
        self.bits(24) as f32 * (1.0 / 16_777_216.0)
    }

    fn next_int_between_inclusive(&mut self, min: i32, max: i32) -> i32 {
        self.next_int_bound(max.wrapping_sub(min).wrapping_add(1)).wrapping_add(min)
    }
}

/// `Mth.sin` / `Mth.cos`: vanilla's quantized sine table.
mod trig {
    use std::sync::OnceLock;

    const SCALE: f64 = 10430.378350470453;

    fn table() -> &'static [f32] {
        static SINE: OnceLock<Box<[f32]>> = OnceLock::new();
        SINE.get_or_init(|| (0..65536).map(|i| (f64::from(i) / SCALE).sin() as f32).collect())
    }

    pub fn sin(angle: f64) -> f32 {
        table()[((angle * SCALE) as i64 & 65535) as usize]
    }

    pub fn cos(angle: f64) -> f32 {
        table()[((angle * SCALE + 16384.0) as i64 & 65535) as usize]
    }
}

/// `minecraft:nether_cave` (`worldgen/carvers.json`): a `minecraft:cave`
/// carver. Its parameters are inlined; the generation context is the Nether's
/// (min Y 0, height 128, so `y` is uniform in 0..=126).
mod nether_cave {
    use super::{ChunkMask, LegacyRandom, trig};
    use std::f32::consts::{FRAC_PI_2, PI, TAU};

    pub const PROBABILITY: f32 = 0.2;
    /// `very_biased_to_bottom`, 0..=9.
    const COUNT_MAX: i32 = 9;
    /// Uniform from `absolute 0` to `below_top 1` in a 128-high dimension.
    const Y_MAX: i32 = 126;
    /// `trapezoid`: min 0, max 6, plateau 2.
    const THICKNESS: (f32, f32, f32) = (0.0, 6.0, 2.0);
    const ROOM_VERTICAL_RADIUS_MULTIPLIER: f32 = 0.5;
    const HORIZONTAL_RADIUS_MULTIPLIER: f32 = 1.0;
    const VERTICAL_RADIUS_MULTIPLIER: f32 = 1.0;
    const START_VERTICAL_RADIUS_MULTIPLIER: f32 = 5.0;
    const FLOOR_LEVEL: f32 = -0.7;
    /// Candidate cells one carve may test (Horizon's guard against runaway paths).
    const BUDGET: usize = 1 << 24;

    pub fn carve(random: &mut LegacyRandom, target: (i32, i32), origin: (i32, i32), out: &mut ChunkMask) {
        let mut value = random.next_int_bound(COUNT_MAX + 1);
        value = random.next_int_bound(value.wrapping_add(1));
        value = random.next_int_bound(value.wrapping_add(1));
        let count = value;
        let mut budget = BUDGET;
        for _ in 0..count {
            let x = f64::from(origin.0.wrapping_shl(4).wrapping_add(random.next_int_bound(16)));
            let y = f64::from(random.next_int_between_inclusive(0, Y_MAX));
            let z = f64::from(origin.1.wrapping_shl(4).wrapping_add(random.next_int_bound(16)));
            let mut run = Run {
                target,
                out: &mut *out,
                budget: &mut budget,
                horizontal: f64::from(HORIZONTAL_RADIUS_MULTIPLIER),
                vertical: f64::from(VERTICAL_RADIUS_MULTIPLIER),
                floor: f64::from(FLOOR_LEVEL),
            };
            let mut tunnels = 1;
            if random.next_int_bound(4) == 0 {
                let multiplier = f64::from(ROOM_VERTICAL_RADIUS_MULTIPLIER);
                let thickness = 1.0 + random.next_float() * 6.0;
                let radius = 1.5 + f64::from(trig::sin(f64::from(FRAC_PI_2)) * thickness);
                run.ellipsoid([x + 1.0, y, z], radius, radius * multiplier);
                tunnels += random.next_int_bound(4);
            }
            for _ in 0..tunnels {
                let yaw = random.next_float() * TAU;
                let pitch = (random.next_float() - 0.5) / 4.0;
                let (min, max, plateau) = THICKNESS;
                let range = max - min;
                let slope = (range - plateau) / 2.0;
                let middle = range - slope;
                let thickness = (min + random.next_float() * middle) + random.next_float() * slope;
                let end = 112 - random.next_int_bound(28);
                run.tunnel(Tunnel {
                    seed: random.next_long(),
                    position: [x, y, z],
                    thickness,
                    yaw,
                    pitch,
                    start: 0,
                    end,
                    vertical_scale: f64::from(START_VERTICAL_RADIUS_MULTIPLIER),
                });
            }
        }
    }

    struct Run<'a> {
        target: (i32, i32),
        out: &'a mut ChunkMask,
        budget: &'a mut usize,
        horizontal: f64,
        vertical: f64,
        floor: f64,
    }

    #[derive(Clone, Copy)]
    struct Tunnel {
        seed: i64,
        position: [f64; 3],
        thickness: f32,
        yaw: f32,
        pitch: f32,
        start: i32,
        end: i32,
        vertical_scale: f64,
    }

    /// `Mth.floor`.
    fn floor(value: f64) -> i32 {
        let i = value as i32;
        if value < f64::from(i) { i.wrapping_sub(1) } else { i }
    }

    /// Whether a path at (`x`, `z`) can still reach the target chunk.
    fn can_reach(chunk: (i32, i32), x: f64, z: f64, step: i32, end: i32, thickness: f32) -> bool {
        let dx = x - f64::from(chunk.0.wrapping_shl(4).wrapping_add(8));
        let dz = z - f64::from(chunk.1.wrapping_shl(4).wrapping_add(8));
        let remaining = f64::from(end.wrapping_sub(step));
        let radius = f64::from((thickness + 2.0) + 16.0);
        dx * dx + dz * dz - remaining * remaining <= radius * radius
    }

    impl Run<'_> {
        /// The ellipsoid's cells in the target chunk, X, then Z, then Y
        /// descending, less the cave's floor and corners.
        fn ellipsoid(&mut self, [x, y, z]: [f64; 3], horizontal: f64, vertical: f64) {
            let cx = self.target.0.wrapping_shl(4);
            let cz = self.target.1.wrapping_shl(4);
            let reach = 16.0 + horizontal * 2.0;
            if (x - f64::from(cx.wrapping_add(8))).abs() > reach || (z - f64::from(cz.wrapping_add(8))).abs() > reach {
                return;
            }
            let min_x = floor(x - horizontal).wrapping_sub(cx).wrapping_sub(1).max(0);
            let max_x = floor(x + horizontal).wrapping_sub(cx).min(15);
            let min_y = floor(y - vertical).wrapping_sub(1).max(ChunkMask::MIN_Y);
            let max_y = floor(y + vertical).wrapping_add(1).min(ChunkMask::MAX_Y);
            let min_z = floor(z - horizontal).wrapping_sub(cz).wrapping_sub(1).max(0);
            let max_z = floor(z + horizontal).wrapping_sub(cz).min(15);
            for lx in min_x..=max_x {
                let nx = (f64::from(cx.wrapping_add(lx)) + 0.5 - x) / horizontal;
                for lz in min_z..=max_z {
                    let nz = (f64::from(cz.wrapping_add(lz)) + 0.5 - z) / horizontal;
                    if nx * nx + nz * nz >= 1.0 {
                        continue;
                    }
                    let mut by = max_y;
                    while by > min_y {
                        if *self.budget == 0 {
                            return;
                        }
                        *self.budget -= 1;
                        let ny = (f64::from(by) - 0.5 - y) / vertical;
                        if !(ny <= self.floor || nx * nx + ny * ny + nz * nz >= 1.0) {
                            self.out.carve(lx, by, lz);
                        }
                        by -= 1;
                    }
                }
            }
        }

        fn tunnel(&mut self, mut path: Tunnel) {
            let mut random = LegacyRandom::new(path.seed);
            let split = random.next_int_bound(path.end / 2) + path.end / 4;
            let gentle = random.next_int_bound(6) == 0;
            let (mut yaw_change, mut pitch_change) = (0f32, 0f32);
            for step in path.start..path.end {
                let radius = 1.5 + f64::from(trig::sin(f64::from(PI * step as f32 / path.end as f32)) * path.thickness);
                let vertical = radius * path.vertical_scale;
                let cos_pitch = trig::cos(f64::from(path.pitch));
                path.position[0] += f64::from(trig::cos(f64::from(path.yaw)) * cos_pitch);
                path.position[1] += f64::from(trig::sin(f64::from(path.pitch)));
                path.position[2] += f64::from(trig::sin(f64::from(path.yaw)) * cos_pitch);
                path.pitch *= if gentle { 0.92 } else { 0.7 };
                path.pitch += pitch_change * 0.1;
                path.yaw += yaw_change * 0.1;
                pitch_change *= 0.9;
                yaw_change *= 0.75;
                pitch_change += (random.next_float() - random.next_float()) * random.next_float() * 2.0;
                yaw_change += (random.next_float() - random.next_float()) * random.next_float() * 4.0;
                if step == split && path.thickness > 1.0 {
                    let seed = random.next_long();
                    let thickness = random.next_float() * 0.5 + 0.5;
                    self.tunnel(Tunnel {
                        seed,
                        thickness,
                        yaw: path.yaw - FRAC_PI_2,
                        pitch: path.pitch / 3.0,
                        start: step,
                        vertical_scale: 1.0,
                        ..path
                    });
                    let seed = random.next_long();
                    let thickness = random.next_float() * 0.5 + 0.5;
                    self.tunnel(Tunnel {
                        seed,
                        thickness,
                        yaw: path.yaw + FRAC_PI_2,
                        pitch: path.pitch / 3.0,
                        start: step,
                        vertical_scale: 1.0,
                        ..path
                    });
                    return;
                }
                if random.next_int_bound(4) == 0 {
                    continue;
                }
                if !can_reach(self.target, path.position[0], path.position[2], step, path.end, path.thickness) {
                    return;
                }
                let (horizontal, vertical_radius) = (radius * self.horizontal, vertical * self.vertical);
                self.ellipsoid(path.position, horizontal, vertical_radius);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_random_matches_java() {
        let mut random = LegacyRandom::new(0);
        assert_eq!(random.bits(32), -1_155_484_576);
        assert_eq!(random.bits(32), -723_955_400);
        assert_eq!(LegacyRandom::new(0).next_long(), -4_962_768_465_676_381_896);
    }

    #[test]
    fn nether_sample_coordinates() {
        // Steps below 8 use the lower block at the native centre.
        assert_eq!(sample_coordinate(100, 3, 1), 103);
        assert_eq!(sample_coordinate(100, 3, 2), 106);
        assert_eq!(sample_coordinate(100, 3, 4), 113);
        // Steps 8 and 16 use the nearest 4-block cell.
        assert_eq!(sample_coordinate(0, 1, 8), 12);
        assert_eq!(sample_coordinate(-16, 0, 16), -8);
        assert_eq!(sample_coordinate(-3, 0, 8), 0);
        assert_eq!(sample_coordinate(-3, 1, 8), 8);
    }

    #[test]
    fn nether_mask_is_stable_and_bounded() {
        let a = nether_cave_mask(1234, -40, 25, 4, 6, 5);
        let b = nether_cave_mask(1234, -40, 25, 4, 6, 5);
        assert_eq!(a.len(), 6 * 5 * 2);
        assert_eq!(a, b);
        // Nothing is carved below block 2 or above 120 (word 1 holds 64..127).
        for pair in a.chunks(2) {
            assert_eq!(pair[0] & 0b11, 0);
            assert_eq!(pair[1] >> (121 - 64), 0);
        }
        assert!(nether_cave_mask(1, 0, 0, 3, 4, 4).is_empty());
        assert!(nether_cave_mask(1, 0, 0, 4, 0, 4).is_empty());
    }

    #[test]
    fn nether_mask_has_caves_and_follows_the_seed() {
        // A 64x64 area at step 1 holds carved cells for some seed in a handful.
        let carved = |seed: i64| {
            nether_cave_mask(seed, 0, 0, 1, 64, 64).iter().map(|w| w.count_ones()).sum::<u32>()
        };
        let counts: Vec<u32> = (0..4).map(carved).collect();
        assert!(counts.iter().any(|&n| n > 0), "{counts:?}");
        assert_ne!(
            nether_cave_mask(1, 0, 0, 1, 32, 32),
            nether_cave_mask(2, 0, 0, 1, 32, 32)
        );
    }

    #[test]
    fn colour_helpers_round_like_java() {
        assert_eq!(round(2.5), 3);
        assert_eq!(round(-2.5), -2);
        assert_eq!(round(0.499_999_97), 0);
        assert_eq!(scale(0xFF_FFFF, 0.5), 0x80_8080);
        assert_eq!(scale(0x10_2030, 2.0), 0x20_4060);
        assert_eq!(mix(0x00_0000, 0xFF_FFFF, 0.5), 0x80_8080);
        assert!(hash(3, -4, CROWN_SALT) >= 0);
    }

    #[test]
    fn overworld_arguments_are_checked() {
        let always = || true;
        assert!(sample_overworld(0, false, 0, 0, 3, 4, 4, &always).is_none());
        assert!(sample_overworld(0, false, 0, 0, 1, 263, 4, &always).is_none());
        assert!(sample_overworld(0, false, i32::MAX, 0, 1, 4, 4, &always).is_none());
        assert!(sample_overworld(0, false, 0, 0, 1, 4, 4, &|| false).is_none());
    }

    /// Needs Horizon's prepared resources: `MERIDIAN_RESOURCES=<.../26.3>
    /// cargo test -- --ignored overworld`.
    #[test]
    #[ignore = "needs Horizon's prepared resources"]
    fn overworld_samples_real_terrain() {
        let started = std::time::Instant::now();
        let grid = sample_overworld(1, false, 0, 0, 1, 16, 16, &|| true).expect("grid");
        eprintln!("first tile (generator build included): {:?}", started.elapsed());
        let started = std::time::Instant::now();
        let more = sample_overworld(1, false, 16, 0, 1, 16, 16, &|| true).expect("grid");
        eprintln!("one more 16x16 tile: {:?}", started.elapsed());
        assert!(grid.heights.iter().all(|h| (-65.0..256.0).contains(h)));
        assert!(grid.floors.iter().zip(&grid.heights).all(|(f, h)| f <= h));
        assert!(grid.colors.iter().all(|&c| c >= 0));
        assert_ne!(grid, more);
        // The cache answers the same area identically.
        assert_eq!(grid, sample_overworld(1, false, 0, 0, 1, 16, 16, &|| true).unwrap());
        // A step-4 grid agrees with the step-1 samples it is made of.
        let coarse = sample_overworld(1, false, 0, 0, 4, 4, 4, &|| true).expect("grid");
        assert_eq!(coarse.heights[0], grid.heights[1 * 16 + 1]);
        // A coarse view: one 16-step tile covers 256x256 blocks.
        let started = std::time::Instant::now();
        let wide = sample_overworld(7, true, -2048, -2048, 16, 64, 64, &|| true).expect("grid");
        eprintln!("64x64 samples at step 16 (16 tiles): {:?}", started.elapsed());
        let (low, high) = wide.heights.iter().fold((f32::MAX, f32::MIN), |(l, h), &v| (l.min(v), h.max(v)));
        let water = wide.heights.iter().zip(&wide.floors).filter(|(h, f)| h > f).count();
        eprintln!("heights {low}..{high}, {water} of 4096 columns under fluid");
        let mut colors = wide.colors.clone();
        colors.sort_unstable();
        colors.dedup();
        eprintln!("{} distinct colours, first {:06x?}", colors.len(), &colors[..colors.len().min(6)]);
    }

    #[test]
    #[ignore = "needs Horizon's prepared resources"]
    fn lattice_columns_match_single_columns() {
        let world = WorldKey { seed: 1, large_biomes: false };
        let context = context(world).expect("generator");
        let mut worker = context.terrain.worker();
        let mut timing = [std::time::Duration::ZERO; 3];
        let (mut columns, mut same_height, mut near_height, mut same_color) = (0, 0, 0, 0);
        let (mut band_height, mut band_near, mut band_color) = (0, 0, 0);
        for step in [1, 2, 4] {
            for t in 0..12 {
                let key = TileKey { world, step, x: t * 7 - 40, z: 13 - t * 5, phase_x: 0, phase_z: 0 };
                let started = std::time::Instant::now();
                let single = context.fill(&mut worker, &key, &|| true, ColumnSource::Single).unwrap();
                timing[0] += started.elapsed();
                let started = std::time::Instant::now();
                let lattice = context.fill(&mut worker, &key, &|| true, ColumnSource::Lattice).unwrap();
                timing[1] += started.elapsed();
                let started = std::time::Instant::now();
                let band = context.fill(&mut worker, &key, &|| true, ColumnSource::Band).unwrap();
                timing[2] += started.elapsed();
                for i in 0..TILE_SIZE * TILE_SIZE {
                    band_height += usize::from(single.heights[i] == band.heights[i]);
                    band_near += usize::from((single.heights[i] - band.heights[i]).abs() <= 1.0);
                    band_color += usize::from(single.colors[i] == band.colors[i]);
                    columns += 1;
                    same_height += usize::from(single.heights[i] == lattice.heights[i]);
                    near_height += usize::from((single.heights[i] - lattice.heights[i]).abs() <= 1.0);
                    same_color += usize::from(single.colors[i] == lattice.colors[i]);
                }
            }
        }
        eprintln!("single {:?}, lattice {:?} ({:.1}x)", timing[0], timing[1], timing[0].as_secs_f64() / timing[1].as_secs_f64());
        eprintln!("{columns} columns: same height {same_height}, within a block {near_height}, same colour {same_color}");
        eprintln!("band {:?} ({:.1}x): same height {band_height}, within a block {band_near}, same colour {band_color}", timing[2], timing[0].as_secs_f64() / timing[2].as_secs_f64());
        assert!(timing[1] < timing[0]);
        assert!(near_height * 100 >= columns * 97);
    }

    #[test]
    #[ignore = "timing"]
    fn nether_chunk_cost() {
        let started = std::time::Instant::now();
        let mask = nether_cave_mask(1, 0, 0, 1, 16, 16);
        eprintln!("one nether chunk mask: {:?}", started.elapsed());
        let started = std::time::Instant::now();
        let big = nether_cave_mask(1, 256, 256, 4, 64, 64);
        eprintln!("64x64 at step 4 (16 chunks): {:?}", started.elapsed());
        let carved = |m: &[u64]| m.iter().map(|w| w.count_ones()).sum::<u32>();
        eprintln!("carved cells: {} / {}", carved(&mask), carved(&big));
    }
}
