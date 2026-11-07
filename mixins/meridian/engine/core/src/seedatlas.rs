//! Seed Atlas inside Meridian: seeds per world or server, and the seed's
//! biome tiles and structure markers, computed on background threads.
//!
//! The seed never leaves the client: it is read from the singleplayer world
//! or entered with `/seedatlas seed`, and kept in Meridian's data folder.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc;

use meridian_seedatlas::atlas::{Hit, marker};
use meridian_seedatlas::structures::kind;
use meridian_seedatlas::{Atlas, Dimension};
use serde::{Deserialize, Serialize};

use crate::config::SeedAtlasSettings;
use crate::geom::RegionPos;
use crate::persist;
use crate::seedatlas_look::{self, BiomeLooks, HeightsRequest, TileInput, VanillaLooks};

/// Minecraft's seed parsing (`WorldOptions.parseSeed`): a number, else the
/// text's Java `hashCode`; blank is no seed.
pub fn parse_seed(input: &str) -> Option<i64> {
    let s = input.trim();
    if s.is_empty() {
        return None;
    }
    if let Ok(n) = s.parse::<i64>() {
        return Some(n);
    }
    let mut h: i32 = 0;
    for unit in s.encode_utf16() {
        h = h.wrapping_mul(31).wrapping_add(unit as i32);
    }
    Some(h as i64)
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SeedEntry {
    pub input: String,
    pub seed: i64,
    /// The world type when the seed was entered. Kept for older files; the
    /// effective setting is the current "World type" option (see
    /// `seedatlas_state::resolve_seed`).
    pub large_biomes: bool,
}

/// The seed of the Nether or the End when it differs from the base seed.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SeedValue {
    pub input: String,
    pub seed: i64,
}

/// Everything needed to generate one dimension: the seed, the world type
/// and the Minecraft version (`meridian_seedatlas::mc`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SeedSpec {
    pub seed: i64,
    pub large_biomes: bool,
    pub mc: i32,
}

/// The index of a dimension in per-dimension arrays: Overworld, Nether, End.
pub fn dim_index(d: Dimension) -> usize {
    match d {
        Dimension::Overworld => 0,
        Dimension::Nether => 1,
        Dimension::End => 2,
    }
}

/// Seeds by world or server (`persist::source_key`), in `seeds.toml`: the
/// base seed (`[seeds.<key>]`, the format of older releases) and the
/// optional own seeds of the Nether and the End (`[nether.<key>]`,
/// `[end.<key>]`). The base seed applies to every dimension without one.
#[derive(Debug, Default)]
pub struct SeedBook {
    path: PathBuf,
    entries: BTreeMap<String, SeedEntry>,
    nether: BTreeMap<String, SeedValue>,
    end: BTreeMap<String, SeedValue>,
    /// Contexts whose world seed must not be picked up again after
    /// `clear`, until the context changes. Not stored.
    suppressed: HashSet<String>,
}

#[derive(Default, Serialize, Deserialize)]
struct SeedFile {
    #[serde(default)]
    seeds: BTreeMap<String, SeedEntry>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    nether: BTreeMap<String, SeedValue>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    end: BTreeMap<String, SeedValue>,
}

impl SeedBook {
    pub fn load(data_dir: &Path) -> Self {
        let path = data_dir.join("seeds.toml");
        let file = std::fs::read_to_string(&path)
            .ok()
            .and_then(|t| toml::from_str::<SeedFile>(&t).ok())
            .unwrap_or_default();
        Self {
            path,
            entries: file.seeds,
            nether: file.nether,
            end: file.end,
            suppressed: HashSet::new(),
        }
    }

    /// The base seed of a context.
    pub fn get(&self, key: &str) -> Option<&SeedEntry> {
        self.entries.get(key)
    }

    /// Sets the base seed (and picks up the world's own seed again).
    pub fn set(&mut self, key: &str, entry: SeedEntry) {
        self.entries.insert(key.to_owned(), entry);
        self.suppressed.remove(key);
        self.save();
    }

    /// Removes the seeds of a context, the Nether's and End's own included;
    /// whether there were any.
    pub fn clear(&mut self, key: &str) -> bool {
        let had = self.entries.remove(key).is_some()
            | self.nether.remove(key).is_some()
            | self.end.remove(key).is_some();
        if had {
            self.save();
        }
        had
    }

    /// True when the context has a base seed or an own seed.
    pub fn has_any(&self, key: &str) -> bool {
        self.entries.contains_key(key)
            || self.nether.contains_key(key)
            || self.end.contains_key(key)
    }

    /// The own seed of the Nether or the End (never of the Overworld).
    pub fn dimension_seed(&self, key: &str, d: Dimension) -> Option<&SeedValue> {
        match d {
            Dimension::Overworld => None,
            Dimension::Nether => self.nether.get(key),
            Dimension::End => self.end.get(key),
        }
    }

    /// Gives the Nether or the End its own seed; the Overworld uses the
    /// base seed, so this refuses it.
    pub fn set_dimension_seed(&mut self, key: &str, d: Dimension, value: SeedValue) -> bool {
        let map = match d {
            Dimension::Overworld => return false,
            Dimension::Nether => &mut self.nether,
            Dimension::End => &mut self.end,
        };
        map.insert(key.to_owned(), value);
        self.save();
        true
    }

    /// Removes the own seed of the Nether or the End; whether there was one.
    pub fn clear_dimension_seed(&mut self, key: &str, d: Dimension) -> bool {
        let had = match d {
            Dimension::Overworld => false,
            Dimension::Nether => self.nether.remove(key).is_some(),
            Dimension::End => self.end.remove(key).is_some(),
        };
        if had {
            self.save();
        }
        had
    }

    /// Stops the world's own seed from being used for a context.
    pub fn suppress_detected(&mut self, key: &str) {
        self.suppressed.insert(key.to_owned());
    }

    /// Whether the world's own seed is ignored for a context.
    pub fn detected_suppressed(&self, key: &str) -> bool {
        self.suppressed.contains(key)
    }

    /// Forgets suppressions of other contexts when the open one changes.
    pub fn context_changed(&mut self, key: &str) {
        self.suppressed.retain(|k| k == key);
    }

    fn save(&self) {
        let file = SeedFile {
            seeds: self.entries.clone(),
            nether: self.nether.clone(),
            end: self.end.clone(),
        };
        match toml::to_string_pretty(&file) {
            Ok(text) => {
                if let Err(e) = persist::write_atomic(&self.path, text.as_bytes()) {
                    log::warn!("meridian: cannot save seeds: {e}");
                }
            }
            Err(e) => log::warn!("meridian: cannot encode seeds: {e}"),
        }
    }
}

// ---------------------------------------------------------------- markers --

/// One marker kind of the Fabric version (`MarkerType`): its id (also its
/// icon's file name), name, the structure kinds it shows, and whether it is
/// a helper (off by default).
#[derive(Clone, Copy, Debug)]
pub struct MarkerKind {
    pub id: &'static str,
    pub name: &'static str,
    pub kinds: &'static [i32],
    pub helper: bool,
}

pub const MARKERS: &[MarkerKind] = &[
    MarkerKind {
        id: "camp",
        name: "Abandoned Camp",
        kinds: &[kind::ABANDONED_CAMP],
        helper: false,
    },
    MarkerKind {
        id: "ancient_city",
        name: "Ancient City",
        kinds: &[kind::ANCIENT_CITY],
        helper: false,
    },
    MarkerKind {
        id: "bastion",
        name: "Bastion Remnant",
        kinds: &[kind::BASTION],
        helper: false,
    },
    MarkerKind {
        id: "chambers",
        name: "Trial Chambers",
        kinds: &[kind::TRIAL_CHAMBERS],
        helper: false,
    },
    MarkerKind {
        id: "desert",
        name: "Desert Pyramid",
        kinds: &[kind::DESERT_PYRAMID],
        helper: false,
    },
    MarkerKind {
        id: "endcity",
        name: "End City",
        kinds: &[kind::END_CITY],
        helper: false,
    },
    MarkerKind {
        id: "end_ship",
        name: "End Ship",
        kinds: &[marker::END_SHIP],
        helper: false,
    },
    MarkerKind {
        id: "fortress",
        name: "Nether Fortress",
        kinds: &[kind::FORTRESS],
        helper: false,
    },
    MarkerKind {
        id: "gateway",
        name: "End Gateway",
        kinds: &[kind::END_GATEWAY],
        helper: false,
    },
    MarkerKind {
        id: "geode",
        name: "Amethyst Geode",
        kinds: &[kind::GEODE],
        helper: true,
    },
    MarkerKind {
        id: "hut",
        name: "Swamp Hut",
        kinds: &[kind::SWAMP_HUT],
        helper: false,
    },
    MarkerKind {
        id: "igloo",
        name: "Igloo",
        kinds: &[kind::IGLOO],
        helper: false,
    },
    MarkerKind {
        id: "jungle",
        name: "Jungle Temple",
        kinds: &[kind::JUNGLE_TEMPLE],
        helper: false,
    },
    MarkerKind {
        id: "mansion",
        name: "Woodland Mansion",
        kinds: &[kind::MANSION],
        helper: false,
    },
    MarkerKind {
        id: "mineshaft",
        name: "Mineshaft",
        kinds: &[kind::MINESHAFT],
        helper: false,
    },
    MarkerKind {
        id: "monument",
        name: "Ocean Monument",
        kinds: &[kind::MONUMENT],
        helper: false,
    },
    MarkerKind {
        id: "orevein",
        name: "Ore Vein",
        kinds: &[marker::ORE_VEIN],
        helper: true,
    },
    MarkerKind {
        id: "outpost",
        name: "Pillager Outpost",
        kinds: &[kind::OUTPOST],
        helper: false,
    },
    MarkerKind {
        id: "portal",
        name: "Ruined Portal",
        kinds: &[kind::RUINED_PORTAL, kind::RUINED_PORTAL_NETHER],
        helper: false,
    },
    MarkerKind {
        id: "ruins",
        name: "Ocean Ruins",
        kinds: &[kind::OCEAN_RUIN],
        helper: false,
    },
    MarkerKind {
        id: "shipwreck",
        name: "Shipwreck",
        kinds: &[kind::SHIPWRECK],
        helper: false,
    },
    MarkerKind {
        id: "slime",
        name: "Slime Chunk",
        kinds: &[marker::SLIME_CHUNK],
        helper: true,
    },
    MarkerKind {
        id: "spawn",
        name: "World Spawn",
        kinds: &[marker::SPAWN],
        helper: true,
    },
    MarkerKind {
        id: "stronghold",
        name: "Stronghold",
        kinds: &[marker::STRONGHOLD],
        helper: false,
    },
    MarkerKind {
        id: "trails",
        name: "Trail Ruins",
        kinds: &[kind::TRAIL_RUINS],
        helper: false,
    },
    MarkerKind {
        id: "treasure",
        name: "Buried Treasure",
        kinds: &[kind::BURIED_TREASURE],
        helper: false,
    },
    MarkerKind {
        id: "village",
        name: "Village",
        kinds: &[kind::VILLAGE],
        helper: false,
    },
    MarkerKind {
        id: "well",
        name: "Desert Well",
        kinds: &[kind::DESERT_WELL],
        helper: true,
    },
];

pub fn marker_for(structure: i32) -> Option<&'static MarkerKind> {
    // Ore vein scans answer copper and iron veins.
    let structure = if structure == marker::ORE_VEIN_COPPER || structure == marker::ORE_VEIN_IRON {
        marker::ORE_VEIN
    } else {
        structure
    };
    MARKERS.iter().find(|m| m.kinds.contains(&structure))
}

pub fn marker_enabled(s: &SeedAtlasSettings, id: &str) -> bool {
    s.markers
        .get(id)
        .copied()
        .unwrap_or_else(|| MARKERS.iter().any(|m| m.id == id && !m.helper))
}

/// The engine's dimension for a game dimension id.
pub fn dimension_of(id: &str) -> Option<Dimension> {
    match id {
        "minecraft:overworld" => Some(Dimension::Overworld),
        "minecraft:the_nether" => Some(Dimension::Nether),
        "minecraft:the_end" => Some(Dimension::End),
        _ => None,
    }
}

pub fn dimension_of_kind(structure: i32) -> Dimension {
    match structure {
        marker::END_SHIP => Dimension::End,
        marker::STRONGHOLD | marker::SPAWN | marker::SLIME_CHUNK | marker::ORE_VEIN => {
            Dimension::Overworld
        }
        k => meridian_seedatlas::structures::config(k, meridian_seedatlas::mc::MC_NEWEST)
            .map_or(Dimension::Overworld, |c| c.dimension),
    }
}

/// The height biomes are sampled at in a dimension.
pub fn sample_y(s: &SeedAtlasSettings, d: Dimension) -> i32 {
    match d {
        Dimension::Overworld => s.overworld_y,
        Dimension::Nether => s.nether_y,
        Dimension::End => s.end_y,
    }
}

// ----------------------------------------------------------------- service --

/// What the service computes: biome tiles and markers, by region.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Job {
    /// One tile of biome colours: `TILE_SAMPLES` samples a side, `step`
    /// blocks apart, covering `tile_span(step)` blocks from the tile's
    /// north-west corner (`region` is the tile's index at that span). The
    /// preview comes first; `refine` adds the expensive terrain (the vanilla
    /// surface, the Nether floor and caves, the End islands), as the Fabric
    /// version's two passes do.
    Tile {
        dimension: Dimension,
        region: RegionPos,
        step: i32,
        y: i32,
        refine: bool,
    },
    /// A region's markers of the enabled kinds.
    Markers {
        dimension: Dimension,
        region: RegionPos,
    },
}

enum Done {
    Tile(Job, Arc<Vec<u32>>),
    Markers(Job, Arc<Vec<Hit>>),
}

/// A computed biome tile.
#[derive(Clone, Debug)]
pub struct Tile {
    /// Samples a side.
    pub size: u32,
    /// ARGB colours, row-major.
    pub pixels: Arc<Vec<u32>>,
    pub revision: u64,
}

const MAX_TILES: usize = 1200;

/// Samples a tile side, whatever the step.
pub const TILE_SAMPLES: i32 = 128;

/// Blocks a tile side covers at `step`.
pub fn tile_span(step: i32) -> i32 {
    TILE_SAMPLES * step
}

/// Previews (and marker scans) waiting at most. The queue holds what the
/// last frame asked for, most wanted first (see [`SeedService::poll`]).
const MAX_QUEUED: usize = 512;
/// Refinements waiting at most.
const MAX_QUEUED_REFINE: usize = 48;

/// The seed's strongholds and spawn, computed once and shared by every
/// marker scan of that seed.
#[derive(Default)]
struct Globals {
    strongholds: std::sync::OnceLock<Vec<Hit>>,
    spawn: std::sync::OnceLock<(i32, i32)>,
}

/// The jobs waiting for the workers, in the order the map asked for them
/// (nearest the middle first), previews before refinements, refinements on
/// at most all but one worker.
#[derive(Default)]
struct Queue {
    preview: std::collections::VecDeque<Work>,
    refine: std::collections::VecDeque<Work>,
    refining: usize,
    workers: usize,
    closed: bool,
}

type SharedQueue = Arc<(parking_lot::Mutex<Queue>, parking_lot::Condvar)>;

/// How tiles are coloured: the terrain look and the biome data it reads.
#[derive(Clone)]
struct TileLook {
    terrain: bool,
    looks: Arc<dyn BiomeLooks>,
    /// Highlighted biomes (empty: no highlighting).
    highlight: Arc<Vec<bool>>,
    surface: Option<Arc<SurfaceSampler>>,
    cave_mask: Option<Arc<CaveMasker>>,
}

/// The game's vanilla base terrain for detailed Overworld tiles: seed, large
/// biomes, origin x/z, step, width, height (the Fabric version's
/// `OverworldTerrainSampler.sample`).
pub type SurfaceSampler = dyn Fn(i64, bool, i32, i32, i32, usize, usize) -> Option<seedatlas_look::SurfaceGrid>
    + Send
    + Sync;
/// The Nether's carved air as a bit mask, two words a sample: seed, origin
/// x/z, step, width, height (`NetherCaveMask.area`).
pub type CaveMasker = dyn Fn(i64, i32, i32, i32, usize, usize) -> Vec<u64> + Send + Sync;

/// Work sent to the threads: the generation it belongs to, the seed's
/// generators, the marker kinds wanted, the tile look and the job.
type Work = (u64, Arc<Atlas>, Vec<i32>, TileLook, Job, Arc<Globals>);

pub struct SeedService {
    /// The generators per dimension (Overworld, Nether, End); equal seeds
    /// share one.
    atlases: [Option<Arc<Atlas>>; 3],
    specs: [Option<SeedSpec>; 3],
    generation: u64,
    kinds: Vec<i32>,
    tiles: HashMap<Job, (Tile, u64)>,
    markers: HashMap<Job, Arc<Vec<Hit>>>,
    pending: HashSet<Job>,
    /// What was asked for since the last poll, in order.
    wanted: Vec<Job>,
    wanted_set: HashSet<Job>,
    look: TileLook,
    clock: u64,
    jobs: Option<SharedQueue>,
    /// Strongholds and spawn per dimension's atlas, shared with the workers.
    globals: [Arc<Globals>; 3],
    done: mpsc::Receiver<(u64, Done)>,
    done_tx: mpsc::Sender<(u64, Done)>,
    workers: usize,
}

impl std::fmt::Debug for SeedService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SeedService")
            .field("seeds", &self.specs.map(|s| s.map(|s| s.seed)))
            .field("tiles", &self.tiles.len())
            .field("markers", &self.markers.len())
            .field("pending", &self.pending.len())
            .finish()
    }
}

impl Default for SeedService {
    fn default() -> Self {
        let (done_tx, done) = mpsc::channel();
        Self {
            atlases: [None, None, None],
            specs: [None; 3],
            generation: 0,
            kinds: Vec::new(),
            tiles: HashMap::new(),
            markers: HashMap::new(),
            pending: HashSet::new(),
            wanted: Vec::new(),
            wanted_set: HashSet::new(),
            look: TileLook {
                terrain: true,
                looks: Arc::new(VanillaLooks),
                highlight: Arc::new(Vec::new()),
                surface: None,
                cave_mask: None,
            },
            clock: 0,
            jobs: None,
            globals: Default::default(),
            done,
            done_tx,
            workers: 0,
        }
    }
}

/// The colours of a region's biome tile. With the terrain look on (and a
/// version that has terrain) the heights the Java tile worker requests are
/// computed here, with a one-sample halo, and the tile gets its relief.
fn tile_pixels(atlas: &Atlas, look: &TileLook, job: Job) -> Vec<u32> {
    let Job::Tile {
        dimension,
        region,
        step,
        y,
        refine,
    } = job
    else {
        return Vec::new();
    };
    let n = TILE_SAMPLES;
    let span = tile_span(step);
    let (x0, z0) = (region.x.wrapping_mul(span), region.z.wrapping_mul(span));
    let ids = atlas
        .biome_area(dimension, x0, z0, y, step, n, n)
        .unwrap_or_default();
    let mut argb: Vec<u32> = ids
        .iter()
        .map(|&id| meridian_seedatlas::biomes::color_in(atlas.mc, id))
        .collect();
    let size = n as usize;
    seedatlas_look::apply_highlight(&mut argb, &ids, &look.highlight);
    let terrain_style = look.terrain && atlas.has_terrain() && look.highlight.is_empty();
    if !terrain_style || ids.len() != size * size {
        return argb;
    }
    // Detailed Overworld tiles: the game's vanilla base terrain, with a
    // one-sample halo (the Fabric version's refinement pass).
    let surface = match (&look.surface, dimension) {
        (Some(sample), Dimension::Overworld) if refine && step <= 4 => sample(
            atlas.seed,
            atlas.large_biomes,
            x0 - step,
            z0 - step,
            step,
            size + 2,
            size + 2,
        ),
        _ => None,
    };
    let request = seedatlas_look::heights_request(
        dimension,
        step,
        terrain_style,
        false,
        refine,
        surface.is_some(),
        atlas.has_terrain(),
    );
    let (hx, hz, hn) = (x0 - step, z0 - step, n + 2);
    let heights = match request {
        HeightsRequest::None => None,
        HeightsRequest::Heights => atlas.height_area(dimension, hx, hz, step, hn, hn).ok(),
        // Carving masks take long per coarse tile: only up to step 4.
        HeightsRequest::NetherFloor => match &look.cave_mask {
            Some(mask) if step <= 4 => {
                let carved = mask(atlas.seed, hx, hz, step, hn as usize, hn as usize);
                atlas
                    .nether_floor_area_carved(hx, hz, y, step, hn, hn, &carved)
                    .ok()
            }
            _ => atlas.nether_floor_area(hx, hz, y, step, hn, hn).ok(),
        },
    };
    seedatlas_look::style_tile(&TileInput {
        looks: look.looks.as_ref(),
        mc: atlas.mc,
        dimension,
        origin_x: x0,
        origin_z: z0,
        step,
        size,
        seed: atlas.seed,
        terrain_style,
        refining: refine,
        ids: &ids,
        argb: &argb,
        heights: heights.as_deref(),
        surface: surface.as_ref(),
        highlight: &look.highlight,
    })
}

fn run(atlas: &Atlas, kinds: &[i32], look: &TileLook, job: Job, globals: &Globals) -> Done {
    match job {
        Job::Tile { .. } => Done::Tile(job, Arc::new(tile_pixels(atlas, look, job))),
        Job::Markers { dimension, region } => {
            let (x0, z0) = region.min_block();
            let b = [x0, z0, x0 + 511, z0 + 511];
            let mut hits = Vec::new();
            for &k in kinds.iter().filter(|&&k| dimension_of_kind(k) == dimension) {
                let found = match k {
                    marker::STRONGHOLD => {
                        let all = globals.strongholds.get_or_init(|| {
                            atlas
                                .scan_strongholds([i32::MIN, i32::MIN, i32::MAX, i32::MAX], true)
                                .unwrap_or_default()
                        });
                        Ok(all
                            .iter()
                            .filter(|h| h.x >= b[0] && h.x <= b[2] && h.z >= b[1] && h.z <= b[3])
                            .copied()
                            .collect())
                    }
                    marker::SLIME_CHUNK => atlas.scan_slime_chunks(b),
                    marker::SPAWN => {
                        let (x, z) = *globals.spawn.get_or_init(|| atlas.spawn(true));
                        Ok(if x >= b[0] && x <= b[2] && z >= b[1] && z <= b[3] {
                            vec![Hit {
                                kind: marker::SPAWN,
                                x,
                                y: None,
                                z,
                                flags: 0,
                                detail: 0,
                            }]
                        } else {
                            Vec::new()
                        })
                    }
                    k => atlas.scan_structures(k, b, true),
                };
                hits.extend(found.unwrap_or_default());
            }
            Done::Markers(job, Arc::new(hits))
        }
    }
}

impl SeedService {
    /// Sets one seed for every dimension (or none) and the settings the
    /// work depends on, generating the newest version.
    pub fn configure(&mut self, seed: Option<(i64, bool)>, s: &SeedAtlasSettings) {
        let spec = seed.map(|(seed, large_biomes)| SeedSpec {
            seed,
            large_biomes,
            mc: meridian_seedatlas::mc::MC_NEWEST,
        });
        self.configure_dimensions([spec; 3], s);
    }

    /// Sets what each dimension (Overworld, Nether, End; see [`dim_index`])
    /// generates from, and the settings the work depends on; a change drops
    /// what was computed.
    pub fn configure_dimensions(&mut self, specs: [Option<SeedSpec>; 3], s: &SeedAtlasSettings) {
        let kinds: Vec<i32> = if s.structures {
            MARKERS
                .iter()
                .filter(|m| marker_enabled(s, m.id))
                .flat_map(|m| m.kinds.iter().copied())
                .collect()
        } else {
            Vec::new()
        };
        if specs != self.specs {
            let old_specs = self.specs;
            let old = std::mem::take(&mut self.atlases);
            for i in 0..3 {
                let Some(spec) = specs[i] else { continue };
                let shared = (0..i)
                    .find(|&j| specs[j] == Some(spec))
                    .and_then(|j| self.atlases[j].clone())
                    .or_else(|| {
                        (0..3)
                            .find(|&j| old_specs[j] == Some(spec))
                            .and_then(|j| old[j].clone())
                    });
                self.globals[i] = Arc::new(Globals::default());
                self.atlases[i] = Some(shared.unwrap_or_else(|| {
                    Arc::new(Atlas::versioned(spec.seed, spec.large_biomes, spec.mc))
                }));
            }
            self.specs = specs;
            self.generation += 1;
            self.tiles.clear();
            self.markers.clear();
            self.pending.clear();
        }
        if kinds != self.kinds {
            self.kinds = kinds;
            self.generation += 1;
            self.markers.clear();
            self.pending.retain(|j| matches!(j, Job::Tile { .. }));
        }
        if s.terrain_look != self.look.terrain {
            self.look.terrain = s.terrain_look;
            self.invalidate();
        }
        let ids: std::collections::BTreeSet<i32> =
            s.highlighted_biomes.iter().map(|&b| i32::from(b)).collect();
        let mask = seedatlas_look::highlight_mask(s.biome_highlight, &ids);
        if *self.look.highlight != mask {
            self.look.highlight = Arc::new(mask);
            self.invalidate();
        }
        let workers = if s.worker_threads == 0 {
            std::thread::available_parallelism()
                .map_or(2, |n| n.get().saturating_sub(2).clamp(1, 4))
        } else {
            s.worker_threads as usize
        };
        if self.jobs.is_none() || workers != self.workers {
            self.start(workers);
        }
    }

    fn start(&mut self, workers: usize) {
        if let Some(old) = self.jobs.take() {
            old.0.lock().closed = true;
            old.1.notify_all();
        }
        let queue: SharedQueue = Arc::new((
            parking_lot::Mutex::new(Queue {
                workers,
                ..Default::default()
            }),
            parking_lot::Condvar::new(),
        ));
        for i in 0..workers {
            let queue = queue.clone();
            let done = self.done_tx.clone();
            let spawned = std::thread::Builder::new()
                .name(format!("meridian-seedatlas-{i}"))
                .spawn(move || {
                    loop {
                        let (work, refining) = {
                            let (lock, cv) = &*queue;
                            let mut q = lock.lock();
                            loop {
                                if q.closed {
                                    return;
                                }
                                if let Some(w) = q.preview.pop_front() {
                                    break (w, false);
                                }
                                if q.refining + 1 < q.workers.max(2)
                                    && let Some(w) = q.refine.pop_front()
                                {
                                    q.refining += 1;
                                    break (w, true);
                                }
                                cv.wait(&mut q);
                            }
                        };
                        let (generation, atlas, kinds, look, job, globals) = work;
                        let result = run(&atlas, &kinds, &look, job, &globals);
                        if refining {
                            let (lock, cv) = &*queue;
                            lock.lock().refining -= 1;
                            cv.notify_one();
                        }
                        if done.send((generation, result)).is_err() {
                            return;
                        }
                    }
                });
            if let Err(e) = spawned {
                log::warn!("meridian: cannot start a Seed Atlas worker: {e}");
            }
        }
        self.jobs = Some(queue);
        self.workers = workers;
        self.pending.clear();
    }

    /// Sets the biome data the terrain look reads (tints, water colour);
    /// tiles computed with the old data are dropped.
    pub fn set_looks(&mut self, looks: Arc<dyn BiomeLooks>) {
        self.look.looks = looks;
        self.invalidate();
    }

    /// Sets the game's terrain sources for detailed tiles (the vanilla
    /// surface and the Nether cave carvers).
    pub fn set_terrain_sources(
        &mut self,
        surface: Arc<SurfaceSampler>,
        cave_mask: Arc<CaveMasker>,
    ) {
        self.look.surface = Some(surface);
        self.look.cave_mask = Some(cave_mask);
        self.invalidate();
    }

    fn invalidate(&mut self) {
        if let Some(q) = &self.jobs {
            let mut q = q.0.lock();
            q.preview.clear();
            q.refine.clear();
        }
        self.generation += 1;
        self.tiles.clear();
        self.markers.clear();
        self.pending.clear();
    }

    pub fn active(&self) -> bool {
        self.atlases.iter().any(Option::is_some)
    }

    /// The Overworld's generators (its seed is the base seed).
    pub fn atlas(&self) -> Option<&Arc<Atlas>> {
        self.atlases[0].as_ref()
    }

    /// The generators of one dimension, with the seed that generates it.
    pub fn atlas_for(&self, dimension: Dimension) -> Option<&Arc<Atlas>> {
        self.atlases[dim_index(dimension)].as_ref()
    }

    /// What one dimension generates from.
    pub fn spec_for(&self, dimension: Dimension) -> Option<SeedSpec> {
        self.specs[dim_index(dimension)]
    }

    /// Collects finished work.
    pub fn poll(&mut self) {
        self.commit();
        while let Ok((generation, done)) = self.done.try_recv() {
            if generation != self.generation {
                continue;
            }
            match done {
                Done::Tile(job, pixels) => {
                    self.pending.remove(&job);
                    let Job::Tile { step, .. } = job else {
                        continue;
                    };
                    self.clock += 1;
                    let _ = step;
                    let tile = Tile {
                        size: TILE_SAMPLES as u32,
                        pixels,
                        revision: self.clock,
                    };
                    self.tiles.insert(job, (tile, self.clock));
                }
                Done::Markers(job, hits) => {
                    self.pending.remove(&job);
                    self.markers.insert(job, hits);
                }
            }
        }
        if self.tiles.len() > MAX_TILES {
            let mut by_age: Vec<(u64, Job)> =
                self.tiles.iter().map(|(j, (_, t))| (*t, *j)).collect();
            by_age.sort_unstable_by_key(|(t, _)| *t);
            for (_, job) in by_age
                .into_iter()
                .take(self.tiles.len() - MAX_TILES * 3 / 4)
            {
                self.tiles.remove(&job);
            }
        }
    }

    /// Asks for a job; the queue takes it at the next poll (see
    /// [`SeedService::commit`]).
    fn submit(&mut self, job: Job) {
        if self.wanted_set.insert(job) {
            self.wanted.push(job);
        }
    }

    fn work_for(&self, job: Job) -> Option<Work> {
        let d = dim_index(job_dimension(&job));
        let atlas = self.atlases[d].as_ref()?;
        Some((
            self.generation,
            atlas.clone(),
            self.kinds.clone(),
            self.look.clone(),
            job,
            self.globals[d].clone(),
        ))
    }

    /// Replaces the waiting jobs with what was asked for since the last
    /// poll, in that order: what scrolled out of view is never computed,
    /// and the middle of the view comes first. Jobs a worker is already on
    /// are left to finish.
    fn commit(&mut self) {
        let wanted = std::mem::take(&mut self.wanted);
        self.wanted_set.clear();
        let Some(queue) = self.jobs.clone() else {
            return;
        };
        let (lock, cv) = &*queue;
        let mut guard = lock.lock();
        let q = &mut *guard;
        let mut waiting: HashMap<Job, Work> = q
            .preview
            .drain(..)
            .chain(q.refine.drain(..))
            .map(|w| (w.4, w))
            .collect();
        for job in wanted {
            let refine = matches!(job, Job::Tile { refine: true, .. });
            let (len, cap) = if refine {
                (q.refine.len(), MAX_QUEUED_REFINE)
            } else {
                (q.preview.len(), MAX_QUEUED)
            };
            if len >= cap {
                continue;
            }
            let work = match waiting.remove(&job) {
                Some(w) => w,
                // Running on a worker already.
                None if self.pending.contains(&job) => continue,
                None => match self.work_for(job) {
                    Some(w) => w,
                    None => continue,
                },
            };
            if refine {
                q.refine.push_back(work);
            } else {
                q.preview.push_back(work);
            }
            self.pending.insert(job);
        }
        for job in waiting.keys() {
            self.pending.remove(job);
        }
        let any = !q.preview.is_empty() || !q.refine.is_empty();
        drop(guard);
        if any {
            cv.notify_all();
        }
    }

    /// A tile's best colours so far: refined when ready, else the preview.
    /// Asks for the preview, then for the refinement where the look needs
    /// one; `job` is the preview's (`refine: false`).
    pub fn tile(&mut self, job: Job) -> Option<Tile> {
        self.clock += 1;
        let clock = self.clock;
        let Job::Tile {
            dimension,
            region,
            step,
            y,
            ..
        } = job
        else {
            return None;
        };
        let preview = Job::Tile {
            dimension,
            region,
            step,
            y,
            refine: false,
        };
        let refined = Job::Tile {
            dimension,
            region,
            step,
            y,
            refine: true,
        };
        if let Some((tile, used)) = self.tiles.get_mut(&refined) {
            *used = clock;
            return Some(tile.clone());
        }
        let found = self.tiles.get_mut(&preview).map(|(tile, used)| {
            *used = clock;
            tile.clone()
        });
        match &found {
            Some(_) => {
                let has_terrain = self.atlas_for(dimension).is_some_and(|a| a.has_terrain());
                if seedatlas_look::needs_surface_refinement(
                    dimension,
                    self.look.terrain && has_terrain,
                    !self.look.highlight.is_empty(),
                    step,
                ) {
                    self.submit(refined);
                }
            }
            None => self.submit(preview),
        }
        found
    }

    /// A tile's colours if already computed (the coarser fallback under a
    /// tile that is not ready yet); asks for nothing.
    pub fn cached_tile(&mut self, job: Job) -> Option<Tile> {
        let Job::Tile {
            dimension,
            region,
            step,
            y,
            ..
        } = job
        else {
            return None;
        };
        [true, false].into_iter().find_map(|refine| {
            self.tiles
                .get(&Job::Tile {
                    dimension,
                    region,
                    step,
                    y,
                    refine,
                })
                .map(|(t, _)| t.clone())
        })
    }

    /// A region's markers, asking for them when missing.
    pub fn markers(&mut self, dimension: Dimension, region: RegionPos) -> Option<Arc<Vec<Hit>>> {
        if self.kinds.is_empty() {
            return None;
        }
        let job = Job::Markers { dimension, region };
        if let Some(hits) = self.markers.get(&job) {
            return Some(hits.clone());
        }
        self.submit(job);
        None
    }

    /// The biome at a block, computed now (for the hover tooltip).
    pub fn biome_at(&self, dimension: Dimension, x: i32, y: i32, z: i32) -> Option<i32> {
        self.atlas_for(dimension)
            .map(|a| a.biome_at(dimension, x, y, z))
    }

    pub fn pending(&self) -> usize {
        self.pending.len()
    }
}

fn job_dimension(job: &Job) -> Dimension {
    match job {
        Job::Tile { dimension, .. } | Job::Markers { dimension, .. } => *dimension,
    }
}

/// Physical pixels per sample: two keeps biome edges crisp on high-density
/// screens at a quarter of the work of one per pixel.
const PIXELS_PER_SAMPLE: f32 = 2.0;

/// The sample step for a region drawn `side_px` physical pixels wide: about
/// one sample per [`PIXELS_PER_SAMPLE`] screen pixels (a power of two, 1 to
/// 1024), no finer than the configured resolution.
pub fn step_for(side_px: f32, resolution: u8) -> i32 {
    let blocks_per_sample = 512.0 * PIXELS_PER_SAMPLE / side_px.max(0.001);
    let mut step = 1;
    while step < 1024 && (step * 2) as f32 <= blocks_per_sample {
        step *= 2;
    }
    step.max(resolution.clamp(1, 16) as i32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seeds_parse_like_minecraft() {
        assert_eq!(parse_seed(" 123 "), Some(123));
        assert_eq!(
            parse_seed("-4172144997902289642"),
            Some(-4172144997902289642)
        );
        // "hello".hashCode() in Java.
        assert_eq!(parse_seed("hello"), Some(99162322));
        assert_eq!(parse_seed("  "), None);
    }

    #[test]
    fn markers_default_like_the_fabric_version() {
        let s = SeedAtlasSettings::default();
        assert!(!s.structures);
        assert!(marker_enabled(&s, "village"));
        assert!(!marker_enabled(&s, "slime"));
        assert_eq!(
            marker_for(kind::RUINED_PORTAL_NETHER).map(|m| m.id),
            Some("portal")
        );
        assert_eq!(MARKERS.len(), 28);
    }

    #[test]
    fn service_computes_tiles_and_markers() {
        let mut s = SeedAtlasSettings {
            structures: true,
            ..Default::default()
        };
        s.markers.insert("village".into(), true);
        let mut svc = SeedService::default();
        svc.configure(Some((1, false)), &s);
        let job = Job::Tile {
            dimension: Dimension::Overworld,
            region: RegionPos::new(0, 0),
            step: 16,
            y: 255,
            refine: false,
        };
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
        let (mut tile, mut marks) = (None, None);
        while (tile.is_none() || marks.is_none()) && std::time::Instant::now() < deadline {
            svc.poll();
            tile = tile.or_else(|| svc.tile(job));
            marks = marks.or_else(|| svc.markers(Dimension::Overworld, RegionPos::new(0, 0)));
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        let tile = tile.expect("a tile");
        assert_eq!(tile.size, TILE_SAMPLES as u32);
        assert!(tile.pixels.iter().all(|p| p >> 24 == 0xFF));
        assert!(marks.is_some());
        assert_eq!(step_for(1200.0, 1), 1);
        assert_eq!(step_for(600.0, 1), 1);
        assert_eq!(step_for(400.0, 1), 2);
        assert_eq!(step_for(400.0, 4), 4);
        assert_eq!(step_for(10.0, 1), 64);
        assert_eq!(step_for(0.1, 1), 1024);
    }

    #[test]
    fn the_queue_follows_the_latest_frame() {
        let mut svc = SeedService::default();
        svc.configure(
            Some((7, false)),
            &SeedAtlasSettings {
                worker_threads: 1,
                ..SeedAtlasSettings::default()
            },
        );
        let job = |x| Job::Tile {
            dimension: Dimension::Overworld,
            region: RegionPos::new(x, 0),
            step: 64,
            y: 255,
            refine: false,
        };
        // Frame 1 asks for 0..40, frame 2 only for 100..103: what frame 1
        // wanted and nobody runs yet is dropped.
        for x in 0..40 {
            svc.tile(job(x));
        }
        svc.poll();
        for x in 100..103 {
            svc.tile(job(x));
        }
        svc.poll();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
        while svc.pending() > 0 && std::time::Instant::now() < deadline {
            for x in 100..103 {
                svc.tile(job(x));
            }
            svc.poll();
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        for x in 100..103 {
            assert!(
                svc.cached_tile(job(x)).is_some(),
                "tile {x} of the latest frame"
            );
        }
        let early = (0..40)
            .filter(|&x| svc.cached_tile(job(x)).is_some())
            .count();
        assert!(
            early <= 2,
            "{early} tiles of the old frame were still computed"
        );
        // Not drawn any more: nothing waits.
        svc.poll();
        assert_eq!(svc.pending(), 0);
    }

    /// The cost of one tile by step (run by hand:
    /// `cargo test -p meridian-core --release tile_cost -- --ignored --nocapture`).
    #[test]
    #[ignore = "measurement"]
    fn tile_cost() {
        let atlas = Atlas::versioned(42, false, meridian_seedatlas::mc::MC_NEWEST);
        let look = TileLook {
            terrain: true,
            looks: Arc::new(VanillaLooks),
            highlight: Arc::new(Vec::new()),
            surface: None,
            cave_mask: None,
        };
        for step in [1, 2, 4, 8, 16, 32, 64, 128, 256] {
            for (terrain, label) in [(false, "biomes"), (true, "terrain look")] {
                let look = TileLook {
                    terrain,
                    ..look.clone()
                };
                let t = std::time::Instant::now();
                let n = 4;
                for i in 0..n {
                    let job = Job::Tile {
                        dimension: Dimension::Overworld,
                        region: RegionPos::new(i, 3),
                        step,
                        y: 255,
                        refine: false,
                    };
                    std::hint::black_box(tile_pixels(&atlas, &look, job));
                }
                println!(
                    "step {step:>3} {label:>12}: {:.1} ms per tile",
                    t.elapsed().as_secs_f64() * 1000.0 / n as f64
                );
            }
        }
    }
}
