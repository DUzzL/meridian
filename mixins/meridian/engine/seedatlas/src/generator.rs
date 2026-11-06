//! One dimension's biome generator for a seed (`Generator` with
//! `setupGenerator`/`applySeed`/`genBiomes` of the engine, 26.3 only).

use crate::biome_noise::{
    BiomeNoise, EndNoise, NetherNoise, Range, SAMPLE_NO_SHIFT, map_voronoi_plane,
    voronoi_access_3d, voronoi_sha, voronoi_src_range,
};
use crate::biomes::{NETHER_WASTES, NONE, THE_END};
use crate::layers::{LayerStack, map_voronoi114_end};
use crate::mc::*;

// TEMP shim: `BiomeNoise::new(seed, large, mc)` is being added in
// `biome_noise.rs` by another change; until it lands this forwards to the
// current constructor.
fn biome_noise_new(seed: u64, large: bool, _mc: i32) -> BiomeNoise {
    BiomeNoise::new(seed, large)
}

// TEMP shim: `EndNoise::new(seed, mc)`, see above.
fn end_noise_new(seed: u64, _mc: i32) -> EndNoise {
    EndNoise::new(seed)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Dimension {
    Overworld,
    Nether,
    End,
}

impl Dimension {
    /// The engine's number for the dimension (-1, 0, 1).
    pub fn id(self) -> i32 {
        match self {
            Dimension::Nether => -1,
            Dimension::Overworld => 0,
            Dimension::End => 1,
        }
    }
}

#[derive(Clone, Debug)]
#[allow(clippy::large_enum_variant)]
enum Noise {
    /// 1.18 and later.
    Overworld(Box<BiomeNoise>),
    /// 1.0 to 1.17: the layer stack.
    Layers(Box<LayerStack>),
    /// 1.16 and later.
    Nether(NetherNoise),
    /// The Nether before 1.16: nether wastes only.
    NetherWastes,
    End(EndNoise),
}

#[derive(Clone, Debug)]
pub struct Generator {
    /// The Minecraft version generated (`crate::mc`).
    pub mc: i32,
    pub seed: u64,
    pub large_biomes: bool,
    pub dimension: Dimension,
    pub sha: u64,
    /// The custom entry layer (`g->entry`) of the layered generator
    /// (scale 0 in `layer_for_scale`), used by `finders`.
    pub entry: Option<usize>,
    noise: Noise,
}

impl Generator {
    /// The newest version's generator.
    pub fn new(seed: u64, dimension: Dimension, large_biomes: bool) -> Self {
        Self::versioned(seed, dimension, crate::mc::MC_NEWEST, large_biomes)
    }

    /// `setupGenerator` + `applySeed` for version `mc`.
    pub fn versioned(seed: u64, dimension: Dimension, mc: i32, large_biomes: bool) -> Self {
        let noise = match dimension {
            Dimension::Overworld => {
                if mc <= MC_1_17 {
                    let mut ls = LayerStack::new(mc, large_biomes);
                    let e = ls.entry_1;
                    ls.set_layer_seed(e, seed);
                    Noise::Layers(Box::new(ls))
                } else {
                    Noise::Overworld(Box::new(biome_noise_new(seed, large_biomes, mc)))
                }
            }
            Dimension::Nether => {
                if mc >= MC_1_16_1 {
                    Noise::Nether(NetherNoise::new(seed))
                } else {
                    Noise::NetherWastes
                }
            }
            Dimension::End => Noise::End(end_noise_new(seed, mc)),
        };
        let sha = if mc >= MC_1_15 {
            match &noise {
                Noise::Layers(ls) if mc <= MC_1_17 => ls.layers[ls.entry_1].start_salt,
                _ => voronoi_sha(seed),
            }
        } else {
            0
        };
        Self {
            mc,
            seed,
            large_biomes,
            dimension,
            sha,
            entry: None,
            noise,
        }
    }

    /// The layer stack of the layered overworld (1.0 to 1.17).
    pub fn layer_stack(&self) -> Option<&LayerStack> {
        match &self.noise {
            Noise::Layers(ls) => Some(ls),
            _ => None,
        }
    }

    /// The layer stack, mutable (to swap map functions as `finders.c`
    /// does).
    pub fn layer_stack_mut(&mut self) -> Option<&mut LayerStack> {
        match &mut self.noise {
            Noise::Layers(ls) => Some(ls),
            _ => None,
        }
    }

    /// `layered_scale` of the glue: the scale the generator produces
    /// directly for `sample_step` (layered overworlds only have 1:4, 1:16,
    /// 1:64 and 1:256).
    pub fn layered_scale(&self, sample_step: i32) -> i32 {
        if self.dimension != Dimension::Overworld || self.mc >= MC_1_18 || sample_step < 4 {
            return sample_step;
        }
        let mut scale = 256;
        while scale > sample_step {
            scale /= 4;
        }
        scale
    }

    /// `getLayerForScale`: the index of the layer that generates `scale`
    /// (0, 1, 4, 16, 64 or 256), if any.
    pub fn layer_for_scale(&self, scale: i32) -> Option<usize> {
        let ls = self.layer_stack()?;
        match scale {
            0 => self.entry,
            1 => Some(ls.entry_1),
            4 => Some(ls.entry_4),
            16 => Some(ls.entry_16),
            64 => Some(ls.entry_64),
            256 => Some(ls.entry_256),
            _ => None,
        }
    }

    pub fn end_noise(&self) -> Option<&EndNoise> {
        match &self.noise {
            Noise::End(en) => Some(en),
            _ => None,
        }
    }

    pub fn overworld(&self) -> Option<&BiomeNoise> {
        match &self.noise {
            Noise::Overworld(bn) => Some(bn),
            _ => None,
        }
    }

    /// The biome of one cell at `scale` (1 = block, 4 = quarter...).
    pub fn biome_at(&self, scale: i32, x: i32, y: i32, z: i32) -> i32 {
        self.gen_biomes(Range {
            scale,
            x,
            z,
            sx: 1,
            sz: 1,
            y,
            sy: 1,
        })[0]
    }

    /// The biomes of a range, y-major then z then x (`genBiomes`).
    pub fn gen_biomes(&self, mut r: Range) -> Vec<i32> {
        if r.sy == 0 {
            r.sy = 1;
        }
        let siz = r.sx as usize * r.sy as usize * r.sz as usize;
        match &self.noise {
            Noise::Layers(ls) => {
                let Some(entry) = self.layer_for_scale(r.scale) else {
                    return vec![NONE; siz];
                };
                let n2d = r.sx as usize * r.sz as usize;
                let len = siz + (ls.min_layer_cache_size(entry, r.sx, r.sz) - n2d);
                let mut cache = vec![0; len];
                if ls.gen_area(entry, &mut cache, r.x, r.z, r.sx, r.sz) != 0 {
                    return vec![NONE; siz];
                }
                // the overworld has no vertical noise: expanding 2D into 3D
                for k in 1..r.sy as usize {
                    cache.copy_within(0..n2d, k * n2d);
                }
                cache.truncate(siz);
                cache
            }
            Noise::NetherWastes => {
                // up to 1.15 the Nether is nether wastes only
                vec![NETHER_WASTES; siz]
            }
            Noise::Overworld(bn) => {
                if r.scale == 1 {
                    let s = voronoi_src_range(r);
                    let src = (siz > 1).then(|| gen_overworld_3d(bn, s, false));
                    let mut out = Vec::with_capacity(siz);
                    for k in 0..r.sy {
                        for j in 0..r.sz {
                            for i in 0..r.sx {
                                let (x4, y4, z4) =
                                    voronoi_access_3d(self.sha, r.x + i, r.y + k, r.z + j);
                                out.push(match &src {
                                    Some(src) => {
                                        let (x4, y4, z4) = (
                                            (x4 - s.x) as i64,
                                            (y4 - s.y) as i64,
                                            (z4 - s.z) as i64,
                                        );
                                        src[(y4 * s.sx as i64 * s.sz as i64 + z4 * s.sx as i64 + x4)
                                            as usize]
                                    }
                                    None => bn.sample(x4, y4, z4, None, 0),
                                });
                            }
                        }
                    }
                    out
                } else {
                    gen_overworld_3d(bn, r, r.scale > 4)
                }
            }
            Noise::Nether(nn) => {
                if r.scale <= 0 {
                    r.scale = 4;
                }
                if r.scale == 1 {
                    let s = voronoi_src_range(r);
                    let src = (siz > 1).then(|| {
                        let mut src = vec![0; s.sx as usize * s.sy.max(1) as usize * s.sz as usize];
                        nn.map_3d(&mut src, s, 1.0);
                        src
                    });
                    let mut out = Vec::with_capacity(siz);
                    for k in 0..r.sy {
                        for j in 0..r.sz {
                            for i in 0..r.sx {
                                let (x4, y4, z4) =
                                    voronoi_access_3d(self.sha, r.x + i, r.y + k, r.z + j);
                                out.push(match &src {
                                    Some(src) => {
                                        let (x4, y4, z4) = (
                                            (x4 - s.x) as i64,
                                            (y4 - s.y) as i64,
                                            (z4 - s.z) as i64,
                                        );
                                        src[(y4 * s.sx as i64 * s.sz as i64 + z4 * s.sx as i64 + x4)
                                            as usize]
                                    }
                                    None => nn.biome(x4, y4, z4).0,
                                });
                            }
                        }
                    }
                    out
                } else {
                    let mut out = vec![0; siz];
                    nn.map_3d(&mut out, r, 1.0);
                    out
                }
            }
            Noise::End(en) => gen_end(en, r, self.sha, self.mc),
        }
    }
}

/// `genBiomeNoise3D`: cells at `r.scale` (4 and up), sampling the middle of
/// each; `opt` skips the shift noise and reuses the last leaf, as the
/// engine does above 1:4.
fn gen_overworld_3d(bn: &BiomeNoise, r: Range, opt: bool) -> Vec<i32> {
    let mut dat = 0u64;
    let flags = if opt { SAMPLE_NO_SHIFT } else { 0 };
    let scale = if r.scale > 4 { r.scale / 4 } else { 1 };
    let mid = scale / 2;
    let mut out = Vec::with_capacity(r.sx as usize * r.sy.max(1) as usize * r.sz as usize);
    for k in 0..r.sy.max(1) {
        let yk = r.y + k;
        for j in 0..r.sz {
            let zj = (r.z + j) * scale + mid;
            for i in 0..r.sx {
                let xi = (r.x + i) * scale + mid;
                out.push(bn.sample(xi, yk, zj, if opt { Some(&mut dat) } else { None }, flags));
            }
        }
    }
    out
}

/// `genEndScaled`.
fn gen_end(en: &EndNoise, r: Range, sha: u64, mc: i32) -> Vec<i32> {
    let plane = r.sx as usize * r.sz as usize;
    let mut out = vec![0i32; plane * r.sy as usize];
    if mc <= MC_1_8 {
        out.fill(THE_END);
        return out;
    }
    match r.scale {
        1 if mc <= MC_1_14 => {
            // up to 1.14 voronoi noise is planar
            let s = voronoi_src_range(r);
            let src = s.sx as usize * s.sz as usize;
            let mut buf = vec![0i32; (src + plane).max(plane * r.sy as usize)];
            en.map(&mut buf, s.x, s.z, s.sx, s.sz);
            map_voronoi114_end(&mut buf, r.x, r.z, r.sx, r.sz);
            buf.truncate(plane);
            buf.resize(plane * r.sy as usize, 0);
            out = buf;
        }
        1 => {
            let s = voronoi_src_range(r);
            let mut src = vec![0i32; s.sx as usize * s.sz as usize];
            en.map(&mut src, s.x, s.z, s.sx, s.sz);
            for iy in 0..r.sy as usize {
                map_voronoi_plane(
                    sha,
                    &mut out[iy * plane..(iy + 1) * plane],
                    &src,
                    r.x,
                    r.z,
                    r.sx,
                    r.sz,
                    r.y + iy as i32,
                    s.x,
                    s.z,
                    s.sx,
                    s.sz,
                );
            }
            return out;
        }
        4 => en.map(&mut out, r.x, r.z, r.sx, r.sz),
        16 => en.map_biome(&mut out, r.x, r.z, r.sx, r.sz),
        _ => {
            use crate::biomes::*;
            let d = r.scale as f32 / 8.0;
            for j in 0..r.sz {
                for i in 0..r.sx {
                    let hx = ((i + r.x) as f32 * d) as i64;
                    let hz = ((j + r.z) as f32 * d) as i64;
                    let rsq = (hx.wrapping_mul(hx)).wrapping_add(hz.wrapping_mul(hz)) as u64;
                    let o = (j * r.sx + i) as usize;
                    out[o] = if rsq <= 16384 {
                        THE_END
                    } else if mc > MC_1_13 && (rsq as u32 as i32) < 0 {
                        END_BARRENS
                    } else {
                        let h = en.height_noise(hx as i32, hz as i32, 4);
                        if h > 40.0 {
                            END_HIGHLANDS
                        } else if h >= 0.0 {
                            END_MIDLANDS
                        } else if h >= -20.0 {
                            END_BARRENS
                        } else {
                            SMALL_END_ISLANDS
                        }
                    };
                }
            }
        }
    }
    for iy in 1..r.sy as usize {
        out.copy_within(0..plane, iy * plane);
    }
    out
}
