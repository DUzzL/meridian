//! Whether a structure start actually generates: its biome checks
//! (`isViableFeatureBiome`, `isViableStructurePos`), start variants
//! (`getVariant`) and abandoned camps, ported from the engine for 26.3.

use crate::biomes::*;
use crate::generator::{Dimension, Generator};
use crate::rng::{JavaRandom, floordiv};
use crate::structures::{chunk_generate_rnd, config, feature_pos, kind, structure_pos};

/// `isOverworld` for 26.3.
pub fn is_overworld(id: i32) -> bool {
    exists(id)
        && !(SMALL_END_ISLANDS..=END_BARRENS).contains(&id)
        && !(SOUL_SAND_VALLEY..=BASALT_DELTAS).contains(&id)
        && ![NETHER_WASTES, THE_END, 20, DEEP_WARM_OCEAN, 127].contains(&id)
}

fn bit_set(id: i32, bits: &[i32]) -> bool {
    (0..64).contains(&id) && bits.contains(&id)
}

pub fn is_oceanic(id: i32) -> bool {
    bit_set(
        id,
        &[
            OCEAN,
            FROZEN_OCEAN,
            WARM_OCEAN,
            LUKEWARM_OCEAN,
            COLD_OCEAN,
            DEEP_OCEAN,
            DEEP_WARM_OCEAN,
            DEEP_LUKEWARM_OCEAN,
            DEEP_COLD_OCEAN,
            DEEP_FROZEN_OCEAN,
        ],
    )
}

pub fn is_deep_ocean(id: i32) -> bool {
    bit_set(
        id,
        &[
            DEEP_OCEAN,
            DEEP_WARM_OCEAN,
            DEEP_LUKEWARM_OCEAN,
            DEEP_COLD_OCEAN,
            DEEP_FROZEN_OCEAN,
        ],
    )
}

/// `isViableFeatureBiome` for 26.3.
pub fn is_viable_feature_biome(structure: i32, b: i32) -> bool {
    use kind::*;
    match structure {
        DESERT_PYRAMID => b == DESERT || b == 17,
        JUNGLE_TEMPLE => b == JUNGLE || b == 22 || b == BAMBOO_JUNGLE || b == 169,
        SWAMP_HUT => b == SWAMP,
        IGLOO => b == SNOWY_PLAINS || b == SNOWY_TAIGA || b == SNOWY_SLOPES,
        OCEAN_RUIN => is_oceanic(b),
        SHIPWRECK => is_oceanic(b) || b == BEACH || b == SNOWY_BEACH,
        RUINED_PORTAL | RUINED_PORTAL_NETHER => true,
        ANCIENT_CITY => b == DEEP_DARK,
        TRAIL_RUINS => [
            TAIGA,
            SNOWY_TAIGA,
            OLD_GROWTH_PINE_TAIGA,
            OLD_GROWTH_SPRUCE_TAIGA,
            OLD_GROWTH_BIRCH_FOREST,
            JUNGLE,
        ]
        .contains(&b),
        TRIAL_CHAMBERS => b != DEEP_DARK && is_overworld(b),
        ABANDONED_CAMP => [
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
        .contains(&b),
        BURIED_TREASURE => b == BEACH || b == SNOWY_BEACH,
        MINESHAFT => is_overworld(b),
        DESERT_WELL => b == DESERT,
        MONUMENT => is_deep_ocean(b),
        OUTPOST => [
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
        .contains(&b),
        VILLAGE => [PLAINS, DESERT, SAVANNA, TAIGA, SNOWY_PLAINS, MEADOW].contains(&b),
        MANSION => b == PALE_GARDEN || b == DARK_FOREST || b == DARK_FOREST + 128,
        FORTRESS => [
            NETHER_WASTES,
            SOUL_SAND_VALLEY,
            WARPED_FOREST,
            CRIMSON_FOREST,
            BASALT_DELTAS,
        ]
        .contains(&b),
        BASTION => [
            NETHER_WASTES,
            SOUL_SAND_VALLEY,
            WARPED_FOREST,
            CRIMSON_FOREST,
        ]
        .contains(&b),
        END_CITY => b == END_MIDLANDS || b == END_HIGHLANDS,
        END_GATEWAY => b == END_HIGHLANDS,
        _ => false,
    }
}

/// A start piece's footprint relative to the start chunk's corner, and its
/// rotation and template (`StructureVariant`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Variant {
    pub biome: i32,
    pub rotation: i32,
    pub start: i32,
    pub abandoned: bool,
    pub special: bool,
    pub x: i32,
    pub y: i32,
    pub z: i32,
    pub sx: i32,
    pub sy: i32,
    pub sz: i32,
}

fn rotate(v: &mut Variant, sx: i32, sy: i32, sz: i32) {
    v.sy = sy;
    (v.x, v.z, v.sx, v.sz) = match v.rotation {
        0 => (0, 0, sx, sz),
        1 => (1 - sz, 0, sz, sx),
        2 => (1 - sx, 1 - sz, sx, sz),
        _ => (0, 1 - sx, sz, sx),
    };
}

/// `getVariant` for the structures the checks need.
pub fn variant(structure: i32, seed: u64, x: i32, z: i32, biome: i32) -> Option<Variant> {
    let mut rng = chunk_generate_rnd(seed, x >> 4, z >> 4);
    let mut v = Variant {
        start: -1,
        biome: -1,
        y: 320,
        ..Default::default()
    };
    match structure {
        kind::ABANDONED_CAMP => {
            if !is_viable_feature_biome(structure, biome) {
                return None;
            }
            v.biome = biome;
            v.special = is_abandoned_camp_special(&mut rng, biome);
            Some(v)
        }
        kind::VILLAGE => {
            if !is_viable_feature_biome(kind::VILLAGE, biome) {
                return None;
            }
            v.biome = biome;
            v.rotation = rng.next_int(4);
            // (bound, start, sx, sy, sz, abandoned) per template.
            let table: &[(i32, i32, i32, i32, i32, bool)] = match biome {
                PLAINS | MEADOW => &[
                    (50, 0, 9, 4, 9, false),
                    (100, 1, 10, 7, 10, false),
                    (150, 2, 8, 5, 15, false),
                    (200, 3, 11, 9, 11, false),
                    (201, 0, 9, 4, 9, true),
                    (202, 1, 10, 7, 10, true),
                    (203, 2, 8, 5, 15, true),
                    (204, 3, 11, 9, 11, true),
                ],
                DESERT => &[
                    (98, 1, 17, 6, 9, false),
                    (196, 2, 12, 6, 12, false),
                    (245, 3, 15, 6, 15, false),
                    (247, 1, 17, 6, 9, true),
                    (249, 2, 12, 6, 12, true),
                    (250, 3, 15, 6, 15, true),
                ],
                SAVANNA => &[
                    (100, 1, 14, 5, 12, false),
                    (150, 2, 11, 6, 11, false),
                    (300, 3, 9, 6, 11, false),
                    (450, 4, 9, 6, 9, false),
                    (452, 1, 14, 5, 12, true),
                    (453, 2, 11, 6, 11, true),
                    (456, 3, 9, 6, 11, true),
                    (459, 4, 9, 6, 9, true),
                ],
                TAIGA => &[
                    (49, 1, 22, 3, 18, false),
                    (98, 2, 9, 7, 9, false),
                    (99, 1, 22, 3, 18, true),
                    (100, 2, 9, 7, 9, true),
                ],
                SNOWY_PLAINS => &[
                    (100, 1, 12, 8, 8, false),
                    (150, 2, 11, 5, 9, false),
                    (300, 3, 7, 7, 7, false),
                    (302, 1, 12, 8, 8, true),
                    (303, 2, 11, 5, 9, true),
                    (306, 3, 7, 7, 7, true),
                ],
                _ => return None,
            };
            if biome == MEADOW {
                v.biome = PLAINS;
            }
            let t = rng.next_int(table.last().expect("templates").0);
            let &(_, start, sx, sy, sz, abandoned) =
                table.iter().find(|e| t < e.0).expect("in range");
            v.start = start;
            v.abandoned = abandoned;
            rotate(&mut v, sx, sy, sz);
            Some(v)
        }
        kind::BASTION => {
            v.rotation = rng.next_int(4);
            v.start = rng.next_int(4);
            let (sx, sy, sz) = match v.start {
                0 => (46, 24, 46),
                1 => (30, 24, 48),
                2 => (38, 48, 38),
                _ => (16, 32, 32),
            };
            rotate(&mut v, sx, sy, sz);
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
        kind::TRIAL_CHAMBERS => {
            v.y = rng.next_int(21) - 40;
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

fn camp_biome_pos(seed: u64, cx: i32, cz: i32) -> (i32, i32) {
    let mut rng = chunk_generate_rnd(seed, cx, cz);
    let rotation = rng.next_int(4);
    let template = rng.next_int(10);
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
    ((min_x + max_x) / 2, (min_z + max_z) / 2)
}

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

fn is_abandoned_camp_special(rng: &mut JavaRandom, biome: i32) -> bool {
    rng.next_int(4);
    let template = rng.next_int(10);
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

// ------------------------------------------------------------ viability --

/// `isViableStructurePos` for 26.3: 0 when the start does not generate,
/// otherwise 1 or the biome that made it viable (villages, camps, End).
pub fn viable_structure_pos(structure: i32, g: &Generator, x: i32, z: i32, flags: u32) -> i32 {
    use kind::*;
    let (cx, cz) = ((x >> 4) as i64, (z >> 4) as i64);
    let biome4 = |sx: i64, sy: i32, sz: i64| g.biome_at(4, sx as i32, sy, sz as i32);
    let biome0 = |sx: i64, sy: i32, sz: i64| g.biome_at(0, sx as i32, sy, sz as i32);
    let footprint = |v: &Variant| {
        (
            ((cx * 32 + 2 * v.x as i64 + v.sx as i64 - 1) / 2) >> 2,
            ((cz * 32 + 2 * v.z as i64 + v.sz as i64 - 1) / 2) >> 2,
        )
    };
    match g.dimension {
        Dimension::Nether => {
            if structure == RUINED_PORTAL_NETHER {
                return 1;
            }
            if structure == FORTRESS {
                let c = config(FORTRESS, g.mc).expect("configured");
                let (rx, rz) = (
                    floordiv(x, (c.region_size as i32) << 4),
                    floordiv(z, (c.region_size as i32) << 4),
                );
                if structure_pos(BASTION, g.mc, g.seed, rx, rz).is_none() {
                    return 1;
                }
                return (viable_structure_pos(BASTION, g, x, z, flags) == 0) as i32;
            }
            let (sx, sy, sz) = if structure == BASTION {
                let v = variant(BASTION, g.seed, x, z, -1).expect("bastions always have a variant");
                let (sx, sz) = footprint(&v);
                (sx, 33 >> 2, sz)
            } else {
                (cx * 4 + 2, 0, cz * 4 + 2)
            };
            return is_viable_feature_biome(structure, biome4(sx, sy, sz)) as i32;
        }
        Dimension::End => {
            if structure != END_CITY && structure != END_GATEWAY {
                return 0;
            }
            let id = g.biome_at(16, cx as i32, 0, cz as i32);
            return if is_viable_feature_biome(structure, id) {
                id
            } else {
                0
            };
        }
        Dimension::Overworld => {}
    }
    let viable = |id: i32| {
        if id < 0 || !is_viable_feature_biome(structure, id) {
            0
        } else {
            1
        }
    };
    match structure {
        ABANDONED_CAMP => {
            let (bx, bz) = camp_biome_pos(g.seed, cx as i32, cz as i32);
            let (sx, sz) = (floordiv(bx, 4), floordiv(bz, 4));
            let Some(bn) = g.overworld() else { return 0 };
            let depth = bn.climate_depth(sx as f64, sz as f64);
            let surface = (depth as f64 / 76.0) as f32;
            let sy = floordiv(surface.floor() as i32 + 1, 4);
            let id = biome0(sx as i64, sy, sz as i64);
            if viable(id) == 0 { 0 } else { id }
        }
        TRAIL_RUINS | OCEAN_RUIN | SHIPWRECK | BURIED_TREASURE | IGLOO | DESERT_PYRAMID
        | JUNGLE_TEMPLE | SWAMP_HUT => viable(biome0(cx * 4 + 2, 319 >> 2, cz * 4 + 2)),
        DESERT_WELL => viable(biome0((x >> 2) as i64, 319 >> 2, (z >> 2) as i64)),
        VILLAGE => {
            for vv in [PLAINS, DESERT, SAVANNA, TAIGA, SNOWY_PLAINS] {
                if flags != 0 && flags != vv as u32 {
                    continue;
                }
                let v = variant(VILLAGE, g.seed, x, z, vv).expect("village biomes have variants");
                let (sx, sz) = footprint(&v);
                let id = biome0(sx, 319 >> 2, sz);
                if id == vv || (id == MEADOW && vv == PLAINS) {
                    return vv;
                }
            }
            0
        }
        OUTPOST => {
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
            let vc = config(VILLAGE, g.mc).expect("configured");
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
                        return 0;
                    }
                }
            }
            let mut rng = chunk_generate_rnd(g.seed, cx as i32, cz as i32);
            let (ox, oz) = match rng.next_int(4) {
                0 => (15, 15),
                1 => (-15, 15),
                2 => (-15, -15),
                _ => (15, -15),
            };
            let sx = ((cx * 32 + ox) / 2) >> 2;
            let sz = ((cz * 32 + oz) / 2) >> 2;
            viable(biome0(sx, 319 >> 2, sz))
        }
        MONUMENT => {
            let (sx, sz) = ((cx * 16 + 8) as i32, (cz * 16 + 8) as i32);
            if !is_deep_ocean(g.biome_at(4, sx >> 2, 36 >> 2, sz >> 2)) {
                return 0;
            }
            areas_viable(g, sx, 63, sz, 29) as i32
        }
        MANSION => viable(biome4((cx * 16 + 7) >> 2, 319 >> 2, (cz * 16 + 7) >> 2)),
        RUINED_PORTAL | RUINED_PORTAL_NETHER | GEODE | MINESHAFT => 1,
        ANCIENT_CITY | TRIAL_CHAMBERS => {
            let v = variant(structure, g.seed, x, z, -1).expect("jigsaw starts have variants");
            let (sx, sz) = footprint(&v);
            viable(biome4(sx, v.y >> 2, sz))
        }
        _ => 0,
    }
}

/// `areBiomesViable` for monuments (1.18+): the corners, then every cell.
fn areas_viable(g: &Generator, x: i32, y: i32, z: i32, rad: i32) -> bool {
    const VALID: [i32; 13] = [
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
        OCEAN,
    ];
    let ok = |id: i32| bit_set(id, &VALID);
    let (x1, x2) = ((x - rad) >> 2, (x + rad) >> 2);
    let (z1, z2) = ((z - rad) >> 2, (z + rad) >> 2);
    let y = (y - rad) >> 2;
    for (cx, cz) in [(x1, z1), (x2, z2), (x1, z2), (x2, z1)] {
        if !ok(g.biome_at(4, cx, y, cz)) {
            return false;
        }
    }
    let Some(bn) = g.overworld() else {
        return false;
    };
    for i in 0..=(x2 - x1) {
        let mut dat = 0u64;
        for j in 0..=(z2 - z1) {
            if !ok(bn.sample(x1 + i, y, z1 + j, Some(&mut dat), 0)) {
                return false;
            }
        }
    }
    true
}
