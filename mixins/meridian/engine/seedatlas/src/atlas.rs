//! The Seed Atlas API, matching the Fabric version's native interface
//! (`seedatlas_xaero.h`, ABI 6) call for call, so parity tests can compare
//! them directly.

use crate::biome_noise::Range;
use crate::generator::{Dimension, Generator};
use crate::rng::floordiv;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AtlasError {
    Argument,
    RangeTooLarge,
    Unsupported,
}

/// Most samples one area call may produce.
pub const MAX_AREA_SAMPLES: i64 = 4 * 1024 * 1024;

/// One seed's generators for the three dimensions.
#[derive(Clone, Debug)]
pub struct Atlas {
    pub seed: i64,
    pub large_biomes: bool,
    /// The Minecraft version generated (`crate::mc`).
    pub mc: i32,
    overworld: Generator,
    nether: Generator,
    end: Generator,
    /// The terrain noise of the glue (1.18 and later).
    terrain: Option<crate::terrain::Terrain>,
}

impl Atlas {
    /// `sax_create`: the newest version.
    pub fn new(seed: i64, large_biomes: bool) -> Self {
        Self::versioned(seed, large_biomes, crate::mc::MC_NEWEST)
    }

    /// `sax_create_versioned` with a version already resolved (`mc::str2mc`).
    pub fn versioned(seed: i64, large_biomes: bool, mc: i32) -> Self {
        let s = seed as u64;
        Self {
            seed,
            large_biomes,
            mc,
            overworld: Generator::versioned(s, Dimension::Overworld, mc, large_biomes),
            nether: Generator::versioned(s, Dimension::Nether, mc, large_biomes),
            end: Generator::versioned(s, Dimension::End, mc, large_biomes),
            terrain: (mc >= crate::terrain::TERRAIN_MC).then(|| crate::terrain::Terrain::new(s)),
        }
    }

    pub fn generator(&self, dimension: Dimension) -> &Generator {
        match dimension {
            Dimension::Overworld => &self.overworld,
            Dimension::Nether => &self.nether,
            Dimension::End => &self.end,
        }
    }

    /// `sax_biome_at`: the biome of one block.
    pub fn biome_at(&self, dimension: Dimension, x: i32, y: i32, z: i32) -> i32 {
        self.generator(dimension).biome_at(1, x, y, z)
    }

    /// `sax_biome_area_sampled`: a `w` x `h` grid of samples `step` blocks
    /// apart from the block (`ox`, `oz`), row-major.
    #[allow(clippy::too_many_arguments)]
    pub fn biome_area(
        &self,
        dimension: Dimension,
        ox: i32,
        oz: i32,
        y: i32,
        step: i32,
        w: i32,
        h: i32,
    ) -> Result<Vec<i32>, AtlasError> {
        if !(1..=1024).contains(&step) || step & (step - 1) != 0 || w <= 0 || h <= 0 {
            return Err(AtlasError::Argument);
        }
        let count = w as i64 * h as i64;
        if count > MAX_AREA_SAMPLES {
            return Err(AtlasError::RangeTooLarge);
        }
        let fits = |v: i64| (i32::MIN as i64..=i32::MAX as i64).contains(&v);
        let (s, w64, h64) = (step as i64, w as i64, h as i64);
        if !fits(ox as i64 + (w64 - 1) * s)
            || !fits(oz as i64 + (h64 - 1) * s)
            || !fits(ox as i64 + w64 * s - 1)
            || !fits(oz as i64 + h64 * s - 1)
        {
            return Err(AtlasError::RangeTooLarge);
        }
        if step <= 2 && (ox < i32::MIN + 2 || oz < i32::MIN + 2) {
            return Err(AtlasError::RangeTooLarge);
        }
        if step >= 4 && (ox % step != 0 || oz % step != 0) {
            return Err(AtlasError::Argument);
        }
        let g = self.generator(dimension);
        Ok(match step {
            1 => g.gen_biomes(Range {
                scale: 1,
                x: ox,
                z: oz,
                sx: w,
                sz: h,
                y,
                sy: 1,
            }),
            2 => {
                if count * 4 > MAX_AREA_SAMPLES {
                    return Err(AtlasError::RangeTooLarge);
                }
                let dense = g.gen_biomes(Range {
                    scale: 1,
                    x: ox,
                    z: oz,
                    sx: w * 2,
                    sz: h * 2,
                    y,
                    sy: 1,
                });
                let mut out = Vec::with_capacity(count as usize);
                for z in 0..h as usize {
                    for x in 0..w as usize {
                        out.push(dense[(z * 2 + 1) * (w as usize * 2) + x * 2 + 1]);
                    }
                }
                out
            }
            // Layered overworlds only have the 1:4, 1:16, 1:64 and 1:256
            // layers: sample a finer one and keep the centre of every cell.
            _ if g.layered_scale(step) != step => {
                let base = g.layered_scale(step);
                let f = step / base;
                if count * (f as i64) * (f as i64) > 4 * MAX_AREA_SAMPLES {
                    return Err(AtlasError::RangeTooLarge);
                }
                let sx = w * f;
                let dense = g.gen_biomes(Range {
                    scale: base,
                    x: ox / base,
                    z: oz / base,
                    sx,
                    sz: h * f,
                    y: floordiv(y, 4),
                    sy: 1,
                });
                let mut out = Vec::with_capacity(count as usize);
                for z in 0..h as i64 {
                    for x in 0..w as i64 {
                        out.push(
                            dense[((z * f as i64 + (f / 2) as i64) * sx as i64
                                + x * f as i64
                                + (f / 2) as i64) as usize],
                        );
                    }
                }
                out
            }
            _ => g.gen_biomes(Range {
                scale: step,
                x: ox / step,
                z: oz / step,
                sx: w,
                sz: h,
                y: floordiv(y, 4),
                sy: 1,
            }),
        })
    }

    /// `sax_biome_chunk_sampled`: one chunk's 16 x 16 blocks, each
    /// `step`-sized cell filled with the biome at its centre.
    pub fn biome_chunk(
        &self,
        dimension: Dimension,
        cx: i32,
        cz: i32,
        y: i32,
        step: i32,
    ) -> Result<[i32; 256], AtlasError> {
        let (bx, bz) = (cx as i64 * 16, cz as i64 * 16);
        let fits = |v: i64| (i32::MIN as i64..=i32::MAX as i64).contains(&v);
        if !fits(bx) || !fits(bz) || ![1, 2, 4, 8, 16].contains(&step) {
            return Err(AtlasError::Argument);
        }
        let g = self.generator(dimension);
        let mut out = [0i32; 256];
        if step == 1 {
            let v = g.gen_biomes(Range {
                scale: 1,
                x: bx as i32,
                z: bz as i32,
                sx: 16,
                sz: 16,
                y,
                sy: 1,
            });
            out.copy_from_slice(&v);
            return Ok(out);
        }
        for sz in (0..16).step_by(step as usize) {
            for sx in (0..16).step_by(step as usize) {
                let id = g.biome_at(1, bx as i32 + sx + step / 2, y, bz as i32 + sz + step / 2);
                for fz in sz..sz + step {
                    for fx in sx..sx + step {
                        out[(fz * 16 + fx) as usize] = id;
                    }
                }
            }
        }
        Ok(out)
    }
}

// ----------------------------------------------------------- structures --

use crate::endcity::{end_city_has_ship, end_city_terrain};
use crate::finders::strongholds;
use crate::noise::DoublePerlin;
use crate::rng::Xoroshiro;
use crate::structures::{
    config, end_islands, fixed_end_gateways, is_slime_chunk, kind, structure_pos,
};
use crate::surface::{DIM_END, SurfaceNoise};
use crate::viability::variant;
use crate::viability::viable_structure_pos;

/// Marker kinds beyond the engine's structures (the Fabric interface's).
pub mod marker {
    pub const STRONGHOLD: i32 = 1001;
    pub const SPAWN: i32 = 1002;
    pub const SLIME_CHUNK: i32 = 1003;
    pub const ORE_VEIN_COPPER: i32 = 1004;
    pub const ORE_VEIN_IRON: i32 = 1005;
    pub const END_SHIP: i32 = 1006;
    pub const ORE_VEIN: i32 = 1007;
}

pub const RESULT_BIOME_CHECKED: i32 = 1;
pub const RESULT_BIOME_VIABLE: i32 = 2;
pub const RESULT_APPROXIMATE: i32 = 4;
pub const RESULT_FIXED: i32 = 8;
pub const CAMP_BIOME_MASK: i32 = 0xff;
pub const CAMP_SPECIAL_LOOT: i32 = 1 << 8;
/// Most cells (regions, chunks) one scan may cover.
pub const MAX_SCAN_CELLS: i64 = 4_194_304;

/// One found start or marker (`SAX_RESULT_STRIDE` row).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Hit {
    pub kind: i32,
    pub x: i32,
    pub y: Option<i32>,
    pub z: i32,
    pub flags: i32,
    pub detail: i32,
}

fn range_too_large(x0: i32, z0: i32, x1: i32, z1: i32) -> bool {
    let w = x1 as i64 - x0 as i64 + 1;
    let h = z1 as i64 - z0 as i64 + 1;
    w <= 0 || h <= 0 || w > MAX_SCAN_CELLS || h > MAX_SCAN_CELLS || w * h > MAX_SCAN_CELLS
}

fn in_box(x: i32, z: i32, b: [i32; 4]) -> bool {
    x >= b[0] && x <= b[2] && z >= b[1] && z <= b[3]
}

/// Structures whose terrain check the engine cannot make for 26.3.
fn needs_unavailable_terrain(kind: i32) -> bool {
    matches!(
        kind,
        kind::DESERT_PYRAMID
            | kind::JUNGLE_TEMPLE
            | kind::MANSION
            | kind::DESERT_WELL
            | kind::GEODE
            | kind::ABANDONED_CAMP
    )
}

struct OreNoise {
    veininess: DoublePerlin,
    vein_a: DoublePerlin,
    vein_b: DoublePerlin,
    positional: Xoroshiro,
}

impl OreNoise {
    fn new(seed: u64) -> Self {
        let mut world = Xoroshiro::new(seed);
        let lo = world.next_long();
        let hi = world.next_long();
        let mut ore = Xoroshiro {
            lo: lo ^ 0x9b88124de600116d,
            hi: hi ^ 0x2ae68055aa4a7761,
        };
        let positional = Xoroshiro {
            lo: ore.next_long(),
            hi: ore.next_long(),
        };
        let dp = |m: [u64; 2], omin: i32| {
            let mut r = Xoroshiro {
                lo: lo ^ m[0],
                hi: hi ^ m[1],
            };
            DoublePerlin::new_x(&mut r, &[1.0], omin, -1)
        };
        let veininess = dp([0x6b86c7820a307171, 0xd87fb0fefd9c1624], -8);
        let vein_a = dp([0x4cd8d69b9a841649, 0xcdd63f17bfe8f5ed], -7);
        let vein_b = dp([0x6b26220b31f7c6c9, 0xae077edebf6aaec1], -7);
        Self {
            veininess,
            vein_a,
            vein_b,
            positional,
        }
    }

    fn random_at(&self, x: i32, y: i32, z: i32) -> Xoroshiro {
        let mut value = ((x as i64).wrapping_mul(3129871) as u64)
            ^ ((z as i64).wrapping_mul(116129781) as u64)
            ^ (y as i64 as u64);
        value = value
            .wrapping_mul(value)
            .wrapping_mul(42317861)
            .wrapping_add(value.wrapping_mul(11));
        let shifted = (value as i64) >> 16;
        let mut r = self.positional;
        r.lo ^= shifted as u64;
        r
    }

    fn vein_at(&self, x: i32, y: i32, z: i32) -> i32 {
        let (fx, fy, fz) = (x as f64, y as f64, z as f64);
        let ridge = self
            .vein_a
            .sample(4.0 * fx, 4.0 * fy, 4.0 * fz)
            .abs()
            .max(self.vein_b.sample(4.0 * fx, 4.0 * fy, 4.0 * fz).abs());
        if ridge >= 0.08 {
            return 0;
        }
        let toggle = self.veininess.sample(1.5 * fx, 1.5 * fy, 1.5 * fz);
        let copper = toggle > 0.0;
        let (min_y, max_y) = if copper { (0, 50) } else { (-60, -8) };
        if y < min_y || y > max_y {
            return 0;
        }
        let edge = if max_y - y < y - min_y {
            max_y - y
        } else {
            y - min_y
        };
        let t = ((edge as f64 - 0.0) / 20.0).clamp(0.0, 1.0);
        if toggle.abs() + (-0.2 + t * (0.0 - -0.2)) < 0.4 {
            return 0;
        }
        let mut r = self.random_at(x, y, z);
        if r.next_float() > 0.7f32 {
            return 0;
        }
        if copper {
            marker::ORE_VEIN_COPPER
        } else {
            marker::ORE_VEIN_IRON
        }
    }
}

impl Atlas {
    fn scan_ore_veins(
        &self,
        requested: i32,
        b: [i32; 4],
        cap: usize,
        stride: i32,
    ) -> Result<Vec<Hit>, AtlasError> {
        const SIZE: i32 = 128;
        let (mx0, mz0, mx1, mz1) = (
            floordiv(b[0], SIZE),
            floordiv(b[1], SIZE),
            floordiv(b[2], SIZE),
            floordiv(b[3], SIZE),
        );
        if range_too_large(mx0, mz0, mx1, mz1) {
            return Err(AtlasError::RangeTooLarge);
        }
        if self.mc < crate::terrain::TERRAIN_MC {
            return Ok(Vec::new());
        }
        let noise = OreNoise::new(self.seed as u64);
        let mut out = Vec::new();
        for mz in mz0..=mz1 {
            for mx in mx0..=mx1 {
                if stride > 1 && (mx.rem_euclid(stride) != 0 || mz.rem_euclid(stride) != 0) {
                    continue;
                }
                let mut found = false;
                'cells: for cz in 0..8 {
                    for cx in 0..8 {
                        let bx64 = mx as i64 * SIZE as i64 + cx * 16;
                        let bz64 = mz as i64 * SIZE as i64 + cz * 16;
                        if bx64 < i32::MIN as i64
                            || bx64 > i32::MAX as i64 - 15
                            || bz64 < i32::MIN as i64
                            || bz64 > i32::MAX as i64 - 15
                        {
                            continue;
                        }
                        let (bx, bz) = (bx64 as i32, bz64 as i32);
                        let mut pick = noise.random_at(bx, 0, bz);
                        let x = bx + pick.next_int(16);
                        let z = bz + pick.next_int(16);
                        let mut y = -60;
                        while y <= 50 {
                            let t = noise.vein_at(x, y, z);
                            if t == 0 || (requested != marker::ORE_VEIN && t != requested) {
                                y += 4;
                                continue;
                            }
                            found = true;
                            if in_box(x, z, b) {
                                if out.len() >= cap {
                                    return Ok(out);
                                }
                                out.push(Hit {
                                    kind: t,
                                    x,
                                    y: Some(y),
                                    z,
                                    flags: RESULT_APPROXIMATE,
                                    detail: 0,
                                });
                            }
                            break;
                        }
                        if found {
                            break 'cells;
                        }
                    }
                }
            }
        }
        Ok(out)
    }

    fn scan_end_islands(
        &self,
        b: [i32; 4],
        check_biome: bool,
        cap: usize,
    ) -> Result<Vec<Hit>, AtlasError> {
        let (x0, z0, x1, z1) = (
            floordiv(b[0], 16),
            floordiv(b[1], 16),
            floordiv(b[2], 16),
            floordiv(b[3], 16),
        );
        if range_too_large(x0, z0, x1, z1) {
            return Err(AtlasError::RangeTooLarge);
        }
        // `getEndIslands` yields nothing without the structure.
        if config(kind::END_ISLAND, self.mc).is_none() {
            return Ok(Vec::new());
        }
        let mut out = Vec::new();
        for cz in z0..=z1 {
            for cx in x0..=x1 {
                let mut flags = 0;
                if check_biome {
                    if self.end.biome_at(16, cx, 0, cz) != crate::biomes::SMALL_END_ISLANDS {
                        continue;
                    }
                    flags = RESULT_BIOME_CHECKED | RESULT_BIOME_VIABLE;
                }
                for i in end_islands(self.mc, self.seed as u64, cx, cz) {
                    if in_box(i.x, i.z, b) {
                        if out.len() >= cap {
                            return Ok(out);
                        }
                        out.push(Hit {
                            kind: kind::END_ISLAND,
                            x: i.x,
                            y: Some(i.y),
                            z: i.z,
                            flags,
                            detail: i.r,
                        });
                    }
                }
            }
        }
        Ok(out)
    }

    /// `sax_scan_structures`: starts of `structure` inside the block box
    /// `[min_x, min_z, max_x, max_z]`, biome-checked when asked.
    pub fn scan_structures(
        &self,
        structure: i32,
        b: [i32; 4],
        check_biome: bool,
    ) -> Result<Vec<Hit>, AtlasError> {
        self.scan_structures_capped(structure, b, check_biome, usize::MAX)
    }

    /// [`Atlas::scan_structures`] with the glue's `capacity`: the scan stops
    /// at `cap` results, the first ones in scan order (`append_result`).
    pub fn scan_structures_capped(
        &self,
        structure: i32,
        b: [i32; 4],
        check_biome: bool,
        cap: usize,
    ) -> Result<Vec<Hit>, AtlasError> {
        self.scan_structures_strided(structure, b, check_biome, cap, 1)
    }

    /// The scan over every `stride`-th cell (region, ore cell) in both
    /// directions, on a grid aligned to the world.
    fn scan_structures_strided(
        &self,
        structure: i32,
        b: [i32; 4],
        check_biome: bool,
        cap: usize,
        stride: i32,
    ) -> Result<Vec<Hit>, AtlasError> {
        if b[0] > b[2] || b[1] > b[3] {
            return Err(AtlasError::Argument);
        }
        if [
            marker::ORE_VEIN,
            marker::ORE_VEIN_COPPER,
            marker::ORE_VEIN_IRON,
        ]
        .contains(&structure)
        {
            return self.scan_ore_veins(structure, b, cap, stride);
        }
        if structure == kind::END_ISLAND {
            return self.scan_end_islands(b, check_biome, cap);
        }
        let engine = if structure == marker::END_SHIP {
            kind::END_CITY
        } else {
            structure
        };
        if !(1..kind::COUNT).contains(&engine) {
            return Err(AtlasError::Unsupported);
        }
        let Some(c) = config(engine, self.mc) else {
            // Known to the newest version but not generated in this one
            // (Trial Chambers before 1.21...): nothing to show.
            return if config(engine, crate::mc::MC_NEWEST).is_some() {
                Ok(Vec::new())
            } else {
                Err(AtlasError::Unsupported)
            };
        };
        let size = c.region_size as i32 * 16;
        let (rx0, rz0, rx1, rz1) = (
            floordiv(b[0], size),
            floordiv(b[1], size),
            floordiv(b[2], size),
            floordiv(b[3], size),
        );
        if range_too_large(rx0, rz0, rx1, rz1) {
            return Err(AtlasError::RangeTooLarge);
        }
        let seed = self.seed as u64;
        let g = self.generator(c.dimension);
        let surface = (check_biome && engine == kind::END_CITY).then(|| self.end_surface());
        let mut out = Vec::new();
        if structure == kind::END_GATEWAY {
            for (i, (x, z)) in fixed_end_gateways(self.mc, seed).into_iter().enumerate() {
                if in_box(x, z, b) {
                    if out.len() >= cap {
                        return Ok(out);
                    }
                    out.push(Hit {
                        kind: structure,
                        x,
                        y: None,
                        z,
                        flags: RESULT_FIXED,
                        detail: i as i32,
                    });
                }
            }
        }
        for rz in rz0..=rz1 {
            for rx in rx0..=rx1 {
                if stride > 1 && (rx.rem_euclid(stride) != 0 || rz.rem_euclid(stride) != 0) {
                    continue;
                }
                let Some((x, z)) = structure_pos(engine, self.mc, seed, rx, rz) else {
                    continue;
                };
                if !in_box(x, z, b) {
                    continue;
                }
                let mut flags = 0;
                let mut detail = if engine == kind::ABANDONED_CAMP {
                    -1
                } else {
                    0
                };
                if check_biome {
                    let biome = viable_structure_pos(engine, g, x, z, 0);
                    if biome == 0 {
                        continue;
                    }
                    if engine == kind::ABANDONED_CAMP {
                        detail = biome & CAMP_BIOME_MASK;
                        if variant(engine, self.mc, seed, x, z, biome).is_some_and(|v| v.special) {
                            detail |= CAMP_SPECIAL_LOOT;
                        }
                    }
                    if engine == kind::END_CITY {
                        let en = self.end.end_noise().expect("the End generator");
                        if end_city_terrain(
                            self.mc,
                            seed,
                            en,
                            surface.as_ref().expect("made above"),
                            x,
                            z,
                        ) == 0
                        {
                            continue;
                        }
                    }
                    flags |= RESULT_BIOME_CHECKED | RESULT_BIOME_VIABLE;
                }
                if structure == marker::END_SHIP && !end_city_has_ship(seed, x, z) {
                    continue;
                }
                if needs_unavailable_terrain(engine) {
                    flags |= RESULT_APPROXIMATE;
                }
                if out.len() >= cap {
                    return Ok(out);
                }
                out.push(Hit {
                    kind: structure,
                    x,
                    y: None,
                    z,
                    flags,
                    detail,
                });
            }
        }
        Ok(out)
    }

    fn end_surface(&self) -> SurfaceNoise {
        SurfaceNoise::new(DIM_END, self.seed as u64)
    }

    /// `sax_scan_strongholds`: strongholds in the box, relocated by biome
    /// when asked (exact), else their ring estimates.
    pub fn scan_strongholds(&self, b: [i32; 4], check_biome: bool) -> Result<Vec<Hit>, AtlasError> {
        if b[0] > b[2] || b[1] > b[3] {
            return Err(AtlasError::Argument);
        }
        // Up to 1.19.2 the ring positions depend on the biome search itself,
        // so every position is exact.
        let check_biome = check_biome || self.mc <= crate::mc::MC_1_19_2;
        let list = strongholds(
            self.seed as u64,
            self.mc,
            check_biome.then_some(&self.overworld),
        );
        let flags = if check_biome {
            RESULT_BIOME_CHECKED | RESULT_BIOME_VIABLE
        } else {
            RESULT_APPROXIMATE
        };
        Ok(list
            .into_iter()
            .filter(|s| in_box(s.x, s.z, b))
            .map(|s| Hit {
                kind: marker::STRONGHOLD,
                x: s.x,
                y: None,
                z: s.z,
                flags,
                detail: s.ring,
            })
            .collect())
    }

    /// `sax_spawn`.
    pub fn spawn(&self, detailed: bool) -> (i32, i32) {
        if detailed {
            crate::finders::spawn(&self.overworld)
        } else {
            crate::finders::estimate_spawn(&self.overworld)
        }
    }

    /// `sax_scan_slime_chunks`: slime chunks (their north-west block) in
    /// the box.
    pub fn scan_slime_chunks(&self, b: [i32; 4]) -> Result<Vec<Hit>, AtlasError> {
        self.scan_slime_chunks_capped(b, usize::MAX)
    }

    /// [`Atlas::scan_slime_chunks`], stopping at `cap` results.
    pub fn scan_slime_chunks_capped(
        &self,
        b: [i32; 4],
        cap: usize,
    ) -> Result<Vec<Hit>, AtlasError> {
        self.scan_slime_chunks_strided(b, cap, 1)
    }

    fn scan_slime_chunks_strided(
        &self,
        b: [i32; 4],
        cap: usize,
        stride: i32,
    ) -> Result<Vec<Hit>, AtlasError> {
        if b[0] > b[2] || b[1] > b[3] {
            return Err(AtlasError::Argument);
        }
        let (x0, z0, x1, z1) = (
            floordiv(b[0], 16),
            floordiv(b[1], 16),
            floordiv(b[2], 16),
            floordiv(b[3], 16),
        );
        if range_too_large(x0, z0, x1, z1) {
            return Err(AtlasError::RangeTooLarge);
        }
        let mut out = Vec::new();
        for cz in z0..=z1 {
            for cx in x0..=x1 {
                if stride > 1 && (cx.rem_euclid(stride) != 0 || cz.rem_euclid(stride) != 0) {
                    continue;
                }
                if is_slime_chunk(self.seed as u64, cx, cz) {
                    if out.len() >= cap {
                        return Ok(out);
                    }
                    out.push(Hit {
                        kind: marker::SLIME_CHUNK,
                        x: cx.wrapping_mul(16),
                        y: None,
                        z: cz.wrapping_mul(16),
                        flags: 0,
                        detail: 0,
                    });
                }
            }
        }
        Ok(out)
    }
}

// ------------------------------------------------------ even thinning --

/// Candidates checked per result kept when a scan is thinned.
const SPREAD_OVERSAMPLE: i64 = 4;

impl Atlas {
    /// The blocks one candidate cell of a scan covers (a structure's region,
    /// an ore vein cell, a chunk), if the kind scans by cells.
    fn scan_cell(&self, structure: i32) -> Option<i32> {
        if [
            marker::ORE_VEIN,
            marker::ORE_VEIN_COPPER,
            marker::ORE_VEIN_IRON,
        ]
        .contains(&structure)
        {
            return Some(128);
        }
        if structure == marker::SLIME_CHUNK {
            return Some(16);
        }
        let engine = if structure == marker::END_SHIP {
            kind::END_CITY
        } else {
            structure
        };
        if !(1..kind::COUNT).contains(&engine) || structure == kind::END_ISLAND {
            return None;
        }
        config(engine, self.mc).map(|c| c.region_size as i32 * 16)
    }

    /// A scan of `structure` (structures, ore veins, slime chunks) that
    /// keeps at most `cap` results spread evenly over the box, instead of
    /// the first ones in scan order: where the box holds far more candidates
    /// than that, only every k-th cell is checked, and the results are
    /// thinned to one per cell of a grid. Both grids are aligned to the
    /// world, so moving the view keeps the markers in place.
    pub fn scan_spread(
        &self,
        structure: i32,
        b: [i32; 4],
        check_biome: bool,
        cap: usize,
    ) -> Result<Vec<Hit>, AtlasError> {
        if b[0] > b[2] || b[1] > b[3] {
            return Err(AtlasError::Argument);
        }
        let cap = cap.max(1);
        let stride = match self.scan_cell(structure) {
            Some(cell) => {
                let w = (b[2] as i64 - b[0] as i64) / cell as i64 + 1;
                let h = (b[3] as i64 - b[1] as i64) / cell as i64 + 1;
                let budget = cap as i64 * SPREAD_OVERSAMPLE;
                let mut k = 1i64;
                while (w / k) * (h / k) > budget && k < 1 << 16 {
                    k *= 2;
                }
                k as i32
            }
            None => 1,
        };
        let all = if structure == marker::SLIME_CHUNK {
            self.scan_slime_chunks_strided(b, usize::MAX, stride)?
        } else {
            self.scan_structures_strided(structure, b, check_biome, usize::MAX, stride)?
        };
        Ok(thin_evenly(all, b, cap))
    }
}

/// Candidate cells a complete scan checks at most, for kinds placed per
/// chunk or ore-vein cell (cheap: a random draw per cell) and for the rest
/// (a biome check per cell).
const COMPLETE_CELLS_CHEAP: i64 = 1 << 20;
const COMPLETE_CELLS: i64 = 1 << 15;

impl Atlas {
    /// Every `structure` in the box, or `None` when the box is too large to
    /// scan whole for it or holds more than `cap`: the map then leaves the
    /// kind out at that zoom rather than showing some of them (markers are
    /// never thinned or merged).
    pub fn scan_complete(
        &self,
        structure: i32,
        b: [i32; 4],
        check_biome: bool,
        cap: usize,
    ) -> Result<Option<Vec<Hit>>, AtlasError> {
        if b[0] > b[2] || b[1] > b[3] {
            return Err(AtlasError::Argument);
        }
        if let Some(cell) = self.scan_cell(structure) {
            let w = (b[2] as i64 - b[0] as i64) / cell as i64 + 1;
            let h = (b[3] as i64 - b[1] as i64) / cell as i64 + 1;
            let limit = if cell <= 128 {
                COMPLETE_CELLS_CHEAP
            } else {
                COMPLETE_CELLS
            };
            if w * h > limit {
                return Ok(None);
            }
        }
        let all = if structure == marker::SLIME_CHUNK {
            self.scan_slime_chunks_strided(b, cap + 1, 1)?
        } else {
            self.scan_structures_strided(structure, b, check_biome, cap + 1, 1)?
        };
        Ok((all.len() <= cap).then_some(all))
    }
}

/// At most `cap` hits, one per cell of a world-aligned power-of-two grid
/// fine enough to hold `cap` cells over the box; the first hit in scan
/// order wins a cell.
pub fn thin_evenly(hits: Vec<Hit>, b: [i32; 4], cap: usize) -> Vec<Hit> {
    if hits.len() <= cap {
        return hits;
    }
    let side = (b[2] as i64 - b[0] as i64 + 1).max(b[3] as i64 - b[1] as i64 + 1);
    let per_side = (cap as f64).sqrt().floor().max(1.0) as i64;
    let mut cell = 1i64;
    while side.div_euclid(cell) + 1 > per_side {
        cell *= 2;
    }
    let mut taken = std::collections::HashSet::new();
    let mut out = Vec::with_capacity(cap);
    for h in hits {
        if taken.insert((h.x as i64).div_euclid(cell) * 1_000_003 + (h.z as i64).div_euclid(cell)) {
            out.push(h);
            if out.len() >= cap {
                break;
            }
        }
    }
    out
}

// ------------------------------------------------- terrain (ABI 6, 1.18+) --

impl Atlas {
    /// `sax_context_has_terrain`.
    pub fn has_terrain(&self) -> bool {
        self.mc >= crate::terrain::TERRAIN_MC
    }

    /// `sax_height_area_sampled`: terrain heights of the Overworld and the
    /// End (0 is void there).
    #[allow(clippy::too_many_arguments)]
    pub fn height_area(
        &self,
        dimension: Dimension,
        ox: i32,
        oz: i32,
        step: i32,
        w: i32,
        h: i32,
    ) -> Result<Vec<f32>, AtlasError> {
        let bn = self.overworld.overworld().expect("the Overworld generator");
        crate::terrain::height_area(self.terrain.as_ref(), bn, dimension, ox, oz, step, w, h)
    }

    /// `sax_nether_floor_area_sampled`.
    #[allow(clippy::too_many_arguments)]
    pub fn nether_floor_area(
        &self,
        ox: i32,
        oz: i32,
        y: i32,
        step: i32,
        w: i32,
        h: i32,
    ) -> Result<Vec<f32>, AtlasError> {
        self.nether_floor(ox, oz, y, step, w, h, None)
    }

    /// `sax_nether_floor_area_carved_sampled`: two mask words a sample, bit
    /// y marks carved air.
    #[allow(clippy::too_many_arguments)]
    pub fn nether_floor_area_carved(
        &self,
        ox: i32,
        oz: i32,
        y: i32,
        step: i32,
        w: i32,
        h: i32,
        carved: &[u64],
    ) -> Result<Vec<f32>, AtlasError> {
        self.nether_floor(ox, oz, y, step, w, h, Some(carved))
    }

    #[allow(clippy::too_many_arguments)]
    fn nether_floor(
        &self,
        ox: i32,
        oz: i32,
        y: i32,
        step: i32,
        w: i32,
        h: i32,
        carved: Option<&[u64]>,
    ) -> Result<Vec<f32>, AtlasError> {
        crate::terrain::nether_floor_area(self.terrain.as_ref(), ox, oz, y, step, w, h, carved)
    }
}
