//! Terrain heights and the Nether floor (ABI 6, Minecraft 1.18 and later),
//! ported from the glue `seedatlas_xaero.c`: Overworld heights from the
//! climate depth, the End's islands from its surface noise on a shared island
//! grid, and the Nether's floor from the vanilla `old_blended_noise` density.

use crate::atlas::{AtlasError, MAX_AREA_SAMPLES};
use crate::biome_noise::{BiomeNoise, SAMPLE_NO_BIOME};
use crate::generator::Dimension;
use crate::noise::{Octave, Perlin};
use crate::rng::{JavaRandom, clamped_lerp, lerp3};

/// The engine's `NP_DEPTH` (shares the slot of the shift noise).
const NP_DEPTH: usize = 4;

/// `SAX_TERRAIN_MC`: the first version with terrain functions.
pub const TERRAIN_MC: i32 = crate::mc::MC_1_18;

/// The engine's `SurfaceNoise` with scales that can differ per use.
#[derive(Clone, Debug)]
pub struct TerrainNoise {
    octmin: Octave,
    octmax: Octave,
    octmain: Octave,
    xz_scale: f64,
    y_scale: f64,
    xz_factor: f64,
    y_factor: f64,
}

impl TerrainNoise {
    /// `initSurfaceNoise` for `DIM_END`: the End's scales (xz 2, y 1,
    /// factors 80 and 160).
    pub fn new_end(seed: u64) -> Self {
        let mut s = JavaRandom::new(seed);
        let octmin = Octave::new_java(&mut s, -15, 16);
        let octmax = Octave::new_java(&mut s, -15, 16);
        let octmain = Octave::new_java(&mut s, -7, 8);
        Self {
            octmin,
            octmax,
            octmain,
            xz_scale: 2.0,
            y_scale: 1.0,
            xz_factor: 80.0,
            y_factor: 160.0,
        }
    }

    /// The glue's Nether noise: the same octaves with `noise_settings/nether`
    /// scales (xz 1, y 3, factors 80 and 60).
    pub fn new_nether(seed: u64) -> Self {
        let mut n = Self::new_end(seed);
        n.xz_scale = 1.0;
        n.y_scale = 3.0;
        n.xz_factor = 80.0;
        n.y_factor = 60.0;
        n
    }

    /// `sampleSurfaceNoiseBetween`.
    fn sample_between(&self, x: i32, y: i32, z: i32, noise_min: f64, noise_max: f64) -> f64 {
        let xz_scale = 684.412 * self.xz_scale;
        let y_scale = 684.412 * self.y_scale;
        let (mut vmin, mut vmax) = (0.0f64, 0.0f64);
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
        let xz_step = xz_scale / self.xz_factor;
        let y_step = y_scale / self.y_factor;
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

    /// `nether_noise` of the glue: `old_blended_noise` the way vanilla's
    /// `BlendedNoise` evaluates it (the main noise picks the blend first, so a
    /// saturated blend skips the limit noise it ignores).
    fn nether_noise(&self, x: i32, y: i32, z: i32) -> f64 {
        let xz_scale = 684.412 * self.xz_scale;
        let y_scale = 684.412 * self.y_scale;
        let xz_step = xz_scale / self.xz_factor;
        let y_step = y_scale / self.y_factor;
        let (mut min_noise, mut max_noise, mut main_noise) = (0.0f64, 0.0f64, 0.0f64);
        let (mut persist, mut contrib) = (1.0f64, 1.0f64);
        for i in 0..8 {
            let sy = y_step * persist;
            main_noise += self.octmain.octaves[i].sample(
                x as f64 * xz_step * persist,
                y as f64 * y_step * persist,
                z as f64 * xz_step * persist,
                sy,
                y as f64 * sy,
            ) * contrib;
            persist *= 0.5;
            contrib *= 2.0;
        }
        let blend = 0.5 + 0.05 * main_noise;
        persist = 1.0;
        contrib = 1.0;
        for i in 0..16 {
            let dx = x as f64 * xz_scale * persist;
            let dy = y as f64 * y_scale * persist;
            let dz = z as f64 * xz_scale * persist;
            let sy = y_scale * persist;
            if blend < 1.0 {
                min_noise += self.octmin.octaves[i].sample(dx, dy, dz, sy, y as f64 * sy) * contrib;
            }
            if blend > 0.0 {
                max_noise += self.octmax.octaves[i].sample(dx, dy, dz, sy, y as f64 * sy) * contrib;
            }
            persist *= 0.5;
            contrib *= 2.0;
        }
        clamped_lerp(blend, min_noise / 512.0, max_noise / 512.0)
    }
}

/// What the glue keeps in `sax_context` for terrain.
#[derive(Clone, Debug)]
pub struct Terrain {
    end_perlin: Perlin,
    end_height_noise: TerrainNoise,
    nether_noise: TerrainNoise,
}

impl Terrain {
    pub fn new(seed: u64) -> Self {
        // The End generator's simplex noise (`setEndSeed`).
        let mut s = JavaRandom::new(seed);
        s.skip(17292);
        Self {
            end_perlin: Perlin::new_java(&mut s),
            end_height_noise: TerrainNoise::new_end(seed),
            nether_noise: TerrainNoise::new_nether(seed),
        }
    }
}

fn floor_div(value: i32, divisor: i32) -> i32 {
    let q = value / divisor;
    let r = value % divisor;
    q - (r < 0) as i32
}

fn floor_div64(value: i64, divisor: i64) -> i64 {
    let q = value / divisor;
    q - ((value % divisor) < 0) as i64
}

fn sample_step_valid(step: i32) -> bool {
    (1..=1024).contains(&step) && step & (step - 1) == 0
}

// ---------------------------------------------------------- Overworld --

/// `height_at_quart`: the climate depth at a quart coordinate on the
/// engine's block scale.
fn height_at_quart(bn: &BiomeNoise, qx: i32, qz: i32) -> f32 {
    let (_, np) = bn.sample_np(qx, 0, qz, None, SAMPLE_NO_BIOME);
    (np[NP_DEPTH] as f64 / 76.0) as f32
}

// ---------------------------------------------------------------- End --

/// `end_island_grid`: island weights of the End height noise for a whole
/// area, 9..21, 0 where there is no island.
struct EndIslandGrid {
    x0: i64,
    z0: i64,
    width: i64,
    weights: Vec<u16>,
}

impl EndIslandGrid {
    /// `end_island_grid_init`.
    fn new(perlin: &Perlin, cell_x0: i32, cell_z0: i32, cell_x1: i32, cell_z1: i32) -> Self {
        let x0 = (cell_x0 / 2) as i64 - 12;
        let z0 = (cell_z0 / 2) as i64 - 12;
        let width = (cell_x1 / 2) as i64 + 12 - x0 + 1;
        let height = (cell_z1 / 2) as i64 + 12 - z0 + 1;
        let mut weights = vec![0u16; (width * height) as usize];
        for j in 0..height {
            for i in 0..width {
                let rx = x0 + i;
                let rz = z0 + j;
                let rsq = rx.wrapping_mul(rx).wrapping_add(rz.wrapping_mul(rz)) as u64;
                if rsq > 4096 && perlin.sample_simplex_2d(rx as f64, rz as f64) < (-0.9f32) as f64 {
                    let t = (rx as f32)
                        .abs()
                        .mul_add(3439.0f32, (rz as f32).abs() * 147.0f32);
                    weights[(j * width + i) as usize] = (t as u32 % 13 + 9) as u16;
                }
            }
        }
        Self {
            x0,
            z0,
            width,
            weights,
        }
    }

    /// `end_height_noise`: `getEndHeightNoise(en, x, z, 0)` with the island
    /// centres read from the grid.
    fn height_noise(&self, x: i32, z: i32) -> f32 {
        let hx = x / 2;
        let hz = z / 2;
        let oddx = x % 2;
        let oddz = z % 2;
        let mut h: i64 = 64i64.wrapping_mul(
            (x as i64)
                .wrapping_mul(x as i64)
                .wrapping_add((z as i64).wrapping_mul(z as i64)),
        );
        for j in -12i32..=12 {
            let row = ((hz + j) as i64 - self.z0) * self.width + (hx as i64 - self.x0);
            for i in -12i32..=12 {
                let v = self.weights[(row + i as i64) as usize];
                if v != 0 {
                    let rx = (oddx - i * 2) as i64;
                    let rz = (oddz - j * 2) as i64;
                    let rsq = rx.wrapping_mul(rx).wrapping_add(rz.wrapping_mul(rz)) as u64;
                    let v = v as u64;
                    let noise = rsq.wrapping_mul(v).wrapping_mul(v) as i64;
                    if noise < h {
                        h = noise;
                    }
                }
            }
        }
        (100.0f32 - (h as f32).sqrt()).clamp(-100.0, 80.0)
    }
}

const UPPER_DROP: [f64; 33] = [
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

const LOWER_DROP: [f64; 33] = [
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

fn glue_lerp(part: f64, from: f64, to: f64) -> f64 {
    from + part * (to - from)
}

/// `end_noise_column`: the engine's `sampleNoiseColumnEnd` on the island
/// grid, for cells `colymin..=colymax`.
fn end_noise_column(
    sn: &TerrainNoise,
    grid: &EndIslandGrid,
    x: i32,
    z: i32,
    colymin: i32,
    colymax: i32,
) -> Vec<f64> {
    let rsq = (x as i64 as u64)
        .wrapping_mul(x as i64 as u64)
        .wrapping_add((z as i64 as u64).wrapping_mul(z as i64 as u64));
    if (rsq as u32 as i32) < 0 {
        return vec![f64::NAN; (colymax - colymin + 1) as usize];
    }
    let depth = (grid.height_noise(x, z) - 8.0f32) as f64;
    (colymin..=colymax)
        .map(|y| {
            let yi = y as usize;
            if LOWER_DROP[yi] == 0.0 {
                return -30.0;
            }
            let noise = sn.sample_between(x, y, z, -128.0, 128.0);
            let clamped = noise + depth;
            let clamped = glue_lerp(UPPER_DROP[yi], -3000.0, clamped);
            glue_lerp(LOWER_DROP[yi], -30.0, clamped)
        })
        .collect()
}

/// `end_surface_height`: the engine's `getSurfaceHeight`.
#[allow(clippy::too_many_arguments)]
fn end_surface_height(
    c00: &[f64],
    c01: &[f64],
    c10: &[f64],
    c11: &[f64],
    colymin: i32,
    colymax: i32,
    blockspercell: i32,
    dx: f64,
    dz: f64,
) -> i32 {
    for celly in (colymin..colymax).rev() {
        let idx = (celly - colymin) as usize;
        for y in (0..blockspercell).rev() {
            let dy = y as f64 / blockspercell as f64;
            let noise = lerp3(
                dy,
                dx,
                dz,
                c00[idx],
                c00[idx + 1],
                c10[idx],
                c10[idx + 1],
                c01[idx],
                c01[idx + 1],
                c11[idx],
                c11[idx + 1],
            );
            if noise > 0.0 {
                return celly * blockspercell + y;
            }
        }
    }
    0
}

/// `end_surface_area`: `mapEndSurfaceHeight` on a shared island grid.
fn end_surface_area(t: &Terrain, x: i32, z: i32, w: i32, h: i32, scale: i32) -> Vec<f32> {
    let (y0, y1) = (2, 18);
    let yn = (y1 - y0 + 1) as usize;
    let cellmid = if scale > 1 { scale as f64 / 16.0 } else { 0.0 };
    let cellsiz = 8 / scale;
    let cx = floor_div(x, cellsiz);
    let cz = floor_div(z, cellsiz);
    let cw = floor_div(x.wrapping_add(w - 1), cellsiz) - cx + 2;
    let grid = EndIslandGrid::new(
        &t.end_perlin,
        cx,
        cz,
        cx + cw - 1,
        floor_div(z.wrapping_add(h - 1), cellsiz) + 1,
    );
    let sn = &t.end_height_noise;
    let columns = |cj: i32| -> Vec<f64> {
        let mut v = Vec::with_capacity(yn * cw as usize);
        for i in 0..cw {
            v.extend(end_noise_column(sn, &grid, cx + i, cj, y0, y1));
        }
        v
    };
    let mut out = vec![0f32; w as usize * h as usize];
    let mut ncol0 = Vec::new();
    let mut ncol1 = columns(cz);
    for j in 0..h {
        let cj = floor_div(z.wrapping_add(j), cellsiz);
        let dj = z.wrapping_add(j) - cj * cellsiz;
        if j == 0 || dj == 0 {
            let next = columns(cj + 1);
            ncol0 = std::mem::replace(&mut ncol1, next);
        }
        for i in 0..w {
            let ci = floor_div(x.wrapping_add(i), cellsiz);
            let di = x.wrapping_add(i) - ci * cellsiz;
            let dx = di as f64 / cellsiz as f64 + cellmid;
            let dz = dj as f64 / cellsiz as f64 + cellmid;
            let o = (ci - cx) as usize * yn;
            out[(j * w + i) as usize] = end_surface_height(
                &ncol0[o..],
                &ncol1[o..],
                &ncol0[o + yn..],
                &ncol1[o + yn..],
                y0,
                y1,
                4,
                dx,
                dz,
            ) as f32;
        }
    }
    out
}

/// `sax_height_area_sampled`; `terrain` is `None` before 1.18.
#[allow(clippy::too_many_arguments)]
pub fn height_area(
    terrain: Option<&Terrain>,
    bn: &BiomeNoise,
    dimension: Dimension,
    ox: i32,
    oz: i32,
    step: i32,
    w: i32,
    h: i32,
) -> Result<Vec<f32>, AtlasError> {
    if !sample_step_valid(step) || w <= 0 || h <= 0 {
        return Err(AtlasError::Argument);
    }
    let Some(t) = terrain.filter(|_| dimension != Dimension::Nether) else {
        return Err(AtlasError::Unsupported);
    };
    let count = w as i64 * h as i64;
    if count > MAX_AREA_SAMPLES {
        return Err(AtlasError::RangeTooLarge);
    }
    let last_x = ox as i64 + w as i64 * step as i64 - 1;
    let last_z = oz as i64 + h as i64 * step as i64 - 1;
    if last_x > i32::MAX as i64 || last_z > i32::MAX as i64 {
        return Err(AtlasError::RangeTooLarge);
    }
    if dimension == Dimension::End {
        // Exact End density terrain; only 1..8 block samples on step-aligned
        // origins. 0 means void.
        if step > 8 {
            return Err(AtlasError::Unsupported);
        }
        if ox % step != 0 || oz % step != 0 {
            return Err(AtlasError::Argument);
        }
        return Ok(end_surface_area(t, ox / step, oz / step, w, h, step));
    }

    let mut out = vec![0f32; count as usize];
    if step >= 4 {
        for z in 0..h {
            let bz = oz as i64 + z as i64 * step as i64 + (step / 2) as i64;
            for x in 0..w {
                let bx = ox as i64 + x as i64 * step as i64 + (step / 2) as i64;
                out[(z as i64 * w as i64 + x as i64) as usize] =
                    height_at_quart(bn, floor_div64(bx, 4) as i32, floor_div64(bz, 4) as i32);
            }
        }
        return Ok(out);
    }
    // Cell q covers blocks 4q..4q+3 with its centre at 4q+2. Positions are
    // kept in eighths of a cell: p = 2*block + step - 4 is the cell-centre
    // relative position of the sample centre.
    let px0 = 2 * ox as i64 + step as i64 - 4;
    let pz0 = 2 * oz as i64 + step as i64 - 4;
    let qx0 = floor_div64(px0, 8);
    let qz0 = floor_div64(pz0, 8);
    let qx1 = floor_div64(px0 + 2 * (w as i64 - 1) * step as i64, 8) + 1;
    let qz1 = floor_div64(pz0 + 2 * (h as i64 - 1) * step as i64, 8) + 1;
    let lw = qx1 - qx0 + 1;
    let lh = qz1 - qz0 + 1;
    let mut lattice = vec![0f32; (lw * lh) as usize];
    for j in 0..lh {
        for i in 0..lw {
            lattice[(j * lw + i) as usize] =
                height_at_quart(bn, (qx0 + i) as i32, (qz0 + j) as i32);
        }
    }
    for z in 0..h as i64 {
        let pz = pz0 + 2 * z * step as i64;
        let cz = floor_div64(pz, 8) - qz0;
        let tz = (pz - (cz + qz0) * 8) as f32 / 8.0f32;
        for x in 0..w as i64 {
            let px = px0 + 2 * x * step as i64;
            let cx = floor_div64(px, 8) - qx0;
            let tx = (px - (cx + qx0) * 8) as f32 / 8.0f32;
            let at = |dz: i64, dx: i64| lattice[((cz + dz) * lw + cx + dx) as usize];
            let (a, b, c, d) = (at(0, 0), at(0, 1), at(1, 0), at(1, 1));
            let top = a + (b - a) * tx;
            let bottom = c + (d - c) * tx;
            out[(z * w as i64 + x) as usize] = top + (bottom - top) * tz;
        }
    }
    Ok(out)
}

// ------------------------------------------------------------- Nether --

/// `nether_density`: vanilla `noise_settings/nether` final density at a
/// noise-cell corner, from `old_blended_noise` divided by 128. Solid when
/// positive.
fn nether_density(base: f64, y: i32) -> f64 {
    let mut ymul = (y + 8) as f64 / 32.0;
    let mut yhigh = (128 - y) as f64 / 24.0;
    ymul = ymul.clamp(0.0, 1.0);
    yhigh = yhigh.clamp(0.0, 1.0);
    0.64 * (2.5 + ymul * (-2.5 + 0.9375 + yhigh * (base - 0.9375)))
}

/// `nether_lattice`: noise-cell corner densities, sampled on first use.
struct NetherLattice {
    values: Vec<f64>,
    cx0: i32,
    cz0: i32,
    width: i64,
    stride: i64,
}

impl NetherLattice {
    /// `nether_corner`.
    fn corner(&mut self, noise: &TerrainNoise, i: i64, j: i64, k: i32) -> f64 {
        let idx = ((j * self.width + i) * self.stride + k as i64) as usize;
        if self.values[idx].is_nan() {
            let x = (self.cx0 as i64 + i) as i32;
            let z = (self.cz0 as i64 + j) as i32;
            self.values[idx] = nether_density(noise.nether_noise(x, k, z) / 128.0, k * 8);
        }
        self.values[idx]
    }
}

/// `nether_column`: one sample's density column, its own noise cell or a
/// bilinear blend of four lattice corners, filled in as the scan reaches it.
struct NetherColumn {
    i: i64,
    j: i64,
    cx: i32,
    cz: i32,
    tx: f64,
    tz: f64,
    direct: bool,
    values: [f64; 17],
}

/// `nether_column_at`.
fn column_at(
    column: &mut NetherColumn,
    lattice: &mut NetherLattice,
    noise: &TerrainNoise,
    k: i32,
) -> f64 {
    if column.values[k as usize].is_nan() {
        let v = if column.direct {
            nether_density(noise.nether_noise(column.cx, k, column.cz) / 128.0, k * 8)
        } else {
            let (i, j) = (column.i, column.j);
            let a = lattice.corner(noise, i, j, k);
            let b = lattice.corner(noise, i + 1, j, k);
            let c = lattice.corner(noise, i, j + 1, k);
            let d = lattice.corner(noise, i + 1, j + 1, k);
            let top = a + (b - a) * column.tx;
            let bottom = c + (d - c) * column.tx;
            top + (bottom - top) * column.tz
        };
        column.values[k as usize] = v;
    }
    column.values[k as usize]
}

/// `nether_solid`.
fn solid(
    column: &mut NetherColumn,
    lattice: &mut NetherLattice,
    noise: &TerrainNoise,
    y: i32,
) -> bool {
    let j = y >> 3;
    let low = column_at(column, lattice, noise, j);
    low + (column_at(column, lattice, noise, j + 1) - low) * ((y & 7) as f64 / 8.0) > 0.0
}

/// `nether_occupied`.
fn occupied(
    column: &mut NetherColumn,
    lattice: &mut NetherLattice,
    noise: &TerrainNoise,
    y: i32,
    carved: Option<&[u64]>,
) -> bool {
    solid(column, lattice, noise, y)
        && carved.is_none_or(|c| c[(y >> 6) as usize] & (1u64 << (y & 63)) == 0)
}

/// `nether_scan`: leave the roof, then select the first solid floor,
/// including carver-created pockets.
fn scan(
    column: &mut NetherColumn,
    lattice: &mut NetherLattice,
    noise: &TerrainNoise,
    ystart: i32,
    carved: Option<&[u64]>,
) -> f32 {
    let mut y = ystart;
    column.values = [f64::NAN; 17];
    while y > 0 && occupied(column, lattice, noise, y, carved) {
        y -= 1;
    }
    while y > 0 && !occupied(column, lattice, noise, y, carved) {
        y -= 1;
    }
    y as f32
}

/// `nether_floor_area` (`sax_nether_floor_area_sampled`, and with `carved`
/// two mask words a sample `sax_nether_floor_area_carved_sampled`).
#[allow(clippy::too_many_arguments)]
pub fn nether_floor_area(
    terrain: Option<&Terrain>,
    ox: i32,
    oz: i32,
    y: i32,
    step: i32,
    w: i32,
    h: i32,
    carved: Option<&[u64]>,
) -> Result<Vec<f32>, AtlasError> {
    if !sample_step_valid(step) || w <= 0 || h <= 0 {
        return Err(AtlasError::Argument);
    }
    let Some(t) = terrain.filter(|_| step <= 16) else {
        return Err(AtlasError::Unsupported);
    };
    let count = w as i64 * h as i64;
    if count > MAX_AREA_SAMPLES {
        return Err(AtlasError::RangeTooLarge);
    }
    if carved.is_some_and(|c| (c.len() as i64) < count * 2) {
        return Err(AtlasError::Argument);
    }
    let last_x = ox as i64 + w as i64 * step as i64;
    let last_z = oz as i64 + h as i64 * step as i64;
    if last_x > i32::MAX as i64 - 8
        || last_z > i32::MAX as i64 - 8
        || ox < i32::MIN + 8
        || oz < i32::MIN + 8
    {
        return Err(AtlasError::RangeTooLarge);
    }
    let ystart = y.clamp(1, 127);
    let cells = (ystart >> 3) + 1;
    let off = (step - 1) as f64 / 2.0; // block offset of the sample centre
    let noise = &t.nether_noise;
    let mask = |index: usize| carved.map(|c| &c[index * 2..index * 2 + 2]);
    let mut out = vec![0f32; count as usize];
    let mut column = NetherColumn {
        i: 0,
        j: 0,
        cx: 0,
        cz: 0,
        tx: 0.0,
        tz: 0.0,
        direct: false,
        values: [0.0; 17],
    };

    if step >= 8 {
        // One column per sample at the nearest noise cell, no interpolation.
        column.direct = true;
        let mut lattice = NetherLattice {
            values: Vec::new(),
            cx0: 0,
            cz0: 0,
            width: 0,
            stride: 0,
        };
        for z in 0..h as i64 {
            let bz = oz as i64 + z * step as i64 + (step / 2) as i64;
            for x in 0..w as i64 {
                let bx = ox as i64 + x * step as i64 + (step / 2) as i64;
                let index = (z * w as i64 + x) as usize;
                column.cx = floor_div64(bx + 2, 4) as i32;
                column.cz = floor_div64(bz + 2, 4) as i32;
                out[index] = scan(&mut column, &mut lattice, noise, ystart, mask(index));
            }
        }
        return Ok(out);
    }

    let cx0 = floor_div64(ox as i64, 4) as i32;
    let cz0 = floor_div64(oz as i64, 4) as i32;
    let cx1 = floor_div64(
        ox as i64 + (w as i64 - 1) * step as i64 + off.ceil() as i64,
        4,
    ) as i32
        + 1;
    let cz1 = floor_div64(
        oz as i64 + (h as i64 - 1) * step as i64 + off.ceil() as i64,
        4,
    ) as i32
        + 1;
    let width = cx1 as i64 - cx0 as i64 + 1;
    let lh = cz1 as i64 - cz0 as i64 + 1;
    let stride = (cells + 1) as i64;
    let mut lattice = NetherLattice {
        values: vec![f64::NAN; (width * lh * stride) as usize],
        cx0,
        cz0,
        width,
        stride,
    };
    for z in 0..h as i64 {
        let bz = oz as f64 + z as f64 * step as f64 + off;
        column.j = (bz / 4.0).floor() as i64 - cz0 as i64;
        column.tz = bz / 4.0 - (bz / 4.0).floor();
        for x in 0..w as i64 {
            let bx = ox as f64 + x as f64 * step as f64 + off;
            let index = (z * w as i64 + x) as usize;
            column.i = (bx / 4.0).floor() as i64 - cx0 as i64;
            column.tx = bx / 4.0 - (bx / 4.0).floor();
            out[index] = scan(&mut column, &mut lattice, noise, ystart, mask(index));
        }
    }
    Ok(out)
}
