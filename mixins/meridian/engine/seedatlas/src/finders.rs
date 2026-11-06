//! Strongholds and the world spawn (`locateBiome`, `initFirstStronghold`,
//! `nextStronghold`, `estimateSpawn`, `getSpawn` of the engine, 26.3).

use std::f64::consts::PI;

use crate::biome_noise::{SAMPLE_NO_BIOME, SAMPLE_NO_DEPTH};
use crate::biomes::*;
use crate::generator::Generator;
use crate::rng::JavaRandom;
use crate::viability::{is_oceanic, is_overworld};

/// `isStrongholdBiome` for 26.3.
pub fn is_stronghold_biome(id: i32) -> bool {
    is_overworld(id)
        && !is_oceanic(id)
        && ![
            RIVER,
            FROZEN_RIVER,
            BEACH,
            SNOWY_BEACH,
            SWAMP + 128,
            MANGROVE_SWAMP,
            DEEP_DARK,
            SWAMP,
            STONY_SHORE,
        ]
        .contains(&id)
}

/// `locateBiome` (1.18+): a random matching quarter cell within `radius`.
fn locate_biome(
    g: &Generator,
    x: i32,
    y: i32,
    z: i32,
    radius: i32,
    valid: impl Fn(i32) -> bool,
    rng: &mut JavaRandom,
) -> (i32, i32) {
    let mut out = (x, z);
    let Some(bn) = g.overworld() else { return out };
    let (x, z, radius) = (x >> 2, z >> 2, radius >> 2);
    let mut dat = 0u64;
    let mut found = 0;
    for j in -radius..=radius {
        for i in -radius..=radius {
            let id = bn.sample(x + i, y, z + j, Some(&mut dat), 0);
            if !valid(id) {
                continue;
            }
            if found == 0 || rng.next_int(found + 1) == 0 {
                out = ((x + i) * 4, (z + j) * 4);
            }
            found += 1;
        }
    }
    out
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Stronghold {
    pub x: i32,
    pub z: i32,
    pub ring: i32,
}

fn ring_pos(angle: f64, dist: f64) -> (i32, i32) {
    (
        ((angle.cos() * dist).round() as i32) * 16 + 8,
        ((angle.sin() * dist).round() as i32) * 16 + 8,
    )
}

/// All 128 strongholds: relocated into a valid biome when `g` is given,
/// else their ring estimates (`initFirstStronghold` + `nextStronghold`).
pub fn strongholds(seed: u64, g: Option<&Generator>) -> Vec<Stronghold> {
    let mut rnds = JavaRandom::new(seed);
    let mut angle = 2.0 * PI * rnds.next_double();
    let mut dist = (4.0 * 32.0) + (rnds.next_double() - 0.5) * 32.0 * 2.5;
    let mut next = ring_pos(angle, dist);
    let (mut index, mut ringnum, mut ringmax, mut ringidx) = (0, 0, 3, 0);
    let mut out = Vec::with_capacity(128);
    loop {
        let pos = match g {
            Some(g) => {
                let mut lbr = JavaRandom::new(rnds.next_long());
                locate_biome(g, next.0, 0, next.1, 112, is_stronghold_biome, &mut lbr)
            }
            None => {
                rnds.next_long();
                next
            }
        };
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
        dist =
            (4.0 * 32.0) + (6.0 * ringnum as f64 * 32.0) + (rnds.next_double() - 0.5) * 32.0 * 2.5;
        next = ring_pos(angle, dist);
        index += 1;
        out.push(Stronghold {
            x: pos.0,
            z: pos.1,
            ring: ringnum,
        });
        if 128 - (index - 1) <= 1 {
            break;
        }
    }
    out
}

fn fitness(g: &Generator, x: i32, z: i32) -> u64 {
    let Some(bn) = g.overworld() else {
        return u64::MAX;
    };
    let (_, np) = bn.sample_np(x >> 2, 0, z >> 2, None, SAMPLE_NO_DEPTH | SAMPLE_NO_BIOME);
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
    let a = (x as i64 * x as i64) as u64;
    let b = (z as i64 * z as i64) as u64;
    ds.wrapping_mul(2048 * 2048).wrapping_add(a).wrapping_add(b)
}

fn find_fittest(g: &Generator, pos: &mut (i32, i32), fit: &mut u64, maxrad: f64, step: f64) {
    let p = *pos;
    let mut rad = step;
    while rad <= maxrad {
        let mut ang = 0.0;
        while ang <= PI * 2.0 {
            let x = p.0 + (ang.sin() * rad) as i32;
            let z = p.1 + (ang.cos() * rad) as i32;
            let f = fitness(g, x, z);
            if f < *fit {
                *pos = (x, z);
                *fit = f;
            }
            ang += step / rad;
        }
        rad += step;
    }
}

/// `estimateSpawn` (1.18+): the fittest climate near the origin.
pub fn estimate_spawn(g: &Generator) -> (i32, i32) {
    let mut spawn = (0, 0);
    let mut fit = fitness(g, 0, 0);
    find_fittest(g, &mut spawn, &mut fit, 2048.0, 512.0);
    find_fittest(g, &mut spawn, &mut fit, 512.0, 32.0);
    ((spawn.0 & !15) + 8, (spawn.1 & !15) + 8)
}

/// `getSpawn` (1.18+): the estimate moved onto approximate land.
pub fn spawn(g: &Generator) -> (i32, i32) {
    let spawn = estimate_spawn(g);
    let Some(bn) = g.overworld() else {
        return spawn;
    };
    let (mut j, mut k, mut u, mut v) = (0i32, 0i32, 0i32, -1i32);
    for _ in 0..121 {
        if (-5..=5).contains(&j) && (-5..=5).contains(&k) {
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
        if j == k || (j < 0 && j == -k) || (j > 0 && j == 1 - k) {
            let tmp = u;
            u = -v;
            v = tmp;
        }
        j += u;
        k += v;
    }
    ((spawn.0 & !15) + 8, (spawn.1 & !15) + 8)
}
