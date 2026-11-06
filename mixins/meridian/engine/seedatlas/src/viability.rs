//! Whether a structure start actually generates: its biome checks
//! (`isViableFeatureBiome`, `isViableStructurePos`, `areBiomesViable`), start
//! variants (`getVariant`) and abandoned camps, ported from the engine for
//! every version.

use crate::biome_noise::Range;
use crate::biomes::*;
use crate::generator::{Dimension, Generator};
use crate::layers::{
    L_BIOME_256, L_RIVER_MIX_4, L_SHORE_16, L_VORONOI_1, Layer, LayerStack, map_biome, map_shore,
};
use crate::mc::*;
use crate::rng::{JavaRandom, Xoroshiro, floordiv};
use crate::structures::{self, StructureConfig, chunk_generate_rnd, feature_pos, kind};

// ----------------------------------------------------------------- shims --

// TEMP shim: forwards to the 26.3-only `config` of structures.rs until it
// takes the version (only the villages and geodes, which this file uses
// before 1.18, are adapted).
fn config(kind: i32, mc: i32) -> Option<StructureConfig> {
    let mut c = structures::config(kind)?;
    match kind {
        kind::VILLAGE if mc < MC_B1_8 => return None,
        kind::VILLAGE if mc <= MC_1_17 => {
            c.region_size = 32;
            c.chunk_range = 24;
        }
        kind::GEODE if mc < MC_1_17 => return None,
        kind::GEODE if mc <= MC_1_17 => c.salt = 20000,
        _ => {}
    }
    Some(c)
}

// TEMP shim: forwards to the 26.3-only `structure_pos` of structures.rs
// until it takes the version.
fn structure_pos(kind: i32, _mc: i32, seed: u64, rx: i32, rz: i32) -> Option<(i32, i32)> {
    structures::structure_pos(kind, seed, rx, rz)
}

// TEMP shim: `getPopulationSeed` for every version (structures.rs only has
// the 1.18+ one).
fn population_seed(mc: i32, ws: u64, x: i32, z: i32) -> u64 {
    if mc >= MC_1_18 {
        return structures::population_seed(ws, x, z);
    }
    let mut s = JavaRandom::new(ws);
    let (mut a, mut b) = (s.next_long(), s.next_long());
    if mc >= MC_1_13 {
        a |= 1;
        b |= 1;
    } else {
        a = (a as i64 / 2 * 2 + 1) as u64;
        b = (b as i64 / 2 * 2 + 1) as u64;
    }
    (x as i64 as u64)
        .wrapping_mul(a)
        .wrapping_add((z as i64 as u64).wrapping_mul(b))
        ^ ws
}

// ----------------------------------------------------------- biome masks --

/// The bit set of biome ids below 64.
const fn bits(ids: &[i32]) -> u64 {
    let mut b = 0u64;
    let mut i = 0;
    while i < ids.len() {
        b |= 1u64 << ids[i];
        i += 1;
    }
    b
}

/// `id_matches`: `b` holds the ids 0 to 63, `m` the ids 128 to 191.
pub fn id_matches(id: i32, b: u64, m: u64) -> bool {
    if id < 0 {
        false
    } else if id < 128 {
        id < 64 && (b >> id) & 1 != 0
    } else {
        id - 128 < 64 && (m >> (id - 128)) & 1 != 0
    }
}

/// `g_monument_biomes2`.
const MONUMENT_BIOMES2: u64 = bits(&[
    DEEP_FROZEN_OCEAN,
    DEEP_COLD_OCEAN,
    DEEP_OCEAN,
    DEEP_LUKEWARM_OCEAN,
    DEEP_WARM_OCEAN,
]);

/// `g_monument_biomes1`.
const MONUMENT_BIOMES1: u64 = bits(&[
    OCEAN,
    DEEP_OCEAN,
    RIVER,
    FROZEN_RIVER,
    FROZEN_OCEAN,
    DEEP_FROZEN_OCEAN,
    COLD_OCEAN,
    DEEP_COLD_OCEAN,
    LUKEWARM_OCEAN,
    DEEP_LUKEWARM_OCEAN,
    WARM_OCEAN,
    DEEP_WARM_OCEAN,
]);

/// `isViableFeatureBiome`: whether the structure can start in the biome.
pub fn is_viable_feature_biome(mc: i32, structure: i32, b: i32) -> bool {
    use kind::*;
    match structure {
        DESERT_PYRAMID => b == DESERT || b == DESERT_HILLS,
        JUNGLE_TEMPLE => {
            b == JUNGLE || b == JUNGLE_HILLS || b == BAMBOO_JUNGLE || b == BAMBOO_JUNGLE_HILLS
        }
        SWAMP_HUT => b == SWAMP,
        IGLOO => {
            if mc <= MC_1_8 {
                return false;
            }
            b == SNOWY_TUNDRA || b == SNOWY_TAIGA || b == SNOWY_SLOPES
        }
        OCEAN_RUIN => {
            if mc <= MC_1_12 {
                return false;
            }
            is_oceanic(b)
        }
        SHIPWRECK => {
            if mc <= MC_1_12 {
                return false;
            }
            is_oceanic(b) || b == BEACH || b == SNOWY_BEACH
        }
        RUINED_PORTAL | RUINED_PORTAL_NETHER => mc >= MC_1_16_1,
        ANCIENT_CITY => {
            if mc <= MC_1_18 {
                return false;
            }
            b == DEEP_DARK
        }
        TRAIL_RUINS => {
            if mc <= MC_1_19 {
                return false;
            }
            [
                TAIGA,
                SNOWY_TAIGA,
                OLD_GROWTH_PINE_TAIGA,
                OLD_GROWTH_SPRUCE_TAIGA,
                OLD_GROWTH_BIRCH_FOREST,
                JUNGLE,
            ]
            .contains(&b)
        }
        TRIAL_CHAMBERS => {
            if mc <= MC_1_20 {
                return false;
            }
            b != DEEP_DARK && is_overworld(mc, b)
        }
        ABANDONED_CAMP => {
            if mc < MC_26_3 {
                return false;
            }
            [
                BAMBOO_JUNGLE,
                BIRCH_FOREST,
                CHERRY_GROVE,
                DAPPLED_FOREST,
                FLOWER_FOREST,
                FOREST,
                MEADOW,
                OLD_GROWTH_BIRCH_FOREST,
                OLD_GROWTH_PINE_TAIGA,
                OLD_GROWTH_SPRUCE_TAIGA,
                PALE_GARDEN,
                SAVANNA,
                SNOWY_TAIGA,
                SPARSE_JUNGLE,
                SWAMP,
                TAIGA,
                WINDSWEPT_FOREST,
                WOODED_BADLANDS,
            ]
            .contains(&b)
        }
        BURIED_TREASURE => {
            if mc <= MC_1_12 {
                return false;
            }
            b == BEACH || b == SNOWY_BEACH
        }
        MINESHAFT => is_overworld(mc, b),
        DESERT_WELL => b == DESERT,
        MONUMENT => {
            if mc <= MC_1_7 {
                return false;
            }
            is_deep_ocean(b)
        }
        OUTPOST | VILLAGE => {
            if structure == OUTPOST {
                if mc <= MC_1_13 {
                    return false;
                }
                if mc >= MC_1_18 {
                    return [
                        DESERT,
                        PLAINS,
                        SAVANNA,
                        SNOWY_PLAINS,
                        TAIGA,
                        MEADOW,
                        FROZEN_PEAKS,
                        JAGGED_PEAKS,
                        STONY_PEAKS,
                        SNOWY_SLOPES,
                        GROVE,
                        CHERRY_GROVE,
                    ]
                    .contains(&b);
                }
            }
            // outposts up to 1.17 follow the village biomes
            b == PLAINS
                || b == DESERT
                || b == SAVANNA
                || (mc >= MC_1_10 && b == TAIGA)
                || (mc >= MC_1_14 && b == SNOWY_TUNDRA)
                || (mc >= MC_1_18 && b == MEADOW)
        }
        MANSION => {
            if mc <= MC_1_10 {
                return false;
            }
            if mc >= MC_1_21_5 && b == PALE_GARDEN {
                return true;
            }
            b == DARK_FOREST || b == DARK_FOREST_HILLS
        }
        FORTRESS => [
            NETHER_WASTES,
            SOUL_SAND_VALLEY,
            WARPED_FOREST,
            CRIMSON_FOREST,
            BASALT_DELTAS,
        ]
        .contains(&b),
        BASTION => {
            if mc <= MC_1_15 {
                return false;
            }
            [
                NETHER_WASTES,
                SOUL_SAND_VALLEY,
                WARPED_FOREST,
                CRIMSON_FOREST,
            ]
            .contains(&b)
        }
        END_CITY => {
            if mc <= MC_1_8 {
                return false;
            }
            b == END_MIDLANDS || b == END_HIGHLANDS
        }
        END_GATEWAY => {
            if mc <= MC_1_12 {
                return false;
            }
            b == END_HIGHLANDS
        }
        _ => false,
    }
}

// --------------------------------------------------------------- variants --

/// `StructureVariant`: a start piece's footprint relative to the start
/// chunk's corner, its rotation and template, and the flags of the
/// structures that have them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Variant {
    /// Zombie village.
    pub abandoned: bool,
    /// Giant portal variant.
    pub giant: bool,
    /// Underground portal.
    pub underground: bool,
    /// Portal with air pocket.
    pub airpocket: bool,
    /// Igloo with basement.
    pub basement: bool,
    /// Geode with crack.
    pub cracked: bool,
    /// Abandoned camp with special loot.
    pub special: bool,
    /// Geode size or igloo middle pieces.
    pub size: i32,
    /// Starting piece index.
    pub start: i32,
    /// Biome variant.
    pub biome: i32,
    /// 0:0, 1:cw90, 2:cw180, 3:cw270.
    pub rotation: i32,
    pub mirror: bool,
    pub x: i32,
    pub y: i32,
    pub z: i32,
    pub sx: i32,
    pub sy: i32,
    pub sz: i32,
}

/// `L_rotate_village_bastion`: places the footprint of a template for the
/// rotation in `v`.
fn rotate(v: &mut Variant, mc: i32, x: i32, z: i32, sx: i32, sy: i32, sz: i32) {
    v.sy = sy;
    let (nx, nz) = ((x < 0) as i32, (z < 0) as i32);
    (v.x, v.z, v.sx, v.sz) = if mc >= MC_1_18 {
        match v.rotation {
            0 => (0, 0, sx, sz),
            1 => (1 - sz, 0, sz, sx),
            2 => (1 - sx, 1 - sz, sx, sz),
            _ => (0, 1 - sx, sz, sx),
        }
    } else {
        match v.rotation {
            0 => (0, 0, sx, sz),
            1 => (nx - sz, 0, sz, sx),
            2 => (nx - sx, nz - sz, sx, sz),
            _ => (0, nz - sx, sz, sx),
        }
    };
}

/// The templates of a village start: (bound, start piece, size, abandoned).
type VillageTemplate = (i32, i32, [i32; 3], bool);

fn village_templates(biome: i32) -> Option<&'static [VillageTemplate]> {
    Some(match biome {
        PLAINS | MEADOW => &[
            (50, 0, [9, 4, 9], false),
            (100, 1, [10, 7, 10], false),
            (150, 2, [8, 5, 15], false),
            (200, 3, [11, 9, 11], false),
            (201, 0, [9, 4, 9], true),
            (202, 1, [10, 7, 10], true),
            (203, 2, [8, 5, 15], true),
            (204, 3, [11, 9, 11], true),
        ],
        DESERT => &[
            (98, 1, [17, 6, 9], false),
            (196, 2, [12, 6, 12], false),
            (245, 3, [15, 6, 15], false),
            (247, 1, [17, 6, 9], true),
            (249, 2, [12, 6, 12], true),
            (250, 3, [15, 6, 15], true),
        ],
        SAVANNA => &[
            (100, 1, [14, 5, 12], false),
            (150, 2, [11, 6, 11], false),
            (300, 3, [9, 6, 11], false),
            (450, 4, [9, 6, 9], false),
            (452, 1, [14, 5, 12], true),
            (453, 2, [11, 6, 11], true),
            (456, 3, [9, 6, 11], true),
            (459, 4, [9, 6, 9], true),
        ],
        TAIGA => &[
            (49, 1, [22, 3, 18], false),
            (98, 2, [9, 7, 9], false),
            (99, 1, [22, 3, 18], true),
            (100, 2, [9, 7, 9], true),
        ],
        SNOWY_TUNDRA => &[
            (100, 1, [12, 8, 8], false),
            (150, 2, [11, 5, 9], false),
            (300, 3, [7, 7, 7], false),
            (302, 1, [12, 8, 8], true),
            (303, 2, [11, 5, 9], true),
            (306, 3, [7, 7, 7], true),
        ],
        _ => return None,
    })
}

/// `getVariant`: data such as rotation and bounding box of a structure
/// instance at the block position (`x`, `z`), for the structures that have
/// them. `biome` is the biome the start is in (villages and camps).
pub fn variant(structure: i32, mc: i32, seed: u64, x: i32, z: i32, biome: i32) -> Option<Variant> {
    let mut rng = chunk_generate_rnd(seed, x >> 4, z >> 4);
    let mut v = Variant {
        start: -1,
        biome: -1,
        y: 320,
        ..Default::default()
    };
    match structure {
        kind::ABANDONED_CAMP => {
            if mc < MC_26_3 || !is_viable_feature_biome(mc, structure, biome) {
                return None;
            }
            v.biome = biome;
            v.special = is_abandoned_camp_special(&mut rng, biome);
            Some(v)
        }
        kind::VILLAGE => {
            if mc <= MC_1_9 || !is_viable_feature_biome(mc, kind::VILLAGE, biome) {
                return None;
            }
            if mc <= MC_1_13 {
                rng.skip(if mc == MC_1_13 { 10 } else { 11 });
                v.abandoned = rng.next_int(50) == 0;
                return Some(v);
            }
            v.biome = biome;
            v.rotation = rng.next_int(4);
            let table = village_templates(biome)?;
            if biome == MEADOW {
                v.biome = PLAINS;
            }
            let t = rng.next_int(table.last()?.0);
            let &(_, start, [sx, sy, sz], abandoned) = table.iter().find(|e| t < e.0)?;
            v.start = start;
            v.abandoned = abandoned;
            rotate(&mut v, mc, x, z, sx, sy, sz);
            Some(v)
        }
        kind::BASTION => {
            v.rotation = rng.next_int(4);
            v.start = rng.next_int(4);
            if mc == MC_1_16_1 {
                // swapped in 1.16.1 only
                std::mem::swap(&mut v.start, &mut v.rotation);
            }
            let (sx, sy, sz) = match v.start {
                0 => (46, 24, 46),
                1 => (30, 24, 48),
                2 => (38, 48, 38),
                _ => (16, 32, 32),
            };
            rotate(&mut v, mc, x, z, sx, sy, sz);
            Some(v)
        }
        kind::ANCIENT_CITY => {
            v.rotation = rng.next_int(4);
            v.start = 1 + rng.next_int(3);
            let (sx, sy, sz) = (18, 31, 41);
            let (bx, bz);
            match v.rotation {
                0 => {
                    (bx, bz) = (-((x > 0) as i32), -((z > 0) as i32));
                    (v.sx, v.sz) = (sx, sz);
                }
                1 => {
                    (bx, bz) = ((x < 0) as i32 - sz, -((z > 0) as i32));
                    (v.sx, v.sz) = (sz, sx);
                }
                2 => {
                    (bx, bz) = ((x < 0) as i32 - sx, (z < 0) as i32 - sz);
                    (v.sx, v.sz) = (sx, sz);
                }
                _ => {
                    (bx, bz) = (-((x > 0) as i32), (z < 0) as i32 - sx);
                    (v.sx, v.sz) = (sz, sx);
                }
            }
            // the city_anchor (13, *, 20) is part of the city_center
            let (ax, az) = (13, 20);
            (v.x, v.z) = match v.rotation {
                0 => (bx - ax, bz - az),
                1 => (bx + az, bz - ax),
                2 => (bx + ax, bz + az),
                _ => (bx - az, bz + ax),
            };
            v.y = -27;
            v.sy = sy;
            Some(v)
        }
        kind::RUINED_PORTAL | kind::RUINED_PORTAL_NETHER => {
            let cat = get_category(mc, biome);
            if [DESERT, JUNGLE, SWAMP, OCEAN, NETHER_WASTES].contains(&cat) {
                v.biome = cat;
            }
            if v.biome == -1 {
                if biome == MANGROVE_SWAMP {
                    v.biome = SWAMP;
                } else if [
                    MOUNTAINS,
                    MOUNTAIN_EDGE,
                    WOODED_MOUNTAINS,
                    GRAVELLY_MOUNTAINS,
                    MODIFIED_GRAVELLY_MOUNTAINS,
                    SAVANNA_PLATEAU,
                    SHATTERED_SAVANNA,
                    SHATTERED_SAVANNA_PLATEAU,
                    BADLANDS,
                    ERODED_BADLANDS,
                    WOODED_BADLANDS_PLATEAU,
                    MODIFIED_BADLANDS_PLATEAU,
                    MODIFIED_WOODED_BADLANDS_PLATEAU,
                    SNOWY_TAIGA_MOUNTAINS,
                    TAIGA_MOUNTAINS,
                    STONE_SHORE,
                    MEADOW,
                    FROZEN_PEAKS,
                    JAGGED_PEAKS,
                    STONY_PEAKS,
                    SNOWY_SLOPES,
                ]
                .contains(&biome)
                {
                    v.biome = MOUNTAINS;
                }
            }
            if v.biome == -1 {
                v.biome = PLAINS;
            }
            if v.biome == PLAINS || v.biome == MOUNTAINS {
                v.underground = rng.next_float() < 0.5;
                v.airpocket = v.underground || rng.next_float() < 0.5;
            } else if v.biome == JUNGLE {
                v.airpocket = rng.next_float() < 0.5;
            }
            v.giant = rng.next_float() < 0.05;
            v.start = if v.giant {
                1 + rng.next_int(3)
            } else {
                1 + rng.next_int(10)
            };
            v.rotation = rng.next_int(4);
            v.mirror = rng.next_float() < 0.5;
            Some(v)
        }
        kind::MONUMENT => {
            (v.x, v.z) = (-29, -29);
            (v.sx, v.sz) = (58, 58);
            Some(v)
        }
        kind::IGLOO => {
            if mc <= MC_1_12 {
                rng.set_seed(population_seed(mc, seed, (x >> 4) - 1, (z >> 4) - 1));
            }
            let orientation = rng.next_int(4);
            v.basement = rng.next_double() < 0.5;
            v.size = rng.next_int(8) + 4;
            let (sx, sy, sz) = (7, 5, 8);
            v.sy = sy;
            // orientation: 0:north, 1:east, 2:south, 3:west
            (v.rotation, v.mirror, v.sx, v.sz) = match orientation {
                0 => (0, false, sx, sz),
                1 => (1, false, sz, sx),
                2 => (0, true, sx, sz),
                _ => (1, true, sz, sx),
            };
            Some(v)
        }
        kind::DESERT_PYRAMID | kind::JUNGLE_TEMPLE | kind::SWAMP_HUT => {
            let (sx, sy, sz) = match structure {
                kind::DESERT_PYRAMID => (21, 15, 21),
                kind::JUNGLE_TEMPLE => (12, 10, 15),
                _ => (7, 7, 9),
            };
            v.sy = sy;
            if mc <= MC_1_19 {
                (v.sx, v.sz) = (sx, sz);
                return Some(v);
            }
            // orientation: 0:north, 1:east, 2:south, 3:west
            (v.rotation, v.mirror, v.sx, v.sz) = match rng.next_int(4) {
                0 => (0, false, sx, sz),
                1 => (1, false, sz, sx),
                2 => (0, true, sx, sz),
                _ => (1, true, sz, sx),
            };
            Some(v)
        }
        kind::GEODE => {
            let sc = config(kind::GEODE, mc)?;
            let salt = sc.salt as i64 as u64;
            if mc >= MC_1_18 {
                let mut xr =
                    Xoroshiro::new(population_seed(mc, seed, x & !15, z & !15).wrapping_add(salt));
                if xr.next_float() >= sc.rarity {
                    return None;
                }
                v.x = xr.next_int_j(16) - (x & 15);
                v.z = xr.next_int_j(16) - (z & 15);
                v.y = xr.next_int_j(1 + 30 + 58) - 58;
                v.size = xr.next_int_j(2) + 3;
                xr.skip(2);
                v.cracked = xr.next_float() < 0.95;
            } else {
                rng.set_seed(population_seed(mc, seed, x & !15, z & !15).wrapping_add(salt));
                if rng.next_float() >= sc.rarity {
                    return None;
                }
                v.x = rng.next_int(16) - (x & 15);
                v.z = rng.next_int(16) - (z & 15);
                v.y = rng.next_int(1 + 46 - 6) + 6;
                v.size = rng.next_int(2) + 3;
                rng.skip(2);
                v.cracked = rng.next_float() < 0.95;
            }
            v.x += 5;
            v.y += 5;
            v.z += 5;
            Some(v)
        }
        kind::TRIAL_CHAMBERS => {
            v.y = rng.next_int(1 + 20) - 40;
            v.rotation = rng.next_int(4);
            v.start = rng.next_int(2);
            (v.sx, v.sy, v.sz) = (19, 20, 19);
            (v.x, v.z) = match v.rotation {
                0 => (0, 0),
                1 => (1 - v.sz, 0),
                2 => (1 - v.sx, 1 - v.sz),
                _ => (0, 1 - v.sx),
            };
            Some(v)
        }
        _ => None,
    }
}

// ---------------------------------------------------------------- camps --

/// `getAbandonedCampBiomePos`.
fn camp_biome_pos(seed: u64, cx: i32, cz: i32) -> (i32, i32) {
    let mut rng = chunk_generate_rnd(seed, cx, cz);
    let rotation = rng.next_int(4);
    let template = rng.next_int(10);
    // templates 3 and 8 are 6x8 blocks; all other start tents are 8x8
    let sx = if template == 2 || template == 7 { 6 } else { 8 };
    let sz = 8;
    let (mut min_x, mut min_z) = (cx * 16, cz * 16);
    let (mut max_x, mut max_z) = (min_x, min_z);
    match rotation {
        1 => {
            min_x -= sz - 1;
            max_z += sx - 1;
        }
        2 => {
            min_x -= sx - 1;
            min_z -= sz - 1;
        }
        3 => {
            max_x += sz - 1;
            min_z -= sx - 1;
        }
        _ => {
            max_x += sx - 1;
            max_z += sz - 1;
        }
    }
    // Java truncates the absolute bounding-box centre toward zero
    ((min_x + max_x) / 2, (min_z + max_z) / 2)
}

/// `getAbandonedCampJigsawCount`.
fn camp_jigsaw_count(biome: i32, template: i32) -> i32 {
    let extra: u16 = match biome {
        BAMBOO_JUNGLE => return [1, 9, 1, 1, 6, 1, 1, 5, 1, 1][template as usize],
        BIRCH_FOREST
        | CHERRY_GROVE
        | OLD_GROWTH_PINE_TAIGA
        | OLD_GROWTH_SPRUCE_TAIGA
        | PALE_GARDEN
        | WOODED_BADLANDS => 1,
        DAPPLED_FOREST => 1 | 2 | 4 | 8 | 32 | 64,
        FLOWER_FOREST | FOREST => 1 | 32,
        MEADOW => 16 | 32,
        OLD_GROWTH_BIRCH_FOREST => 8 | 512,
        SAVANNA => 1 | 16 | 512,
        SNOWY_TAIGA => 256 | 512,
        SPARSE_JUNGLE => 2,
        TAIGA => 2 | 64 | 128,
        WINDSWEPT_FOREST => 1 | 16 | 32 | 512,
        _ => 0,
    };
    1 + ((extra >> template) & 1) as i32
}

/// `abandonedCampElementHasSpecialLoot`.
fn camp_element_has_special_loot(biome: i32, element: i32) -> bool {
    if (5..=9).contains(&element)
        || (25..=30).contains(&element)
        || element == 33
        || element == 35
        || (37..=39).contains(&element)
        || element == 42
    {
        return true;
    }
    if !(45..=48).contains(&element) {
        return false;
    }
    let specific: u8 = match biome {
        BAMBOO_JUNGLE | CHERRY_GROVE | DAPPLED_FOREST | FOREST | MEADOW | WINDSWEPT_FOREST
        | WOODED_BADLANDS => 4 | 8,
        BIRCH_FOREST | OLD_GROWTH_SPRUCE_TAIGA | TAIGA => 8,
        FLOWER_FOREST
        | OLD_GROWTH_BIRCH_FOREST
        | OLD_GROWTH_PINE_TAIGA
        | PALE_GARDEN
        | SAVANNA
        | SPARSE_JUNGLE
        | SWAMP => 2 | 8,
        SNOWY_TAIGA => 1,
        _ => 0,
    };
    (specific >> (element - 45)) & 1 != 0
}

/// `isAbandonedCampSpecial`: reproduces the camp pool selection.
fn is_abandoned_camp_special(rng: &mut JavaRandom, biome: i32) -> bool {
    rng.next_int(4);
    let template = rng.next_int(10);
    // the camp connector is the first connector before Util.shuffle()
    let mut camp_slot = 0;
    let count = camp_jigsaw_count(biome, template);
    let mut i = count;
    while i > 1 {
        let j = rng.next_int(i);
        if camp_slot == i - 1 {
            camp_slot = j;
        } else if camp_slot == j {
            camp_slot = i - 1;
        }
        i -= 1;
    }
    // each preceding tree only consumes the shuffle of four rotations
    for _ in 0..camp_slot {
        rng.next_int(4);
        rng.next_int(3);
        rng.next_int(2);
    }
    let mut pool: [u8; 49] = std::array::from_fn(|i| i as u8);
    for i in (2..=49).rev() {
        let j = rng.next_int(i) as usize;
        pool.swap(i as usize - 1, j);
    }
    camp_element_has_special_loot(biome, pool[0] as i32)
}

// ------------------------------------------------- layered viability maps --

/// `mapViableBiome`: the biome layer, stopping the generation (returning 1)
/// unless a biome the structure needs is among the cells. `l.data` holds the
/// structure type.
fn map_viable_biome(
    ls: &LayerStack,
    l: &Layer,
    out: &mut [i32],
    x: i32,
    z: i32,
    w: i32,
    h: i32,
) -> i32 {
    let err = map_biome(ls, l, out, x, z, w, h);
    if err != 0 {
        return err;
    }
    let styp = l.data[0];
    for &id in &out[..w as usize * h as usize] {
        let found = match styp {
            kind::DESERT_PYRAMID | kind::DESERT_WELL => id == DESERT || is_mesa(id),
            kind::JUNGLE_TEMPLE => id == JUNGLE,
            kind::SWAMP_HUT => id == SWAMP,
            kind::IGLOO => id == SNOWY_TUNDRA || id == SNOWY_TAIGA,
            kind::BURIED_TREASURE | kind::OCEAN_RUIN | kind::SHIPWRECK | kind::MONUMENT => {
                is_oceanic(id)
            }
            kind::MANSION => id == DARK_FOREST,
            _ => return 0,
        };
        if found {
            return 0;
        }
    }
    // required biomes not found: stop the generator
    1
}

/// `mapViableShore`: the shore layer, stopping the generation unless a
/// viable biome is among the cells. `l.data` holds the structure type and
/// the version.
fn map_viable_shore(
    ls: &LayerStack,
    l: &Layer,
    out: &mut [i32],
    x: i32,
    z: i32,
    w: i32,
    h: i32,
) -> i32 {
    let err = map_shore(ls, l, out, x, z, w, h);
    if err != 0 {
        return err;
    }
    let (styp, mc) = (l.data[0], l.data[1]);
    for &id in &out[..w as usize * h as usize] {
        match styp {
            kind::DESERT_PYRAMID
            | kind::JUNGLE_TEMPLE
            | kind::SWAMP_HUT
            | kind::IGLOO
            | kind::OCEAN_RUIN
            | kind::SHIPWRECK
            | kind::VILLAGE
            | kind::MONUMENT
            | kind::MANSION
            | kind::BURIED_TREASURE
            | kind::DESERT_WELL => {
                if is_viable_feature_biome(mc, styp, id) {
                    return 0;
                }
            }
            _ => return 0,
        }
    }
    1
}

/// The layer stack of an overworld generator (1.0 to 1.17) with the biome
/// and shore layers swapped for the viability maps, as `isViableStructurePos`
/// does with the generator.
fn viable_layers(g: &Generator, structure: i32) -> Option<LayerStack> {
    let mut ls = g.layer_stack()?.clone();
    let data = vec![structure, g.mc];
    ls.layers[L_BIOME_256].data = data.clone();
    ls.layers[L_BIOME_256].get_map = map_viable_biome;
    ls.layers[L_SHORE_16].data = data;
    ls.layers[L_SHORE_16].get_map = map_viable_shore;
    Some(ls)
}

/// A generator as the viability checks use it: the layer stack to sample
/// (possibly the swapped one) and the custom entry layer (`g->entry`).
struct View<'a> {
    g: &'a Generator,
    ls: Option<&'a LayerStack>,
    entry: Option<usize>,
}

impl<'a> View<'a> {
    fn of(g: &'a Generator) -> Self {
        Self {
            g,
            ls: g.layer_stack(),
            entry: g.entry,
        }
    }

    fn layer(&self, ls: &LayerStack, scale: i32) -> Option<usize> {
        match scale {
            0 => self.entry,
            1 => Some(ls.entry_1),
            4 => Some(ls.entry_4),
            16 => Some(ls.entry_16),
            64 => Some(ls.entry_64),
            256 => Some(ls.entry_256),
            _ => None,
        }
    }

    /// `getBiomeAt`: `NONE` when the generation fails or stops.
    fn biome(&self, scale: i32, x: i32, y: i32, z: i32) -> i32 {
        let Some(ls) = self.ls else {
            return self.g.biome_at(scale, x, y, z);
        };
        match self
            .layer(ls, scale)
            .and_then(|l| ls.gen_area_vec(l, x, z, 1, 1))
        {
            Some(v) => v[0],
            None => NONE,
        }
    }

    /// `genBiomes` of a scale 4 area; `None` on error.
    fn area4(&self, r: Range) -> Option<Vec<i32>> {
        let Some(ls) = self.ls else {
            return Some(self.g.gen_biomes(r));
        };
        ls.gen_area_vec(self.layer(ls, 4)?, r.x, r.z, r.sx, r.sz)
    }
}

/// `areBiomesViable` on a view.
#[allow(clippy::too_many_arguments)]
fn biomes_viable(
    v: &View,
    x: i32,
    y: i32,
    z: i32,
    rad: i32,
    valid_b: u64,
    valid_m: u64,
    approx: i32,
) -> bool {
    let (x1, x2) = ((x - rad) >> 2, (x + rad) >> 2);
    let (z1, z2) = ((z - rad) >> 2, (z + rad) >> 2);
    let (sx, sz) = (x2 - x1 + 1, z2 - z1 + 1);
    // From 1.18 the area is also checked in y; this is only used for
    // monuments (ocean or river), where the lowest y is enough.
    let y = (y - rad) >> 2;

    for (cx, cz) in [(x1, z1), (x2, z2), (x1, z2), (x2, z1)] {
        let id = v.biome(4, cx, y, cz);
        if id < 0 || !id_matches(id, valid_b, valid_m) {
            return false;
        }
    }
    if approx >= 1 {
        return true;
    }
    if v.g.mc >= MC_1_18 {
        let Some(bn) = v.g.overworld() else {
            return false;
        };
        for i in 0..sx {
            let mut dat = 0u64;
            for j in 0..sz {
                let id = bn.sample(x1 + i, y, z1 + j, Some(&mut dat), 0);
                if id < 0 || !id_matches(id, valid_b, valid_m) {
                    return false;
                }
            }
        }
        true
    } else {
        let r = Range {
            scale: 4,
            x: x1,
            z: z1,
            sx,
            sz,
            y,
            sy: 1,
        };
        let Some(ids) = v.area4(r) else {
            return false;
        };
        ids.iter().all(|&id| id_matches(id, valid_b, valid_m))
    }
}

/// `areBiomesViable`: whether every cell around (`x`, `y`, `z`) within
/// `rad` blocks has a biome of the sets (see [`id_matches`]); with `approx`
/// at 1 or more only the corners are checked.
#[allow(clippy::too_many_arguments)]
pub fn are_biomes_viable(
    g: &Generator,
    x: i32,
    y: i32,
    z: i32,
    rad: i32,
    valid_b: u64,
    valid_m: u64,
    approx: i32,
) -> bool {
    biomes_viable(&View::of(g), x, y, z, rad, valid_b, valid_m, approx)
}

// ------------------------------------------------------------ viability --

/// `isViableStructurePos`: 0 when the start does not generate, otherwise 1
/// or the biome that made it viable (villages, camps, End structures).
/// `flags` restricts villages to one biome (0 for any).
///
/// The version is `g.mc`. Up to 1.17 the check runs on a copy of the layer
/// stack with the viability maps swapped in (the C code swaps them in the
/// generator and restores them).
pub fn viable_structure_pos(structure: i32, g: &Generator, x: i32, z: i32, flags: u32) -> i32 {
    use kind::*;
    let mc = g.mc;
    let (cx, cz) = ((x >> 4) as i64, (z >> 4) as i64);
    let footprint = |v: &Variant| {
        (
            ((cx * 32 + 2 * v.x as i64 + v.sx as i64 - 1) / 2) >> 2,
            ((cz * 32 + 2 * v.z as i64 + v.sz as i64 - 1) / 2) >> 2,
        )
    };
    match g.dimension {
        Dimension::Nether => {
            if structure == FORTRESS && mc <= MC_1_17 {
                return 1;
            }
            if mc <= MC_1_15 {
                return 0;
            }
            if structure == RUINED_PORTAL_NETHER {
                return 1;
            }
            if structure == FORTRESS {
                // from 1.18 fortresses generate everywhere bastions don't
                let Some(c) = config(FORTRESS, mc) else {
                    return 0;
                };
                let (rx, rz) = (
                    floordiv(x, (c.region_size as i32) << 4),
                    floordiv(z, (c.region_size as i32) << 4),
                );
                if structure_pos(BASTION, mc, g.seed, rx, rz).is_none() {
                    return 1;
                }
                return (viable_structure_pos(BASTION, g, x, z, flags) == 0) as i32;
            }
            let mut sample_y = 0;
            let (sx, sz) = if mc >= MC_1_18 && structure == BASTION {
                let v = variant(BASTION, mc, g.seed, x, z, -1).expect("bastions have a variant");
                if mc >= MC_1_19_2 {
                    // nether biomes don't actually vary in Y
                    sample_y = 33 >> 2;
                }
                footprint(&v)
            } else {
                (cx * 4 + 2, cz * 4 + 2)
            };
            let id = g.biome_at(4, sx as i32, sample_y, sz as i32);
            return is_viable_feature_biome(mc, structure, id) as i32;
        }
        Dimension::End => {
            match structure {
                END_CITY if mc <= MC_1_8 => return 0,
                END_GATEWAY if mc <= MC_1_12 => return 0,
                END_CITY | END_GATEWAY => {}
                _ => return 0,
            }
            // End biomes vary only on a per-chunk scale (1:16)
            let id = g.biome_at(16, cx as i32, 0, cz as i32);
            return if is_viable_feature_biome(mc, structure, id) {
                id
            } else {
                0
            };
        }
        Dimension::Overworld => {}
    }

    let swapped = if mc <= MC_1_17 {
        viable_layers(g, structure)
    } else {
        None
    };
    let mut v = View::of(g);
    if swapped.is_some() {
        v.ls = swapped.as_ref();
    }
    let viable = |id: i32| (id >= 0 && is_viable_feature_biome(mc, structure, id)) as i32;

    match structure {
        ABANDONED_CAMP | TRAIL_RUINS | OCEAN_RUIN | SHIPWRECK | BURIED_TREASURE | IGLOO
        | DESERT_PYRAMID | JUNGLE_TEMPLE | SWAMP_HUT => {
            let unavailable = match structure {
                ABANDONED_CAMP => mc < MC_26_3,
                TRAIL_RUINS => mc <= MC_1_19,
                OCEAN_RUIN | SHIPWRECK | BURIED_TREASURE => mc <= MC_1_12,
                IGLOO => mc <= MC_1_8,
                _ => false,
            };
            if unavailable {
                return 0;
            }
            if structure == ABANDONED_CAMP {
                let (bx, bz) = camp_biome_pos(g.seed, cx as i32, cz as i32);
                let (sx, sz) = (floordiv(bx, 4), floordiv(bz, 4));
                // Jigsaw camps are projected to WORLD_SURFACE_WG and the biome
                // is checked at that height.
                let Some(bn) = g.overworld() else { return 0 };
                let depth = bn.climate_depth(sx as f64, sz as f64);
                let surface = (depth as f64 / 76.0) as f32;
                let sy = floordiv(surface.floor() as i32 + 1, 4);
                let id = v.biome(0, sx, sy, sz);
                return if viable(id) == 0 { 0 } else { id };
            }
            let (sx, sz);
            if mc <= MC_1_15 {
                v.entry = Some(L_VORONOI_1);
                (sx, sz) = (cx * 16 + 9, cz * 16 + 9);
            } else {
                if mc <= MC_1_17 {
                    v.entry = Some(L_RIVER_MIX_4);
                }
                (sx, sz) = (cx * 4 + 2, cz * 4 + 2);
            }
            viable(v.biome(0, sx as i32, 319 >> 2, sz as i32))
        }
        DESERT_WELL => {
            let (sx, sz);
            if mc <= MC_1_15 {
                v.entry = Some(L_VORONOI_1);
                (sx, sz) = (x, z);
            } else {
                if mc <= MC_1_17 {
                    v.entry = Some(L_RIVER_MIX_4);
                }
                (sx, sz) = (x >> 2, z >> 2);
            }
            viable(v.biome(0, sx, 319 >> 2, sz))
        }
        VILLAGE => {
            if mc <= MC_1_17 {
                let (sx, sz);
                if mc == MC_1_15 {
                    // only in 1.15 villages used the biome check of the other
                    // structures
                    v.entry = Some(L_VORONOI_1);
                    (sx, sz) = (cx * 16 + 9, cz * 16 + 9);
                } else {
                    v.entry = Some(L_RIVER_MIX_4);
                    (sx, sz) = (cx * 4 + 2, cz * 4 + 2);
                }
                let id = v.biome(0, sx as i32, 0, sz as i32);
                if id < 0 || !is_viable_feature_biome(mc, structure, id) {
                    return 0;
                }
                if flags != 0 && id as u32 != flags {
                    return 0;
                }
                if mc <= MC_1_9 {
                    // before 1.10 villages did not spread into invalid
                    // biomes, which could make them fail the first check at
                    // block (2, 2) of the start chunk
                    let id = v.biome(1, (cx * 16 + 2) as i32, 0, (cz * 16 + 2) as i32);
                    if id < 0 || !is_viable_feature_biome(mc, structure, id) {
                        return 0;
                    }
                }
                // the biome for viability, useful for further analysis
                return id;
            }
            // from 1.18 the village types are checked separately
            for vv in [PLAINS, DESERT, SAVANNA, TAIGA, SNOWY_TUNDRA] {
                if flags != 0 && flags != vv as u32 {
                    continue;
                }
                let sv =
                    variant(VILLAGE, mc, g.seed, x, z, vv).expect("village biomes have variants");
                let (sx, sz) = footprint(&sv);
                let id = v.biome(0, sx as i32, 319 >> 2, sz as i32);
                if id == vv || (id == MEADOW && vv == PLAINS) {
                    return vv;
                }
            }
            0
        }
        OUTPOST => {
            if mc <= MC_1_13 {
                return 0;
            }
            // `setAttemptSeed`
            let mut rng = {
                let s = g.seed
                    ^ ((cx as i32 >> 4) as i64 as u64)
                    ^ (((cz as i32 >> 4) as i64 as u64) << 4);
                let mut r = JavaRandom::new(s);
                r.next(31);
                r
            };
            if rng.next_int(5) != 0 {
                return 0;
            }
            // look for villages within 10 chunks
            let Some(vc) = config(VILLAGE, mc) else {
                return 0;
            };
            let (cx0, cx1, cz0, cz1) = (
                cx as i32 - 10,
                cx as i32 + 10,
                cz as i32 - 10,
                cz as i32 + 10,
            );
            let rs = vc.region_size as i32;
            for rz in floordiv(cz0, rs)..=floordiv(cz1, rs) {
                for rx in floordiv(cx0, rs)..=floordiv(cx1, rs) {
                    let p = feature_pos(&vc, g.seed, rx, rz);
                    let (pcx, pcz) = (p.0 >> 4, p.1 >> 4);
                    if pcx >= cx0 && pcx <= cx1 && pcz >= cz0 && pcz <= cz1 {
                        if mc >= MC_1_16_1 {
                            return 0;
                        }
                        if viable_structure_pos(VILLAGE, g, p.0, p.1, 0) != 0 {
                            return 0;
                        }
                    }
                }
            }
            let (sx, sz);
            if mc >= MC_1_18 {
                let mut rng = chunk_generate_rnd(g.seed, cx as i32, cz as i32);
                let (ox, oz) = match rng.next_int(4) {
                    0 => (15, 15),
                    1 => (-15, 15),
                    2 => (-15, -15),
                    _ => (15, -15),
                };
                sx = ((cx * 32 + ox) / 2) >> 2;
                sz = ((cz * 32 + oz) / 2) >> 2;
            } else if mc >= MC_1_16_1 {
                v.entry = Some(L_RIVER_MIX_4);
                (sx, sz) = (cx * 4 + 2, cz * 4 + 2);
            } else {
                v.entry = Some(L_VORONOI_1);
                (sx, sz) = (cx * 16 + 9, cz * 16 + 9);
            }
            viable(v.biome(0, sx as i32, 319 >> 2, sz as i32))
        }
        MONUMENT => {
            if mc <= MC_1_7 {
                return 0;
            }
            if mc == MC_1_8 {
                // in 1.8 monuments require only a single deep ocean block
                let id = v.biome(1, (cx * 16 + 8) as i32, 0, (cz * 16 + 8) as i32);
                if id < 0 || !is_deep_ocean(id) {
                    return 0;
                }
            } else if mc <= MC_1_17 {
                // Monuments require two viability checks with the ocean layer
                // branch, so check for potential deep ocean beforehand.
                v.entry = Some(L_SHORE_16);
                let id = v.biome(0, cx as i32, 0, cz as i32);
                if id < 0 || !is_deep_ocean(id) {
                    return 0;
                }
            }
            let (sx, sz) = ((cx * 16 + 8) as i32, (cz * 16 + 8) as i32);
            if (MC_1_9..=MC_1_17).contains(&mc) {
                // check for the deep ocean centre
                if !biomes_viable(&v, sx, 63, sz, 16, MONUMENT_BIOMES2, 0, 0) {
                    return 0;
                }
            } else if mc >= MC_1_18 {
                // the check is done at the ocean floor, about y = 36
                if !is_deep_ocean(v.biome(4, sx >> 2, 36 >> 2, sz >> 2)) {
                    return 0;
                }
            }
            biomes_viable(&v, sx, 63, sz, 29, MONUMENT_BIOMES1, 0, 0) as i32
        }
        MANSION => {
            if mc <= MC_1_10 {
                return 0;
            }
            if mc <= MC_1_17 {
                let (sx, sz) = ((cx * 16 + 8) as i32, (cz * 16 + 8) as i32);
                let b = bits(&[DARK_FOREST]);
                let m = 1u64 << (DARK_FOREST_HILLS - 128);
                if !biomes_viable(&v, sx, 0, sz, 32, b, m, 0) {
                    return 0;
                }
                1
            } else {
                // From 1.18 the generation takes the minimum surface height
                // among the four corners, which has to be at least 60; the
                // biome check is done at the centre (the height is not
                // checked here).
                let (sx, sz) = ((cx * 16 + 7) as i32, (cz * 16 + 7) as i32);
                viable(v.biome(4, sx >> 2, 319 >> 2, sz >> 2))
            }
        }
        RUINED_PORTAL | RUINED_PORTAL_NETHER => (mc > MC_1_15) as i32,
        GEODE => (mc > MC_1_16) as i32,
        ANCIENT_CITY | TRIAL_CHAMBERS => {
            if (structure == ANCIENT_CITY && mc <= MC_1_18)
                || (structure == TRIAL_CHAMBERS && mc <= MC_1_20)
            {
                return 0;
            }
            let sv = variant(structure, mc, g.seed, x, z, -1).expect("jigsaw starts have variants");
            let (sx, sz) = footprint(&sv);
            viable(v.biome(4, sx as i32, sv.y >> 2, sz as i32))
        }
        MINESHAFT => 1,
        _ => 0,
    }
}
