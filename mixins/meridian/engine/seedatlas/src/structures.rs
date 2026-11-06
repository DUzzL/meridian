//! Where structures may start, from the seed alone (`getStructureConfig`,
//! `getStructurePos` and their helpers in the engine's `finders.c`).

use crate::generator::Dimension;
use crate::mc::{MC_1_13, MC_1_16, MC_1_16_1, MC_1_17, MC_1_18};
use crate::rng::{JavaRandom, Xoroshiro};

/// The engine's structure numbers (also the Fabric interface's, 1..25).
pub mod kind {
    /// Pre-1.13 temple generation attempts.
    pub const FEATURE: i32 = 0;
    pub const DESERT_PYRAMID: i32 = 1;
    pub const JUNGLE_TEMPLE: i32 = 2;
    pub const SWAMP_HUT: i32 = 3;
    pub const IGLOO: i32 = 4;
    pub const VILLAGE: i32 = 5;
    pub const OCEAN_RUIN: i32 = 6;
    pub const SHIPWRECK: i32 = 7;
    pub const MONUMENT: i32 = 8;
    pub const MANSION: i32 = 9;
    pub const OUTPOST: i32 = 10;
    pub const RUINED_PORTAL: i32 = 11;
    pub const RUINED_PORTAL_NETHER: i32 = 12;
    pub const ANCIENT_CITY: i32 = 13;
    pub const BURIED_TREASURE: i32 = 14;
    pub const MINESHAFT: i32 = 15;
    pub const DESERT_WELL: i32 = 16;
    pub const GEODE: i32 = 17;
    pub const FORTRESS: i32 = 18;
    pub const BASTION: i32 = 19;
    pub const END_CITY: i32 = 20;
    pub const END_GATEWAY: i32 = 21;
    pub const END_ISLAND: i32 = 22;
    pub const TRAIL_RUINS: i32 = 23;
    pub const TRIAL_CHAMBERS: i32 = 24;
    pub const ABANDONED_CAMP: i32 = 25;
    /// One past the last engine structure.
    pub const COUNT: i32 = 26;
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StructureConfig {
    pub salt: i32,
    pub region_size: i8,
    pub chunk_range: i8,
    pub kind: i32,
    pub dimension: Dimension,
    pub rarity: f32,
}

const fn sc(
    salt: i32,
    region_size: i8,
    chunk_range: i8,
    kind: i32,
    dimension: Dimension,
    rarity: f32,
) -> StructureConfig {
    StructureConfig {
        salt,
        region_size,
        chunk_range,
        kind,
        dimension,
        rarity,
    }
}

/// `getStructureConfig`: the placement of a structure kind in version `mc`,
/// or `None` when the version does not generate it.
pub fn config(kind: i32, mc: i32) -> Option<StructureConfig> {
    use crate::mc::*;
    use Dimension::*;
    use kind::*;
    let (avail, c) = match kind {
        FEATURE => (mc <= MC_1_12, sc(14357617, 32, 24, kind, Overworld, 0.0)),
        DESERT_PYRAMID => (mc >= MC_1_3, sc(14357617, 32, 24, kind, Overworld, 0.0)),
        JUNGLE_TEMPLE => (
            mc >= MC_1_3,
            sc(
                if mc <= MC_1_12 { 14357617 } else { 14357619 },
                32,
                24,
                kind,
                Overworld,
                0.0,
            ),
        ),
        SWAMP_HUT => (
            mc >= MC_1_4,
            sc(
                if mc <= MC_1_12 { 14357617 } else { 14357620 },
                32,
                24,
                kind,
                Overworld,
                0.0,
            ),
        ),
        IGLOO => (
            mc >= MC_1_9,
            sc(
                if mc <= MC_1_12 { 14357617 } else { 14357618 },
                32,
                24,
                kind,
                Overworld,
                0.0,
            ),
        ),
        VILLAGE => (
            mc >= MC_B1_8,
            if mc <= MC_1_17 {
                sc(10387312, 32, 24, kind, Overworld, 0.0)
            } else {
                sc(10387312, 34, 26, kind, Overworld, 0.0)
            },
        ),
        OCEAN_RUIN => (
            mc >= MC_1_13,
            if mc <= MC_1_15 {
                sc(14357621, 16, 8, kind, Overworld, 0.0)
            } else {
                sc(14357621, 20, 12, kind, Overworld, 0.0)
            },
        ),
        SHIPWRECK => (
            mc >= MC_1_13,
            if mc <= MC_1_15 {
                sc(165745295, 16, 8, kind, Overworld, 0.0)
            } else {
                sc(165745295, 24, 20, kind, Overworld, 0.0)
            },
        ),
        MONUMENT => (mc >= MC_1_8, sc(10387313, 32, 27, kind, Overworld, 0.0)),
        MANSION => (mc >= MC_1_11, sc(10387319, 80, 60, kind, Overworld, 0.0)),
        OUTPOST => (mc >= MC_1_14, sc(165745296, 32, 24, kind, Overworld, 0.0)),
        RUINED_PORTAL => (mc >= MC_1_16_1, sc(34222645, 40, 25, kind, Overworld, 0.0)),
        RUINED_PORTAL_NETHER => (
            mc >= MC_1_16_1,
            if mc <= MC_1_17 {
                sc(34222645, 25, 15, RUINED_PORTAL_NETHER, Nether, 0.0)
            } else {
                sc(34222645, 40, 25, RUINED_PORTAL, Nether, 0.0)
            },
        ),
        ANCIENT_CITY => (mc >= MC_1_19_2, sc(20083232, 24, 16, kind, Overworld, 0.0)),
        BURIED_TREASURE => (mc >= MC_1_13, sc(10387320, 1, 1, kind, Overworld, 0.0)),
        MINESHAFT => (mc >= MC_B1_8, sc(0, 1, 1, kind, Overworld, 0.0)),
        DESERT_WELL => (
            mc >= MC_1_13,
            sc(
                if mc <= MC_1_15 {
                    30010
                } else if mc <= MC_1_17 {
                    40013
                } else {
                    40002
                },
                1,
                1,
                kind,
                Overworld,
                1.0 / 1000.0,
            ),
        ),
        GEODE => (
            mc >= MC_1_17,
            sc(
                if mc <= MC_1_17 { 20000 } else { 20002 },
                1,
                1,
                kind,
                Overworld,
                1.0 / 24.0,
            ),
        ),
        FORTRESS => (
            mc >= MC_1_0,
            if mc <= MC_1_15 {
                sc(0, 16, 8, kind, Nether, 0.0)
            } else {
                sc(30084232, 27, 23, kind, Nether, 0.0)
            },
        ),
        BASTION => (mc >= MC_1_16_1, sc(30084232, 27, 23, kind, Nether, 0.0)),
        END_CITY => (mc >= MC_1_9, sc(10387313, 20, 9, kind, End, 0.0)),
        END_GATEWAY => (
            mc >= MC_1_13,
            if mc <= MC_1_15 {
                sc(30000, 1, 1, kind, End, 700.0)
            } else if mc <= MC_1_16 {
                sc(40013, 1, 1, kind, End, 700.0)
            } else if mc <= MC_1_17 {
                sc(40013, 1, 1, kind, End, 1.0 / 700.0)
            } else {
                sc(40000, 1, 1, kind, End, 1.0 / 700.0)
            },
        ),
        END_ISLAND => (
            mc >= MC_1_13,
            if mc <= MC_1_16 {
                sc(0, 1, 1, kind, End, 14.0)
            } else {
                sc(0, 1, 1, kind, End, 1.0 / 14.0)
            },
        ),
        TRAIL_RUINS => (mc >= MC_1_20, sc(83469867, 34, 26, kind, Overworld, 0.0)),
        TRIAL_CHAMBERS => (mc >= MC_1_21_1, sc(94251327, 34, 22, kind, Overworld, 0.0)),
        ABANDONED_CAMP => (mc >= MC_26_3, sc(91231127, 37, 29, kind, Overworld, 0.0)),
        _ => return None,
    };
    avail.then_some(c)
}

fn region_seed(seed: u64, rx: i32, rz: i32, salt: i32) -> JavaRandom {
    JavaRandom::new(
        seed.wrapping_add((rx as i64 as u64).wrapping_mul(341873128712))
            .wrapping_add((rz as i64 as u64).wrapping_mul(132897987541))
            .wrapping_add(salt as i64 as u64),
    )
}

fn to_block(region: i32, size: i8, offset: i32) -> i32 {
    (((region as i64 as u64)
        .wrapping_mul(size as i64 as u64)
        .wrapping_add(offset as i64 as u64))
        << 4) as i32
}

/// `getFeaturePos`: a linear spread start in a region.
pub fn feature_pos(c: &StructureConfig, seed: u64, rx: i32, rz: i32) -> (i32, i32) {
    let mut s = region_seed(seed, rx, rz, c.salt);
    let x = s.next_int(c.chunk_range as i32);
    let z = s.next_int(c.chunk_range as i32);
    (
        to_block(rx, c.region_size, x),
        to_block(rz, c.region_size, z),
    )
}

/// `getLargeStructurePos`: a triangular spread start in a region.
pub fn large_structure_pos(c: &StructureConfig, seed: u64, rx: i32, rz: i32) -> (i32, i32) {
    let mut s = region_seed(seed, rx, rz, c.salt);
    let r = c.chunk_range as i32;
    let x = (s.next_int(r) + s.next_int(r)) >> 1;
    let z = (s.next_int(r) + s.next_int(r)) >> 1;
    (
        to_block(rx, c.region_size, x),
        to_block(rz, c.region_size, z),
    )
}

/// `setAttemptSeed`.
fn attempt_seed(seed: u64, cx: i32, cz: i32) -> JavaRandom {
    let s = seed ^ ((cx >> 4) as i64 as u64) ^ (((cz >> 4) as i64 as u64) << 4);
    let mut r = JavaRandom::new(s);
    r.next(31);
    r
}

/// `getPopulationSeed`.
pub fn population_seed(mc: i32, ws: u64, x: i32, z: i32) -> u64 {
    let (mut a, mut b);
    if mc >= MC_1_18 {
        let mut xr = Xoroshiro::new(ws);
        a = xr.next_long_j();
        b = xr.next_long_j();
    } else {
        let mut s = JavaRandom::new(ws);
        a = s.next_long();
        b = s.next_long();
    }
    if mc >= MC_1_13 {
        a |= 1;
        b |= 1;
    } else {
        a = (a as i64 / 2).wrapping_mul(2).wrapping_add(1) as u64;
        b = (b as i64 / 2).wrapping_mul(2).wrapping_add(1) as u64;
    }
    ((x as i64 as u64).wrapping_mul(a)).wrapping_add((z as i64 as u64).wrapping_mul(b)) ^ ws
}

/// `chunkGenerateRnd`.
pub fn chunk_generate_rnd(ws: u64, cx: i32, cz: i32) -> JavaRandom {
    let mut r = JavaRandom::new(ws);
    let a = r.next_long().wrapping_mul(cx as i64 as u64);
    let b = r.next_long().wrapping_mul(cz as i64 as u64);
    JavaRandom::new(a ^ b ^ ws)
}

/// `getRegPos`: like `feature_pos`, but also returns the generator.
fn reg_pos(c: &StructureConfig, seed: u64, rx: i32, rz: i32) -> ((i32, i32), JavaRandom) {
    let mut s = region_seed(seed, rx, rz, c.salt);
    let x = s.next_int(c.chunk_range as i32);
    let z = s.next_int(c.chunk_range as i32);
    (
        (
            to_block(rx, c.region_size, x),
            to_block(rz, c.region_size, z),
        ),
        s,
    )
}

/// `getStructurePos`: the start of `kind` in region (`rx`, `rz`), if one
/// may generate there by the seed alone. `None` as well when the version
/// does not have the structure.
pub fn structure_pos(kind: i32, mc: i32, seed: u64, rx: i32, rz: i32) -> Option<(i32, i32)> {
    use kind::*;
    let c = config(kind, mc)?;
    match kind {
        FEATURE | DESERT_PYRAMID | JUNGLE_TEMPLE | SWAMP_HUT | IGLOO | VILLAGE | OCEAN_RUIN
        | SHIPWRECK | RUINED_PORTAL | RUINED_PORTAL_NETHER | ANCIENT_CITY | TRAIL_RUINS
        | TRIAL_CHAMBERS | ABANDONED_CAMP => Some(feature_pos(&c, seed, rx, rz)),
        MONUMENT | MANSION => Some(large_structure_pos(&c, seed, rx, rz)),
        END_CITY => {
            let p = large_structure_pos(&c, seed, rx, rz);
            (p.0 as i64 * p.0 as i64 + p.1 as i64 * p.1 as i64 >= 1008 * 1008).then_some(p)
        }
        OUTPOST => {
            let p = feature_pos(&c, seed, rx, rz);
            let mut s = attempt_seed(seed, p.0 >> 4, p.1 >> 4);
            (s.next_int(5) == 0).then_some(p)
        }
        BURIED_TREASURE => {
            let p = (
                rx.wrapping_mul(16).wrapping_add(9),
                rz.wrapping_mul(16).wrapping_add(9),
            );
            let mut s = region_seed(seed, rx, rz, c.salt);
            ((s.next_float() as f64) < 0.01).then_some(p)
        }
        MINESHAFT => {
            mineshaft(mc, seed, rx, rz).then_some((rx.wrapping_mul(16), rz.wrapping_mul(16)))
        }
        FORTRESS => {
            if mc >= MC_1_18 {
                // Fortresses generate where bastions do not (biome dependent).
                Some(feature_pos(&c, seed, rx, rz))
            } else if mc >= MC_1_16_1 {
                let (p, mut s) = reg_pos(&c, seed, rx, rz);
                (s.next_int(5) < 2).then_some(p)
            } else {
                let mut s = attempt_seed(seed, rx.wrapping_mul(16), rz.wrapping_mul(16));
                let valid = s.next_int(3) == 0;
                let x = rx
                    .wrapping_mul(16)
                    .wrapping_add(s.next_int(8))
                    .wrapping_add(4);
                let z = rz
                    .wrapping_mul(16)
                    .wrapping_add(s.next_int(8))
                    .wrapping_add(4);
                valid.then_some((x.wrapping_mul(16), z.wrapping_mul(16)))
            }
        }
        BASTION => {
            if mc >= MC_1_18 {
                let p = feature_pos(&c, seed, rx, rz);
                let mut s = chunk_generate_rnd(seed, p.0 >> 4, p.1 >> 4);
                (s.next_int(5) >= 2).then_some(p)
            } else {
                let (p, mut s) = reg_pos(&c, seed, rx, rz);
                (s.next_int(5) >= 2).then_some(p)
            }
        }
        END_GATEWAY | END_ISLAND | DESERT_WELL | GEODE => {
            // Decorator features.
            let (x, z) = (rx.wrapping_mul(16), rz.wrapping_mul(16));
            let s = population_seed(mc, seed, x, z);
            let salted = s.wrapping_add(c.salt as i64 as u64);
            if mc >= MC_1_18 {
                let mut xr = Xoroshiro::new(salted);
                if xr.next_float() >= c.rarity {
                    return None;
                }
                let dx = xr.next_int_j(16);
                let dz = xr.next_int_j(16);
                Some((x.wrapping_add(dx), z.wrapping_add(dz)))
            } else {
                let mut r = JavaRandom::new(salted);
                if c.rarity < 1.0 {
                    if r.next_float() >= c.rarity {
                        return None;
                    }
                } else if r.next_int(c.rarity as i32) != 0 {
                    return None;
                }
                let dx = r.next_int(16);
                let dz = r.next_int(16);
                Some((x.wrapping_add(dx), z.wrapping_add(dz)))
            }
        }
        _ => None,
    }
}

/// `getMineshafts` for one chunk.
fn mineshaft(mc: i32, seed: u64, cx: i32, cz: i32) -> bool {
    let mut s = JavaRandom::new(seed);
    let a = s.next_long();
    let b = s.next_long();
    let aix = (cx as i64 as u64).wrapping_mul(a) ^ seed;
    let mut s = JavaRandom::new(aix ^ (cz as i64 as u64).wrapping_mul(b));
    if mc >= MC_1_13 {
        return s.next_double() < 0.004;
    }
    s.skip(1);
    if s.next_double() < 0.004 {
        let mut d = cx;
        if cx.wrapping_neg() > d {
            d = cx.wrapping_neg();
        }
        if cz > d {
            d = cz;
        }
        if cz.wrapping_neg() > d {
            d = cz.wrapping_neg();
        }
        return d >= 80 || s.next_int(80) < d;
    }
    false
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EndIsland {
    pub x: i32,
    pub y: i32,
    pub z: i32,
    pub r: i32,
}

/// `for (r = radius; r > 0.5; r -= draw() + 0.5);` with a float `r`: only
/// consumes the generator.
fn shrink_radius(radius: i32, mut draw: impl FnMut() -> i32) {
    let mut r = radius as f32;
    while r as f64 > 0.5 {
        r = (r as f64 - (draw() as f64 + 0.5)) as f32;
    }
}

/// `getEndIslands`: the small islands a chunk's decorator places (none
/// before 1.13).
pub fn end_islands(mc: i32, seed: u64, cx: i32, cz: i32) -> Vec<EndIsland> {
    let Some(c) = config(kind::END_ISLAND, mc) else {
        return Vec::new();
    };
    let (x, z) = (cx.wrapping_mul(16), cz.wrapping_mul(16));
    let rng = population_seed(mc, seed, x, z);
    let salted = rng.wrapping_add(c.salt as i64 as u64);
    let isl = |x: i32, y: i32, z: i32, r: i32| EndIsland { x, y, z, r };
    if mc <= MC_1_16 {
        let mut r = JavaRandom::new(salted);
        if r.next_int(c.rarity as i32) != 0 {
            return Vec::new();
        }
        let x0 = r.next_int(16).wrapping_add(x);
        let y0 = r.next_int(16) + 55;
        let z0 = r.next_int(16).wrapping_add(z);
        if r.next_int(4) != 0 {
            let r0 = r.next_int(3) + 4;
            return vec![isl(x0, y0, z0, r0)];
        }
        let x1 = r.next_int(16).wrapping_add(x);
        let y1 = r.next_int(16) + 55;
        let z1 = r.next_int(16).wrapping_add(z);
        let r0 = r.next_int(3) + 4;
        shrink_radius(r0, || r.next_int(2));
        let r1 = r.next_int(3) + 4;
        vec![isl(x0, y0, z0, r0), isl(x1, y1, z1, r1)]
    } else if mc <= MC_1_17 {
        let mut r = JavaRandom::new(salted);
        if r.next_float() >= c.rarity {
            return Vec::new();
        }
        let second = r.next_int(4) == 0;
        let x0 = r.next_int(16).wrapping_add(x);
        let z0 = r.next_int(16).wrapping_add(z);
        let y0 = r.next_int(16) + 55;
        let r0 = r.next_int(3) + 4;
        shrink_radius(r0, || r.next_int(2));
        if !second {
            return vec![isl(x0, y0, z0, r0)];
        }
        let x1 = r.next_int(16).wrapping_add(x);
        let z1 = r.next_int(16).wrapping_add(z);
        let y1 = r.next_int(16) + 55;
        let r1 = r.next_int(3) + 4;
        vec![isl(x0, y0, z0, r0), isl(x1, y1, z1, r1)]
    } else {
        let mut xr = Xoroshiro::new(salted);
        if xr.next_float() >= c.rarity {
            return Vec::new();
        }
        let second = xr.next_int_j(4) == 3;
        let x0 = xr.next_int_j(16).wrapping_add(x);
        let z0 = xr.next_int_j(16).wrapping_add(z);
        let y0 = xr.next_int_j(16) + 55;
        let r0 = xr.next_int_j(3) + 4;
        if !second {
            return vec![isl(x0, y0, z0, r0)];
        }
        shrink_radius(r0, || xr.next_int_j(2));
        let x1 = xr.next_int_j(16).wrapping_add(x);
        let z1 = xr.next_int_j(16).wrapping_add(z);
        let y1 = xr.next_int_j(16) + 55;
        let r1 = xr.next_int_j(3) + 4;
        vec![isl(x0, y0, z0, r0), isl(x1, y1, z1, r1)]
    }
}

/// `getFixedEndGateways`: the 20 gateways around the exit portal, in the
/// order the dragon fights open them.
pub fn fixed_end_gateways(_mc: i32, seed: u64) -> [(i32, i32); 20] {
    const FIXED: [(i32, i32); 20] = [
        (96, 0),
        (91, 29),
        (77, 56),
        (56, 77),
        (29, 91),
        (-1, 96),
        (-30, 91),
        (-57, 77),
        (-78, 56),
        (-92, 29),
        (-96, -1),
        (-92, -30),
        (-78, -57),
        (-57, -78),
        (-30, -92),
        (0, -96),
        (29, -92),
        (56, -78),
        (77, -57),
        (91, -30),
    ];
    let mut order: [usize; 20] = std::array::from_fn(|i| 19 - i);
    let mut rng = JavaRandom::new(seed);
    for i in 0..20 {
        let j = (19 - rng.next_int(20 - i as i32)) as u8 as usize;
        order.swap(i, j);
    }
    std::array::from_fn(|i| FIXED[order[i]])
}

/// Whether the chunk is a slime chunk (the Fabric interface's formula).
pub fn is_slime_chunk(seed: u64, cx: i32, cz: i32) -> bool {
    let (x, z) = (cx as u32, cz as u32);
    let mut r = seed;
    r = r.wrapping_add(x.wrapping_mul(0x5ac0db) as i32 as i64 as u64);
    r = r.wrapping_add(x.wrapping_mul(x).wrapping_mul(0x4c1906) as i32 as i64 as u64);
    r = r.wrapping_add(z.wrapping_mul(0x5f24f) as i32 as i64 as u64);
    r = r.wrapping_add((z.wrapping_mul(z) as i32 as i64 as u64).wrapping_mul(0x4307a7));
    r ^= 0x3ad8025f;
    JavaRandom::new(r).next_int(10) == 0
}
