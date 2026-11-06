//! The terrain noise of the overworld and the End (`SurfaceNoise`), the
//! biome depth and scale table and the End's column and height helpers,
//! ported from the engine's `biomenoise.c`.

use crate::biome_noise::EndNoise;
use crate::mc::MC_1_13;
use crate::noise::Octave;
use crate::rng::{JavaRandom, clamped_lerp, floordiv, lerp, lerp3};

/// The engine's dimension numbers (`DIM_*`), as `initSurfaceNoise` takes them.
pub const DIM_NETHER: i32 = -1;
pub const DIM_OVERWORLD: i32 = 0;
pub const DIM_END: i32 = 1;

/// `SurfaceNoise`: the interpolated terrain noise of a dimension.
#[derive(Clone, Debug)]
pub struct SurfaceNoise {
    pub xz_scale: f64,
    pub y_scale: f64,
    pub xz_factor: f64,
    pub y_factor: f64,
    octmin: Octave,
    octmax: Octave,
    octmain: Octave,
    /// Only initialised for the overworld.
    octsurf: Option<Octave>,
    /// Only initialised for the overworld.
    octdepth: Option<Octave>,
}

impl SurfaceNoise {
    /// `initSurfaceNoise`: `dim` is `DIM_END` for the End, anything else
    /// is initialised as the overworld.
    pub fn new(dim: i32, seed: u64) -> Self {
        let mut s = JavaRandom::new(seed);
        let octmin = Octave::new_java(&mut s, -15, 16);
        let octmax = Octave::new_java(&mut s, -15, 16);
        let octmain = Octave::new_java(&mut s, -7, 8);
        if dim == DIM_END {
            Self {
                xz_scale: 2.0,
                y_scale: 1.0,
                xz_factor: 80.0,
                y_factor: 160.0,
                octmin,
                octmax,
                octmain,
                octsurf: None,
                octdepth: None,
            }
        } else {
            let octsurf = Octave::new_java(&mut s, -3, 4);
            s.skip(262 * 10);
            let octdepth = Octave::new_java(&mut s, -15, 16);
            Self {
                xz_scale: 0.999_999_981_450_774_5,
                y_scale: 0.999_999_981_450_774_5,
                xz_factor: 80.0,
                y_factor: 160.0,
                octmin,
                octmax,
                octmain,
                octsurf: Some(octsurf),
                octdepth: Some(octdepth),
            }
        }
    }

    /// The surface octaves of the overworld (`octsurf`), if any.
    pub fn octsurf(&self) -> Option<&Octave> {
        self.octsurf.as_ref()
    }

    /// The depth octaves of the overworld (`octdepth`), if any.
    pub fn octdepth(&self) -> Option<&Octave> {
        self.octdepth.as_ref()
    }

    /// `sampleSurfaceNoiseBetween`: the noise at a point, cut short once it
    /// is known to be outside `noise_min..=noise_max`.
    pub fn sample_between(&self, x: i32, y: i32, z: i32, noise_min: f64, noise_max: f64) -> f64 {
        let xz_scale = 684.412 * self.xz_scale;
        let y_scale = 684.412 * self.y_scale;
        let mut vmin = 0.0f64;
        let mut vmax = 0.0f64;

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
        let mut vmain = 0.5f64;

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

    /// `sampleSurfaceNoise`: the full terrain noise at a point.
    pub fn sample(&self, x: i32, y: i32, z: i32) -> f64 {
        let xz_scale = 684.412 * self.xz_scale;
        let y_scale = 684.412 * self.y_scale;
        let xz_step = xz_scale / self.xz_factor;
        let y_step = y_scale / self.y_factor;

        let mut min_noise = 0.0f64;
        let mut max_noise = 0.0f64;
        let mut main_noise = 0.0f64;
        let mut persist = 1.0f64;
        let mut contrib = 1.0f64;

        // `maintainPrecision` is the identity in the engine.
        for i in 0..16 {
            let dx = x as f64 * xz_scale * persist;
            let dy = y as f64 * y_scale * persist;
            let dz = z as f64 * xz_scale * persist;
            let sy = y_scale * persist;
            let ty = y as f64 * sy;

            min_noise += self.octmin.octaves[i].sample(dx, dy, dz, sy, ty) * contrib;
            max_noise += self.octmax.octaves[i].sample(dx, dy, dz, sy, ty) * contrib;

            if i < 8 {
                let dx = x as f64 * xz_step * persist;
                let dy = y as f64 * y_step * persist;
                let dz = z as f64 * xz_step * persist;
                let sy = y_step * persist;
                let ty = y as f64 * sy;
                main_noise += self.octmain.octaves[i].sample(dx, dy, dz, sy, ty) * contrib;
            }
            persist *= 0.5;
            contrib *= 2.0;
        }

        clamped_lerp(
            0.5 + 0.05 * main_noise,
            min_noise / 512.0,
            max_noise / 512.0,
        )
    }
}

/// `sampleNoiseColumnEnd`: the End's terrain density for the cells
/// `colymin..=colymax` (at most 0..=32) of the column at cell `x`, `z`,
/// written to `column[0..=colymax - colymin]`. Above 1.13 the outer rings
/// are empty (`NaN`).
pub fn sample_noise_column_end(
    column: &mut [f64],
    sn: &SurfaceNoise,
    en: &EndNoise,
    x: i32,
    z: i32,
    colymin: i32,
    colymax: i32,
) {
    // Clamped (32 + 46 - y) / 64.0.
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
    // Clamped (y - 1) / 7.0.
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

    if en.mc > MC_1_13 {
        // The outer end rings.
        let rsq = (x as i64 as u64)
            .wrapping_mul(x as i64 as u64)
            .wrapping_add((z as i64 as u64).wrapping_mul(z as i64 as u64));
        if (rsq as u32 as i32) < 0 {
            for y in colymin..=colymax {
                column[(y - colymin) as usize] = f64::NAN;
            }
            return;
        }
    }

    let depth = (en.height_noise(x, z, 0) - 8.0f32) as f64;
    for y in colymin..=colymax {
        let o = (y - colymin) as usize;
        if LOWER_DROP[y as usize] == 0.0 {
            column[o] = -30.0;
            continue;
        }
        let noise = sn.sample_between(x, y, z, -128.0, 128.0);
        let mut clamped = noise + depth;
        clamped = lerp(UPPER_DROP[y as usize], -3000.0, clamped);
        clamped = lerp(LOWER_DROP[y as usize], -30.0, clamped);
        column[o] = clamped;
    }
}

/// `getSurfaceHeight`: given the four bordering noise columns (each of
/// `colymax - colymin + 1` cells) and the fractional position `dx`, `dz`
/// between them, the surface block height (where the interpolated noise
/// is above 0).
#[allow(clippy::too_many_arguments)]
pub fn get_surface_height(
    ncol00: &[f64],
    ncol01: &[f64],
    ncol10: &[f64],
    ncol11: &[f64],
    colymin: i32,
    colymax: i32,
    blockspercell: i32,
    dx: f64,
    dz: f64,
) -> i32 {
    for celly in (colymin..colymax).rev() {
        let idx = (celly - colymin) as usize;
        let v000 = ncol00[idx];
        let v001 = ncol01[idx];
        let v100 = ncol10[idx];
        let v101 = ncol11[idx];
        let v010 = ncol00[idx + 1];
        let v011 = ncol01[idx + 1];
        let v110 = ncol10[idx + 1];
        let v111 = ncol11[idx + 1];

        for y in (0..blockspercell).rev() {
            let dy = y as f64 / blockspercell as f64;
            // Note: the engine passes dy, dx, dz here, not x, y, z.
            let noise = lerp3(dy, dx, dz, v000, v010, v100, v110, v001, v011, v101, v111);
            if noise > 0.0 {
                return celly * blockspercell + y;
            }
        }
    }
    0
}

/// `getEndSurfaceHeight`: the End's terrain height at a block column.
pub fn get_end_surface_height(mc: i32, seed: u64, x: i32, z: i32) -> i32 {
    let en = EndNoise::new(seed, mc);
    let sn = SurfaceNoise::new(DIM_END, seed);

    // End noise columns vary on a grid of cell size eight.
    let cellx = x >> 3;
    let cellz = z >> 3;
    let dx = (x & 7) as f64 / 8.0;
    let dz = (z & 7) as f64 / 8.0;

    const Y0: i32 = 0;
    const Y1: i32 = 32;
    const YN: usize = (Y1 - Y0 + 1) as usize;
    let mut ncol00 = [0.0f64; YN];
    let mut ncol01 = [0.0f64; YN];
    let mut ncol10 = [0.0f64; YN];
    let mut ncol11 = [0.0f64; YN];
    sample_noise_column_end(&mut ncol00, &sn, &en, cellx, cellz, Y0, Y1);
    sample_noise_column_end(&mut ncol01, &sn, &en, cellx, cellz + 1, Y0, Y1);
    sample_noise_column_end(&mut ncol10, &sn, &en, cellx + 1, cellz, Y0, Y1);
    sample_noise_column_end(&mut ncol11, &sn, &en, cellx + 1, cellz + 1, Y0, Y1);

    get_surface_height(&ncol00, &ncol01, &ncol10, &ncol11, Y0, Y1, 4, dx, dz)
}

/// `mapEndSurfaceHeight`: the End's terrain heights over a `w` by `h` area
/// at `scale` (1, 2, 4 or 8) blocks per cell, indexed `j * w + i`, or
/// `None` for another scale. `ymin` is the lowest block height of interest.
#[allow(clippy::too_many_arguments)]
pub fn map_end_surface_height(
    en: &EndNoise,
    sn: &SurfaceNoise,
    x: i32,
    z: i32,
    w: i32,
    h: i32,
    scale: i32,
    ymin: i32,
) -> Option<Vec<f32>> {
    if scale != 1 && scale != 2 && scale != 4 && scale != 8 {
        return None;
    }

    let y0 = (ymin >> 2).clamp(2, 17);
    let y1 = 18;
    let yn = (y1 - y0 + 1) as usize;
    let cellmid = if scale > 1 { scale as f64 / 16.0 } else { 0.0 };
    let cellsiz = 8 / scale;
    let cx = floordiv(x, cellsiz);
    let cz = floordiv(z, cellsiz);
    let cw = (floordiv(x + w - 1, cellsiz) - cx + 2) as usize;

    let mut ncol0 = vec![0.0f64; yn * cw];
    let mut ncol1 = vec![0.0f64; yn * cw];
    for i in 0..cw {
        sample_noise_column_end(
            &mut ncol1[i * yn..(i + 1) * yn],
            sn,
            en,
            cx + i as i32,
            cz,
            y0,
            y1,
        );
    }

    let mut out = vec![0.0f32; w.max(0) as usize * h.max(0) as usize];
    for j in 0..h {
        let cj = floordiv(z + j, cellsiz);
        let dj = z + j - cj * cellsiz;
        if j == 0 || dj == 0 {
            std::mem::swap(&mut ncol0, &mut ncol1);
            for i in 0..cw {
                sample_noise_column_end(
                    &mut ncol1[i * yn..(i + 1) * yn],
                    sn,
                    en,
                    cx + i as i32,
                    cj + 1,
                    y0,
                    y1,
                );
            }
        }

        for i in 0..w {
            let ci = floordiv(x + i, cellsiz);
            let di = x + i - ci * cellsiz;
            let dx = di as f64 / cellsiz as f64 + cellmid;
            let dz = dj as f64 / cellsiz as f64 + cellmid;
            let o = (ci - cx) as usize * yn;
            out[(j * w + i) as usize] = get_surface_height(
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
    Some(out)
}

/// The depth, scale and grass height of a pre-1.18 biome
/// (`getBiomeDepthAndScale`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BiomeDepthScale {
    pub depth: f64,
    pub scale: f64,
    pub grass: i32,
}

/// `getBiomeDepthAndScale`: `None` for biomes the table does not know
/// (`return 0` in the engine).
pub fn get_biome_depth_and_scale(id: i32) -> Option<BiomeDepthScale> {
    const DH: i32 = 62; // default height
    // (id, scale, depth, grass), in the engine's order.
    let (s, d, g): (f64, f64, i32) = match id {
        0 => (0.100, -1.000, DH),   // ocean
        1 => (0.050, 0.125, DH),    // plains
        2 => (0.050, 0.125, 0),     // desert
        3 => (0.500, 1.000, DH),    // mountains
        4 => (0.200, 0.100, DH),    // forest
        5 => (0.200, 0.200, DH),    // taiga
        6 => (0.100, -0.200, DH),   // swamp
        7 => (0.000, -0.500, 60),   // river
        10 => (0.100, -1.000, DH),  // frozen_ocean
        11 => (0.000, -0.500, 60),  // frozen_river
        12 => (0.050, 0.125, DH),   // snowy_tundra
        13 => (0.300, 0.450, DH),   // snowy_mountains
        14 => (0.300, 0.200, 0),    // mushroom_fields
        15 => (0.025, 0.000, 0),    // mushroom_field_shore
        16 => (0.025, 0.000, 64),   // beach
        17 => (0.300, 0.450, 0),    // desert_hills
        18 => (0.300, 0.450, DH),   // wooded_hills
        19 => (0.300, 0.450, DH),   // taiga_hills
        20 => (0.300, 0.800, DH),   // mountain_edge
        21 => (0.200, 0.100, DH),   // jungle
        22 => (0.300, 0.450, DH),   // jungle_hills
        23 => (0.200, 0.100, DH),   // jungle_edge
        24 => (0.100, -1.800, DH),  // deep_ocean
        25 => (0.800, 0.100, 64),   // stone_shore
        26 => (0.025, 0.000, 64),   // snowy_beach
        27 => (0.200, 0.100, DH),   // birch_forest
        28 => (0.300, 0.450, DH),   // birch_forest_hills
        29 => (0.200, 0.100, DH),   // dark_forest
        30 => (0.200, 0.200, DH),   // snowy_taiga
        31 => (0.300, 0.450, DH),   // snowy_taiga_hills
        32 => (0.200, 0.200, DH),   // giant_tree_taiga
        33 => (0.300, 0.450, DH),   // giant_tree_taiga_hills
        34 => (0.500, 1.000, DH),   // wooded_mountains
        35 => (0.050, 0.125, DH),   // savanna
        36 => (0.025, 1.500, DH),   // savanna_plateau
        37 => (0.200, 0.100, 0),    // badlands
        38 => (0.025, 1.500, 0),    // wooded_badlands_plateau
        39 => (0.025, 1.500, 0),    // badlands_plateau
        44 => (0.100, -1.000, 0),   // warm_ocean
        45 => (0.100, -1.000, DH),  // lukewarm_ocean
        46 => (0.100, -1.000, DH),  // cold_ocean
        47 => (0.100, -1.800, 0),   // deep_warm_ocean
        48 => (0.100, -1.800, DH),  // deep_lukewarm_ocean
        49 => (0.100, -1.800, DH),  // deep_cold_ocean
        50 => (0.100, -1.800, DH),  // deep_frozen_ocean
        129 => (0.050, 0.125, DH),  // sunflower_plains
        130 => (0.250, 0.225, 0),   // desert_lakes
        131 => (0.500, 1.000, DH),  // gravelly_mountains
        132 => (0.400, 0.100, DH),  // flower_forest
        133 => (0.400, 0.300, DH),  // taiga_mountains
        134 => (0.300, -0.100, DH), // swamp_hills
        140 => (0.450, 0.425, 0),   // ice_spikes
        149 => (0.400, 0.200, DH),  // modified_jungle
        151 => (0.400, 0.200, DH),  // modified_jungle_edge
        155 => (0.400, 0.200, DH),  // tall_birch_forest
        156 => (0.500, 0.550, DH),  // tall_birch_hills
        157 => (0.400, 0.200, DH),  // dark_forest_hills
        158 => (0.400, 0.300, DH),  // snowy_taiga_mountains
        160 => (0.200, 0.200, DH),  // giant_spruce_taiga
        161 => (0.200, 0.200, DH),  // giant_spruce_taiga_hills
        162 => (0.500, 1.000, DH),  // modified_gravelly_mountains
        163 => (1.225, 0.3625, DH), // shattered_savanna
        164 => (1.212, 1.050, DH),  // shattered_savanna_plateau
        165 => (0.200, 0.100, 0),   // eroded_badlands
        166 => (0.300, 0.450, 0),   // modified_wooded_badlands_plateau
        167 => (0.300, 0.450, 0),   // modified_badlands_plateau
        168 => (0.200, 0.100, DH),  // bamboo_jungle
        169 => (0.300, 0.450, DH),  // bamboo_jungle_hills
        _ => return None,
    };
    Some(BiomeDepthScale {
        depth: d,
        scale: s,
        grass: g,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mc::{MC_1_12, MC_1_18, MC_26_3};

    const SEED: u64 = 0x1234_5678_9abc_def0;

    #[test]
    fn overworld_and_end_init_differ() {
        let ow = SurfaceNoise::new(DIM_OVERWORLD, SEED);
        let end = SurfaceNoise::new(DIM_END, SEED);
        assert!(ow.octsurf().is_some() && ow.octdepth().is_some());
        assert!(end.octsurf().is_none() && end.octdepth().is_none());
        assert_eq!(end.xz_scale, 2.0);
        assert_eq!(end.y_scale, 1.0);
        // Nether is initialised as the overworld, as in the engine.
        assert!(SurfaceNoise::new(DIM_NETHER, SEED).octsurf().is_some());
    }

    #[test]
    fn between_agrees_with_full_noise_inside_bounds() {
        let sn = SurfaceNoise::new(DIM_OVERWORLD, SEED);
        for &(x, y, z) in &[(0, 64, 0), (100, 30, -250), (-1234, 100, 777), (5, 5, 5)] {
            let full = sn.sample(x, y, z);
            let cut = sn.sample_between(x, y, z, -1e9, 1e9);
            assert!(
                (full - cut).abs() < 1e-6 * full.abs().max(1.0),
                "{x} {y} {z}"
            );
        }
    }

    #[test]
    fn between_is_clamped() {
        let sn = SurfaceNoise::new(DIM_END, SEED);
        for x in (-64..64).step_by(13) {
            let v = sn.sample_between(x, 20, x * 3, -128.0, 128.0);
            assert!((-128.0..=128.0).contains(&v) || v.abs() < 1e6);
        }
    }

    #[test]
    fn surface_height_finds_the_top_of_solid_cells() {
        // A column that is solid up to cell 1 and empty above.
        let col = [10.0, 10.0, -10.0];
        let h = get_surface_height(&col, &col, &col, &col, 0, 2, 4, 0.0, 0.0);
        assert!((4..8).contains(&h));
        let air = [-1.0, -1.0, -1.0];
        assert_eq!(
            get_surface_height(&air, &air, &air, &air, 0, 2, 4, 0.5, 0.5),
            0
        );
    }

    #[test]
    fn end_rings_are_nan_above_1_13() {
        let sn = SurfaceNoise::new(DIM_END, SEED);
        let new = EndNoise::new(SEED, MC_26_3);
        let old = EndNoise::new(SEED, MC_1_12);
        // 2^16 cells away: x * x + z * z has bit 31 set.
        let (x, z) = (46341, 0);
        let mut col = [0.0; 4];
        sample_noise_column_end(&mut col, &sn, &new, x, z, 0, 3);
        assert!(col.iter().all(|v| v.is_nan()));
        sample_noise_column_end(&mut col, &sn, &old, 1000, 1000, 0, 3);
        assert_eq!(col[0], -30.0);
        assert_eq!(col[1], -30.0);
        assert!(col[2].is_finite() && col[3].is_finite());
    }

    #[test]
    fn end_surface_height_matches_the_map() {
        let en = EndNoise::new(SEED, MC_1_18);
        let sn = SurfaceNoise::new(DIM_END, SEED);
        let map = map_end_surface_height(&en, &sn, -20, 90, 24, 17, 1, 0).unwrap();
        for j in 0..17 {
            for i in 0..24 {
                let h = get_end_surface_height(MC_1_18, SEED, -20 + i, 90 + j);
                // The map starts at cell row 2, so only compare solid columns.
                let m = map[(j * 24 + i) as usize] as i32;
                if h >= 8 {
                    assert_eq!(h, m, "{i} {j}");
                }
            }
        }
        assert!(map_end_surface_height(&en, &sn, 0, 0, 4, 4, 3, 0).is_none());
    }

    #[test]
    fn depth_and_scale_table() {
        let plains = get_biome_depth_and_scale(1).unwrap();
        assert_eq!(
            (plains.depth, plains.scale, plains.grass),
            (0.125, 0.050, 62)
        );
        let river = get_biome_depth_and_scale(7).unwrap();
        assert_eq!((river.depth, river.scale, river.grass), (-0.5, 0.0, 60));
        assert_eq!(get_biome_depth_and_scale(163).unwrap().scale, 1.225);
        assert!(get_biome_depth_and_scale(9).is_none());
        assert!(get_biome_depth_and_scale(177).is_none());
    }
}
