//! The terrain look of seed-only tiles: surface colours, hill shading and
//! water depth tones, ported from the Fabric version's `BiomeTerrainStyler`,
//! `BiomeSurfacePalette` and `BiomeHighlighter`, plus the decision logic of
//! its tile worker ([`style_tile`]).
//!
//! Everything here is pure: no game, no threads. Where the Java reads
//! Minecraft data (biome registry, grass/foliage/water tints) it goes through
//! [`BiomeLooks`], which the caller implements; [`VanillaLooks`] is a default
//! built from vanilla's well-known tint colours. The Java's `int`/`float`
//! arithmetic, `Math.round`, shifts and wrapping are kept exactly; colours are
//! ARGB `u32` (0xAARRGGBB), the Java's ABGR upload conversion is not done.

use std::collections::BTreeSet;

use meridian_seedatlas::{Dimension, biomes};

/// Number of engine biome ids the styling covers; higher ids are left alone.
pub const LIMIT: usize = 256;

const WATER: u8 = 1;
const DEEP: u8 = 2;
const LAND: u8 = 3;

const END_STONE: u32 = 0xD9D99A;
const END_VOID: u32 = 0x1A1A2E;
const NETHER_LAVA: u32 = 0xE0600A;
const MAIN_ISLAND_RADIUS_SQUARED: i64 = 100 * 100;

const fn fill_i32(table: &mut [i32; LIMIT], ids: &[usize], value: i32) {
    let mut i = 0;
    while i < ids.len() {
        table[ids[i]] = value;
        i += 1;
    }
}

const fn fill_u8(table: &mut [u8; LIMIT], ids: &[usize], value: u8) {
    let mut i = 0;
    while i < ids.len() {
        table[ids[i]] = value;
        i += 1;
    }
}

const fn build_relief() -> [i32; LIMIT] {
    let mut r = [14; LIMIT];
    fill_i32(
        &mut r,
        &[2, 17, 130, 35, 36, 163, 164, 38, 39, 37, 165, 166, 167],
        20,
    );
    fill_i32(
        &mut r,
        &[
            3, 20, 34, 131, 162, 177, 178, 179, 180, 181, 182, 12, 13, 140,
        ],
        38,
    );
    fill_i32(&mut r, &[180, 181, 182, 3, 34, 131, 162], 52);
    fill_i32(
        &mut r,
        &[
            4, 5, 18, 19, 21, 22, 23, 27, 28, 29, 30, 31, 32, 33, 132, 133, 155, 156, 157, 158,
            160, 161, 168, 169, 185, 184, 186, 188, 6, 134,
        ],
        22,
    );
    // Beaches and shores stay flat.
    fill_i32(&mut r, &[16, 25, 26], 6);
    r
}

const fn build_kind() -> [u8; LIMIT] {
    let mut k = [LAND; LIMIT];
    fill_u8(&mut k, &[0, 10, 44, 45, 46, 47], WATER);
    fill_u8(&mut k, &[24, 48, 49, 50], DEEP);
    fill_u8(&mut k, &[7, 11], WATER);
    k
}

/// Relief strength per biome id (0 = flat).
static RELIEF: [i32; LIMIT] = build_relief();
static KIND: [u8; LIMIT] = build_kind();

/// Whether a biome id is an ocean, river or other water biome.
pub fn water_biome(id: i32) -> bool {
    id >= 0 && (id as usize) < LIMIT && KIND[id as usize] != LAND
}

// ---------------------------------------------------------- Java helpers --

/// Java's `Math.round(float)`: ties go up, NaN is 0, out-of-range saturates.
pub fn round(f: f32) -> i32 {
    (f as f64 + 0.5).floor() as i32
}

/// Java's `Math.max(float, float)`: a NaN wins.
fn fmax(a: f32, b: f32) -> f32 {
    if a.is_nan() || b.is_nan() {
        f32::NAN
    } else {
        a.max(b)
    }
}

/// Java's `Math.min(float, float)`: a NaN wins.
fn fmin(a: f32, b: f32) -> f32 {
    if a.is_nan() || b.is_nan() {
        f32::NAN
    } else {
        a.min(b)
    }
}

/// `hash(x, z, seed)`: a non-negative 63-bit mix.
pub fn hash(x: i32, z: i32, seed: i64) -> i64 {
    let mut h = seed
        ^ (x as i64).wrapping_mul(0x9E37_79B9_7F4A_7C15_u64 as i64)
        ^ (z as i64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F_u64 as i64);
    h ^= ((h as u64) >> 32) as i64;
    h = h.wrapping_mul(0xD6E8_FEB8_6659_FD93_u64 as i64);
    h ^= ((h as u64) >> 32) as i64;
    h & i64::MAX
}

/// Multi-octave noise in about [-1, 1].
fn height(x: i32, z: i32, seed: i64) -> f32 {
    0.55 * noise(x as f32 / 40.0, z as f32 / 40.0, seed)
        + 0.30 * noise(x as f32 / 15.0, z as f32 / 15.0, seed.wrapping_add(1))
        + 0.15 * noise(x as f32 / 6.0, z as f32 / 6.0, seed.wrapping_add(2))
}

fn noise(x: f32, z: f32, seed: i64) -> f32 {
    let x0 = x.floor() as i32;
    let z0 = z.floor() as i32;
    let mut fx = x - x0 as f32;
    let mut fz = z - z0 as f32;
    fx = fx * fx * (3.0 - 2.0 * fx);
    fz = fz * fz * (3.0 - 2.0 * fz);
    let a = lattice(x0, z0, seed);
    let b = lattice(x0.wrapping_add(1), z0, seed);
    let c = lattice(x0, z0.wrapping_add(1), seed);
    let e = lattice(x0.wrapping_add(1), z0.wrapping_add(1), seed);
    (a + (b - a) * fx) + ((c + (e - c) * fx) - (a + (b - a) * fx)) * fz
}

fn lattice(x: i32, z: i32, seed: i64) -> f32 {
    (hash(x, z, seed) % 2001) as f32 / 1000.0 - 1.0
}

/// Scales the RGB channels by `100 + percent` per cent; alpha is kept.
pub fn shade(colour: u32, percent: i32) -> u32 {
    let r = channel((colour >> 16) as i32, percent);
    let g = channel((colour >> 8) as i32, percent);
    let b = channel(colour as i32, percent);
    (colour & 0xFF00_0000) | ((r as u32) << 16) | ((g as u32) << 8) | b as u32
}

fn channel(value: i32, percent: i32) -> i32 {
    ((value & 0xFF) * (100 + percent) / 100).clamp(0, 255)
}

fn id_of(raw: i32) -> usize {
    (raw & 0xFFFF) as usize
}

// ---------------------------------------------------------------- styler --

/// Overworld tile with estimated heights (or none) and per-sample surface
/// colours: the fast preview that shares the surface palette.
/// `floors` is the `(size+2)^2` height grid with a one-sample halo; `colors`
/// holds `0x00RRGGBB` per sample or -1 for unknown. Alpha and ids are kept.
pub fn apply_preview(
    argb: &mut [u32],
    ids: &[i32],
    step: i32,
    size: usize,
    floors: Option<&[f32]>,
    colors: &[i32],
) {
    let n = size * size;
    if size == 0 || step <= 0 || argb.len() < n || ids.len() < n || colors.len() < n {
        return;
    }
    let hw = size + 2;
    let floors = floors.filter(|f| f.len() == hw * hw);
    for z in 0..size {
        for x in 0..size {
            let i = z * size + x;
            if id_of(ids[i]) >= LIMIT || colors[i] < 0 {
                continue;
            }
            let mut delta = -8;
            if let Some(floors) = floors {
                if water_biome(id_of(ids[i]) as i32) {
                    let depth = fmax(0.0, 62.0 - floors[(z + 1) * hw + x + 1]);
                    delta -= round(fmin(24.0, depth * 0.65));
                } else {
                    let slope =
                        (floors[(z + 2) * hw + x + 2] - floors[z * hw + x]) / (2.0 * step as f32);
                    if slope.is_finite() {
                        delta += round(fmax(-28.0, fmin(28.0, slope * 65.0)));
                    }
                }
            }
            argb[i] = shade((argb[i] & 0xFF00_0000) | colors[i] as u32, delta);
        }
    }
}

/// Vanilla-backed surface colours and heights, all with the same one-sample
/// halo (`(size+2)^2`).
pub fn apply_surface(
    argb: &mut [u32],
    ids: &[i32],
    step: i32,
    size: usize,
    heights: &[f32],
    floors: &[f32],
    colors: &[i32],
) {
    let hw = size + 2;
    let n = size * size;
    if size == 0
        || step <= 0
        || heights.len() != hw * hw
        || floors.len() != hw * hw
        || colors.len() != hw * hw
        || argb.len() < n
        || ids.len() < n
    {
        return;
    }
    for z in 0..size {
        for x in 0..size {
            let i = z * size + x;
            let h = (z + 1) * hw + x + 1;
            if id_of(ids[i]) >= LIMIT
                || colors[h] < 0
                || !heights[h].is_finite()
                || !floors[h].is_finite()
            {
                continue;
            }
            let depth = heights[h] - floors[h];
            let mut delta = -8;
            if depth > 0.5 {
                delta -= round(fmin(24.0, depth * 0.65));
            } else {
                let slope =
                    (heights[(z + 2) * hw + x + 2] - heights[z * hw + x]) / (2.0 * step as f32);
                if slope.is_finite() {
                    delta += round(fmax(-28.0, fmin(28.0, slope * 65.0)));
                }
            }
            argb[i] = shade(
                (argb[i] & 0xFF00_0000) | (colors[h] as u32 & 0xFF_FFFF),
                delta,
            );
        }
    }
}

/// End terrain: density heights decide both the island outline and its
/// colour. `heights` is the `(size+2)^2` halo grid; 0 means void. A missing
/// or wrong-sized grid leaves the tile flat.
pub fn apply_end(
    argb: &mut [u32],
    ids: &[i32],
    origin_x: i32,
    origin_z: i32,
    step: i32,
    size: usize,
    heights: &[f32],
) {
    let hw = size + 2;
    let n = size * size;
    if size == 0 || step <= 0 || heights.len() != hw * hw || argb.len() < n || ids.len() < n {
        return;
    }
    for z in 0..size {
        for x in 0..size {
            let i = z * size + x;
            let h = heights[(z + 1) * hw + x + 1];
            if id_of(ids[i]) >= LIMIT || !h.is_finite() {
                continue;
            }
            let alpha = argb[i] & 0xFF00_0000;
            if h <= 0.0 {
                argb[i] = alpha | END_VOID;
                continue;
            }
            // Void has no surface height: do not turn the drop to zero into a rim.
            let mut nw = heights[z * hw + x];
            let mut se = heights[(z + 2) * hw + x + 2];
            if !nw.is_finite() || nw <= 0.0 {
                nw = h;
            }
            if !se.is_finite() || se <= 0.0 {
                se = h;
            }
            let slope = (se - nw) / (2.0 * step as f32);
            let mut delta = round(fmax(-40.0, fmin(40.0, slope * 120.0)));
            delta += end_grain(origin_x, origin_z, step, x, z);
            argb[i] = shade(alpha | END_STONE, delta);
        }
    }
}

fn end_grain(origin_x: i32, origin_z: i32, step: i32, x: usize, z: usize) -> i32 {
    (hash(
        (origin_x / step).wrapping_add(x as i32),
        (origin_z / step).wrapping_add(z as i32),
        0x5EED5,
    ) % 7) as i32
        - 3
}

/// End look without density heights: distant views, and the moment before
/// the heights arrive.
pub fn apply_end_preview(
    argb: &mut [u32],
    ids: &[i32],
    origin_x: i32,
    origin_z: i32,
    step: i32,
    size: usize,
) {
    let n = size * size;
    if size == 0 || step <= 0 || argb.len() < n || ids.len() < n {
        return;
    }
    let centre = (step / 2) as i64;
    for z in 0..size {
        for x in 0..size {
            let i = z * size + x;
            let id = id_of(ids[i]) as i32;
            let alpha = argb[i] & 0xFF00_0000;
            let grain = end_grain(origin_x, origin_z, step, x, z);
            let bx = origin_x as i64 + x as i64 * step as i64 + centre;
            let bz = origin_z as i64 + z as i64 * step as i64 + centre;
            argb[i] = match id {
                biomes::END_HIGHLANDS => shade(alpha | END_STONE, grain - 4),
                biomes::END_MIDLANDS => shade(alpha | END_STONE, grain - 16),
                // The main island sits inside the central the_end biome.
                biomes::THE_END => {
                    if bx * bx + bz * bz < MAIN_ISLAND_RADIUS_SQUARED {
                        shade(alpha | END_STONE, grain - 4)
                    } else {
                        alpha | END_VOID
                    }
                }
                biomes::SMALL_END_ISLANDS | biomes::END_BARRENS => alpha | END_VOID,
                _ => argb[i],
            };
        }
    }
}

/// Nether cave-mode surface: `heights` is the predicted floor at the map
/// layer (`(size+2)^2` halo grid). Columns below the lava sea (y 31) become
/// lava, other ground is shaded by slope with the north-west light.
pub fn apply_nether(
    argb: &mut [u32],
    ids: &[i32],
    origin_x: i32,
    origin_z: i32,
    step: i32,
    size: usize,
    heights: &[f32],
) {
    let hw = size + 2;
    let n = size * size;
    if step <= 0 || heights.len() != hw * hw || argb.len() < n || ids.len() < n {
        return;
    }
    for z in 0..size {
        for x in 0..size {
            let i = z * size + x;
            if id_of(ids[i]) >= LIMIT {
                continue;
            }
            let h = heights[(z + 1) * hw + x + 1];
            let grain = (hash(
                (origin_x / step).wrapping_add(x as i32),
                (origin_z / step).wrapping_add(z as i32),
                0x4E7B,
            ) % 5) as i32
                - 2;
            if h < 31.0 {
                argb[i] = shade((argb[i] & 0xFF00_0000) | NETHER_LAVA, grain * 2);
                continue;
            }
            let slope = (heights[(z + 2) * hw + x + 2] - heights[z * hw + x]) / (2.0 * step as f32);
            let delta = round(fmax(-45.0, fmin(45.0, slope * 60.0)));
            argb[i] = shade(argb[i], delta + grain - 10);
        }
    }
}

/// Rewrites row-major ARGB samples in place with cosmetic relief: from the
/// optional `(size+2)^2` height grid (its `[1,1]` entry is sample `[0,0]`),
/// else from noise keyed by `seed`. Alpha and ids are never changed.
#[allow(clippy::too_many_arguments)]
pub fn apply(
    argb: &mut [u32],
    ids: &[i32],
    origin_x: i32,
    origin_z: i32,
    step: i32,
    size: usize,
    seed: i64,
    heights: Option<&[f32]>,
) {
    let count = argb.len().min(ids.len()).min(size * size);
    if count < size * size || step <= 0 {
        return;
    }
    let hw = size + 2;
    let heights = heights.filter(|h| h.len() == hw * hw);
    // The engine's biomes are 4x4-block cells, so edges look like stairs. Warp
    // each sample's lookup by smooth noise to make edges organic.
    let warp = 5.0f32 / step as f32;
    let mut colour = vec![0u32; count];
    let mut id = vec![0usize; count];
    for z in 0..size {
        for x in 0..size {
            let (mut sx, mut sz) = (x, z);
            if warp >= 0.6 {
                let wx = origin_x.wrapping_add((x as i32).wrapping_mul(step));
                let wz = origin_z.wrapping_add((z as i32).wrapping_mul(step));
                let last = size as i32 - 1;
                sx = (x as i32
                    + round(noise(wx as f32 / 9.0, wz as f32 / 9.0, seed.wrapping_add(7)) * warp))
                .clamp(0, last) as usize;
                sz = (z as i32
                    + round(noise(wx as f32 / 9.0, wz as f32 / 9.0, seed.wrapping_add(8)) * warp))
                .clamp(0, last) as usize;
            }
            colour[z * size + x] = argb[sz * size + sx];
            id[z * size + x] = id_of(ids[sz * size + sx]);
        }
    }
    let d = step.max(2);
    for z in 0..size {
        for x in 0..size {
            let i = z * size + x;
            let bid = id[i];
            if bid >= LIMIT || id_of(ids[i]) >= LIMIT {
                continue; // unknown samples keep their original colour
            }
            let wx = origin_x.wrapping_add((x as i32).wrapping_mul(step));
            let wz = origin_z.wrapping_add((z as i32).wrapping_mul(step));
            let kind = KIND[bid];
            let cell = hash(wx / step, wz / step, seed);
            let mut delta;
            if kind == LAND {
                if let Some(heights) = heights {
                    // Light from the north-west: ground rising to the south-east is bright.
                    let slope =
                        (heights[(z + 2) * hw + x + 2] - heights[z * hw + x]) / (2.0 * step as f32);
                    delta = round(fmax(-35.0, fmin(35.0, slope * 90.0)));
                    // Gentle elevation tint around sea level.
                    delta += round(fmax(
                        -12.0,
                        fmin(12.0, (heights[(z + 1) * hw + x + 1] - 63.0) * 0.12),
                    ));
                } else {
                    let amp = RELIEF[bid];
                    let lit = height(wx.wrapping_sub(d), wz.wrapping_sub(d), seed)
                        - height(wx.wrapping_add(d), wz.wrapping_add(d), seed);
                    delta = round(lit * amp as f32 * 3.2)
                        + round(height(wx, wz, seed) * amp as f32 * 0.5);
                }
                delta += (cell % 9) as i32 - 4; // per-block grain
                delta -= 14; // explored land reads darker and more muted
            } else {
                if let Some(heights) = heights {
                    // Water: darker where the estimated floor is further below sea level.
                    let below = fmax(0.0, fmin(70.0, 63.0 - heights[(z + 1) * hw + x + 1]));
                    delta = -16 - round(below * 16.0 / 70.0) - if kind == DEEP { 10 } else { 0 };
                } else {
                    // Water: broad depth bands plus faint ripples.
                    let depth = height(wx.wrapping_mul(2), wz.wrapping_mul(2), seed ^ 0xABCDEF);
                    delta = round(depth * if kind == DEEP { 14.0 } else { 9.0 })
                        - if kind == DEEP { 10 } else { 0 };
                    delta -= 22;
                }
                delta += (cell % 5) as i32 - 2;
            }
            // Biome-edge outline: lighter on the north-west side, darker on the south-east.
            if x > 0 && z > 0 && id[i - size - 1] != bid {
                delta += 14;
            }
            if x + 1 < size && z + 1 < size && id[i + size + 1] != bid {
                delta -= 14;
            }
            argb[i] = shade(colour[i], delta);
        }
    }
}

// ----------------------------------------------------------- highlighter --

/// Brightening applied to highlighted biomes.
pub const BRIGHTEN: u32 = 24;
/// Base value of the grey every other biome is reduced to.
pub const GREY_BASE: u32 = 18;
/// Divisor of the luminance used for that grey.
pub const GREY_DIVISOR: u32 = 5;

/// A lookup mask over biome ids, or an empty vector when highlighting is off
/// or nothing is selected.
pub fn highlight_mask(enabled: bool, highlighted: &BTreeSet<i32>) -> Vec<bool> {
    if !enabled || highlighted.is_empty() {
        return Vec::new();
    }
    let mut mask = vec![false; LIMIT];
    for &id in highlighted {
        if (0..LIMIT as i32).contains(&id) {
            mask[id as usize] = true;
        }
    }
    mask
}

/// Highlighted biomes stay vivid and are brightened, everything else turns
/// to a dark neutral grey. An empty mask changes nothing.
pub fn apply_highlight(argb: &mut [u32], ids: &[i32], highlighted: &[bool]) {
    if highlighted.is_empty() {
        return;
    }
    for (px, &raw) in argb.iter_mut().zip(ids) {
        let colour = *px;
        let mut red = (colour >> 16) & 0xFF;
        let mut green = (colour >> 8) & 0xFF;
        let mut blue = colour & 0xFF;
        let id = id_of(raw);
        if id < highlighted.len() && highlighted[id] {
            red = (BRIGHTEN + red).min(255);
            green = (BRIGHTEN + green).min(255);
            blue = (BRIGHTEN + blue).min(255);
        } else {
            let grey =
                (((54 * red + 183 * green + 19 * blue) >> 8) / GREY_DIVISOR + GREY_BASE).min(255);
            red = grey;
            green = grey;
            blue = grey;
        }
        *px = (colour & 0xFF00_0000) | (red << 16) | (green << 8) | blue;
    }
}

// --------------------------------------------------------------- palette --

/// What the surface palette needs from the game's biome data. Names are the
/// biome's registry path without namespace (`dark_forest`); the palette
/// strips a `minecraft:` prefix before calling. All colours are `0xRRGGBB`.
pub trait BiomeLooks: Send + Sync {
    /// Whether the biome exists in the current game's biome registry. Unknown
    /// biomes (removed pre-1.18 ones) keep their flat biome colour.
    fn known(&self, name: &str) -> bool;

    /// The biome's water tint: vanilla `Biome.getWaterColor()` (for example
    /// `0x3F76E4` for the ocean, `0x617B64` for the swamp).
    fn water_color(&self, name: &str) -> u32;

    /// The grass tint at a block column: vanilla `Biome.getGrassColor(x, z)`,
    /// i.e. the biome's override or the client grass colormap at its
    /// temperature and downfall, after the biome's grass colour modifier.
    fn grass_color(&self, name: &str, x: i32, z: i32) -> u32;

    /// The foliage tint: vanilla `Biome.getFoliageColor()`, i.e. the biome's
    /// override or the client foliage colormap at its temperature/downfall.
    fn foliage_color(&self, name: &str) -> u32;

    /// Vanilla `Biome.coldEnoughToSnow(pos, seaLevel)`: whether the biome's
    /// temperature at the block (height-adjusted above sea level + 17) is
    /// below 0.15, so snow covers the ground.
    fn cold_enough_to_snow(&self, name: &str, x: i32, y: i32, z: i32, sea_level: i32) -> bool;
}

const SNOW: i32 = 0xE8EFF0;
const SAND: i32 = 0xD8CF9B;
const STONE: i32 = 0x858585;
const TERRACOTTA: i32 = 0xB06B48;
const MYCELIUM: i32 = 0x7A6D72;
const MUD: i32 = 0x514B40;
/// Texture brightness of `grass_block_top`, offsetting the -8% base shade.
const GRASS_TEXTURE: f32 = 0.62;
/// Texture brightness of `water_still`.
const WATER_TEXTURE: f32 = 0.70;
const CROWN_CELL: i32 = 5;
const CROWN_SALT: i64 = 0x5EED_A71A5;

// Vanilla's fixed leaf tints (`FoliageColor`).
const FOLIAGE_BIRCH: i32 = 0x80A755;
const FOLIAGE_EVERGREEN: i32 = 0x619961;
const FOLIAGE_MANGROVE: i32 = 0x92C648;

/// Grass/foliage colormap corners (hot-dry, hot-wet, cold).
pub const GRASS_CORNERS: [i32; 3] = [0xBFB755, 0x47CD33, 0x80B497];
pub const FOLIAGE_CORNERS: [i32; 3] = [0xAEA42A, 0x1ABF00, 0x60A17B];

/// Leaf tint source of a forest biome's crowns: a fixed colour (or -1 for the
/// biome's own foliage colour), the texture brightness and the coverage.
#[derive(Clone, Copy, Debug)]
struct Canopy {
    fixed: i32,
    texture: f32,
    density: f32,
}

fn canopy(name: &str) -> Option<Canopy> {
    let own = |texture, density| Canopy {
        fixed: -1,
        texture,
        density,
    };
    let fixed = |fixed, texture, density| Canopy {
        fixed,
        texture,
        density,
    };
    Some(match name {
        "forest" => own(0.60, 0.55),
        "flower_forest" => own(0.60, 0.35),
        "dappled_forest" => own(0.60, 0.45),
        "windswept_forest" => own(0.55, 0.45),
        "dark_forest" => own(0.50, 0.90),
        "birch_forest" => fixed(FOLIAGE_BIRCH, 0.62, 0.55),
        "old_growth_birch_forest" => fixed(FOLIAGE_BIRCH, 0.62, 0.60),
        "taiga" => fixed(FOLIAGE_EVERGREEN, 0.42, 0.55),
        "old_growth_pine_taiga" => fixed(FOLIAGE_EVERGREEN, 0.42, 0.70),
        "old_growth_spruce_taiga" => fixed(FOLIAGE_EVERGREEN, 0.42, 0.75),
        "jungle" => own(0.62, 0.85),
        "bamboo_jungle" => own(0.62, 0.75),
        "sparse_jungle" => own(0.62, 0.30),
        "savanna" => own(0.62, 0.12),
        "savanna_plateau" => own(0.62, 0.15),
        "windswept_savanna" => own(0.62, 0.08),
        "wooded_badlands" => own(0.60, 0.25),
        "swamp" => own(0.60, 0.15),
        "mangrove_swamp" => fixed(FOLIAGE_MANGROVE, 0.55, 0.75),
        // Cherry and pale oak leaves are not tinted: texture averages.
        "cherry_grove" => fixed(0xE5ADC2, 0.95, 0.50),
        "pale_garden" => fixed(0x757A73, 1.05, 0.85),
        _ => return None,
    })
}

fn bare(name: &str) -> &str {
    name.strip_prefix("minecraft:").unwrap_or(name)
}

/// `OverworldTerrainSampler.previewColor`: the cheap LOD colour of a column,
/// `0x00RRGGBB`, or -1 when the biome has no name or is not in the registry.
pub fn preview_color(
    looks: &dyn BiomeLooks,
    name: Option<&str>,
    x: i32,
    y: i32,
    z: i32,
    water: bool,
    step: i32,
) -> i32 {
    match name.map(bare) {
        Some(n) if looks.known(n) => surface_color(looks, n, x, y, z, water, 63, step),
        _ => -1,
    }
}

/// Top colour of a base-terrain column at one sample; `step` selects crown
/// dots or blending.
#[allow(clippy::too_many_arguments)]
pub fn surface_color(
    looks: &dyn BiomeLooks,
    name: &str,
    x: i32,
    y: i32,
    z: i32,
    fluid: bool,
    sea_level: i32,
    step: i32,
) -> i32 {
    let name = bare(name);
    let ground = ground(looks, name, x, y, z, fluid, sea_level);
    // Snow layers also cover leaves, so snowy forests read as snow.
    let canopy = if fluid || ground == SNOW {
        None
    } else {
        canopy(name)
    };
    match canopy {
        None => ground,
        Some(c) => crowns(ground, leaf(looks, name), c.density, x, z, step),
    }
}

/// Colour of the visible block between trees: water, snow, sand, stone or
/// tinted grass.
pub fn ground(
    looks: &dyn BiomeLooks,
    name: &str,
    x: i32,
    y: i32,
    z: i32,
    fluid: bool,
    sea_level: i32,
) -> i32 {
    let name = bare(name);
    if fluid {
        return water(looks.water_color(name) as i32);
    }
    // Vanilla surface rules need a chunk, so the top material is inferred from the biome.
    if looks.cold_enough_to_snow(name, x, y, z, sea_level) {
        return SNOW;
    }
    match name {
        "desert" | "beach" => SAND,
        "stony_shore" | "stony_peaks" => STONE,
        "badlands" | "eroded_badlands" | "wooded_badlands" => TERRACOTTA,
        "mushroom_fields" => MYCELIUM,
        "mangrove_swamp" => MUD,
        _ => grass((looks.grass_color(name, x, z) & 0xFF_FFFF) as i32),
    }
}

/// Leaf colour of a forest biome's crowns, or -1 for biomes without trees.
pub fn leaf(looks: &dyn BiomeLooks, name: &str) -> i32 {
    let name = bare(name);
    let Some(c) = canopy(name) else { return -1 };
    let tint = if c.fixed >= 0 {
        c.fixed
    } else {
        (looks.foliage_color(name) & 0xFF_FFFF) as i32
    };
    scale(tint, c.texture)
}

pub fn grass(tint: i32) -> i32 {
    scale(tint, GRASS_TEXTURE)
}

pub fn water(tint: i32) -> i32 {
    scale(tint, WATER_TEXTURE)
}

/// Tree crowns of radius 1.8..2.7 on a jittered 5-block grid, lit from the
/// north-west. Samples of 8+ blocks blend ground and leaves by coverage.
pub fn crowns(ground: i32, leaf: i32, density: f32, x: i32, z: i32, step: i32) -> i32 {
    if step >= 8 {
        return mix(ground, leaf, density);
    }
    let cell_x = x.div_euclid(CROWN_CELL);
    let cell_z = z.div_euclid(CROWN_CELL);
    // A crown covers about 0.6 of its cell, so this probability yields the coverage.
    let threshold = round(fmin(1.0, density / 0.6) * 1000.0) as i64;
    for dz in -1..=1 {
        for dx in -1..=1 {
            let h = hash(cell_x + dx, cell_z + dz, CROWN_SALT);
            if h % 1000 >= threshold {
                continue;
            }
            let centre_x =
                ((cell_x + dx) * CROWN_CELL) as f32 + 0.5 + ((h >> 12) % CROWN_CELL as i64) as f32;
            let centre_z =
                ((cell_z + dz) * CROWN_CELL) as f32 + 0.5 + ((h >> 20) % CROWN_CELL as i64) as f32;
            let radius = 1.8 + ((h >> 28) % 10) as f32 / 10.0;
            let off_x = x as f32 + 0.5 - centre_x;
            let off_z = z as f32 + 0.5 - centre_z;
            if off_x * off_x + off_z * off_z > radius * radius {
                continue;
            }
            // The north-west half of a crown faces the light.
            let light = round(-(off_x + off_z) / radius * 9.0);
            return scale(leaf, 1.0 + light as f32 / 100.0);
        }
    }
    ground
}

/// Barycentric form of vanilla's colormap lookup (x = 1 - temperature,
/// y = 1 - downfall * temperature).
pub fn colormap(corners: &[i32; 3], temperature: f32, downfall: f32) -> i32 {
    let t = temperature.clamp(0.0, 1.0);
    let d = downfall.clamp(0.0, 1.0) * t;
    let hot_dry = t - d;
    let hot_wet = d;
    let cold = 1.0 - t;
    let mut rgb = 0;
    for shift in [16, 8, 0] {
        let channel = hot_dry * ((corners[0] >> shift) & 0xFF) as f32
            + hot_wet * ((corners[1] >> shift) & 0xFF) as f32
            + cold * ((corners[2] >> shift) & 0xFF) as f32;
        rgb |= round(channel).clamp(0, 255) << shift;
    }
    rgb
}

/// Scales the RGB channels of `0xRRGGBB` by `factor`.
pub fn scale(rgb: i32, factor: f32) -> i32 {
    let mut result = 0;
    for shift in [16, 8, 0] {
        result |= round(((rgb >> shift) & 0xFF) as f32 * factor).clamp(0, 255) << shift;
    }
    result
}

/// Mixes two `0xRRGGBB` colours.
pub fn mix(from: i32, to: i32, amount: f32) -> i32 {
    let mut result = 0;
    for shift in [16, 8, 0] {
        let a = ((from >> shift) & 0xFF) as f32;
        let b = ((to >> shift) & 0xFF) as f32;
        result |= round(a + (b - a) * amount).clamp(0, 255) << shift;
    }
    result
}

// ---------------------------------------------------------- default looks --

/// Vanilla's climate and tint data for the modern biomes, without the game:
/// the colormap is the corner triangle (as the Java's pre-resources
/// fallback), the swamp's and dark forest's grass modifiers use their noise
/// mean, and snow uses the base temperature (frozen-ocean patches and the
/// temperature noise are ignored).
#[derive(Clone, Copy, Debug, Default)]
pub struct VanillaLooks;

#[derive(Clone, Copy)]
enum Modifier {
    None,
    DarkForest,
    Swamp,
}

struct Climate {
    temperature: f32,
    downfall: f32,
    water: u32,
    grass: Option<u32>,
    foliage: Option<u32>,
    modifier: Modifier,
}

fn climate(name: &str) -> Option<Climate> {
    const W: u32 = 0x3F76E4;
    const COLD: u32 = 0x3D57D6;
    let c = |temperature, downfall, water| Climate {
        temperature,
        downfall,
        water,
        grass: None,
        foliage: None,
        modifier: Modifier::None,
    };
    Some(match name {
        "ocean" | "deep_ocean" | "river" | "the_void" => c(0.5, 0.5, W),
        "lukewarm_ocean" | "deep_lukewarm_ocean" => c(0.5, 0.5, 0x45ADF2),
        "warm_ocean" => c(0.5, 0.5, 0x43D5EE),
        "cold_ocean" | "deep_cold_ocean" => c(0.5, 0.5, COLD),
        "frozen_ocean" | "frozen_river" => c(0.0, 0.5, 0x3938C9),
        "deep_frozen_ocean" => c(0.5, 0.5, 0x3938C9),
        "beach" | "plains" | "sunflower_plains" | "dripstone_caves" | "deep_dark" => c(0.8, 0.4, W),
        "snowy_beach" => c(0.05, 0.3, COLD),
        "stony_shore" | "windswept_hills" | "windswept_gravelly_hills" | "windswept_forest" => {
            c(0.2, 0.3, W)
        }
        "snowy_plains" | "ice_spikes" => c(0.0, 0.5, COLD),
        "desert" | "savanna" => c(2.0, 0.0, W),
        "swamp" => Climate {
            foliage: Some(0x6A7039),
            modifier: Modifier::Swamp,
            ..c(0.8, 0.9, 0x617B64)
        },
        "mangrove_swamp" => Climate {
            foliage: Some(0x8DB127),
            ..c(0.8, 0.9, 0x3A7A6A)
        },
        "forest" | "flower_forest" | "dappled_forest" => c(0.7, 0.8, W),
        "dark_forest" => Climate {
            modifier: Modifier::DarkForest,
            ..c(0.7, 0.8, W)
        },
        "birch_forest" | "old_growth_birch_forest" => c(0.6, 0.6, W),
        "pale_garden" => Climate {
            grass: Some(0x778272),
            foliage: Some(0x878D76),
            ..c(0.7, 0.8, 0x76889D)
        },
        "taiga" | "old_growth_spruce_taiga" => c(0.25, 0.8, COLD),
        "old_growth_pine_taiga" => c(0.3, 0.8, COLD),
        "snowy_taiga" => c(-0.5, 0.4, COLD),
        "savanna_plateau" => c(1.0, 0.0, W),
        "windswept_savanna" => c(1.1, 0.0, W),
        "jungle" | "bamboo_jungle" => c(0.95, 0.9, W),
        "sparse_jungle" => c(0.95, 0.8, W),
        "badlands" | "eroded_badlands" | "wooded_badlands" => Climate {
            grass: Some(0x90814D),
            foliage: Some(0x9E814D),
            ..c(2.0, 0.0, W)
        },
        "meadow" => c(0.5, 0.8, 0x0E4ECF),
        "cherry_grove" => Climate {
            grass: Some(0xB6DB61),
            foliage: Some(0xB6DB61),
            ..c(0.5, 0.8, 0x5DB7EF)
        },
        "grove" => c(-0.2, 0.8, COLD),
        "snowy_slopes" => c(-0.3, 0.9, COLD),
        "frozen_peaks" | "jagged_peaks" => c(-0.7, 0.9, COLD),
        "stony_peaks" => c(1.0, 0.3, W),
        "mushroom_fields" => c(0.9, 1.0, W),
        "lush_caves" => c(0.5, 0.5, W),
        "nether_wastes" | "crimson_forest" | "warped_forest" | "soul_sand_valley"
        | "basalt_deltas" => c(2.0, 0.0, W),
        "the_end" | "small_end_islands" | "end_midlands" | "end_highlands" | "end_barrens" => {
            c(0.5, 0.5, W)
        }
        _ => return None,
    })
}

impl BiomeLooks for VanillaLooks {
    fn known(&self, name: &str) -> bool {
        climate(bare(name)).is_some()
    }

    fn water_color(&self, name: &str) -> u32 {
        climate(bare(name)).map_or(0x3F76E4, |c| c.water)
    }

    fn grass_color(&self, name: &str, _x: i32, _z: i32) -> u32 {
        let Some(c) = climate(bare(name)) else {
            return colormap(&GRASS_CORNERS, 0.8, 0.4) as u32;
        };
        let base = c
            .grass
            .unwrap_or_else(|| colormap(&GRASS_CORNERS, c.temperature, c.downfall) as u32);
        match c.modifier {
            Modifier::None => base,
            Modifier::DarkForest => (((base & 0xFE_FEFE) + 0x28_340A) >> 1) & 0xFF_FFFF,
            Modifier::Swamp => 0x6A7039,
        }
    }

    fn foliage_color(&self, name: &str) -> u32 {
        let Some(c) = climate(bare(name)) else {
            return colormap(&FOLIAGE_CORNERS, 0.8, 0.4) as u32;
        };
        c.foliage
            .unwrap_or_else(|| colormap(&FOLIAGE_CORNERS, c.temperature, c.downfall) as u32)
    }

    fn cold_enough_to_snow(&self, name: &str, _x: i32, y: i32, _z: i32, sea_level: i32) -> bool {
        let Some(c) = climate(bare(name)) else {
            return false;
        };
        let mut t = c.temperature;
        if y > sea_level + 17 {
            t -= (y - (sea_level + 17)) as f32 * 0.05 / 40.0;
        }
        t < 0.15
    }
}

// ------------------------------------------------------------ tile worker --

/// Vanilla-backed surface grid of an Overworld tile, all with the one-sample
/// halo (`(size+2)^2`): highest non-air Y, highest solid Y and `0x00RRGGBB`
/// colours (-1 unknown).
#[derive(Clone, Debug, Default)]
pub struct SurfaceGrid {
    pub heights: Vec<f32>,
    pub floors: Vec<f32>,
    pub colors: Vec<i32>,
}

/// Samples a refined tile's surface is sampled apart, in tile samples: the
/// vanilla base terrain is by far the dearest part of a tile, and at one or
/// two screen pixels a sample half its resolution shows the same relief.
pub const SURFACE_SPACING: usize = 2;

/// The side of the coarse surface grid behind a tile of `size` samples: its
/// samples [`SURFACE_SPACING`] tile samples apart, from one coarse sample
/// before the tile's halo.
pub fn coarse_surface_side(size: usize) -> usize {
    (size + 2) / SURFACE_SPACING + 2
}

/// A tile's surface grid (`(size+2)^2`, with its one-sample halo) from the
/// coarse one ([`coarse_surface_side`]): heights and floors bilinear,
/// colours from the nearest coarse sample. Fine sample `i` (halo origin)
/// sits at coarse position `(i + SURFACE_SPACING - 1) / SURFACE_SPACING`
/// when the coarse grid starts `SURFACE_SPACING` tile samples before the halo.
pub fn upsample_surface(coarse: &SurfaceGrid, size: usize) -> Option<SurfaceGrid> {
    let side = coarse_surface_side(size);
    if coarse.heights.len() != side * side
        || coarse.floors.len() != side * side
        || coarse.colors.len() != side * side
    {
        return None;
    }
    let fine = size + 2;
    let k = SURFACE_SPACING as f32;
    let at = |i: usize| {
        let u = (i as f32 + k - 1.0) / k;
        let j = (u.floor() as usize).min(side - 2);
        (j, u - j as f32)
    };
    let mut out = SurfaceGrid {
        heights: vec![0.0; fine * fine],
        floors: vec![0.0; fine * fine],
        colors: vec![-1; fine * fine],
    };
    for z in 0..fine {
        let (jz, tz) = at(z);
        for x in 0..fine {
            let (jx, tx) = at(x);
            let idx = |dx: usize, dz: usize| (jz + dz) * side + jx + dx;
            let mix = |v: &[f32]| {
                let top = v[idx(0, 0)] + (v[idx(1, 0)] - v[idx(0, 0)]) * tx;
                let bottom = v[idx(0, 1)] + (v[idx(1, 1)] - v[idx(0, 1)]) * tx;
                top + (bottom - top) * tz
            };
            let o = z * fine + x;
            out.heights[o] = mix(&coarse.heights);
            out.floors[o] = mix(&coarse.floors).min(out.heights[o]);
            let near = idx(usize::from(tx >= 0.5), usize::from(tz >= 0.5));
            out.colors[o] = coarse.colors[near];
        }
    }
    Some(out)
}

/// What the tile worker asks the atlas for besides the biomes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HeightsRequest {
    None,
    /// `Atlas::height_area` over the halo (Overworld and End).
    Heights,
    /// `Atlas::nether_floor_area` over the halo.
    NetherFloor,
}

/// The Java's `needsSurfaceRefinement`: whether a styled tile has a slower
/// second pass that adds the terrain.
pub fn needs_surface_refinement(
    dimension: Dimension,
    styled: bool,
    focused: bool,
    step: i32,
) -> bool {
    if !styled || focused {
        return false;
    }
    match dimension {
        Dimension::Overworld => step <= 4,
        Dimension::Nether => step <= 16,
        Dimension::End => step <= 8,
    }
}

/// Which heights the tile worker requests (all needing the halo origin
/// `origin - step` and side `size + 2`). Nether floors and End islands take
/// far longer than biomes, so the first pass (`refining == false`) of those
/// requests none. `has_terrain` is `Atlas::has_terrain`.
pub fn heights_request(
    dimension: Dimension,
    step: i32,
    terrain_style: bool,
    focused: bool,
    refining: bool,
    has_surface: bool,
    has_terrain: bool,
) -> HeightsRequest {
    let deferred = dimension != Dimension::Overworld && !refining;
    if has_surface || deferred || !terrain_style || focused || step >= 64 || !has_terrain {
        return HeightsRequest::None;
    }
    match dimension {
        Dimension::Nether if step <= 16 => HeightsRequest::NetherFloor,
        Dimension::Overworld => HeightsRequest::Heights,
        Dimension::End if step <= 8 => HeightsRequest::Heights,
        _ => HeightsRequest::None,
    }
}

/// The tile worker's input.
#[derive(Clone, Copy)]
pub struct TileInput<'a> {
    pub looks: &'a dyn BiomeLooks,
    /// Minecraft version of the atlas (`Atlas::mc`), for biome names.
    pub mc: i32,
    pub dimension: Dimension,
    /// Block coordinates of sample `[0, 0]`.
    pub origin_x: i32,
    pub origin_z: i32,
    /// Blocks per sample.
    pub step: i32,
    /// Samples a side.
    pub size: usize,
    pub seed: i64,
    /// The `terrain_look` setting and the version having terrain
    /// (`Atlas::has_terrain`).
    pub terrain_style: bool,
    /// The slow second pass; a worker without two passes sets this.
    pub refining: bool,
    /// Biome ids, `size * size`.
    pub ids: &'a [i32],
    /// Flat biome colours, ARGB, `size * size`.
    pub argb: &'a [u32],
    /// Heights (Overworld/End) or Nether floors with the one-sample halo,
    /// `(size + 2)^2`, as requested by [`heights_request`].
    pub heights: Option<&'a [f32]>,
    /// Vanilla-backed Overworld surface, if the caller has one.
    pub surface: Option<&'a SurfaceGrid>,
    /// Highlight mask ([`highlight_mask`]); empty when off.
    pub highlight: &'a [bool],
}

impl std::fmt::Debug for TileInput<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TileInput")
            .field("dimension", &self.dimension)
            .field("origin", &(self.origin_x, self.origin_z))
            .field("step", &self.step)
            .field("size", &self.size)
            .finish_non_exhaustive()
    }
}

/// `previewColors` of the Java worker: the Overworld palette per sample.
fn preview_colors(input: &TileInput, heights: Option<&[f32]>) -> Vec<i32> {
    let size = input.size;
    let hw = size + 2;
    let heights = heights.filter(|h| h.len() == hw * hw);
    let centre = (input.step - 1) / 2;
    let mut names: [Option<&'static str>; LIMIT] = [None; LIMIT];
    for &raw in input.ids {
        let id = id_of(raw);
        if id < LIMIT && names[id].is_none() {
            names[id] = biomes::name_in(input.mc, id as i32);
        }
    }
    let mut colors = vec![-1; input.ids.len()];
    for z in 0..size {
        for x in 0..size {
            let i = z * size + x;
            let id = id_of(input.ids[i]);
            if id >= LIMIT {
                continue;
            }
            let y = heights.map_or(64, |h| round(h[(z + 1) * hw + x + 1]));
            colors[i] = preview_color(
                input.looks,
                names[id],
                input
                    .origin_x
                    .wrapping_add((x as i32).wrapping_mul(input.step))
                    .wrapping_add(centre),
                y,
                input
                    .origin_z
                    .wrapping_add((z as i32).wrapping_mul(input.step))
                    .wrapping_add(centre),
                water_biome(id as i32),
                input.step,
            );
        }
    }
    colors
}

/// The colours of a seed-only tile: the flat biome colours, highlighted when
/// a mask is set, then (unless focused) the terrain look the Java tile
/// worker picks per dimension, step, pass and available data. The result is
/// ARGB; `ids` and alpha are untouched.
pub fn style_tile(input: &TileInput) -> Vec<u32> {
    let n = input.size * input.size;
    let mut pixels: Vec<u32> = input.argb.iter().copied().take(n).collect();
    pixels.resize(n, 0);
    let ids = input.ids;
    let (ox, oz, step, size) = (input.origin_x, input.origin_z, input.step, input.size);
    // Highlighting rewrites the samples first, like the desktop renderer.
    apply_highlight(&mut pixels, ids, input.highlight);
    // Focus mode stays flat so highlighted biomes remain readable.
    if !input.terrain_style || !input.highlight.is_empty() {
        return pixels;
    }
    let hw = size + 2;
    let heights = input.heights.filter(|h| h.len() == hw * hw);
    if let Some(surface) = input.surface {
        apply_surface(
            &mut pixels,
            ids,
            step,
            size,
            &surface.heights,
            &surface.floors,
            &surface.colors,
        );
    } else if input.dimension == Dimension::Overworld {
        let colors = preview_colors(input, heights);
        apply_preview(&mut pixels, ids, step, size, heights, &colors);
    } else if input.dimension == Dimension::Nether
        && heights.is_none()
        && !input.refining
        && needs_surface_refinement(Dimension::Nether, true, false, step)
    {
        // Flat preview; the refinement pass adds the cave floor.
    } else if input.dimension == Dimension::End {
        match heights {
            None => apply_end_preview(&mut pixels, ids, ox, oz, step, size),
            Some(h) => apply_end(&mut pixels, ids, ox, oz, step, size, h),
        }
    } else if let (Dimension::Nether, Some(h), true) = (input.dimension, heights, step < 64) {
        apply_nether(&mut pixels, ids, ox, oz, step, size, h);
    } else if step < 64 {
        apply(&mut pixels, ids, ox, oz, step, size, input.seed, heights);
    }
    pixels
}

#[cfg(test)]
mod tests {
    use super::*;

    const OCEAN: i32 = biomes::OCEAN;
    const PLAINS: i32 = biomes::PLAINS;

    fn rgb(c: u32) -> (u32, u32, u32) {
        ((c >> 16) & 0xFF, (c >> 8) & 0xFF, c & 0xFF)
    }

    fn sum(c: u32) -> u32 {
        let (r, g, b) = rgb(c);
        r + g + b
    }

    fn flat_heights(size: usize, value: f32) -> Vec<f32> {
        vec![value; (size + 2) * (size + 2)]
    }

    #[test]
    fn java_round_and_shade() {
        assert_eq!(round(2.5), 3);
        assert_eq!(round(-2.5), -2);
        assert_eq!(round(0.499_999_97), 0);
        assert_eq!(round(f32::NAN), 0);
        // (100 * 90) / 100 and truncation of 255 * 1.05.
        assert_eq!(shade(0xFF64_6464, -10), 0xFF5A_5A5A);
        assert_eq!(shade(0xFFFF_FFFF, 50), 0xFFFF_FFFF);
        assert_eq!(shade(0x80FF_FFFF, -100), 0x8000_0000);
        assert!(hash(-5, 17, 99) >= 0);
        assert_eq!(hash(3, 4, 5), hash(3, 4, 5));
    }

    #[test]
    fn water_depth_tones_get_darker_with_depth() {
        let ids = [OCEAN; 4];
        let colors = [0x3F76E4; 4];
        let shallow = flat_heights(2, 55.0);
        let deep = flat_heights(2, 10.0);
        let mut a = [0xFF3F_76E4_u32; 4];
        let mut b = a;
        apply_preview(&mut a, &ids, 1, 2, Some(&shallow), &colors);
        apply_preview(&mut b, &ids, 1, 2, Some(&deep), &colors);
        assert!(sum(b[0]) < sum(a[0]));
        // Depth 7 gives round(4.55) = 5 on top of the base -8.
        assert_eq!(a[0], shade(0xFF3F_76E4, -13));
        // The darkening is capped at 24.
        assert_eq!(b[0], shade(0xFF3F_76E4, -32));
        // The noise-free fallback with heights: deeper water is darker too.
        let mut c = [0xFF00_00FF_u32; 4];
        let mut d = c;
        apply(&mut c, &ids, 0, 0, 1, 2, 1, Some(&flat_heights(2, 60.0)));
        apply(&mut d, &ids, 0, 0, 1, 2, 1, Some(&flat_heights(2, -20.0)));
        assert!(sum(d[0]) < sum(c[0]));
    }

    #[test]
    fn coarse_surfaces_upsample_to_the_tile() {
        let size = 4;
        let side = coarse_surface_side(size);
        assert_eq!(side, 5);
        // Heights rise one a coarse sample eastwards; colours by column.
        let coarse = SurfaceGrid {
            heights: (0..side * side).map(|i| (i % side) as f32 * 2.0).collect(),
            floors: vec![-10.0; side * side],
            colors: (0..side * side).map(|i| (i % side) as i32).collect(),
        };
        let fine = upsample_surface(&coarse, size).unwrap();
        let row: Vec<f32> = fine.heights[..size + 2].to_vec();
        // Fine sample i at coarse (i + 1) / 2.
        assert_eq!(row, [1.0, 2.0, 3.0, 4.0, 5.0, 6.0]);
        assert_eq!(&fine.colors[..size + 2], &[1, 1, 2, 2, 3, 3]);
        assert!(fine.floors.iter().all(|&f| f == -10.0));
        assert!(
            upsample_surface(&coarse, size + 2).is_none(),
            "wrong coarse side"
        );
    }

    #[test]
    fn surface_water_depth_uses_heights_minus_floors() {
        let ids = [OCEAN; 4];
        let heights = flat_heights(2, 62.0);
        let floors = flat_heights(2, 12.0);
        let colors = vec![0x3F76E4; 16];
        let mut px = [0xFF00_0000_u32; 4];
        apply_surface(&mut px, &ids, 1, 2, &heights, &floors, &colors);
        // Depth 50 caps the darkening at 24 on top of -8.
        assert_eq!(px[0], shade(0xFF3F_76E4, -32));
    }

    #[test]
    fn relief_is_bright_where_ground_rises_to_the_south_east() {
        let size = 2;
        let hw = size + 2;
        let ids = [PLAINS; 4];
        let colors = [0x808080; 4];
        let mut rising = vec![0.0; hw * hw];
        for z in 0..hw {
            for x in 0..hw {
                rising[z * hw + x] = 64.0 + (x + z) as f32;
            }
        }
        let mut falling = rising.clone();
        for z in 0..hw {
            for x in 0..hw {
                falling[z * hw + x] = 64.0 - (x + z) as f32;
            }
        }
        let mut flat = [0xFF80_8080_u32; 4];
        let mut up = flat;
        let mut down = flat;
        apply_preview(
            &mut flat,
            &ids,
            1,
            size,
            Some(&flat_heights(size, 64.0)),
            &colors,
        );
        apply_preview(&mut up, &ids, 1, size, Some(&rising), &colors);
        apply_preview(&mut down, &ids, 1, size, Some(&falling), &colors);
        assert!(sum(up[0]) > sum(flat[0]));
        assert!(sum(down[0]) < sum(flat[0]));
        // The same direction in the height-based fallback relief.
        let (mut a, mut b) = ([0xFF80_8080_u32; 4], [0xFF80_8080_u32; 4]);
        apply(&mut a, &ids, 0, 0, 1, size, 7, Some(&rising));
        apply(&mut b, &ids, 0, 0, 1, size, 7, Some(&falling));
        assert!(sum(a[0]) > sum(b[0]));
    }

    #[test]
    fn noise_relief_is_deterministic_and_keeps_alpha() {
        let ids = vec![PLAINS; 64];
        let mut a = vec![0x7F40_A040_u32; 64];
        let mut b = a.clone();
        apply(&mut a, &ids, 100, -300, 4, 8, 42, None);
        apply(&mut b, &ids, 100, -300, 4, 8, 42, None);
        assert_eq!(a, b);
        assert!(a.iter().all(|p| p >> 24 == 0x7F));
        assert!(a.iter().any(|&p| p != 0x7F40_A040));
        // Unknown ids keep their colour.
        let mut c = vec![0xFF11_2233_u32; 4];
        apply(&mut c, &[-1; 4], 0, 0, 1, 2, 1, None);
        assert_eq!(c, vec![0xFF11_2233; 4]);
    }

    #[test]
    fn end_has_void_and_stone() {
        let size = 3;
        let ids = [biomes::END_HIGHLANDS; 9];
        let mut heights = flat_heights(size, 60.0);
        let hw = size + 2;
        heights[hw + 1] = 0.0; // sample [0,0] is void
        let mut px = [0xFF12_3456_u32; 9];
        apply_end(&mut px, &ids, 0, 0, 1, size, &heights);
        assert_eq!(px[0], 0xFF00_0000 | END_VOID);
        let (r, g, b) = rgb(px[4]);
        assert!(r > 150 && g > 150 && b > 80 && r >= b, "stone: {r} {g} {b}");
        // A wrong-sized grid leaves the tile untouched.
        let mut flat = [0xFF12_3456_u32; 9];
        apply_end(&mut flat, &ids, 0, 0, 1, size, &[1.0; 3]);
        assert_eq!(flat, [0xFF12_3456; 9]);
    }

    #[test]
    fn end_preview_matches_the_refined_palette() {
        // The main island sits around the origin inside the_end.
        let mut px = [0xFFAB_CDEF_u32; 4];
        apply_end_preview(&mut px, &[biomes::THE_END; 4], 0, 0, 1, 2);
        assert!(
            px.iter()
                .all(|&p| p != 0xFF00_0000 | END_VOID && sum(p) > 300)
        );
        let mut far = [0xFFAB_CDEF_u32; 4];
        apply_end_preview(&mut far, &[biomes::THE_END; 4], 4000, 4000, 1, 2);
        assert_eq!(far, [0xFF00_0000 | END_VOID; 4]);
        let mut barrens = [0xFFAB_CDEF_u32; 1];
        apply_end_preview(&mut barrens, &[biomes::END_BARRENS], 0, 0, 1, 1);
        assert_eq!(barrens[0], 0xFF00_0000 | END_VOID);
        let mut other = [0xFFAB_CDEF_u32; 1];
        apply_end_preview(&mut other, &[PLAINS], 0, 0, 1, 1);
        assert_eq!(other[0], 0xFFAB_CDEF);
    }

    #[test]
    fn nether_floor_is_lava_below_the_sea_and_shaded_above() {
        let size = 2;
        let ids = [biomes::NETHER_WASTES; 4];
        let mut low = [0xFF72_2D2D_u32; 4];
        apply_nether(&mut low, &ids, 0, 0, 1, size, &flat_heights(size, 20.0));
        let (r, g, b) = rgb(low[0]);
        assert!(r > 150 && r > g && g > b, "lava: {r} {g} {b}");
        let mut ground = [0xFF72_2D2D_u32; 4];
        apply_nether(&mut ground, &ids, 0, 0, 1, size, &flat_heights(size, 70.0));
        // Flat ground is the biome colour dimmed by about 10 per cent.
        assert!(sum(ground[0]) < sum(0xFF72_2D2D));
        assert!(sum(ground[0]) > sum(0xFF72_2D2D) * 7 / 10);
        let hw = size + 2;
        let mut rising = flat_heights(size, 70.0);
        for z in 0..hw {
            for x in 0..hw {
                rising[z * hw + x] = 70.0 + (x + z) as f32 * 2.0;
            }
        }
        let mut lit = [0xFF72_2D2D_u32; 4];
        apply_nether(&mut lit, &ids, 0, 0, 1, size, &rising);
        assert!(sum(lit[0]) > sum(ground[0]));
    }

    #[test]
    fn highlight_mask_keeps_selected_biomes_and_greys_the_rest() {
        let set: BTreeSet<i32> = [PLAINS, 999, -3].into_iter().collect();
        assert!(highlight_mask(false, &set).is_empty());
        assert!(highlight_mask(true, &BTreeSet::new()).is_empty());
        let mask = highlight_mask(true, &set);
        assert_eq!(mask.len(), LIMIT);
        assert_eq!(mask.iter().filter(|&&m| m).count(), 1);
        let mut px = [0xFF10_2030_u32, 0xFFC8_6432];
        apply_highlight(&mut px, &[PLAINS, OCEAN], &mask);
        assert_eq!(px[0], 0xFF28_3848);
        let grey = ((54 * 200 + 183 * 100 + 19 * 50) >> 8) / 5 + 18;
        assert_eq!(px[1], 0xFF00_0000 | (grey << 16) | (grey << 8) | grey);
        // An empty mask is a no-op.
        let mut same = [0xFF10_2030_u32];
        apply_highlight(&mut same, &[PLAINS], &[]);
        assert_eq!(same[0], 0xFF10_2030);
    }

    #[test]
    fn palette_picks_ground_water_snow_and_crowns() {
        let looks = VanillaLooks;
        assert_eq!(water(0x3F76E4), scale(0x3F76E4, 0.70));
        assert_eq!(
            surface_color(&looks, "minecraft:ocean", 0, 64, 0, true, 63, 1),
            water(0x3F76E4)
        );
        assert_eq!(
            surface_color(&looks, "desert", 0, 70, 0, false, 63, 1),
            SAND
        );
        assert_eq!(
            surface_color(&looks, "snowy_plains", 0, 70, 0, false, 63, 1),
            SNOW
        );
        assert_eq!(
            surface_color(&looks, "badlands", 0, 70, 0, false, 63, 1),
            TERRACOTTA
        );
        let plains = surface_color(&looks, "plains", 0, 70, 0, false, 63, 1);
        assert_eq!(plains, grass(looks.grass_color("plains", 0, 0) as i32));
        // High peaks of a warm-ish biome turn cold enough to snow.
        assert!(looks.cold_enough_to_snow("windswept_hills", 0, 200, 0, 63));
        assert!(!looks.cold_enough_to_snow("windswept_hills", 0, 70, 0, 63));
        // Coarse forest samples blend ground and leaves.
        let ground = ground(&looks, "forest", 8, 70, 8, false, 63);
        let leaves = leaf(&looks, "forest");
        assert!(leaves >= 0);
        assert_eq!(
            surface_color(&looks, "forest", 8, 70, 8, false, 63, 8),
            mix(ground, leaves, 0.55)
        );
        // Fine samples are either ground or a lit crown, never anything else.
        let mut crowns_seen = 0;
        for x in 0..40 {
            let c = surface_color(&looks, "dark_forest", x, 70, 3, false, 63, 1);
            if c != ground_of(&looks, "dark_forest", x) {
                crowns_seen += 1;
            }
        }
        assert!(crowns_seen > 10);
        assert_eq!(leaf(&looks, "plains"), -1);
        assert_eq!(
            preview_color(&looks, Some("removed_biome"), 0, 64, 0, false, 1),
            -1
        );
        assert_eq!(preview_color(&looks, None, 0, 64, 0, false, 1), -1);
        assert!(preview_color(&looks, Some("minecraft:plains"), 0, 64, 0, false, 1) >= 0);
    }

    fn ground_of(looks: &VanillaLooks, name: &str, x: i32) -> i32 {
        ground(looks, name, x, 70, 3, false, 63)
    }

    #[test]
    fn colormap_corners() {
        assert_eq!(colormap(&GRASS_CORNERS, 1.0, 0.0), 0xBFB755);
        assert_eq!(colormap(&GRASS_CORNERS, 1.0, 1.0), 0x47CD33);
        assert_eq!(colormap(&GRASS_CORNERS, 0.0, 0.5), 0x80B497);
    }

    #[test]
    fn heights_requests_follow_the_worker() {
        use Dimension::*;
        let req = |d, step, refining| heights_request(d, step, true, false, refining, false, true);
        assert_eq!(req(Overworld, 4, false), HeightsRequest::Heights);
        assert_eq!(req(Overworld, 64, false), HeightsRequest::None);
        assert_eq!(req(End, 8, true), HeightsRequest::Heights);
        assert_eq!(req(End, 16, true), HeightsRequest::None);
        assert_eq!(req(End, 4, false), HeightsRequest::None);
        assert_eq!(req(Nether, 16, true), HeightsRequest::NetherFloor);
        assert_eq!(req(Nether, 32, true), HeightsRequest::None);
        assert_eq!(
            heights_request(Overworld, 1, true, true, false, false, true),
            HeightsRequest::None
        );
        assert_eq!(
            heights_request(Overworld, 1, true, false, false, true, true),
            HeightsRequest::None
        );
        assert_eq!(
            heights_request(Overworld, 1, false, false, false, false, true),
            HeightsRequest::None
        );
        assert_eq!(
            heights_request(Overworld, 1, true, false, false, false, false),
            HeightsRequest::None
        );
    }

    fn input<'a>(
        looks: &'a VanillaLooks,
        dimension: Dimension,
        ids: &'a [i32],
        argb: &'a [u32],
        heights: Option<&'a [f32]>,
        highlight: &'a [bool],
    ) -> TileInput<'a> {
        TileInput {
            looks,
            mc: meridian_seedatlas::mc::MC_NEWEST,
            dimension,
            origin_x: 0,
            origin_z: 0,
            step: 1,
            size: 2,
            seed: 1,
            terrain_style: true,
            refining: true,
            ids,
            argb,
            heights,
            surface: None,
            highlight,
        }
    }

    #[test]
    fn style_tile_picks_the_look() {
        let looks = VanillaLooks;
        let argb = [biomes::color(PLAINS); 4];
        let ids = [PLAINS; 4];
        // Overworld without heights: palette colour at -8 per cent.
        let t = style_tile(&input(&looks, Dimension::Overworld, &ids, &argb, None, &[]));
        let want = shade(
            0xFF00_0000
                | preview_color(
                    &looks,
                    biomes::name_in(meridian_seedatlas::mc::MC_NEWEST, PLAINS),
                    0,
                    64,
                    0,
                    false,
                    1,
                ) as u32,
            -8,
        );
        assert_eq!(t[0], want);
        // Look off: the flat colours.
        let mut off = input(&looks, Dimension::Overworld, &ids, &argb, None, &[]);
        off.terrain_style = false;
        assert_eq!(style_tile(&off), argb.to_vec());
        // Highlight: no terrain look, grey for the others.
        let mask = highlight_mask(true, &[OCEAN].into_iter().collect());
        let h = style_tile(&input(
            &looks,
            Dimension::Overworld,
            &ids,
            &argb,
            None,
            &mask,
        ));
        let (r, g, b) = rgb(h[0]);
        assert!(r == g && g == b);
        // End: void from heights, End preview without them.
        let end_ids = [biomes::END_HIGHLANDS; 4];
        let end_argb = [biomes::color(biomes::END_HIGHLANDS); 4];
        let void = flat_heights(2, 0.0);
        let e = style_tile(&input(
            &looks,
            Dimension::End,
            &end_ids,
            &end_argb,
            Some(&void),
            &[],
        ));
        assert_eq!(e, vec![0xFF00_0000 | END_VOID; 4]);
        let e = style_tile(&input(
            &looks,
            Dimension::End,
            &end_ids,
            &end_argb,
            None,
            &[],
        ));
        assert!(e.iter().all(|&p| p != 0xFF00_0000 | END_VOID));
        // Nether: the first pass stays flat, the second adds the floor.
        let n_ids = [biomes::NETHER_WASTES; 4];
        let n_argb = [biomes::color(biomes::NETHER_WASTES); 4];
        let mut first = input(&looks, Dimension::Nether, &n_ids, &n_argb, None, &[]);
        first.refining = false;
        assert_eq!(style_tile(&first), n_argb.to_vec());
        let low = flat_heights(2, 10.0);
        let n = style_tile(&input(
            &looks,
            Dimension::Nether,
            &n_ids,
            &n_argb,
            Some(&low),
            &[],
        ));
        assert_ne!(n, n_argb.to_vec());
    }
}
