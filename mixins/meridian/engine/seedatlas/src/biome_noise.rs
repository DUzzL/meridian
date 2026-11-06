//! Biome generation of the overworld (1.18+ climate noise), the Nether and
//! the End, ported from the engine's `biomenoise.c` and the Voronoi parts of
//! `layers.c`, for Minecraft 26.3 only.

use crate::biomes::*;
use crate::btree21wd as bt;
use crate::noise::{DoublePerlin, Perlin};
use crate::rng::{JavaRandom, Xoroshiro, lerp, mc_step_seed};

/// A box of biome cells: `scale` blocks per cell, `y` in cells of the
/// scale (quarter blocks for 4 and up, blocks for 1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Range {
    pub scale: i32,
    pub x: i32,
    pub z: i32,
    pub sx: i32,
    pub sz: i32,
    pub y: i32,
    pub sy: i32,
}

pub const NP_TEMPERATURE: usize = 0;
pub const NP_HUMIDITY: usize = 1;
pub const NP_CONTINENTALNESS: usize = 2;
pub const NP_EROSION: usize = 3;
pub const NP_SHIFT: usize = 4;
pub const NP_WEIRDNESS: usize = 5;

pub const SAMPLE_NO_SHIFT: u32 = 0x1;
pub const SAMPLE_NO_DEPTH: u32 = 0x2;

// ------------------------------------------------------------- splines --

const SP_CONTINENTALNESS: usize = 0;
const SP_EROSION: usize = 1;
const SP_RIDGES: usize = 2;

#[derive(Clone, Debug)]
enum Spline {
    Fix(f32),
    Multi {
        typ: usize,
        loc: Vec<f32>,
        der: Vec<f32>,
        val: Vec<usize>,
    },
}

#[derive(Clone, Debug, Default)]
struct Splines {
    nodes: Vec<Spline>,
}

fn offset_value(weirdness: f32, continentalness: f32) -> f32 {
    let f0 = 1.0f32 - (1.0f32 - continentalness) * 0.5f32;
    let f1 = 0.5f32 * (1.0f32 - continentalness);
    let f2 = (weirdness + 1.17f32) * 0.460_829_47_f32;
    let off = f2 * f0 - f1;
    if weirdness < -0.7f32 {
        if off > -0.2222f32 { off } else { -0.2222f32 }
    } else if off > 0.0 {
        off
    } else {
        0.0
    }
}

/// `lerp` on doubles, as the C helper, narrowed to a float.
fn lerpf(part: f32, from: f32, to: f32) -> f32 {
    lerp(part as f64, from as f64, to as f64) as f32
}

impl Splines {
    fn fix(&mut self, v: f32) -> usize {
        self.nodes.push(Spline::Fix(v));
        self.nodes.len() - 1
    }

    fn multi(&mut self, typ: usize) -> usize {
        self.nodes.push(Spline::Multi {
            typ,
            loc: Vec::new(),
            der: Vec::new(),
            val: Vec::new(),
        });
        self.nodes.len() - 1
    }

    fn add(&mut self, sp: usize, loc: f32, val: usize, der: f32) {
        if let Spline::Multi {
            loc: l,
            der: d,
            val: v,
            ..
        } = &mut self.nodes[sp]
        {
            l.push(loc);
            v.push(val);
            d.push(der);
        }
    }

    fn spline_38219(&mut self, f: f32, bl: bool) -> usize {
        let sp = self.multi(SP_RIDGES);
        let i = offset_value(-1.0, f);
        let k = offset_value(1.0, f);
        let mut l = 1.0f32 - (1.0f32 - f) * 0.5f32;
        let mut u = 0.5f32 * (1.0f32 - f);
        l = u / (0.460_829_47_f32 * l) - 1.17f32;
        if -0.65f32 < l && l < 1.0f32 {
            u = offset_value(-0.65, f);
            let p = offset_value(-0.75, f);
            let q = (p - i) * 4.0f32;
            let r = offset_value(l, f);
            let s = (k - r) / (1.0f32 - l);
            let a = self.fix(i);
            self.add(sp, -1.0, a, q);
            let a = self.fix(p);
            self.add(sp, -0.75, a, 0.0);
            let a = self.fix(u);
            self.add(sp, -0.65, a, 0.0);
            let a = self.fix(r);
            self.add(sp, l - 0.01f32, a, 0.0);
            let a = self.fix(r);
            self.add(sp, l, a, s);
            let a = self.fix(k);
            self.add(sp, 1.0, a, s);
        } else {
            u = (k - i) * 0.5f32;
            if bl {
                let v = if (i as f64) > 0.2 { i as f64 } else { 0.2 };
                let a = self.fix(v as f32);
                self.add(sp, -1.0, a, 0.0);
                let a = self.fix(lerpf(0.5, i, k));
                self.add(sp, 0.0, a, u);
            } else {
                let a = self.fix(i);
                self.add(sp, -1.0, a, u);
            }
            let a = self.fix(k);
            self.add(sp, 1.0, a, u);
        }
        sp
    }

    fn flat_offset(&mut self, f: f32, g: f32, h: f32, i: f32, j: f32, k: f32) -> usize {
        let sp = self.multi(SP_RIDGES);
        let mut l = 0.5f32 * (g - f);
        if l < k {
            l = k;
        }
        let m = 5.0f32 * (h - g);
        let a = self.fix(f);
        self.add(sp, -1.0, a, l);
        let a = self.fix(g);
        self.add(sp, -0.4, a, if l < m { l } else { m });
        let a = self.fix(h);
        self.add(sp, 0.0, a, m);
        let a = self.fix(i);
        self.add(sp, 0.4, a, 2.0f32 * (i - h));
        let a = self.fix(j);
        self.add(sp, 1.0, a, 0.7f32 * (j - i));
        sp
    }

    #[allow(clippy::too_many_arguments)]
    fn land(&mut self, f: f32, g: f32, h: f32, i: f32, j: f32, k: f32, bl: bool) -> usize {
        let sp1 = self.spline_38219(lerpf(i, 0.6, 1.5), bl);
        let sp2 = self.spline_38219(lerpf(i, 0.6, 1.0), bl);
        let sp3 = self.spline_38219(i, bl);
        let ih = 0.5f32 * i;
        let sp4 = self.flat_offset(f - 0.15f32, ih, ih, ih, i * 0.6f32, 0.5);
        let sp5 = self.flat_offset(f, j * i, g * i, ih, i * 0.6f32, 0.5);
        let sp6 = self.flat_offset(f, j, j, g, h, 0.5);
        let sp7 = self.flat_offset(f, j, j, g, h, 0.5);
        let sp8 = self.multi(SP_RIDGES);
        let a = self.fix(f);
        self.add(sp8, -1.0, a, 0.0);
        self.add(sp8, -0.4, sp6, 0.0);
        let a = self.fix(h + 0.07f32);
        self.add(sp8, 0.0, a, 0.0);
        let sp9 = self.flat_offset(-0.02, k, k, g, h, 0.0);
        let sp = self.multi(SP_EROSION);
        self.add(sp, -0.85, sp1, 0.0);
        self.add(sp, -0.7, sp2, 0.0);
        self.add(sp, -0.4, sp3, 0.0);
        self.add(sp, -0.35, sp4, 0.0);
        self.add(sp, -0.1, sp5, 0.0);
        self.add(sp, 0.2, sp6, 0.0);
        if bl {
            self.add(sp, 0.4, sp7, 0.0);
            self.add(sp, 0.45, sp8, 0.0);
            self.add(sp, 0.55, sp8, 0.0);
            self.add(sp, 0.58, sp7, 0.0);
        }
        self.add(sp, 0.7, sp9, 0.0);
        sp
    }

    fn get(&self, sp: usize, vals: &[f32; 4]) -> f32 {
        match &self.nodes[sp] {
            Spline::Fix(v) => *v,
            Spline::Multi { typ, loc, der, val } => {
                let len = loc.len();
                let f = vals[*typ];
                let mut i = 0;
                while i < len && loc[i] < f {
                    i += 1;
                }
                if i == 0 || i == len {
                    i = i.saturating_sub(1);
                    let v = self.get(val[i], vals);
                    return v + der[i] * (f - loc[i]);
                }
                let g = loc[i - 1];
                let h = loc[i];
                let k = (f - g) / (h - g);
                let l = der[i - 1];
                let m = der[i];
                let n = self.get(val[i - 1], vals);
                let o = self.get(val[i], vals);
                let p = l * (h - g) - (o - n);
                let q = -m * (h - g) + (o - n);
                (lerp(k as f64, n as f64, o as f64)
                    + (k * (1.0f32 - k)) as f64 * lerp(k as f64, p as f64, q as f64))
                    as f32
            }
        }
    }
}

// ------------------------------------------------------------ overworld --

#[derive(Clone, Debug)]
pub struct BiomeNoise {
    pub climate: [DoublePerlin; 6],
    splines: Splines,
    root: usize,
}

const NP_AMPS: [(usize, &[f64]); 6] = [
    (NP_TEMPERATURE, &[1.5, 0.0, 1.0, 0.0, 0.0, 0.0]),
    (NP_HUMIDITY, &[1.0, 1.0, 0.0, 0.0, 0.0, 0.0]),
    (
        NP_CONTINENTALNESS,
        &[1.0, 1.0, 2.0, 2.0, 2.0, 1.0, 1.0, 1.0, 1.0],
    ),
    (NP_EROSION, &[1.0, 1.0, 0.0, 1.0, 1.0]),
    (NP_SHIFT, &[1.0, 1.0, 1.0, 0.0]),
    (NP_WEIRDNESS, &[1.0, 2.0, 1.0, 0.0, 0.0, 0.0]),
];

fn init_climate(xlo: u64, xhi: u64, large: bool, np: usize) -> DoublePerlin {
    let (md5, omin): ([u64; 2], i32) = match np {
        NP_SHIFT => ([0x080518cf6af25384, 0x3f3dfb40a54febd5], -3),
        NP_TEMPERATURE => {
            if large {
                ([0x944b0073edf549db, 0x4ff44347e9d22b96], -12)
            } else {
                ([0x5c7e6b29735f0d7f, 0xf7d86f1bbc734988], -10)
            }
        }
        NP_HUMIDITY => {
            if large {
                ([0x71b8ab943dbd5301, 0xbb63ddcf39ff7a2b], -10)
            } else {
                ([0x81bb4d22e8dc168e, 0xf1c8b4bea16303cd], -8)
            }
        }
        NP_CONTINENTALNESS => {
            if large {
                ([0x9a3f51a113fce8dc, 0xee2dbd157e5dcdad], -11)
            } else {
                ([0x83886c9d0ae3a662, 0xafa638a61b42e8ad], -9)
            }
        }
        NP_EROSION => {
            if large {
                ([0x8c984b1f8702a951, 0xead7b1f92bae535f], -11)
            } else {
                ([0xd02491e6058f6fd8, 0x4792512c94c17a80], -9)
            }
        }
        _ => ([0xefc8ef4d36102b34, 0x1beeeb324a0f24ea], -7),
    };
    let amp = NP_AMPS
        .iter()
        .find(|(n, _)| *n == np)
        .map(|(_, a)| *a)
        .expect("a climate parameter");
    let mut pxr = Xoroshiro {
        lo: xlo ^ md5[0],
        hi: xhi ^ md5[1],
    };
    DoublePerlin::new_x(&mut pxr, amp, omin, -1)
}

impl BiomeNoise {
    pub fn new(seed: u64, large: bool) -> Self {
        let mut pxr = Xoroshiro::new(seed);
        let xlo = pxr.next_long();
        let xhi = pxr.next_long();
        let climate = std::array::from_fn(|i| init_climate(xlo, xhi, large, i));
        let mut ss = Splines::default();
        let sp = ss.multi(SP_CONTINENTALNESS);
        let sp1 = ss.land(-0.15, 0.00, 0.0, 0.1, 0.00, -0.03, false);
        let sp2 = ss.land(-0.10, 0.03, 0.1, 0.1, 0.01, -0.03, false);
        let sp3 = ss.land(-0.10, 0.03, 0.1, 0.7, 0.01, -0.03, true);
        let sp4 = ss.land(-0.05, 0.03, 0.1, 1.0, 0.01, 0.01, true);
        for (loc, v) in [
            (-1.10f32, 0.044f32),
            (-1.02, -0.2222),
            (-0.51, -0.2222),
            (-0.44, -0.12),
            (-0.18, -0.12),
        ] {
            let a = ss.fix(v);
            ss.add(sp, loc, a, 0.0);
        }
        ss.add(sp, -0.16, sp1, 0.0);
        ss.add(sp, -0.15, sp1, 0.0);
        ss.add(sp, -0.10, sp2, 0.0);
        ss.add(sp, 0.25, sp3, 0.0);
        ss.add(sp, 1.00, sp4, 0.0);
        Self {
            climate,
            splines: ss,
            root: sp,
        }
    }

    /// The biome at a quarter-resolution cell (`sampleBiomeNoise`); `dat`
    /// carries the last tree leaf between calls when given.
    pub fn sample(&self, x: i32, y: i32, z: i32, dat: Option<&mut u64>, flags: u32) -> i32 {
        let (mut px, mut pz) = (x as f64, z as f64);
        if flags & SAMPLE_NO_SHIFT == 0 {
            px += self.climate[NP_SHIFT].sample(x as f64, 0.0, z as f64) * 4.0;
            pz += self.climate[NP_SHIFT].sample(z as f64, x as f64, 0.0) * 4.0;
        }
        let c = self.climate[NP_CONTINENTALNESS].sample(px, 0.0, pz) as f32;
        let e = self.climate[NP_EROSION].sample(px, 0.0, pz) as f32;
        let w = self.climate[NP_WEIRDNESS].sample(px, 0.0, pz) as f32;
        let mut d = 0.0f32;
        if flags & SAMPLE_NO_DEPTH == 0 {
            let vals = [
                c,
                e,
                -3.0f32 * ((w.abs() - 0.666_666_7_f32).abs() - 0.333_333_34_f32),
                w,
            ];
            let off = (self.splines.get(self.root, &vals) + 0.015f32) as f64;
            d = (1.0 - (y * 4) as f64 / 128.0 - 83.0 / 160.0 + off) as f32;
        }
        let t = self.climate[NP_TEMPERATURE].sample(px, 0.0, pz) as f32;
        let h = self.climate[NP_HUMIDITY].sample(px, 0.0, pz) as f32;
        let np: [i64; 6] = [
            (10000.0f32 * t) as i64,
            (10000.0f32 * h) as i64,
            (10000.0f32 * c) as i64,
            (10000.0f32 * e) as i64,
            (10000.0f32 * d) as i64,
            (10000.0f32 * w) as i64,
        ];
        climate_to_biome(&np, dat)
    }
}

// -------------------------------------------------------- biome R-tree --

fn np_dist(np: &[i64; 6], idx: usize) -> u64 {
    let node = bt::NODES[idx];
    let mut ds = 0u64;
    for (i, &p) in np.iter().enumerate() {
        let k = ((node >> (8 * i)) & 0xFF) as usize;
        let a = (p as u64).wrapping_sub(bt::PARAM[k][1] as i64 as u64);
        let b = (bt::PARAM[k][0] as i64 as u64).wrapping_sub(p as u64);
        let d = if (a as i64) > 0 {
            a
        } else if (b as i64) > 0 {
            b
        } else {
            0
        };
        ds = ds.wrapping_add(d.wrapping_mul(d));
    }
    ds
}

fn resulting_node(np: &[i64; 6], idx: usize, alt: usize, mut ds: u64, mut depth: usize) -> usize {
    if bt::STEPS[depth] == 0 {
        return idx;
    }
    let len = bt::NODES.len();
    let mut step;
    loop {
        step = bt::STEPS[depth] as usize;
        depth += 1;
        if idx + step < len {
            break;
        }
    }
    let node = bt::NODES[idx];
    let mut inner = (node >> 48) as u16 as usize;
    let mut leaf = alt;
    for _ in 0..bt::ORDER {
        let ds_inner = np_dist(np, inner);
        if ds_inner < ds {
            let leaf2 = resulting_node(np, inner, leaf, ds, depth);
            let ds_leaf2 = if inner == leaf2 {
                ds_inner
            } else {
                np_dist(np, leaf2)
            };
            if ds_leaf2 < ds {
                ds = ds_leaf2;
                leaf = leaf2;
            }
        }
        inner = (inner + step) as u16 as usize;
        if inner >= len {
            break;
        }
    }
    leaf
}

fn ranges(idx: usize) -> [[i32; 2]; 6] {
    let node = bt::NODES[idx];
    std::array::from_fn(|i| bt::PARAM[((node >> (8 * i)) & 0xFF) as usize])
}

fn is_pale_garden_point(idx: usize) -> bool {
    const CHANGED: [[i32; 5]; 10] = [
        [3000, -7799, -3750, -10000, -9333],
        [3000, -3750, -2225, -10000, -9333],
        [300, -3750, -2225, -9333, -7666],
        [3000, -2225, 500, -9333, -7666],
        [300, -3750, -2225, -7666, -5666],
        [3000, -2225, 500, -7666, -5666],
        [300, -3750, -2225, -5666, -4000],
        [3000, -2225, 500, -5666, -4000],
        [3000, -7799, -3750, -4000, -2666],
        [3000, -3750, -2225, -4000, -2666],
    ];
    let r = ranges(idx);
    if r[0] != [-1500, 2000]
        || r[1] != [3000, 10000]
        || r[2][1] != 10000
        || !(r[4] == [0, 0] || r[4] == [10000, 10000])
    {
        return false;
    }
    CHANGED
        .iter()
        .any(|c| r[2][0] == c[0] && r[3] == [c[1], c[2]] && r[5] == [c[3], c[4]])
}

fn is_dappled_forest_point(idx: usize) -> bool {
    let r = ranges(idx);
    r[0] == [-4500, -1500] && r[1] == [-10000, -3500] && r[5][0] >= -500
}

/// The biome nearest to a climate point (`climateToBiome` for 26.3).
pub fn climate_to_biome(np: &[i64; 6], dat: Option<&mut u64>) -> i32 {
    let idx = match dat {
        Some(dat) => {
            let alt = *dat as i32 as usize;
            let ds = np_dist(np, alt);
            let idx = resulting_node(np, 0, alt, ds, 0);
            *dat = idx as u64;
            idx
        }
        None => resulting_node(np, 0, 0, u64::MAX, 0),
    };
    let mut biome = ((bt::NODES[idx] >> 48) & 0xFF) as i32;
    if biome == DARK_FOREST && is_pale_garden_point(idx) {
        biome = PALE_GARDEN;
    }
    if biome == PLAINS && is_dappled_forest_point(idx) {
        biome = DAPPLED_FOREST;
    }
    const SULFUR: [[i64; 2]; 6] = [
        [-10000, 10000],
        [-10000, 10000],
        [-1900, 5500],
        [4500, 10000],
        [2000, 9000],
        [-11000, -8500],
    ];
    let mut ds = 0u64;
    for i in 0..6 {
        let p = np[i];
        let d = if p > SULFUR[i][1] {
            p - SULFUR[i][1]
        } else if p < SULFUR[i][0] {
            SULFUR[i][0] - p
        } else {
            0
        };
        ds = ds.wrapping_add(d.wrapping_mul(d) as u64);
    }
    let winner = np_dist(np, idx);
    if ds < winner || (ds == winner && biome == DEEP_DARK) {
        biome = SULFUR_CAVES;
    }
    biome
}

// --------------------------------------------------------------- nether --

#[derive(Clone, Debug)]
pub struct NetherNoise {
    temperature: DoublePerlin,
    humidity: DoublePerlin,
}

impl NetherNoise {
    pub fn new(seed: u64) -> Self {
        let mut s = JavaRandom::new(seed);
        let temperature = DoublePerlin::new_java(&mut s, -7, 2);
        let mut s = JavaRandom::new(seed.wrapping_add(1));
        let humidity = DoublePerlin::new_java(&mut s, -7, 2);
        Self {
            temperature,
            humidity,
        }
    }

    /// The biome at a quarter-resolution cell, and the noise distance to the
    /// second nearest biome.
    pub fn biome(&self, x: i32, _y: i32, z: i32) -> (i32, f32) {
        const NPOINTS: [[f32; 4]; 5] = [
            [0.0, 0.0, 0.0, NETHER_WASTES as f32],
            [0.0, -0.5, 0.0, SOUL_SAND_VALLEY as f32],
            [0.4, 0.0, 0.0, CRIMSON_FOREST as f32],
            [0.0, 0.5, (0.375 * 0.375) as f32, WARPED_FOREST as f32],
            [-0.5, 0.0, (0.175 * 0.175) as f32, BASALT_DELTAS as f32],
        ];
        let temp = self.temperature.sample(x as f64, 0.0, z as f64) as f32;
        let humidity = self.humidity.sample(x as f64, 0.0, z as f64) as f32;
        let (mut id, mut dmin, mut dmin2) = (0, f32::MAX, f32::MAX);
        for (i, p) in NPOINTS.iter().enumerate() {
            let dx = p[0] - temp;
            let dy = p[1] - humidity;
            let dsq = dx * dx + dy * dy + p[2];
            if dsq < dmin {
                dmin2 = dmin;
                dmin = dsq;
                id = i;
            } else if dsq < dmin2 {
                dmin2 = dsq;
            }
        }
        (NPOINTS[id][3] as i32, dmin2.sqrt() - dmin.sqrt())
    }

    /// `mapNether3D`: cells of `r` (scale 4 and up), filling the cells
    /// around each sample that must share its biome.
    pub fn map_3d(&self, out: &mut [i32], mut r: Range, confidence: f32) {
        if r.sy <= 0 {
            r.sy = 1;
        }
        let scale = r.scale / 4;
        let (sx, sy, sz) = (r.sx as usize, r.sy as usize, r.sz as usize);
        out[..sx * sy * sz].fill(0);
        let invgrad = (1.0 / (confidence as f64 * 0.05 * 2.0) / scale as f64) as f32;
        for k in 0..sy {
            for j in 0..sz {
                for i in 0..sx {
                    if out[k * sx * sz + j * sx + i] != 0 {
                        continue;
                    }
                    let xi = (r.x + i as i32) * scale;
                    let yk = r.y + k as i32;
                    let zj = (r.z + j as i32) * scale;
                    let (v, noisedelta) = self.biome(xi, yk, zj);
                    out[k * sx * sz + j * sx + i] = v;
                    fill_rad_3d(
                        out,
                        i as i32,
                        j as i32,
                        k as i32,
                        r.sx,
                        r.sy,
                        r.sz,
                        v,
                        noisedelta * invgrad,
                    );
                }
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn fill_rad_3d(
    out: &mut [i32],
    x: i32,
    y: i32,
    z: i32,
    sx: i32,
    sy: i32,
    sz: i32,
    id: i32,
    rad: f32,
) {
    let r = rad as i32;
    if r <= 0 {
        return;
    }
    let rsq = ((rad * rad) as f64).floor() as i32;
    for k in -r..=r {
        let ak = y + k;
        if ak < 0 || ak >= sy {
            continue;
        }
        let ksq = k * k;
        for j in -r..=r {
            let aj = z + j;
            if aj < 0 || aj >= sz {
                continue;
            }
            let jksq = j * j + ksq;
            for i in -r..=r {
                let ai = x + i;
                if ai < 0 || ai >= sx {
                    continue;
                }
                if i * i + jksq > rsq {
                    continue;
                }
                out[(ak as i64 * sx as i64 * sz as i64 + aj as i64 * sx as i64 + ai as i64)
                    as usize] = id;
            }
        }
    }
}

// ------------------------------------------------------------------ end --

#[derive(Clone, Debug)]
pub struct EndNoise {
    perlin: Perlin,
}

fn end_island_weight(rx: i64, rz: i64) -> u32 {
    (rx as f32)
        .abs()
        .mul_add(3439.0f32, (rz as f32).abs() * 147.0f32) as u32
        % 13
        + 9
}

fn end_biome(hx: i64, hz: i64, hmap: &[u16], start: usize, hw: usize) -> i32 {
    const DS: [u16; 26] = [
        625, 529, 441, 361, 289, 225, 169, 121, 81, 49, 25, 9, 1, 1, 9, 25, 49, 81, 121, 169, 225,
        289, 361, 441, 529, 625,
    ];
    let dsi = &DS[(hx < 0) as usize..];
    let dsj = &DS[(hz < 0) as usize..];
    let mut h: u32 = if (hx as i32).abs() <= 15 && (hz as i32).abs() <= 15 {
        (64 * ((hx as i32) * (hx as i32) + (hz as i32) * (hz as i32))) as u32
    } else {
        14401
    };
    for (j, &dj) in dsj.iter().enumerate().take(25) {
        let row = &hmap[start + j * hw..start + j * hw + 25];
        for (i, &e) in row.iter().enumerate() {
            if e != 0 {
                let u = (dsi[i] as u32 + dj as u32) * e as u32;
                if u < h {
                    h = u;
                }
            }
        }
    }
    if h < 3600 {
        END_HIGHLANDS
    } else if h <= 10000 {
        END_MIDLANDS
    } else if h <= 14400 {
        END_BARRENS
    } else {
        SMALL_END_ISLANDS
    }
}

impl EndNoise {
    pub fn new(seed: u64) -> Self {
        let mut s = JavaRandom::new(seed);
        s.skip(17292);
        Self {
            perlin: Perlin::new_java(&mut s),
        }
    }

    /// `mapEndBiome`: biomes at 1:16 (chunk) resolution.
    pub fn map_biome(&self, out: &mut [i32], x: i32, z: i32, w: i32, h: i32) {
        let hw = w as i64 + 26;
        let hh = h as i64 + 26;
        let mut hmap = vec![0u16; (hw * hh) as usize];
        for j in 0..hh {
            for i in 0..hw {
                let rx = x as i64 + i - 12;
                let rz = z as i64 + j - 12;
                let rsq = (rx * rx + rz * rz) as u64;
                let mut v = 0u16;
                if rsq > 4096
                    && self.perlin.sample_simplex_2d(rx as f64, rz as f64) < (-0.9f32) as f64
                {
                    v = end_island_weight(rx, rz) as u16;
                    v = v.wrapping_mul(v);
                }
                hmap[(j * hw + i) as usize] = v;
            }
        }
        for j in 0..h as i64 {
            for i in 0..w as i64 {
                let mut hx = i + x as i64;
                let mut hz = j + z as i64;
                let mut rsq = (hx * hx + hz * hz) as u64;
                let o = (j * w as i64 + i) as usize;
                if rsq <= 4096 {
                    out[o] = THE_END;
                    continue;
                }
                hx = 2 * hx + 1;
                hz = 2 * hz + 1;
                rsq = (hx.wrapping_mul(hx)).wrapping_add(hz.wrapping_mul(hz)) as u64;
                if (rsq as u32 as i32) < 0 {
                    out[o] = END_BARRENS;
                    continue;
                }
                let start = ((hz / 2 - z as i64) * hw + (hx / 2 - x as i64)) as usize;
                out[o] = end_biome(hx, hz, &hmap, start, hw as usize);
            }
        }
    }

    /// `mapEnd`: biomes at 1:4.
    pub fn map(&self, out: &mut [i32], x: i32, z: i32, w: i32, h: i32) {
        let cx = x >> 2;
        let cz = z >> 2;
        let cw = ((x + w) >> 2) + 1 - cx;
        let ch = ((z + h) >> 2) + 1 - cz;
        let mut buf = vec![0i32; (cw * ch) as usize];
        self.map_biome(&mut buf, cx, cz, cw, ch);
        for j in 0..h {
            let cj = ((z + j) >> 2) - cz;
            for i in 0..w {
                let ci = ((x + i) >> 2) - cx;
                out[(j * w + i) as usize] = buf[(cj * cw + ci) as usize];
            }
        }
    }

    /// `getEndHeightNoise`, for scales other than 1, 4 and 16.
    pub fn height_noise(&self, x: i32, z: i32, mut range: i32) -> f32 {
        let hx = x / 2;
        let hz = z / 2;
        let oddx = x % 2;
        let oddz = z % 2;
        let mut h = 64 * (x as i64 * x as i64 + z as i64 * z as i64);
        if range == 0 {
            range = 12;
        }
        for j in -range..=range {
            for i in -range..=range {
                let rx = (hx + i) as i64;
                let rz = (hz + j) as i64;
                let rsq = (rx * rx + rz * rz) as u64;
                if rsq > 4096
                    && self.perlin.sample_simplex_2d(rx as f64, rz as f64) < (-0.9f32) as f64
                {
                    let v = end_island_weight(rx, rz) as i64;
                    let rx = (oddx - i * 2) as i64;
                    let rz = (oddz - j * 2) as i64;
                    let noise = (rx * rx + rz * rz) * v * v;
                    if noise < h {
                        h = noise;
                    }
                }
            }
        }
        let ret = 100.0f32 - (h as f32).sqrt();
        ret.clamp(-100.0, 80.0)
    }
}

// -------------------------------------------------------------- voronoi --

/// `getVoronoiSHA`: the first eight bytes of SHA-256 over the seed.
pub fn voronoi_sha(seed: u64) -> u64 {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];
    const B: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];
    let mut m = [0u32; 64];
    m[0] = (seed as u32).swap_bytes();
    m[1] = ((seed >> 32) as u32).swap_bytes();
    m[2] = 0x8000_0000;
    m[15] = 0x40;
    for i in 16..64 {
        m[i] = m[i - 7].wrapping_add(m[i - 16]);
        let x = m[i - 15];
        m[i] = m[i].wrapping_add(x.rotate_right(7) ^ x.rotate_right(18) ^ (x >> 3));
        let x = m[i - 2];
        m[i] = m[i].wrapping_add(x.rotate_right(17) ^ x.rotate_right(19) ^ (x >> 10));
    }
    let mut a = B;
    for i in 0..64 {
        let mut x = a[7].wrapping_add(K[i]).wrapping_add(m[i]);
        x = x.wrapping_add(a[4].rotate_right(6) ^ a[4].rotate_right(11) ^ a[4].rotate_right(25));
        x = x.wrapping_add((a[4] & a[5]) ^ (!a[4] & a[6]));
        let mut y = a[0].rotate_right(2) ^ a[0].rotate_right(13) ^ a[0].rotate_right(22);
        y = y.wrapping_add((a[0] & a[1]) ^ (a[0] & a[2]) ^ (a[1] & a[2]));
        a = [
            x.wrapping_add(y),
            a[0],
            a[1],
            a[2],
            a[3].wrapping_add(x),
            a[4],
            a[5],
            a[6],
        ];
    }
    let a0 = a[0].wrapping_add(B[0]);
    let a1 = a[1].wrapping_add(B[1]);
    a0.swap_bytes() as u64 | ((a1.swap_bytes() as u64) << 32)
}

fn voronoi_cell(sha: u64, a: i32, b: i32, c: i32) -> (i32, i32, i32) {
    let (a, b, c) = (a as i64 as u64, b as i64 as u64, c as i64 as u64);
    let mut s = sha;
    for v in [a, b, c, a, b, c] {
        s = mc_step_seed(s, v);
    }
    let f = |s: u64| ((((s >> 24) & 1023) as i64 - 512) * 36) as i32;
    let x = f(s);
    s = mc_step_seed(s, sha);
    let y = f(s);
    s = mc_step_seed(s, sha);
    (x, y, f(s))
}

/// `voronoiAccess3D`: the quarter cell a block takes its biome from.
pub fn voronoi_access_3d(sha: u64, x: i32, y: i32, z: i32) -> (i32, i32, i32) {
    let (x, y, z) = (x - 2, y - 2, z - 2);
    let (px, py, pz) = (x >> 2, y >> 2, z >> 2);
    let (dx, dy, dz) = ((x & 3) * 10240, (y & 3) * 10240, (z & 3) * 10240);
    let (mut ax, mut ay, mut az) = (0, 0, 0);
    let mut dmin = u64::MAX;
    for i in 0..8 {
        let (bx, by, bz) = (
            ((i & 4) != 0) as i32,
            ((i & 2) != 0) as i32,
            ((i & 1) != 0) as i32,
        );
        let (cx, cy, cz) = (px + bx, py + by, pz + bz);
        let (mut rx, mut ry, mut rz) = voronoi_cell(sha, cx, cy, cz);
        rx += dx - 40 * 1024 * bx;
        ry += dy - 40 * 1024 * by;
        rz += dz - 40 * 1024 * bz;
        let sq = |v: i32| (v as i64 as u64).wrapping_mul(v as i64 as u64);
        let d = sq(rx).wrapping_add(sq(ry)).wrapping_add(sq(rz));
        if d < dmin {
            dmin = d;
            (ax, ay, az) = (cx, cy, cz);
        }
    }
    (ax, ay, az)
}

/// `getVoronoiSrcRange`: the 1:4 cells a 1:1 range reads.
pub fn voronoi_src_range(r: Range) -> Range {
    let x = r.x - 2;
    let z = r.z - 2;
    let (sx_, sz_) = (x >> 2, z >> 2);
    let (y, sy) = if r.sy < 1 {
        (0, 0)
    } else {
        let ty = r.y - 2;
        (ty >> 2, ((ty + r.sy) >> 2) - (ty >> 2) + 2)
    };
    Range {
        scale: 4,
        x: sx_,
        z: sz_,
        sx: ((x + r.sx) >> 2) - sx_ + 2,
        sz: ((z + r.sz) >> 2) - sz_ + 2,
        y,
        sy,
    }
}

/// `mapVoronoiPlane`: the End's 1:1 plane at height `y` from its 1:4 map.
#[allow(clippy::too_many_arguments)]
pub fn map_voronoi_plane(
    sha: u64,
    out: &mut [i32],
    src: &[i32],
    x: i32,
    z: i32,
    w: i32,
    h: i32,
    y: i32,
    px: i32,
    pz: i32,
    pw: i32,
    ph: i32,
) {
    let (x, y, z) = (x - 2, y - 2, z - 2);
    let (w64, pw64) = (w as i64, pw as i64);
    let mut c = [(0i32, 0i32, 0i32); 8]; // 000 001 010 011 100 101 110 111
    for pj in 0..ph - 1 {
        let mut v00 = src[(pj as i64 * pw64) as usize];
        let mut v10 = src[((pj + 1) as i64 * pw64) as usize];
        let pjz = pz + pj;
        let j4 = pjz * 4 - z;
        let mut prev_skip = true;
        for pi in 0..pw - 1 {
            let v01 = src[(pj as i64 * pw64 + (pi + 1) as i64) as usize];
            let v11 = src[((pj + 1) as i64 * pw64 + (pi + 1) as i64) as usize];
            let pix = px + pi;
            let i4 = pix * 4 - x;
            if v00 == v01 && v00 == v10 && v00 == v11 {
                for jj in 0..4 {
                    let j = j4 + jj;
                    if j < 0 || j >= h {
                        continue;
                    }
                    for ii in 0..4 {
                        let i = i4 + ii;
                        if i < 0 || i >= w {
                            continue;
                        }
                        out[(j as i64 * w64 + i as i64) as usize] = v00;
                    }
                }
                prev_skip = true;
                continue;
            }
            if prev_skip {
                c[0] = voronoi_cell(sha, pix, y - 1, pjz);
                c[1] = voronoi_cell(sha, pix, y, pjz);
                c[4] = voronoi_cell(sha, pix, y - 1, pjz + 1);
                c[5] = voronoi_cell(sha, pix, y, pjz + 1);
                prev_skip = false;
            }
            c[2] = voronoi_cell(sha, pix + 1, y - 1, pjz);
            c[3] = voronoi_cell(sha, pix + 1, y, pjz);
            c[6] = voronoi_cell(sha, pix + 1, y - 1, pjz + 1);
            c[7] = voronoi_cell(sha, pix + 1, y, pjz + 1);
            const A: i64 = 40 * 1024;
            const B: i64 = 20 * 1024;
            for jj in 0..4 {
                let j = j4 + jj;
                if j < 0 || j >= h {
                    continue;
                }
                for ii in 0..4 {
                    let i = i4 + ii;
                    if i < 0 || i >= w {
                        continue;
                    }
                    let dx = ii as i64 * 10 * 1024;
                    let dz = jj as i64 * 10 * 1024;
                    let mut dmin = u64::MAX;
                    let mut v = v00;
                    // (cell, x offset, y offset, z offset, value)
                    let cand = [
                        (c[0], 0, B, 0, v00),
                        (c[1], 0, -B, 0, v00),
                        (c[2], -A, B, 0, v01),
                        (c[3], -A, -B, 0, v01),
                        (c[4], 0, B, -A, v10),
                        (c[5], 0, -B, -A, v10),
                        (c[6], -A, B, -A, v11),
                        (c[7], -A, -B, -A, v11),
                    ];
                    for ((cx, cy, cz), ox, oy, oz, val) in cand {
                        let r1 = cx as i64 + ox + dx;
                        let r2 = cy as i64 + oy;
                        let r3 = cz as i64 + oz + dz;
                        let d = (r1.wrapping_mul(r1) as u64)
                            .wrapping_add(r2.wrapping_mul(r2) as u64)
                            .wrapping_add(r3.wrapping_mul(r3) as u64);
                        if d < dmin {
                            dmin = d;
                            v = val;
                        }
                    }
                    out[(j as i64 * w64 + i as i64) as usize] = v;
                }
            }
            c[0] = c[2];
            c[4] = c[6];
            c[1] = c[3];
            c[5] = c[7];
            v00 = v01;
            v10 = v11;
        }
    }
}
