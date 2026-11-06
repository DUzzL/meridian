//! The Seed Atlas API, matching the Fabric version's native interface
//! (`seedatlas_xaero.h`, ABI 4) call for call, so parity tests can compare
//! them directly.

use crate::biome_noise::Range;
use crate::generator::{Dimension, Generator};
use crate::rng::floordiv;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AtlasError {
    Argument,
    RangeTooLarge,
}

/// Most samples one area call may produce.
pub const MAX_AREA_SAMPLES: i64 = 4 * 1024 * 1024;

/// One seed's generators for the three dimensions.
#[derive(Clone, Debug)]
pub struct Atlas {
    pub seed: i64,
    pub large_biomes: bool,
    overworld: Generator,
    nether: Generator,
    end: Generator,
}

impl Atlas {
    pub fn new(seed: i64, large_biomes: bool) -> Self {
        let s = seed as u64;
        Self {
            seed,
            large_biomes,
            overworld: Generator::new(s, Dimension::Overworld, large_biomes),
            nether: Generator::new(s, Dimension::Nether, large_biomes),
            end: Generator::new(s, Dimension::End, large_biomes),
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
