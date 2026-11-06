//! Strongholds and the world spawn (`locateBiome`, `isStrongholdBiome`,
//! `initFirstStronghold`, `nextStronghold`, `calcFitness`, `findFittest`,
//! `estimateSpawn`, `getSpawn` and `mapApproxHeight` of the engine) for
//! every version.

use std::f64::consts::PI;

use crate::biome_noise::{Range, SAMPLE_NO_BIOME, SAMPLE_NO_DEPTH};
use crate::biomes::*;
use crate::generator::{Dimension, Generator};
use crate::mc::*;
use crate::rng::JavaRandom;
use crate::surface::{DIM_OVERWORLD, SurfaceNoise, get_biome_depth_and_scale};
use crate::viability::id_matches;

/// `isStrongholdBiome`.
pub fn is_stronghold_biome(mc: i32, id: i32) -> bool {
    if !is_overworld(mc, id) || is_oceanic(id) {
        return false;
    }
    match id {
        PLAINS | MUSHROOM_FIELDS | TAIGA_HILLS => mc >= MC_1_7,
        SWAMP => mc <= MC_1_6,
        RIVER | FROZEN_RIVER | BEACH | SNOWY_BEACH | SWAMP_HILLS => false,
        MUSHROOM_FIELD_SHORE => mc >= MC_1_13,
        STONE_SHORE => mc <= MC_1_17,
        // simulate MC-199298
        BAMBOO_JUNGLE | BAMBOO_JUNGLE_HILLS => mc <= MC_1_15 || mc >= MC_1_18,
        MANGROVE_SWAMP | DEEP_DARK => false,
        _ => true,
    }
}

/// The ids below 64 (`.0`) and 128 to 191 (`.1`) with a biome for which `f`
/// holds, as the masks `locateBiome` takes.
fn masks(f: impl Fn(i32) -> bool) -> (u64, u64) {
    let (mut b, mut m) = (0u64, 0u64);
    for i in 0..64 {
        if f(i) {
            b |= 1 << i;
        }
        if f(i + 128) {
            m |= 1 << i;
        }
    }
    (b, m)
}

/// `locateBiome`: a random matching quarter cell within `radius` blocks of
/// (`x`, `z`) at height `y`, as (position, passes). `passes` is the count
/// the engine reports: from 1.13 to 1.17 it is the number of matches minus
/// one (0 for a single match), before 1.13 the number of replacements.
#[allow(clippy::too_many_arguments)]
pub fn locate_biome(
    g: &Generator,
    x: i32,
    y: i32,
    z: i32,
    radius: i32,
    valid_b: u64,
    valid_m: u64,
    rng: &mut JavaRandom,
) -> ((i32, i32), i32) {
    let mut out = (x, z);
    let mut found = 0;
    if g.mc >= MC_1_18 {
        let Some(bn) = g.overworld() else {
            return (out, 0);
        };
        let (x, z, radius) = (x >> 2, z >> 2, radius >> 2);
        let mut dat = 0u64;
        for j in -radius..=radius {
            for i in -radius..=radius {
                // emulate order-dependent biome generation MC-241546
                let id = bn.sample(x + i, y, z + j, Some(&mut dat), 0);
                if !id_matches(id, valid_b, valid_m) {
                    continue;
                }
                if found == 0 || rng.next_int(found + 1) == 0 {
                    out = ((x + i) * 4, (z + j) * 4);
                }
                found += 1;
            }
        }
    } else {
        let x1 = (x - radius) >> 2;
        let z1 = (z - radius) >> 2;
        let x2 = (x + radius) >> 2;
        let z2 = (z + radius) >> 2;
        let width = x2 - x1 + 1;
        let height = z2 - z1 + 1;
        let ids = g.gen_biomes(Range {
            scale: 4,
            x: x1,
            z: z1,
            sx: width,
            sz: height,
            y,
            sy: 1,
        });
        if g.mc >= MC_1_13 {
            let mut j = 2;
            for (i, &id) in ids.iter().enumerate().take((width * height) as usize) {
                if !id_matches(id, valid_b, valid_m) {
                    continue;
                }
                let replace = found == 0 || {
                    j += 1;
                    rng.next_int(j - 1) == 0
                };
                if replace {
                    out = ((x1 + i as i32 % width) * 4, (z1 + i as i32 / width) * 4);
                    found = 1;
                }
            }
            found = j - 2;
        } else {
            for (i, &id) in ids.iter().enumerate().take((width * height) as usize) {
                if !id_matches(id, valid_b, valid_m) {
                    continue;
                }
                if found == 0 || rng.next_int(found + 1) == 0 {
                    out = ((x1 + i as i32 % width) * 4, (z1 + i as i32 / width) * 4);
                    found += 1;
                }
            }
        }
    }
    (out, found)
}

/// A stronghold: its position (the staircase, at (4, 4) in its chunk) and
/// ring number.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Stronghold {
    pub x: i32,
    pub z: i32,
    pub ring: i32,
}

/// The block position of a ring estimate at `angle` and `dist` chunks.
fn ring_pos(angle: f64, dist: f64) -> (i32, i32) {
    (
        ((angle.cos() * dist).round() as i32) * 16 + 8,
        ((angle.sin() * dist).round() as i32) * 16 + 8,
    )
}

/// `initFirstStronghold` + `nextStronghold`: the strongholds of the world,
/// 128 from 1.9 and 3 before, none before Beta 1.8.
///
/// Up to 1.19.2 the ring positions depend on the biome search itself, so the
/// generator is needed (without it the ring estimates are returned); from
/// 1.19.3 a generator relocates the strongholds into a valid biome and
/// without one the ring estimates are returned.
pub fn strongholds(seed: u64, mc: i32, g: Option<&Generator>) -> Vec<Stronghold> {
    if mc < MC_B1_8 {
        return Vec::new();
    }
    let total = if mc >= MC_1_9 { 128 } else { 3 };
    let ring_dist = |rnds: &mut JavaRandom, ringnum: i32| {
        if mc >= MC_1_9 {
            (4.0 * 32.0) + (6.0 * ringnum as f64 * 32.0) + (rnds.next_double() - 0.5) * 32.0 * 2.5
        } else {
            (1.25 + rnds.next_double()) * 32.0
        }
    };

    // initFirstStronghold
    let mut rnds = JavaRandom::new(seed);
    let mut angle = 2.0 * PI * rnds.next_double();
    let mut dist = ring_dist(&mut rnds, 0);
    let mut next_approx = ring_pos(angle, dist);
    let (mut index, mut ringnum, mut ringmax, mut ringidx) = (0, 0, 3, 0);
    let (valid_b, valid_m) = masks(|id| is_stronghold_biome(mc, id));

    let mut out = Vec::with_capacity(total as usize);
    loop {
        // nextStronghold
        let pos = if mc > MC_1_19_2 {
            match g {
                Some(g) => {
                    let mut lbr = JavaRandom::new(rnds.next_long());
                    locate_biome(
                        g,
                        next_approx.0,
                        0,
                        next_approx.1,
                        112,
                        valid_b,
                        valid_m,
                        &mut lbr,
                    )
                    .0
                }
                None => {
                    rnds.next_long();
                    next_approx
                }
            }
        } else {
            match g {
                Some(g) => {
                    locate_biome(
                        g,
                        next_approx.0,
                        0,
                        next_approx.1,
                        112,
                        valid_b,
                        valid_m,
                        &mut rnds,
                    )
                    .0
                }
                None => next_approx,
            }
        };
        // the staircase is located at (4, 4) in the chunk
        let pos = ((pos.0 & !15) + 4, (pos.1 & !15) + 4);

        ringidx += 1;
        angle += 2.0 * PI / ringmax as f64;
        if ringidx == ringmax {
            ringnum += 1;
            ringidx = 0;
            ringmax = ringmax + 2 * ringmax / (ringnum + 1);
            if ringmax > 128 - index {
                ringmax = 128 - index;
            }
            angle += rnds.next_double() * PI * 2.0;
        }
        dist = ring_dist(&mut rnds, ringnum);
        next_approx = ring_pos(angle, dist);
        index += 1;

        // the ring of the stronghold is the one it was generated in, which
        // the iterator has already advanced past when this was the last of
        // its ring (as `sh->ringnum` in the glue)
        out.push(Stronghold {
            x: pos.0,
            z: pos.1,
            ring: ringnum,
        });
        if total - (index - 1) <= 1 {
            break;
        }
    }
    out
}

/// `calcFitness`: how far the climate at a block is from the spawn climate,
/// weighted by the distance from the origin.
fn calc_fitness(g: &Generator, x: i32, z: i32) -> u64 {
    let Some(bn) = g.overworld() else {
        return u64::MAX;
    };
    let (_, np) = bn.sample_np(x >> 2, 0, z >> 2, None, SAMPLE_NO_DEPTH | SAMPLE_NO_BIOME);
    // [6]: weirdness for the second noise point
    const SPAWN_NP: [[i64; 2]; 7] = [
        [-10000, 10000],
        [-10000, 10000],
        [-1100, 10000],
        [-10000, 10000],
        [0, 0],
        [-10000, -1600],
        [1600, 10000],
    ];
    let q = |v: i64, r: [i64; 2]| {
        let a = (v as u64).wrapping_sub(r[1] as u64);
        let b = (v.wrapping_neg() as u64).wrapping_add(r[0] as u64);
        let q = if (a as i64) > 0 {
            a
        } else if (b as i64) > 0 {
            b
        } else {
            0
        };
        q.wrapping_mul(q)
    };
    let mut ds = 0u64;
    for i in 0..5 {
        ds = ds.wrapping_add(q(np[i], SPAWN_NP[i]));
    }
    let ds1 = ds.wrapping_add(q(np[5], SPAWN_NP[5]));
    let ds2 = ds.wrapping_add(q(np[5], SPAWN_NP[6]));
    let ds = if ds1 <= ds2 { ds1 } else { ds2 };
    // apply dependence on distance from origin
    let a = (x as i64).wrapping_mul(x as i64) as u64;
    let b = (z as i64).wrapping_mul(z as i64) as u64;
    if g.mc <= MC_1_21_1 {
        let s = a.wrapping_add(b) as f64 / (2500 * 2500) as f64;
        ((s * s * 1e8) as u64).wrapping_add(ds)
    } else {
        ds.wrapping_mul(2048 * 2048).wrapping_add(a).wrapping_add(b)
    }
}

/// `findFittest`.
fn find_fittest(g: &Generator, pos: &mut (i32, i32), fitness: &mut u64, maxrad: f64, step: f64) {
    let p = *pos;
    let mut rad = step;
    while rad <= maxrad {
        let mut ang = 0.0;
        while ang <= PI * 2.0 {
            let x = p.0.wrapping_add((ang.sin() * rad) as i32);
            let z = p.1.wrapping_add((ang.cos() * rad) as i32);
            let fit = calc_fitness(g, x, z);
            // update pos and fitness if the combined total is lower
            if fit < *fitness {
                *pos = (x, z);
                *fitness = fit;
            }
            ang += step / rad;
        }
        rad += step;
    }
}

/// `findFittestPos`.
fn find_fittest_pos(g: &Generator) -> (i32, i32) {
    let mut spawn = (0, 0);
    let mut fitness = calc_fitness(g, 0, 0);
    find_fittest(g, &mut spawn, &mut fitness, 2048.0, 512.0);
    find_fittest(g, &mut spawn, &mut fitness, 512.0, 32.0);
    // the centre of the chunk
    ((spawn.0 & !15) + 8, (spawn.1 & !15) + 8)
}

/// The valid spawn biomes up to 1.17 (`g_spawn_biomes_17`).
const SPAWN_BIOMES_17: [i32; 7] = [
    FOREST,
    PLAINS,
    TAIGA,
    TAIGA_HILLS,
    WOODED_HILLS,
    JUNGLE,
    JUNGLE_HILLS,
];

/// `estimateSpawn` with the random state it leaves (`rng`, only meaningful
/// up to 1.17).
fn estimate_spawn_rng(g: &Generator) -> ((i32, i32), JavaRandom) {
    let mut s = JavaRandom::new(g.seed);
    if g.mc <= MC_B1_7 {
        // finds a random sand block (the location is not fixed)
        return ((0, 0), s);
    }
    if g.mc > MC_1_17 {
        return (find_fittest_pos(g), s);
    }
    let spawn_biomes: &[i32] = if g.mc <= MC_1_0 {
        &[FOREST, SWAMP, TAIGA]
    } else {
        &SPAWN_BIOMES_17
    };
    let b = spawn_biomes.iter().fold(0u64, |b, &i| b | (1u64 << i));
    let (mut spawn, found) = locate_biome(g, 0, 63, 0, 256, b, 0, &mut s);
    if found == 0 {
        spawn = (8, 8);
    }
    (spawn, s)
}

/// `estimateSpawn`: the approximate spawn point of the world.
pub fn estimate_spawn(g: &Generator) -> (i32, i32) {
    estimate_spawn_rng(g).0
}

/// `mapApproxHeight` for the overworld up to 1.17: the approximate surface
/// height (and the biome) of the `w` x `h` quarter cells at (`x`, `z`), from
/// the biome depth and scale and the terrain noise. Row-major, `None` when
/// the biomes cannot be generated.
fn map_approx_height(
    g: &Generator,
    sn: &SurfaceNoise,
    x: i32,
    z: i32,
    w: i32,
    h: i32,
) -> Option<(Vec<f32>, Vec<i32>)> {
    // with 10 / (sqrt(i**2 + j**2) + 0.2)
    let kernel: [f32; 25] = [
        3.302044127_f64,
        4.104975761,
        4.545454545,
        4.104975761,
        3.302044127,
        4.104975761,
        6.194967155,
        8.333333333,
        6.194967155,
        4.104975761,
        4.545454545,
        8.333333333,
        50.00000000,
        8.333333333,
        4.545454545,
        4.104975761,
        6.194967155,
        8.333333333,
        6.194967155,
        4.104975761,
        3.302044127,
        4.104975761,
        4.545454545,
        4.104975761,
        3.302044127,
    ]
    .map(|v| v as f32);
    let (wu, hu) = (w as usize, h as usize);
    let mut depth = vec![0.0f64; wu * hu];
    let mut scale = vec![0.0f64; wu * hu];
    let mut ids = vec![0i32; wu * hu];

    let r = Range {
        scale: 4,
        x: x - 2,
        z: z - 2,
        sx: w + 5,
        sz: h + 5,
        y: 0,
        sy: 1,
    };
    let cache = g.gen_biomes(r);
    if cache.iter().any(|&id| id < 0) {
        return None;
    }
    let rw = r.sx as usize;
    let ds = |id: i32| {
        get_biome_depth_and_scale(id)
            .map(|b| (b.depth, b.scale))
            .unwrap_or((0.0, 0.0))
    };
    for j in 0..hu {
        for i in 0..wu {
            let (mut wt, mut ws, mut wd) = (0.0f64, 0.0f64, 0.0f64);
            let id0 = cache[(j + 2) * rw + (i + 2)];
            let (d0, _) = ds(id0);
            for jj in 0..5 {
                for ii in 0..5 {
                    let id = cache[(j + jj) * rw + (i + ii)];
                    let (d, s) = ds(id);
                    let mut weight = (kernel[jj * 5 + ii] as f64 / (d + 2.0)) as f32;
                    if d > d0 {
                        weight = (weight as f64 * 0.5) as f32;
                    }
                    ws = s.mul_add(weight as f64, ws);
                    wd = d.mul_add(weight as f64, wd);
                    wt += weight as f64;
                }
            }
            ws /= wt;
            wd /= wt;
            ws = ws.mul_add(0.9, 0.1);
            wd = (wd * 4.0 - 1.0) / 8.0;
            ws = 96.0 / ws;
            wd = wd * 17. / 64.;
            depth[j * wu + i] = wd;
            scale[j * wu + i] = ws;
            ids[j * wu + i] = id0;
        }
    }

    let octdepth = sn.octdepth()?;
    let mut y = vec![0.0f32; wu * hu];
    for j in 0..h {
        for i in 0..w {
            let (px, pz) = (x.wrapping_add(i), z.wrapping_add(j));
            let o = (j * w + i) as usize;
            let mut off = octdepth.sample_amp(
                px.wrapping_mul(200) as f64,
                10.0,
                pz.wrapping_mul(200) as f64,
                1.0,
                0.0,
                true,
            );
            off *= 65535. / 8000.;
            if off < 0.0 {
                off *= -0.3;
            }
            off = off.mul_add(3.0, -2.0);
            if off > 1.0 {
                off = 1.0;
            }
            off *= 17. / 64.;
            if off < 0.0 {
                off *= 1. / 28.;
            } else {
                off *= 1. / 40.;
            }

            let (mut vmin, mut vmax) = (0.0f64, 0.0f64);
            let (mut ytest, mut ymin, mut ymax) = (8, 0, 32);
            loop {
                let mut v = [0.0f64; 2];
                for (k, vk) in v.iter_mut().enumerate() {
                    let py = ytest + k as i32;
                    let mut n0 = sn.sample(px, py, pz);
                    let mut fall = 1.0 - (2 * py) as f64 / 32.0 + off - 0.46875;
                    fall = scale[o] * (fall + depth[o]);
                    n0 += if fall > 0.0 { 4.0 * fall } else { fall };
                    *vk = n0;
                    if n0 >= 0.0 && py > ymin {
                        ymin = py;
                        vmin = n0;
                    }
                    if n0 < 0.0 && py < ymax {
                        ymax = py;
                        vmax = n0;
                    }
                }
                let mut dy = v[0] / (v[0] - v[1]);
                // round away from zero
                dy = if dy <= 0.0 { dy.floor() } else { dy.ceil() };
                ytest += dy as i32;
                if ytest <= ymin {
                    ytest = ymin + 1;
                }
                if ytest >= ymax {
                    ytest = ymax - 1;
                }
                if ymax - ymin <= 1 {
                    break;
                }
            }
            y[o] = (8.0 * (vmin / (vmin - vmax) + ymin as f64)) as f32;
        }
    }
    Some((y, ids))
}

/// `getSpawn`: the world spawn: the estimate moved onto land (a grass
/// block up to 1.17, an approximate land surface from 1.18).
///
/// Slow, and may be inaccurate because the world spawn depends on grass
/// blocks.
pub fn spawn(g: &Generator) -> (i32, i32) {
    let (mut spawn, mut rng) = estimate_spawn_rng(g);
    if g.mc <= MC_B1_7 || g.dimension != Dimension::Overworld {
        return spawn;
    }
    let (mut j, mut k, mut u, mut v) = (0i32, 0i32, 0i32, -1i32);
    let grass_of = |id: i32| get_biome_depth_and_scale(id).map_or(0, |b| b.grass);
    // step of the spiral over the chunks around the spawn
    let step = |j: &mut i32, k: &mut i32, u: &mut i32, v: &mut i32| {
        if *j == *k || (*j < 0 && *j == -*k) || (*j > 0 && *j == 1 - *k) {
            let tmp = *u;
            *u = -*v;
            *v = tmp;
        }
        *j += *u;
        *k += *v;
    };

    if g.mc <= MC_1_12 {
        let sn = SurfaceNoise::new(DIM_OVERWORLD, g.seed);
        for _ in 0..1000 {
            let Some((y, ids)) = map_approx_height(g, &sn, spawn.0 >> 2, spawn.1 >> 2, 1, 1) else {
                break;
            };
            let grass = grass_of(ids[0]);
            if grass > 0 && y[0] >= grass as f32 {
                break;
            }
            let (a, b) = (rng.next_int(64), rng.next_int(64));
            spawn.0 = spawn.0.wrapping_add(a - b);
            let (a, b) = (rng.next_int(64), rng.next_int(64));
            spawn.1 = spawn.1.wrapping_add(a - b);
        }
    } else if g.mc <= MC_1_17 {
        let sn = SurfaceNoise::new(DIM_OVERWORLD, g.seed);
        for _ in 0..1024 {
            if j > -16 && j <= 16 && k > -16 && k <= 16 {
                // find the server spawn point in the chunk
                let cx0 = (spawn.0 & !15) + j * 16;
                let cz0 = (spawn.1 & !15) + k * 16;
                if let Some((y, ids)) = map_approx_height(g, &sn, cx0 >> 2, cz0 >> 2, 4, 4) {
                    for ii in 0..4 {
                        for jj in 0..4 {
                            let o = jj * 4 + ii;
                            let grass = grass_of(ids[o]);
                            if grass <= 0 || y[o] < grass as f32 {
                                continue;
                            }
                            return (cx0 + ii as i32 * 4, cz0 + jj as i32 * 4);
                        }
                    }
                }
            }
            step(&mut j, &mut k, &mut u, &mut v);
        }
        // the chunk centre
        spawn = ((spawn.0 & !15) + 8, (spawn.1 & !15) + 8);
    } else {
        let Some(bn) = g.overworld() else {
            return spawn;
        };
        for _ in 0..121 {
            if (-5..=5).contains(&j) && (-5..=5).contains(&k) {
                // find the server spawn point in the chunk
                let cx0 = (spawn.0 & !15) + j * 16;
                let cz0 = (spawn.1 & !15) + k * 16;
                for ii in 0..4 {
                    for jj in 0..4 {
                        let (x, z) = (cx0 + ii * 4, cz0 + jj * 4);
                        let (id, np) = bn.sample_np(x >> 2, 0, z >> 2, None, 0);
                        let y = (np[4] as f64 / 76.0) as f32;
                        if y > 63.0
                            || id == FROZEN_OCEAN
                            || id == DEEP_FROZEN_OCEAN
                            || id == FROZEN_RIVER
                        {
                            return (x, z);
                        }
                    }
                }
            }
            step(&mut j, &mut k, &mut u, &mut v);
        }
        // the chunk centre
        spawn = ((spawn.0 & !15) + 8, (spawn.1 & !15) + 8);
    }
    spawn
}
