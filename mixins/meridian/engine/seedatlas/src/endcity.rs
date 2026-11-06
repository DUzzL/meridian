//! End cities: whether the island under a start is high enough
//! (`isViableEndCityTerrain` with the End's surface noise) and whether the
//! city has a ship (`getEndCityPieces`), ported from the engine for 26.3.

use crate::biome_noise::EndNoise;
use crate::noise::Octave;
use crate::rng::{JavaRandom, clamped_lerp, lerp, lerp3};
use crate::structures::chunk_generate_rnd;

/// The End's terrain noise (`SurfaceNoise` for `DIM_END`).
#[derive(Clone, Debug)]
pub struct EndSurface {
    octmin: Octave,
    octmax: Octave,
    octmain: Octave,
}

impl EndSurface {
    pub fn new(seed: u64) -> Self {
        let mut s = JavaRandom::new(seed);
        let octmin = Octave::new_java(&mut s, -15, 16);
        let octmax = Octave::new_java(&mut s, -15, 16);
        let octmain = Octave::new_java(&mut s, -7, 8);
        Self {
            octmin,
            octmax,
            octmain,
        }
    }

    /// `sampleSurfaceNoiseBetween` with the End's scales.
    fn sample_between(&self, x: i32, y: i32, z: i32, noise_min: f64, noise_max: f64) -> f64 {
        let xz_scale = 684.412 * 2.0;
        let y_scale = 684.412 * 1.0;
        let (mut vmin, mut vmax) = (0.0, 0.0);
        let mut persist = 1.0 / 32768.0;
        let mut amp = 64.0;
        for i in (0..16).rev() {
            let dx = x as f64 * xz_scale * persist;
            let dz = z as f64 * xz_scale * persist;
            let sy = y_scale * persist;
            let dy = y as f64 * sy;
            vmin += self.octmin.octaves[i].sample(dx, dy, dz, sy, dy) * amp;
            vmax += self.octmax.octaves[i].sample(dx, dy, dz, sy, dy) * amp;
            if vmin - amp > noise_max && vmax - amp > noise_max {
                return noise_max;
            }
            if vmin + amp < noise_min && vmax + amp < noise_min {
                return noise_min;
            }
            amp *= 0.5;
            persist *= 2.0;
        }
        let xz_step = xz_scale / 80.0;
        let y_step = y_scale / 160.0;
        let mut vmain = 0.5;
        persist = 1.0 / 128.0;
        amp = 0.05 * 128.0;
        for i in (0..8).rev() {
            let dx = x as f64 * xz_step * persist;
            let dz = z as f64 * xz_step * persist;
            let sy = y_step * persist;
            let dy = y as f64 * sy;
            vmain += self.octmain.octaves[i].sample(dx, dy, dz, sy, dy) * amp;
            if vmain - amp > 1.0 {
                return vmax;
            }
            if vmain + amp < 0.0 {
                return vmin;
            }
            amp *= 0.5;
            persist *= 2.0;
        }
        clamped_lerp(vmain, vmin, vmax)
    }

    /// `sampleNoiseColumnEnd` for cells `y0..=y1`.
    fn column(&self, en: &EndNoise, x: i32, z: i32, y0: i32, y1: i32) -> Vec<f64> {
        const UPPER: [f64; 33] = [
            1.0,
            1.0,
            1.0,
            1.0,
            1.0,
            1.0,
            1.0,
            1.0,
            1.0,
            1.0,
            1.0,
            1.0,
            1.0,
            1.0,
            1.0,
            63. / 64.,
            62. / 64.,
            61. / 64.,
            60. / 64.,
            59. / 64.,
            58. / 64.,
            57. / 64.,
            56. / 64.,
            55. / 64.,
            54. / 64.,
            53. / 64.,
            52. / 64.,
            51. / 64.,
            50. / 64.,
            49. / 64.,
            48. / 64.,
            47. / 64.,
            46. / 64.,
        ];
        const LOWER: [f64; 33] = [
            0.0,
            0.0,
            1. / 7.,
            2. / 7.,
            3. / 7.,
            4. / 7.,
            5. / 7.,
            6. / 7.,
            1.0,
            1.0,
            1.0,
            1.0,
            1.0,
            1.0,
            1.0,
            1.0,
            1.0,
            1.0,
            1.0,
            1.0,
            1.0,
            1.0,
            1.0,
            1.0,
            1.0,
            1.0,
            1.0,
            1.0,
            1.0,
            1.0,
            1.0,
            1.0,
            1.0,
        ];
        let rsq = (x as i64 as u64)
            .wrapping_mul(x as i64 as u64)
            .wrapping_add((z as i64 as u64).wrapping_mul(z as i64 as u64));
        if (rsq as u32 as i32) < 0 {
            return vec![f64::NAN; (y1 - y0 + 1) as usize];
        }
        let depth = (en.height_noise(x, z, 0) - 8.0f32) as f64;
        (y0..=y1)
            .map(|y| {
                let y = y as usize;
                if LOWER[y] == 0.0 {
                    return -30.0;
                }
                let noise = self.sample_between(x, y as i32, z, -128.0, 128.0);
                let clamped = lerp(UPPER[y], -3000.0, noise + depth);
                lerp(LOWER[y], -30.0, clamped)
            })
            .collect()
    }
}

/// `getSurfaceHeight`.
#[allow(clippy::too_many_arguments)]
fn surface_height(
    c00: &[f64],
    c01: &[f64],
    c10: &[f64],
    c11: &[f64],
    y0: i32,
    y1: i32,
    cell: i32,
    dx: f64,
    dz: f64,
) -> i32 {
    for celly in (y0..y1).rev() {
        let i = (celly - y0) as usize;
        for y in (0..cell).rev() {
            let dy = y as f64 / cell as f64;
            let noise = lerp3(
                dy,
                dx,
                dz,
                c00[i],
                c00[i + 1],
                c10[i],
                c10[i + 1],
                c01[i],
                c01[i + 1],
                c11[i],
                c11[i + 1],
            );
            if noise > 0.0 {
                return celly * cell + y;
            }
        }
    }
    0
}

/// `isViableEndCityTerrain` (1.19+): the lowest of four corner heights of
/// the start piece, or 0 below Y 60.
pub fn end_city_terrain(seed: u64, en: &EndNoise, sn: &EndSurface, x: i32, z: i32) -> i32 {
    let (cx, cz) = (x >> 4, z >> 4);
    let (bx, bz) = (cx * 16 + 7, cz * 16 + 7);
    let (cellx, cellz) = (bx >> 3, bz >> 3);
    const Y0: i32 = 15;
    const Y1: i32 = 18;
    let col = |dx: i32, dz: i32| sn.column(en, cellx + dx, cellz + dz, Y0, Y1);
    let frac = |v: i32| (v & 7) as f64 / 8.0;
    let (c00, c01, c10, c11) = (col(0, 0), col(0, 1), col(1, 0), col(1, 1));
    let h = |a: &[f64], b: &[f64], c: &[f64], d: &[f64], dx: f64, dz: f64| {
        surface_height(a, b, c, d, Y0, Y1, 4, dx, dz)
    };
    let mut h00 = h(&c00, &c01, &c10, &c11, frac(bx), frac(bz));
    let mut rng = chunk_generate_rnd(seed, cx, cz);
    let (h01, h10, h11) = match rng.next_int(4) {
        0 => {
            let (c02, c12, c20, c21, c22) = (col(0, 2), col(1, 2), col(2, 0), col(2, 1), col(2, 2));
            (
                h(&c01, &c02, &c11, &c12, frac(bx), frac(bz + 5)),
                h(&c10, &c11, &c20, &c21, frac(bx + 5), frac(bz)),
                h(&c11, &c12, &c21, &c22, frac(bx + 5), frac(bz + 5)),
            )
        }
        1 => {
            let (c02, c12) = (col(0, 2), col(1, 2));
            (
                h(&c01, &c02, &c11, &c12, frac(bx), frac(bz + 5)),
                h(&c00, &c01, &c10, &c11, frac(bx - 5), frac(bz)),
                h(&c01, &c02, &c11, &c12, frac(bx - 5), frac(bz + 5)),
            )
        }
        2 => (
            h(&c00, &c01, &c10, &c11, frac(bx), frac(bz - 5)),
            h(&c00, &c01, &c10, &c11, frac(bx - 5), frac(bz)),
            h(&c00, &c01, &c10, &c11, frac(bx - 5), frac(bz - 5)),
        ),
        _ => {
            let (c20, c21) = (col(2, 0), col(2, 1));
            (
                h(&c00, &c01, &c10, &c11, frac(bx), frac(bz - 5)),
                h(&c10, &c11, &c20, &c21, frac(bx + 5), frac(bz)),
                h(&c10, &c11, &c20, &c21, frac(bx + 5), frac(bz - 5)),
            )
        }
    };
    h00 = h00.min(h01).min(h10).min(h11);
    if h00 >= 60 { h00 } else { 0 }
}

// --------------------------------------------------------------- pieces --

const BASE_FLOOR: usize = 0;
const BASE_ROOF: usize = 1;
const BRIDGE_END: usize = 2;
const BRIDGE_GENTLE_STAIRS: usize = 3;
const BRIDGE_PIECE: usize = 4;
const BRIDGE_STEEP_STAIRS: usize = 5;
const FAT_TOWER_BASE: usize = 6;
const FAT_TOWER_MIDDLE: usize = 7;
const FAT_TOWER_TOP: usize = 8;
const SECOND_FLOOR_1: usize = 9;
const SECOND_FLOOR_2: usize = 10;
const SECOND_ROOF: usize = 11;
pub const END_SHIP: usize = 12;
const THIRD_FLOOR_1: usize = 13;
const THIRD_FLOOR_2: usize = 14;
const THIRD_ROOF: usize = 15;
const TOWER_BASE: usize = 16;
const TOWER_PIECE: usize = 18;
const TOWER_TOP: usize = 19;

const SIZES: [[i32; 3]; 20] = [
    [9, 3, 9],
    [11, 1, 11],
    [4, 5, 1],
    [4, 6, 7],
    [4, 5, 3],
    [4, 6, 3],
    [12, 3, 12],
    [12, 7, 12],
    [16, 5, 16],
    [11, 7, 11],
    [11, 7, 11],
    [13, 1, 13],
    [12, 23, 28],
    [13, 7, 13],
    [13, 7, 13],
    [15, 1, 15],
    [6, 6, 6],
    [6, 3, 6],
    [6, 3, 6],
    [8, 4, 8],
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Piece {
    pub kind: usize,
    pub rot: i32,
    pub depth: i8,
    pub pos: [i32; 3],
    pub bb0: [i32; 3],
    pub bb1: [i32; 3],
}

struct City {
    pieces: Vec<Piece>,
    rng: JavaRandom,
    ship: bool,
}

/// Where a generator writes: its list starts at `base` and holds `n`.
struct Env {
    base: usize,
    n: usize,
    y: i32,
}

type Gen = fn(&mut City, &mut Env, usize, i32) -> bool;

impl City {
    #[allow(clippy::too_many_arguments)]
    fn add(
        &mut self,
        env: &mut Env,
        prev: Option<usize>,
        rot: i32,
        px: i32,
        py: i32,
        pz: i32,
        kind: usize,
    ) -> usize {
        let at = env.base + env.n;
        self.pieces.truncate(at);
        env.n += 1;
        let pos = prev.map_or([px, py, pz], |p| self.pieces[p].pos);
        let mut p = Piece {
            kind,
            rot,
            depth: 0,
            pos,
            bb0: pos,
            bb1: pos,
        };
        let [sx, sy, sz] = SIZES[kind];
        p.bb1[1] += sy;
        match rot {
            0 => {
                p.bb1[0] += sx;
                p.bb1[2] += sz;
            }
            1 => {
                p.bb0[0] -= sz;
                p.bb1[2] += sx;
            }
            2 => {
                p.bb0[0] -= sx;
                p.bb0[2] -= sz;
            }
            _ => {
                p.bb1[0] += sz;
                p.bb0[2] -= sx;
            }
        }
        if let Some(prev) = prev {
            let (dx, dz) = match self.pieces[prev].rot {
                0 => (px, pz),
                1 => (-pz, px),
                2 => (-px, -pz),
                _ => (pz, -px),
            };
            for v in [&mut p.pos, &mut p.bb0, &mut p.bb1] {
                v[0] += dx;
                v[1] += py;
                v[2] += dz;
            }
        }
        self.pieces.push(p);
        at
    }

    /// `genPiecesRecusively`.
    fn recurse(&mut self, generate: Gen, env: &mut Env, current: usize, depth: i32) -> bool {
        if depth > 8 {
            return false;
        }
        let mut local = Env {
            base: env.base + env.n,
            n: 0,
            y: env.y,
        };
        if !generate(self, &mut local, current, depth) {
            return false;
        }
        let gendepth = self.rng.next(32) as i8;
        for i in 0..local.n {
            let p = local.base + i;
            self.pieces[p].depth = gendepth;
            let pb = self.pieces[p];
            for j in 0..env.n {
                let q = self.pieces[env.base + j];
                if q.bb1[0] >= pb.bb0[0]
                    && q.bb0[0] <= pb.bb1[0]
                    && q.bb1[2] >= pb.bb0[2]
                    && q.bb0[2] <= pb.bb1[2]
                    && q.bb1[1] >= pb.bb0[1]
                    && q.bb0[1] <= pb.bb1[1]
                {
                    if self.pieces[current].depth != q.depth {
                        return false;
                    }
                    break;
                }
            }
        }
        env.n += local.n;
        true
    }
}

fn gen_tower(c: &mut City, env: &mut Env, current: usize, depth: i32) -> bool {
    let rot = c.pieces[current].rot;
    let x = 3 + c.rng.next_int(2);
    let z = 3 + c.rng.next_int(2);
    let mut base = c.add(env, Some(current), rot, x, -3, z, TOWER_BASE);
    base = c.add(env, Some(base), rot, 0, 7, 0, TOWER_PIECE);
    let mut floor = (c.rng.next_int(3) == 0).then_some(base);
    let floorcnt = 1 + c.rng.next_int(3);
    for i in 0..floorcnt {
        base = c.add(env, Some(base), rot, 0, 4, 0, TOWER_PIECE);
        if i < floorcnt - 1 && c.rng.next(1) != 0 {
            floor = Some(base);
        }
    }
    if floor.is_some() {
        const BINFO: [[i32; 4]; 4] = [[0, 1, -1, 0], [1, 6, -1, 1], [3, 0, -1, 5], [2, 5, -1, 6]];
        for b in BINFO {
            if c.rng.next(1) == 0 {
                continue;
            }
            let brot = (rot + b[0]) & 3;
            let bridge = c.add(env, Some(base), brot, b[1], b[2], b[3], BRIDGE_END);
            c.recurse(gen_bridge, env, bridge, depth + 1);
        }
    } else if depth != 7 {
        return c.recurse(gen_fat_tower, env, base, depth + 1);
    }
    c.add(env, Some(base), rot, -1, 4, -1, TOWER_TOP);
    true
}

fn gen_bridge(c: &mut City, env: &mut Env, current: usize, depth: i32) -> bool {
    let rot = c.pieces[current].rot;
    let floorcnt = 1 + c.rng.next_int(4);
    let mut base = c.add(env, Some(current), rot, 0, 0, -4, BRIDGE_PIECE);
    c.pieces[base].depth = -1;
    let mut y = 0;
    for _ in 0..floorcnt {
        if c.rng.next(1) != 0 {
            base = c.add(env, Some(base), rot, 0, y, -4, BRIDGE_PIECE);
            y = 0;
            continue;
        }
        base = if c.rng.next(1) != 0 {
            c.add(env, Some(base), rot, 0, y, -4, BRIDGE_STEEP_STAIRS)
        } else {
            c.add(env, Some(base), rot, 0, y, -8, BRIDGE_GENTLE_STAIRS)
        };
        y = 4;
    }
    if !c.ship && c.rng.next_int(10 - depth) == 0 {
        let x = -8 + c.rng.next_int(8);
        let z = -70 + c.rng.next_int(10);
        base = c.add(env, Some(base), rot, x, y, z, END_SHIP);
        c.ship = true;
    } else {
        env.y = y + 1;
        if !c.recurse(gen_house_tower, env, base, depth + 1) {
            return false;
        }
    }
    base = c.add(env, Some(base), (rot + 2) & 3, 4, y, 0, BRIDGE_END);
    c.pieces[base].depth = -1;
    true
}

fn gen_house_tower(c: &mut City, env: &mut Env, current: usize, depth: i32) -> bool {
    if depth > 8 {
        return false;
    }
    let rot = c.pieces[current].rot;
    let mut base = c.add(env, Some(current), rot, -3, env.y, -11, BASE_FLOOR);
    let size = c.rng.next_int(3);
    if size == 0 {
        c.add(env, Some(base), rot, -1, 4, -1, BASE_ROOF);
        return true;
    }
    base = c.add(env, Some(base), rot, -1, 0, -1, SECOND_FLOOR_2);
    if size == 1 {
        base = c.add(env, Some(base), rot, -1, 8, -1, SECOND_ROOF);
    } else {
        base = c.add(env, Some(base), rot, -1, 4, -1, THIRD_FLOOR_2);
        base = c.add(env, Some(base), rot, -1, 8, -1, THIRD_ROOF);
    }
    c.recurse(gen_tower, env, base, depth + 1);
    true
}

fn gen_fat_tower(c: &mut City, env: &mut Env, current: usize, depth: i32) -> bool {
    let rot = c.pieces[current].rot;
    let mut base = c.add(env, Some(current), rot, -3, 4, -3, FAT_TOWER_BASE);
    base = c.add(env, Some(base), rot, 0, 4, 0, FAT_TOWER_MIDDLE);
    const BINFO: [[i32; 4]; 4] = [[0, 4, -1, 0], [1, 12, -1, 4], [3, 0, -1, 8], [2, 8, -1, 12]];
    let mut j = 0;
    while j < 2 && c.rng.next_int(3) != 0 {
        base = c.add(env, Some(base), rot, 0, 8, 0, FAT_TOWER_MIDDLE);
        for b in BINFO {
            if c.rng.next(1) == 0 {
                continue;
            }
            let brot = (rot + b[0]) & 3;
            let bridge = c.add(env, Some(base), brot, b[1], b[2], b[3], BRIDGE_END);
            c.recurse(gen_bridge, env, bridge, depth + 1);
        }
        j += 1;
    }
    c.add(env, Some(base), rot, -2, 8, -2, FAT_TOWER_TOP);
    true
}

/// `getEndCityPieces`: the city's pieces.
pub fn end_city_pieces(seed: u64, cx: i32, cz: i32) -> Vec<Piece> {
    let mut rng = chunk_generate_rnd(seed, cx, cz);
    let rot = rng.next_int(4);
    let mut c = City {
        pieces: Vec::new(),
        rng,
        ship: false,
    };
    let mut env = Env {
        base: 0,
        n: 0,
        y: 0,
    };
    let (x, z) = (cx * 16 + 8, cz * 16 + 8);
    let mut base = c.add(&mut env, None, rot, x, 0, z, BASE_FLOOR);
    base = c.add(&mut env, Some(base), rot, -1, 0, -1, SECOND_FLOOR_1);
    base = c.add(&mut env, Some(base), rot, -1, 4, -1, THIRD_FLOOR_1);
    base = c.add(&mut env, Some(base), rot, -1, 8, -1, THIRD_ROOF);
    c.recurse(gen_tower, &mut env, base, 1);
    c.pieces.truncate(env.n);
    c.pieces
}

/// Whether the city at a start has a ship.
pub fn end_city_has_ship(seed: u64, x: i32, z: i32) -> bool {
    end_city_pieces(seed, x >> 4, z >> 4)
        .iter()
        .any(|p| p.kind == END_SHIP)
}
