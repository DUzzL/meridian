//! The layered overworld biome generator of Minecraft 1.0 to 1.17, ported
//! from the engine's `layers.c`/`layers.h` and the layer stack parts of
//! `generator.c` (`setupLayerStack`, `setupLayer`, `genArea`...).
//!
//! The C code links layers by pointer and swaps their map functions at
//! runtime (`finders.c` does so for `L_BIOME_256` and `L_SHORE_16`). Here the
//! layers live in an arena (`LayerStack::layers`), parents are indices, and
//! the map function of a layer is a plain function pointer that can be
//! replaced (`layers[L_BIOME_256].get_map = ...`). Like in C, every map
//! function writes into one shared output buffer that also serves as scratch
//! space behind the result, so buffers must be at least
//! [`LayerStack::min_cache_size`] long.

use crate::biome_noise::{map_voronoi_plane, voronoi_sha};
use crate::biomes::*;
use crate::mc::*;
use crate::noise::Perlin;
use crate::rng::{
    JavaRandom, chunk_seed, layer_salt, mc_first_int, mc_first_is_zero, mc_step_seed,
};

/// `LAYER_INIT_SHA`: layer salt marking a layer seeded with the Voronoi SHA.
pub const LAYER_INIT_SHA: u64 = !0u64;

// `enum BiomeTempCategory`
pub const OCEANIC: i32 = 0;
pub const WARM: i32 = 1;
pub const LUSH: i32 = 2;
pub const COLD: i32 = 3;
pub const FREEZING: i32 = 4;
pub const SPECIAL: i32 = 5;

// `enum LayerId`: the indices of the layers in the stack (with the old
// names as aliases).
pub const L_CONTINENT_4096: usize = 0;
pub const L_ISLAND_4096: usize = L_CONTINENT_4096;
pub const L_ZOOM_4096: usize = 1;
pub const L_LAND_4096: usize = 2;
pub const L_ZOOM_2048: usize = 3;
pub const L_LAND_2048: usize = 4;
pub const L_ADD_ISLAND_2048: usize = L_LAND_2048;
pub const L_ZOOM_1024: usize = 5;
pub const L_LAND_1024_A: usize = 6;
pub const L_ADD_ISLAND_1024A: usize = L_LAND_1024_A;
pub const L_LAND_1024_B: usize = 7;
pub const L_ADD_ISLAND_1024B: usize = L_LAND_1024_B;
pub const L_LAND_1024_C: usize = 8;
pub const L_ADD_ISLAND_1024C: usize = L_LAND_1024_C;
pub const L_ISLAND_1024: usize = 9;
pub const L_REMOVE_OCEAN_1024: usize = L_ISLAND_1024;
pub const L_SNOW_1024: usize = 10;
pub const L_ADD_SNOW_1024: usize = L_SNOW_1024;
pub const L_LAND_1024_D: usize = 11;
pub const L_ADD_ISLAND_1024D: usize = L_LAND_1024_D;
pub const L_COOL_1024: usize = 12;
pub const L_COOL_WARM_1024: usize = L_COOL_1024;
pub const L_HEAT_1024: usize = 13;
pub const L_HEAT_ICE_1024: usize = L_HEAT_1024;
pub const L_SPECIAL_1024: usize = 14;
pub const L_ZOOM_512: usize = 15;
pub const L_LAND_512: usize = 16;
pub const L_ZOOM_256: usize = 17;
pub const L_LAND_256: usize = 18;
pub const L_ADD_ISLAND_256: usize = L_LAND_256;
pub const L_MUSHROOM_256: usize = 19;
pub const L_ADD_MUSHROOM_256: usize = L_MUSHROOM_256;
pub const L_DEEP_OCEAN_256: usize = 20;
pub const L_BIOME_256: usize = 21;
pub const L_BAMBOO_256: usize = 22;
pub const L14_BAMBOO_256: usize = L_BAMBOO_256;
pub const L_ZOOM_128: usize = 23;
pub const L_ZOOM_64: usize = 24;
pub const L_BIOME_EDGE_64: usize = 25;
pub const L_NOISE_256: usize = 26;
pub const L_RIVER_INIT_256: usize = L_NOISE_256;
pub const L_ZOOM_128_HILLS: usize = 27;
pub const L_ZOOM_64_HILLS: usize = 28;
pub const L_HILLS_64: usize = 29;
pub const L_SUNFLOWER_64: usize = 30;
pub const L_RARE_BIOME_64: usize = L_SUNFLOWER_64;
pub const L_ZOOM_32: usize = 31;
pub const L_LAND_32: usize = 32;
pub const L_ADD_ISLAND_32: usize = L_LAND_32;
pub const L_ZOOM_16: usize = 33;
/// In 1.0 this slot is scale 1:32.
pub const L_SHORE_16: usize = 34;
pub const L_SWAMP_RIVER_16: usize = 35;
pub const L_ZOOM_8: usize = 36;
pub const L_ZOOM_4: usize = 37;
pub const L_SMOOTH_4: usize = 38;
pub const L_ZOOM_128_RIVER: usize = 39;
pub const L_ZOOM_64_RIVER: usize = 40;
pub const L_ZOOM_32_RIVER: usize = 41;
pub const L_ZOOM_16_RIVER: usize = 42;
pub const L_ZOOM_8_RIVER: usize = 43;
pub const L_ZOOM_4_RIVER: usize = 44;
pub const L_RIVER_4: usize = 45;
pub const L_SMOOTH_4_RIVER: usize = 46;
pub const L_RIVER_MIX_4: usize = 47;
pub const L_OCEAN_TEMP_256: usize = 48;
pub const L13_OCEAN_TEMP_256: usize = L_OCEAN_TEMP_256;
pub const L_ZOOM_128_OCEAN: usize = 49;
pub const L13_ZOOM_128: usize = L_ZOOM_128_OCEAN;
pub const L_ZOOM_64_OCEAN: usize = 50;
pub const L13_ZOOM_64: usize = L_ZOOM_64_OCEAN;
pub const L_ZOOM_32_OCEAN: usize = 51;
pub const L13_ZOOM_32: usize = L_ZOOM_32_OCEAN;
pub const L_ZOOM_16_OCEAN: usize = 52;
pub const L13_ZOOM_16: usize = L_ZOOM_16_OCEAN;
pub const L_ZOOM_8_OCEAN: usize = 53;
pub const L13_ZOOM_8: usize = L_ZOOM_8_OCEAN;
pub const L_ZOOM_4_OCEAN: usize = 54;
pub const L13_ZOOM_4: usize = L_ZOOM_4_OCEAN;
pub const L_OCEAN_MIX_4: usize = 55;
pub const L13_OCEAN_MIX_4: usize = L_OCEAN_MIX_4;
pub const L_VORONOI_1: usize = 56;
pub const L_VORONOI_ZOOM_1: usize = L_VORONOI_1;
// largeBiomes layers
pub const L_ZOOM_LARGE_A: usize = 57;
pub const L_ZOOM_LARGE_B: usize = 58;
pub const L_ZOOM_L_RIVER_A: usize = 59;
pub const L_ZOOM_L_RIVER_B: usize = 60;
pub const L_NUM: usize = 61;

/// `mapfunc_t`: fills `out` with the `w` x `h` cells at (`x`, `z`) of layer
/// `l` of the stack `ls`; returns 0 on success.
pub type MapFunc = fn(&LayerStack, &Layer, &mut [i32], i32, i32, i32, i32) -> i32;

/// `Layer`.
#[derive(Clone, Debug)]
pub struct Layer {
    /// The map function; replaceable.
    pub get_map: MapFunc,
    /// Minecraft version.
    pub mc: i32,
    /// Zoom factor of the layer.
    pub zoom: i32,
    /// Maximum border required from the parent layer.
    pub edge: i32,
    /// Scale of the layer (cell = scale x scale blocks).
    pub scale: i32,
    /// Processed salt or initialization mode.
    pub layer_salt: u64,
    /// Depends on the world seed; used to step the PRNG forward.
    pub start_salt: u64,
    /// Depends on the world seed; start for chunk seeds.
    pub start_seed: u64,
    /// Whether the layer uses the stack's ocean noise (`noise`).
    pub noise: bool,
    /// Generic data for custom layers (`data`; e.g. `finders.c` stores its
    /// integers here).
    pub data: Vec<i32>,
    /// Parent layers (indices into the stack's arena).
    pub p: Option<usize>,
    pub p2: Option<usize>,
}

/// A layer that has not been set up; mapping it fails.
fn map_unset(_: &LayerStack, _: &Layer, _: &mut [i32], _: i32, _: i32, _: i32, _: i32) -> i32 {
    1
}

impl Default for Layer {
    fn default() -> Self {
        Self {
            get_map: map_unset,
            mc: 0,
            zoom: 0,
            edge: 0,
            scale: 0,
            layer_salt: 0,
            start_salt: 0,
            start_seed: 0,
            noise: false,
            data: Vec::new(),
            p: None,
            p2: None,
        }
    }
}

/// `LayerStack`: the overworld biome generator up to 1.17.
#[derive(Clone, Debug)]
pub struct LayerStack {
    /// The layers; the first `L_NUM` are the standard ones, custom entry
    /// layers (`setup_layer` on a new index) can be appended.
    pub layers: Vec<Layer>,
    /// Entry scale (1:1) [`L_VORONOI_1`].
    pub entry_1: usize,
    /// Entry scale (1:4) [`L_RIVER_MIX_4`] or [`L_OCEAN_MIX_4`].
    pub entry_4: usize,
    // unofficial entries for other scales (latest sensible layers):
    /// [`L_SWAMP_RIVER_16`] or [`L_SHORE_16`].
    pub entry_16: usize,
    /// [`L_HILLS_64`] or [`L_SUNFLOWER_64`].
    pub entry_64: usize,
    /// [`L_BIOME_256`] or [`L_BAMBOO_256`].
    pub entry_256: usize,
    /// The ocean temperature noise (`oceanRnd`), set by the world seed.
    pub ocean_rnd: Option<Perlin>,
}

// ------------------------------------------------------------ helpers --

/// `mapOceanMixMod`: ocean mix that takes the land from `p` and the ocean
/// types from `p2` (used by `FORCE_OCEAN_VARIANTS`).
pub fn map_ocean_mix_mod(
    ls: &LayerStack,
    l: &Layer,
    out: &mut [i32],
    x: i32,
    z: i32,
    w: i32,
    h: i32,
) -> i32 {
    let n = w as usize * h as usize;
    ls.get_map(l.p2.unwrap(), out, x, z, w, h);
    let otyp: Vec<i32> = out[..n].to_vec();
    ls.get_map(l.p.unwrap(), out, x, z, w, h);

    for idx in 0..n {
        let land_id = out[idx];
        if !is_oceanic(land_id) {
            continue;
        }
        let mut ocean_id = otyp[idx];
        if land_id == DEEP_OCEAN {
            match ocean_id {
                LUKEWARM_OCEAN => ocean_id = DEEP_LUKEWARM_OCEAN,
                OCEAN => ocean_id = DEEP_OCEAN,
                COLD_OCEAN => ocean_id = DEEP_COLD_OCEAN,
                FROZEN_OCEAN => ocean_id = DEEP_FROZEN_OCEAN,
                _ => {}
            }
        }
        out[idx] = ocean_id;
    }
    0
}

impl LayerStack {
    /// `getMap` of layer `idx`.
    #[inline]
    pub fn get_map(&self, idx: usize, out: &mut [i32], x: i32, z: i32, w: i32, h: i32) -> i32 {
        let l = &self.layers[idx];
        (l.get_map)(self, l, out, x, z, w, h)
    }

    /// `setupLayer`: sets up layer `id` (growing the arena for a custom
    /// layer) and returns its index.
    #[allow(clippy::too_many_arguments)]
    pub fn setup_layer(
        &mut self,
        id: usize,
        map: MapFunc,
        mc: i32,
        zoom: i32,
        edge: i32,
        saltbase: u64,
        p: Option<usize>,
        p2: Option<usize>,
    ) -> usize {
        if id >= self.layers.len() {
            self.layers.resize(id + 1, Layer::default());
        }
        let layer_salt_ = if saltbase == 0 || saltbase == LAYER_INIT_SHA {
            saltbase
        } else {
            layer_salt(saltbase)
        };
        self.layers[id] = Layer {
            get_map: map,
            mc,
            zoom,
            edge,
            scale: 0,
            layer_salt: layer_salt_,
            start_salt: 0,
            start_seed: 0,
            noise: false,
            data: Vec::new(),
            p,
            p2,
        };
        id
    }

    /// `setupScale`.
    fn setup_scale(&mut self, idx: usize, scale: i32) {
        self.layers[idx].scale = scale;
        let (p, p2, zoom) = {
            let l = &self.layers[idx];
            (l.p, l.p2, l.zoom)
        };
        if let Some(p) = p {
            self.setup_scale(p, scale * zoom);
        }
        if let Some(p2) = p2 {
            self.setup_scale(p2, scale * zoom);
        }
    }

    /// `setLayerSeed`: applies the world seed to the layer and all layers
    /// it depends on.
    pub fn set_layer_seed(&mut self, idx: usize, world_seed: u64) {
        if let Some(p2) = self.layers[idx].p2 {
            self.set_layer_seed(p2, world_seed);
        }
        if let Some(p) = self.layers[idx].p {
            self.set_layer_seed(p, world_seed);
        }
        if self.layers[idx].noise {
            let mut s = JavaRandom::new(world_seed);
            self.ocean_rnd = Some(Perlin::new_java(&mut s));
        }
        let layer = &mut self.layers[idx];
        let ls = layer.layer_salt;
        if ls == 0 {
            // Pre 1.13 the Hills branch stays zero-initialized
            layer.start_salt = 0;
            layer.start_seed = 0;
        } else if ls == LAYER_INIT_SHA {
            // Post 1.14 Voronoi uses SHA256 for initialization
            layer.start_salt = voronoi_sha(world_seed);
            layer.start_seed = 0;
        } else {
            let mut st = world_seed;
            st = mc_step_seed(st, ls);
            st = mc_step_seed(st, ls);
            st = mc_step_seed(st, ls);
            layer.start_salt = st;
            layer.start_seed = mc_step_seed(st, 0);
        }
    }

    /// `setupLayerStack`: builds the layer stack of version `mc`.
    pub fn new(mc: i32, large_biomes: bool) -> Self {
        let large_biomes = large_biomes && mc >= MC_1_3;
        let mut g = LayerStack {
            layers: vec![Layer::default(); L_NUM],
            entry_1: 0,
            entry_4: 0,
            entry_16: 0,
            entry_64: 0,
            entry_256: 0,
            ocean_rnd: None,
        };
        let map_land: MapFunc;
        let mut p: usize;
        // L: layer, M: mapping function, V: minecraft version, Z: zoom,
        // E: edge, S: salt base, P1: parent 1, P2: parent 2
        macro_rules! sl {
            ($id:expr, $map:expr, $z:expr, $e:expr, $s:expr, $p:expr, $p2:expr) => {
                g.setup_layer($id, $map, mc, $z, $e, $s, $p, $p2)
            };
        }

        if mc == MC_B1_8 {
            // NOTE: reusing slot for continent:4096, but scale is 1:8192
            map_land = map_land_b18;
            p = sl!(L_CONTINENT_4096, map_continent, 1, 0, 1, None, None);
            p = sl!(L_ZOOM_4096, map_zoom_fuzzy, 2, 3, 2000, Some(p), None);
            p = sl!(L_LAND_4096, map_land, 1, 2, 1, Some(p), None);
            p = sl!(L_ZOOM_2048, map_zoom, 2, 3, 2001, Some(p), None);
            p = sl!(L_LAND_2048, map_land, 1, 2, 2, Some(p), None);
            p = sl!(L_ZOOM_1024, map_zoom, 2, 3, 2002, Some(p), None);
            p = sl!(L_LAND_1024_A, map_land, 1, 2, 3, Some(p), None);
            p = sl!(L_ZOOM_512, map_zoom, 2, 3, 2003, Some(p), None);
            p = sl!(L_LAND_512, map_land, 1, 2, 3, Some(p), None);
            p = sl!(L_ZOOM_256, map_zoom, 2, 3, 2004, Some(p), None);
            p = sl!(L_LAND_256, map_land, 1, 2, 3, Some(p), None);
            p = sl!(L_BIOME_256, map_biome, 1, 0, 200, Some(p), None);
            p = sl!(L_ZOOM_128, map_zoom, 2, 3, 1000, Some(p), None);
            sl!(L_ZOOM_64, map_zoom, 2, 3, 1001, Some(p), None);
            // river noise layer chain, also used to determine where hills generate
            p = sl!(L_NOISE_256, map_noise, 1, 0, 100, Some(L_LAND_256), None);
        } else if mc <= MC_1_6 {
            map_land = map_land16;
            p = sl!(L_CONTINENT_4096, map_continent, 1, 0, 1, None, None);
            p = sl!(L_ZOOM_2048, map_zoom_fuzzy, 2, 3, 2000, Some(p), None);
            p = sl!(L_LAND_2048, map_land, 1, 2, 1, Some(p), None);
            p = sl!(L_ZOOM_1024, map_zoom, 2, 3, 2001, Some(p), None);
            p = sl!(L_LAND_1024_A, map_land, 1, 2, 2, Some(p), None);
            p = sl!(L_SNOW_1024, map_snow16, 1, 2, 2, Some(p), None);
            p = sl!(L_ZOOM_512, map_zoom, 2, 3, 2002, Some(p), None);
            p = sl!(L_LAND_512, map_land, 1, 2, 3, Some(p), None);
            p = sl!(L_ZOOM_256, map_zoom, 2, 3, 2003, Some(p), None);
            p = sl!(L_LAND_256, map_land, 1, 2, 4, Some(p), None);
            p = sl!(L_MUSHROOM_256, map_mushroom, 1, 2, 5, Some(p), None);
            p = sl!(L_BIOME_256, map_biome, 1, 0, 200, Some(p), None);
            p = sl!(L_ZOOM_128, map_zoom, 2, 3, 1000, Some(p), None);
            sl!(L_ZOOM_64, map_zoom, 2, 3, 1001, Some(p), None);
            // river noise layer chain, also used to determine where hills generate
            p = sl!(
                L_NOISE_256,
                map_noise,
                1,
                0,
                100,
                Some(L_MUSHROOM_256),
                None
            );
        } else {
            map_land = map_land_fn;
            p = sl!(L_CONTINENT_4096, map_continent, 1, 0, 1, None, None);
            p = sl!(L_ZOOM_2048, map_zoom_fuzzy, 2, 3, 2000, Some(p), None);
            p = sl!(L_LAND_2048, map_land, 1, 2, 1, Some(p), None);
            p = sl!(L_ZOOM_1024, map_zoom, 2, 3, 2001, Some(p), None);
            p = sl!(L_LAND_1024_A, map_land, 1, 2, 2, Some(p), None);
            p = sl!(L_LAND_1024_B, map_land, 1, 2, 50, Some(p), None);
            p = sl!(L_LAND_1024_C, map_land, 1, 2, 70, Some(p), None);
            p = sl!(L_ISLAND_1024, map_island, 1, 2, 2, Some(p), None);
            p = sl!(L_SNOW_1024, map_snow, 1, 2, 2, Some(p), None);
            p = sl!(L_LAND_1024_D, map_land, 1, 2, 3, Some(p), None);
            p = sl!(L_COOL_1024, map_cool, 1, 2, 2, Some(p), None);
            p = sl!(L_HEAT_1024, map_heat, 1, 2, 2, Some(p), None);
            p = sl!(L_SPECIAL_1024, map_special, 1, 2, 3, Some(p), None);
            p = sl!(L_ZOOM_512, map_zoom, 2, 3, 2002, Some(p), None);
            p = sl!(L_ZOOM_256, map_zoom, 2, 3, 2003, Some(p), None);
            p = sl!(L_LAND_256, map_land, 1, 2, 4, Some(p), None);
            p = sl!(L_MUSHROOM_256, map_mushroom, 1, 2, 5, Some(p), None);
            p = sl!(L_DEEP_OCEAN_256, map_deep_ocean, 1, 2, 4, Some(p), None);
            p = sl!(L_BIOME_256, map_biome, 1, 0, 200, Some(p), None);
            if mc >= MC_1_14 {
                p = sl!(L_BAMBOO_256, map_bamboo, 1, 0, 1001, Some(p), None);
            }
            p = sl!(L_ZOOM_128, map_zoom, 2, 3, 1000, Some(p), None);
            p = sl!(L_ZOOM_64, map_zoom, 2, 3, 1001, Some(p), None);
            sl!(L_BIOME_EDGE_64, map_biome_edge, 1, 2, 1000, Some(p), None);
            // river noise layer chain, also used to determine where hills generate
            p = sl!(
                L_RIVER_INIT_256,
                map_noise,
                1,
                0,
                100,
                Some(L_DEEP_OCEAN_256),
                None
            );
        }

        if mc <= MC_1_0 {
        } else if mc <= MC_1_12 {
            p = sl!(L_ZOOM_128_HILLS, map_zoom, 2, 3, 0, Some(p), None);
            sl!(L_ZOOM_64_HILLS, map_zoom, 2, 3, 0, Some(p), None);
        } else {
            p = sl!(L_ZOOM_128_HILLS, map_zoom, 2, 3, 1000, Some(p), None);
            sl!(L_ZOOM_64_HILLS, map_zoom, 2, 3, 1001, Some(p), None);
        }

        if mc <= MC_1_0 {
            p = sl!(L_ZOOM_32, map_zoom, 2, 3, 1000, Some(L_ZOOM_64), None);
            p = sl!(L_LAND_32, map_land, 1, 2, 3, Some(p), None);
            // NOTE: reusing slot for shore:16, but scale is 1:32
            p = sl!(L_SHORE_16, map_shore, 1, 2, 1000, Some(p), None);
            p = sl!(L_ZOOM_16, map_zoom, 2, 3, 1001, Some(p), None);
            p = sl!(L_ZOOM_8, map_zoom, 2, 3, 1002, Some(p), None);
            p = sl!(L_ZOOM_4, map_zoom, 2, 3, 1003, Some(p), None);

            sl!(L_SMOOTH_4, map_smooth, 1, 2, 1000, Some(p), None);

            // river layer chain
            p = sl!(
                L_ZOOM_128_RIVER,
                map_zoom,
                2,
                3,
                1000,
                Some(L_NOISE_256),
                None
            );
            p = sl!(L_ZOOM_64_RIVER, map_zoom, 2, 3, 1001, Some(p), None);
            p = sl!(L_ZOOM_32_RIVER, map_zoom, 2, 3, 1002, Some(p), None);
            p = sl!(L_ZOOM_16_RIVER, map_zoom, 2, 3, 1003, Some(p), None);
            p = sl!(L_ZOOM_8_RIVER, map_zoom, 2, 3, 1004, Some(p), None);
            p = sl!(L_ZOOM_4_RIVER, map_zoom, 2, 3, 1005, Some(p), None);

            p = sl!(L_RIVER_4, map_river, 1, 2, 1, Some(p), None);
            sl!(L_SMOOTH_4_RIVER, map_smooth, 1, 2, 1000, Some(p), None);
        } else if mc <= MC_1_6 {
            p = sl!(
                L_HILLS_64,
                map_hills,
                1,
                2,
                1000,
                Some(L_ZOOM_64),
                Some(L_ZOOM_64_HILLS)
            );

            p = sl!(L_ZOOM_32, map_zoom, 2, 3, 1000, Some(p), None);
            p = sl!(L_LAND_32, map_land, 1, 2, 3, Some(p), None);
            p = sl!(L_ZOOM_16, map_zoom, 2, 3, 1001, Some(p), None);
            p = sl!(L_SHORE_16, map_shore, 1, 2, 1000, Some(p), None);
            p = sl!(L_SWAMP_RIVER_16, map_swamp_river, 1, 0, 1000, Some(p), None);
            p = sl!(L_ZOOM_8, map_zoom, 2, 3, 1002, Some(p), None);
            p = sl!(L_ZOOM_4, map_zoom, 2, 3, 1003, Some(p), None);

            if large_biomes {
                p = sl!(L_ZOOM_LARGE_A, map_zoom, 2, 3, 1004, Some(p), None);
                p = sl!(L_ZOOM_LARGE_B, map_zoom, 2, 3, 1005, Some(p), None);
            }

            sl!(L_SMOOTH_4, map_smooth, 1, 2, 1000, Some(p), None);

            // river layer chain
            p = sl!(
                L_ZOOM_128_RIVER,
                map_zoom,
                2,
                3,
                1000,
                Some(L_NOISE_256),
                None
            );
            p = sl!(L_ZOOM_64_RIVER, map_zoom, 2, 3, 1001, Some(p), None);
            p = sl!(L_ZOOM_32_RIVER, map_zoom, 2, 3, 1002, Some(p), None);
            p = sl!(L_ZOOM_16_RIVER, map_zoom, 2, 3, 1003, Some(p), None);
            p = sl!(L_ZOOM_8_RIVER, map_zoom, 2, 3, 1004, Some(p), None);
            p = sl!(L_ZOOM_4_RIVER, map_zoom, 2, 3, 1005, Some(p), None);

            if large_biomes {
                p = sl!(L_ZOOM_L_RIVER_A, map_zoom, 2, 3, 1006, Some(p), None);
                p = sl!(L_ZOOM_L_RIVER_B, map_zoom, 2, 3, 1007, Some(p), None);
            }

            p = sl!(L_RIVER_4, map_river, 1, 2, 1, Some(p), None);
            sl!(L_SMOOTH_4_RIVER, map_smooth, 1, 2, 1000, Some(p), None);
        } else {
            // mc >= MC_1_7
            p = sl!(
                L_HILLS_64,
                map_hills,
                1,
                2,
                1000,
                Some(L_BIOME_EDGE_64),
                Some(L_ZOOM_64_HILLS)
            );

            p = sl!(L_SUNFLOWER_64, map_sunflower, 1, 0, 1001, Some(p), None);
            p = sl!(L_ZOOM_32, map_zoom, 2, 3, 1000, Some(p), None);
            p = sl!(L_LAND_32, map_land, 1, 2, 3, Some(p), None);
            p = sl!(L_ZOOM_16, map_zoom, 2, 3, 1001, Some(p), None);
            p = sl!(L_SHORE_16, map_shore, 1, 2, 1000, Some(p), None);
            p = sl!(L_ZOOM_8, map_zoom, 2, 3, 1002, Some(p), None);
            p = sl!(L_ZOOM_4, map_zoom, 2, 3, 1003, Some(p), None);

            if large_biomes {
                p = sl!(L_ZOOM_LARGE_A, map_zoom, 2, 3, 1004, Some(p), None);
                p = sl!(L_ZOOM_LARGE_B, map_zoom, 2, 3, 1005, Some(p), None);
            }

            sl!(L_SMOOTH_4, map_smooth, 1, 2, 1000, Some(p), None);

            // river layer chain
            p = sl!(
                L_ZOOM_128_RIVER,
                map_zoom,
                2,
                3,
                1000,
                Some(L_RIVER_INIT_256),
                None
            );
            p = sl!(L_ZOOM_64_RIVER, map_zoom, 2, 3, 1001, Some(p), None);
            p = sl!(L_ZOOM_32_RIVER, map_zoom, 2, 3, 1000, Some(p), None);
            p = sl!(L_ZOOM_16_RIVER, map_zoom, 2, 3, 1001, Some(p), None);
            p = sl!(L_ZOOM_8_RIVER, map_zoom, 2, 3, 1002, Some(p), None);
            p = sl!(L_ZOOM_4_RIVER, map_zoom, 2, 3, 1003, Some(p), None);

            if large_biomes && mc == MC_1_7 {
                p = sl!(L_ZOOM_L_RIVER_A, map_zoom, 2, 3, 1004, Some(p), None);
                p = sl!(L_ZOOM_L_RIVER_B, map_zoom, 2, 3, 1005, Some(p), None);
            }

            p = sl!(L_RIVER_4, map_river, 1, 2, 1, Some(p), None);
            sl!(L_SMOOTH_4_RIVER, map_smooth, 1, 2, 1000, Some(p), None);
        }

        p = sl!(
            L_RIVER_MIX_4,
            map_river_mix,
            1,
            0,
            100,
            Some(L_SMOOTH_4),
            Some(L_SMOOTH_4_RIVER)
        );

        if mc <= MC_1_12 {
            p = sl!(L_VORONOI_1, map_voronoi114, 4, 3, 10, Some(p), None);
        } else {
            // ocean variants
            p = sl!(L_OCEAN_TEMP_256, map_ocean_temp, 1, 0, 2, None, None);
            g.layers[p].noise = true;
            p = sl!(L_ZOOM_128_OCEAN, map_zoom, 2, 3, 2001, Some(p), None);
            p = sl!(L_ZOOM_64_OCEAN, map_zoom, 2, 3, 2002, Some(p), None);
            p = sl!(L_ZOOM_32_OCEAN, map_zoom, 2, 3, 2003, Some(p), None);
            p = sl!(L_ZOOM_16_OCEAN, map_zoom, 2, 3, 2004, Some(p), None);
            p = sl!(L_ZOOM_8_OCEAN, map_zoom, 2, 3, 2005, Some(p), None);
            sl!(L_ZOOM_4_OCEAN, map_zoom, 2, 3, 2006, Some(p), None);
            p = sl!(
                L_OCEAN_MIX_4,
                map_ocean_mix,
                1,
                17,
                100,
                Some(L_RIVER_MIX_4),
                Some(L_ZOOM_4_OCEAN)
            );

            if mc <= MC_1_14 {
                p = sl!(L_VORONOI_1, map_voronoi114, 4, 3, 10, Some(p), None);
            } else {
                p = sl!(
                    L_VORONOI_1,
                    map_voronoi,
                    4,
                    3,
                    LAYER_INIT_SHA,
                    Some(p),
                    None
                );
            }
        }

        g.entry_1 = p;
        g.entry_4 = if mc <= MC_1_12 {
            L_RIVER_MIX_4
        } else {
            L_OCEAN_MIX_4
        };
        if large_biomes {
            g.entry_16 = L_ZOOM_4;
            g.entry_64 = if mc <= MC_1_6 {
                L_SWAMP_RIVER_16
            } else {
                L_SHORE_16
            };
            g.entry_256 = if mc <= MC_1_6 {
                L_HILLS_64
            } else {
                L_SUNFLOWER_64
            };
        } else if mc >= MC_1_1 {
            g.entry_16 = if mc <= MC_1_6 {
                L_SWAMP_RIVER_16
            } else {
                L_SHORE_16
            };
            g.entry_64 = if mc <= MC_1_6 {
                L_HILLS_64
            } else {
                L_SUNFLOWER_64
            };
            g.entry_256 = if mc <= MC_1_14 {
                L_BIOME_256
            } else {
                L_BAMBOO_256
            };
        } else {
            g.entry_16 = L_ZOOM_16;
            g.entry_64 = L_ZOOM_64;
            g.entry_256 = L_BIOME_256;
        }
        let e1 = g.entry_1;
        g.setup_scale(e1, 1);
        g
    }

    /// The `FORCE_OCEAN_VARIANTS` part of `setupGenerator`: adds the custom
    /// ocean-mixing entry layers at 1:16, 1:64 and 1:256 (indices `L_NUM + 2`
    /// to `L_NUM + 4`, as `xlayer + 2..4` in C) for `mc >= MC_1_13`.
    pub fn force_ocean_variants(&mut self, mc: i32) {
        if mc < MC_1_13 {
            return;
        }
        self.entry_16 = self.setup_layer(
            L_NUM + 2,
            map_ocean_mix_mod,
            mc,
            1,
            0,
            0,
            Some(self.entry_16),
            Some(L_ZOOM_16_OCEAN),
        );
        self.entry_64 = self.setup_layer(
            L_NUM + 3,
            map_ocean_mix_mod,
            mc,
            1,
            0,
            0,
            Some(self.entry_64),
            Some(L_ZOOM_64_OCEAN),
        );
        self.entry_256 = self.setup_layer(
            L_NUM + 4,
            map_ocean_mix_mod,
            mc,
            1,
            0,
            0,
            Some(self.entry_256),
            Some(L_OCEAN_TEMP_256),
        );
    }

    /// `getMaxArea`: recursively calculates the buffer size required to
    /// generate an area from layer `layer` onwards.
    fn max_area(
        &self,
        layer: Option<usize>,
        mut area_x: i32,
        mut area_z: i32,
        max_x: &mut i32,
        max_z: &mut i32,
        siz: &mut usize,
    ) {
        let Some(idx) = layer else {
            return;
        };
        let l = &self.layers[idx];
        area_x += l.edge;
        area_z += l.edge;

        // multi-layers and zoom-layers use a temporary copy of their parent area
        if l.p2.is_some() || l.zoom != 1 {
            *siz += (area_x * area_z) as usize;
        }
        if area_x > *max_x {
            *max_x = area_x;
        }
        if area_z > *max_z {
            *max_z = area_z;
        }
        if l.zoom == 2 {
            area_x >>= 1;
            area_z >>= 1;
        } else if l.zoom == 4 {
            area_x >>= 2;
            area_z >>= 2;
        }
        self.max_area(l.p, area_x, area_z, max_x, max_z, siz);
        if l.p2.is_some() {
            self.max_area(l.p2, area_x, area_z, max_x, max_z, siz);
        }
    }

    /// `getMinLayerCacheSize`: the minimum buffer length for an area of
    /// `size_x` x `size_z` cells generated by layer `layer`.
    pub fn min_layer_cache_size(&self, layer: usize, size_x: i32, size_z: i32) -> usize {
        let (mut max_x, mut max_z) = (size_x, size_z);
        let mut bufsiz = 0usize;
        self.max_area(
            Some(layer),
            size_x,
            size_z,
            &mut max_x,
            &mut max_z,
            &mut bufsiz,
        );
        bufsiz + max_x as usize * max_z as usize
    }

    /// `genArea`: generates the `area_width` x `area_height` cells at
    /// (`area_x`, `area_z`) of layer `layer` into `out` (which must be at
    /// least [`min_layer_cache_size`](Self::min_layer_cache_size) long).
    pub fn gen_area(
        &self,
        layer: usize,
        out: &mut [i32],
        area_x: i32,
        area_z: i32,
        area_width: i32,
        area_height: i32,
    ) -> i32 {
        let n = area_width as usize * area_height as usize;
        out[..n].fill(0);
        self.get_map(layer, out, area_x, area_z, area_width, area_height)
    }

    /// Generates an area into a freshly allocated buffer of exactly
    /// `area_width * area_height` cells; `None` on error.
    pub fn gen_area_vec(
        &self,
        layer: usize,
        area_x: i32,
        area_z: i32,
        area_width: i32,
        area_height: i32,
    ) -> Option<Vec<i32>> {
        let mut cache = vec![0; self.min_layer_cache_size(layer, area_width, area_height)];
        let n = area_width as usize * area_height as usize;
        if cache.len() < n {
            cache.resize(n, 0);
        }
        if self.gen_area(layer, &mut cache, area_x, area_z, area_width, area_height) != 0 {
            return None;
        }
        cache.truncate(n);
        Some(cache)
    }
}

// ------------------------------------------------------------- layers --

/// Convenience function used in several layers.
#[inline]
fn is_any4(id: i32, a: i32, b: i32, c: i32, d: i32) -> bool {
    id == a || id == b || id == c || id == d
}

/// Index of the cell (`i`, `j`) in a row-major buffer of width `w`.
#[inline]
fn ix(i: usize, j: usize, w: usize) -> usize {
    i + j * w
}

/// `mapContinent` (old name `mapIsland`).
pub fn map_continent(
    _ls: &LayerStack,
    l: &Layer,
    out: &mut [i32],
    x: i32,
    z: i32,
    w: i32,
    h: i32,
) -> i32 {
    let ss = l.start_seed;
    for j in 0..h {
        for i in 0..w {
            let cs = chunk_seed(ss, i + x, j + z);
            out[(j as i64 * w as i64 + i as i64) as usize] = mc_first_is_zero(cs, 10) as i32;
        }
    }
    if x > -w && x <= 0 && z > -h && z <= 0 {
        out[(-(z as i64) * w as i64 - x as i64) as usize] = 1;
    }
    0
}

/// The zoom layers: `mapZoomFuzzy` (`fuzzy`) and `mapZoom`.
#[allow(clippy::too_many_arguments)]
fn zoom_impl(
    ls: &LayerStack,
    l: &Layer,
    out: &mut [i32],
    x: i32,
    z: i32,
    w: i32,
    h: i32,
    fuzzy: bool,
) -> i32 {
    let p_x = x >> 1;
    let p_z = z >> 1;
    let p_w = (((x + w) >> 1) - p_x + 1) as usize;
    let p_h = (((z + h) >> 1) - p_z + 1) as usize;

    let err = ls.get_map(l.p.unwrap(), out, p_x, p_z, p_w as i32, p_h as i32);
    if err != 0 {
        return err;
    }

    let new_w = p_w * 2;
    let buf = p_w * p_h; // offset of the scratch area in `out`

    let st = l.start_salt as u32;
    let ss = l.start_seed as u32;

    for j in 0..p_h {
        let mut idx = (j * 2) * new_w;

        let mut v00 = out[j * p_w];
        let mut v01 = out[(j + 1) * p_w];

        for i in 0..p_w {
            let v10 = out[i + 1 + j * p_w];
            let v11 = out[i + 1 + (j + 1) * p_w];

            'cell: {
                if v00 == v01 && v00 == v10 && v00 == v11 {
                    out[buf + idx] = v00;
                    out[buf + idx + 1] = v00;
                    out[buf + idx + new_w] = v00;
                    out[buf + idx + new_w + 1] = v00;
                    idx += 2;
                    break 'cell;
                }

                let chunk_x = (i as i32 + p_x).wrapping_mul(2);
                let chunk_z = (j as i32 + p_z).wrapping_mul(2);

                let step =
                    |cs: u32| cs.wrapping_mul(cs.wrapping_mul(1284865837).wrapping_add(4150755663));
                let mut cs = ss;
                cs = cs.wrapping_add(chunk_x as u32);
                cs = step(cs);
                cs = cs.wrapping_add(chunk_z as u32);
                cs = step(cs);
                cs = cs.wrapping_add(chunk_x as u32);
                cs = step(cs);
                cs = cs.wrapping_add(chunk_z as u32);

                out[buf + idx] = v00;
                out[buf + idx + new_w] = if (cs >> 24) & 1 != 0 { v01 } else { v00 };
                idx += 1;

                cs = step(cs);
                cs = cs.wrapping_add(st);
                out[buf + idx] = if (cs >> 24) & 1 != 0 { v10 } else { v00 };

                out[buf + idx + new_w] = if fuzzy {
                    cs = step(cs);
                    cs = cs.wrapping_add(st);
                    let r = (cs >> 24) & 3;
                    match r {
                        0 => v00,
                        1 => v10,
                        2 => v01,
                        _ => v11,
                    }
                } else {
                    select4(cs, st, v00, v01, v10, v11)
                };

                idx += 1;
            }
            v00 = v10;
            v01 = v11;
        }
    }

    let (wu, xo, zo) = (w as usize, (x & 1) as usize, (z & 1) as usize);
    for j in 0..h as usize {
        let src = buf + (j + zo) * new_w + xo;
        out.copy_within(src..src + wu, j * wu);
    }
    0
}

/// `mapZoomFuzzy`.
pub fn map_zoom_fuzzy(
    ls: &LayerStack,
    l: &Layer,
    out: &mut [i32],
    x: i32,
    z: i32,
    w: i32,
    h: i32,
) -> i32 {
    zoom_impl(ls, l, out, x, z, w, h, true)
}

#[inline]
fn select4(mut cs: u32, st: u32, v00: i32, v01: i32, v10: i32, v11: i32) -> i32 {
    let cv00 = (v00 == v10) as i32 + (v00 == v01) as i32 + (v00 == v11) as i32;
    let cv10 = (v10 == v01) as i32 + (v10 == v11) as i32;
    let cv01 = (v01 == v11) as i32;
    if cv00 > cv10 && cv00 > cv01 {
        v00
    } else if cv10 > cv00 {
        v10
    } else if cv01 > cv00 {
        v01
    } else {
        cs = cs.wrapping_mul(cs.wrapping_mul(1284865837).wrapping_add(4150755663));
        cs = cs.wrapping_add(st);
        let r = (cs >> 24) & 3;
        match r {
            0 => v00,
            1 => v10,
            2 => v01,
            _ => v11,
        }
    }
}

/// `mapZoom`: the most common layer.
pub fn map_zoom(
    ls: &LayerStack,
    l: &Layer,
    out: &mut [i32],
    x: i32,
    z: i32,
    w: i32,
    h: i32,
) -> i32 {
    zoom_impl(ls, l, out, x, z, w, h, false)
}

/// `mapLand` (old name `mapAddIsland`).
pub fn map_land_fn(
    ls: &LayerStack,
    l: &Layer,
    out: &mut [i32],
    x: i32,
    z: i32,
    w: i32,
    h: i32,
) -> i32 {
    let p_w = w as usize + 2;
    let err = ls.get_map(l.p.unwrap(), out, x - 1, z - 1, w + 2, h + 2);
    if err != 0 {
        return err;
    }

    let st = l.start_salt;
    let ss = l.start_seed;
    let wu = w as usize;

    for j in 0..h as usize {
        let z0 = j * p_w;
        let z1 = (j + 1) * p_w;
        let z2 = (j + 2) * p_w;

        let mut v00 = out[z0];
        let mut vt0 = out[z0 + 1];
        let mut v02 = out[z2];
        let mut vt2 = out[z2 + 1];

        for i in 0..wu {
            let v11 = out[z1 + i + 1];
            let v20 = out[z0 + i + 2];
            let v22 = out[z2 + i + 2];
            let mut v = v11;

            match v11 {
                OCEAN => {
                    if v00 != 0 || v20 != 0 || v02 != 0 || v22 != 0 {
                        // corners have non-ocean
                        let mut cs = chunk_seed(ss, i as i32 + x, j as i32 + z);
                        let mut inc = 0;
                        v = 1;

                        if v00 != OCEAN {
                            inc += 1;
                            v = v00;
                            cs = mc_step_seed(cs, st);
                        }
                        if v20 != OCEAN {
                            inc += 1;
                            if inc == 1 || mc_first_is_zero(cs, 2) {
                                v = v20;
                            }
                            cs = mc_step_seed(cs, st);
                        }
                        if v02 != OCEAN {
                            inc += 1;
                            match inc {
                                1 => v = v02,
                                2 => {
                                    if mc_first_is_zero(cs, 2) {
                                        v = v02;
                                    }
                                }
                                _ => {
                                    if mc_first_is_zero(cs, 3) {
                                        v = v02;
                                    }
                                }
                            }
                            cs = mc_step_seed(cs, st);
                        }
                        if v22 != OCEAN {
                            inc += 1;
                            match inc {
                                1 => v = v22,
                                2 => {
                                    if mc_first_is_zero(cs, 2) {
                                        v = v22;
                                    }
                                }
                                3 => {
                                    if mc_first_is_zero(cs, 3) {
                                        v = v22;
                                    }
                                }
                                _ => {
                                    if mc_first_is_zero(cs, 4) {
                                        v = v22;
                                    }
                                }
                            }
                            cs = mc_step_seed(cs, st);
                        }

                        if v != FOREST && !mc_first_is_zero(cs, 3) {
                            v = OCEAN;
                        }
                    }
                }
                FOREST => {}
                _ => {
                    if v00 == 0 || v20 == 0 || v02 == 0 || v22 == 0 {
                        let cs = chunk_seed(ss, i as i32 + x, j as i32 + z);
                        if mc_first_is_zero(cs, 5) {
                            v = 0;
                        }
                    }
                }
            }

            out[ix(i, j, wu)] = v;
            v00 = vt0;
            vt0 = v20;
            v02 = vt2;
            vt2 = v22;
        }
    }
    0
}

/// `mapLand16`.
pub fn map_land16(
    ls: &LayerStack,
    l: &Layer,
    out: &mut [i32],
    x: i32,
    z: i32,
    w: i32,
    h: i32,
) -> i32 {
    let p_w = w as usize + 2;
    let err = ls.get_map(l.p.unwrap(), out, x - 1, z - 1, w + 2, h + 2);
    if err != 0 {
        return err;
    }

    let st = l.start_salt;
    let ss = l.start_seed;
    let wu = w as usize;

    for j in 0..h as usize {
        let z0 = j * p_w;
        let z1 = (j + 1) * p_w;
        let z2 = (j + 2) * p_w;

        let mut v00 = out[z0];
        let mut vt0 = out[z0 + 1];
        let mut v02 = out[z2];
        let mut vt2 = out[z2 + 1];

        for i in 0..wu {
            let v11 = out[z1 + i + 1];
            let v20 = out[z0 + i + 2];
            let v22 = out[z2 + i + 2];
            let mut v = v11;

            if v11 != 0 || (v00 == 0 && v20 == 0 && v02 == 0 && v22 == 0) {
                if v11 != 0 && (v00 == 0 || v20 == 0 || v02 == 0 || v22 == 0) {
                    let cs = chunk_seed(ss, i as i32 + x, j as i32 + z);
                    if mc_first_is_zero(cs, 5) {
                        v = if v == SNOWY_TUNDRA {
                            FROZEN_OCEAN
                        } else {
                            OCEAN
                        };
                    }
                }
            } else {
                let mut cs = chunk_seed(ss, i as i32 + x, j as i32 + z);
                let mut inc = 0;
                v = 1;

                if v00 != OCEAN {
                    inc += 1;
                    v = v00;
                    cs = mc_step_seed(cs, st);
                }
                if v20 != OCEAN {
                    inc += 1;
                    if inc == 1 || mc_first_is_zero(cs, 2) {
                        v = v20;
                    }
                    cs = mc_step_seed(cs, st);
                }
                if v02 != OCEAN {
                    inc += 1;
                    match inc {
                        1 => v = v02,
                        2 => {
                            if mc_first_is_zero(cs, 2) {
                                v = v02;
                            }
                        }
                        _ => {
                            if mc_first_is_zero(cs, 3) {
                                v = v02;
                            }
                        }
                    }
                    cs = mc_step_seed(cs, st);
                }
                if v22 != OCEAN {
                    inc += 1;
                    match inc {
                        1 => v = v22,
                        2 => {
                            if mc_first_is_zero(cs, 2) {
                                v = v22;
                            }
                        }
                        3 => {
                            if mc_first_is_zero(cs, 3) {
                                v = v22;
                            }
                        }
                        _ => {
                            if mc_first_is_zero(cs, 4) {
                                v = v22;
                            }
                        }
                    }
                    cs = mc_step_seed(cs, st);
                }

                if !mc_first_is_zero(cs, 3) {
                    v = if v == SNOWY_TUNDRA {
                        FROZEN_OCEAN
                    } else {
                        OCEAN
                    };
                }
            }

            out[ix(i, j, wu)] = v;
            v00 = vt0;
            vt0 = v20;
            v02 = vt2;
            vt2 = v22;
        }
    }
    0
}

/// `mapLandB18`.
pub fn map_land_b18(
    ls: &LayerStack,
    l: &Layer,
    out: &mut [i32],
    x: i32,
    z: i32,
    w: i32,
    h: i32,
) -> i32 {
    let p_w = w as usize + 2;
    let err = ls.get_map(l.p.unwrap(), out, x - 1, z - 1, w + 2, h + 2);
    if err != 0 {
        return err;
    }

    let ss = l.start_seed;
    let wu = w as usize;

    for j in 0..h as usize {
        let z0 = j * p_w;
        let z1 = (j + 1) * p_w;
        let z2 = (j + 2) * p_w;

        let mut v00 = out[z0];
        let mut vt0 = out[z0 + 1];
        let mut v02 = out[z2];
        let mut vt2 = out[z2 + 1];

        for i in 0..wu {
            let v11 = out[z1 + i + 1];
            let v20 = out[z0 + i + 2];
            let v22 = out[z2 + i + 2];
            let mut v = v11;

            if v11 == 0 && (v00 != 0 || v02 != 0 || v20 != 0 || v22 != 0) {
                let cs = chunk_seed(ss, i as i32 + x, j as i32 + z);
                v = mc_first_int(cs, 3) / 2;
            } else if v11 == 1 && (v00 != 1 || v02 != 1 || v20 != 1 || v22 != 1) {
                let cs = chunk_seed(ss, i as i32 + x, j as i32 + z);
                v = 1 - mc_first_int(cs, 5) / 4;
            }

            out[ix(i, j, wu)] = v;
            v00 = vt0;
            vt0 = v20;
            v02 = vt2;
            vt2 = v22;
        }
    }
    0
}

/// `mapIsland` (old name `mapRemoveTooMuchOcean`).
pub fn map_island(
    ls: &LayerStack,
    l: &Layer,
    out: &mut [i32],
    x: i32,
    z: i32,
    w: i32,
    h: i32,
) -> i32 {
    let p_w = w as usize + 2;
    let err = ls.get_map(l.p.unwrap(), out, x - 1, z - 1, w + 2, h + 2);
    if err != 0 {
        return err;
    }

    let ss = l.start_seed;
    let wu = w as usize;

    for j in 0..h as usize {
        for i in 0..wu {
            let v11 = out[i + 1 + (j + 1) * p_w];
            out[ix(i, j, wu)] = v11;

            if v11 == OCEANIC {
                if out[i + 1 + j * p_w] != OCEANIC {
                    continue;
                }
                if out[i + 2 + (j + 1) * p_w] != OCEANIC {
                    continue;
                }
                if out[i + (j + 1) * p_w] != OCEANIC {
                    continue;
                }
                if out[i + 1 + (j + 2) * p_w] != OCEANIC {
                    continue;
                }

                let cs = chunk_seed(ss, i as i32 + x, j as i32 + z);
                if mc_first_is_zero(cs, 2) {
                    out[ix(i, j, wu)] = 1;
                }
            }
        }
    }
    0
}

/// `mapSnow16`.
pub fn map_snow16(
    ls: &LayerStack,
    l: &Layer,
    out: &mut [i32],
    x: i32,
    z: i32,
    w: i32,
    h: i32,
) -> i32 {
    let p_w = w as usize + 2;
    let err = ls.get_map(l.p.unwrap(), out, x - 1, z - 1, w + 2, h + 2);
    if err != 0 {
        return err;
    }

    let ss = l.start_seed;
    let wu = w as usize;

    for j in 0..h as usize {
        for i in 0..wu {
            let mut v11 = out[i + 1 + (j + 1) * p_w];
            if v11 != OCEAN {
                let cs = chunk_seed(ss, i as i32 + x, j as i32 + z);
                v11 = if mc_first_is_zero(cs, 5) {
                    SNOWY_TUNDRA
                } else {
                    PLAINS
                };
            }
            out[ix(i, j, wu)] = v11;
        }
    }
    0
}

/// `mapSnow` (old name `mapAddSnow`).
pub fn map_snow(
    ls: &LayerStack,
    l: &Layer,
    out: &mut [i32],
    x: i32,
    z: i32,
    w: i32,
    h: i32,
) -> i32 {
    let p_w = w as usize + 2;
    let err = ls.get_map(l.p.unwrap(), out, x - 1, z - 1, w + 2, h + 2);
    if err != 0 {
        return err;
    }

    let ss = l.start_seed;
    let wu = w as usize;

    for j in 0..h as usize {
        for i in 0..wu {
            let mut v11 = out[i + 1 + (j + 1) * p_w];
            if !is_shallow_ocean(v11) {
                let cs = chunk_seed(ss, i as i32 + x, j as i32 + z);
                let r = mc_first_int(cs, 6);
                v11 = if r == 0 {
                    FREEZING
                } else if r <= 1 {
                    COLD
                } else {
                    WARM
                };
            }
            out[ix(i, j, wu)] = v11;
        }
    }
    0
}

/// `mapCool` (old name `mapCoolWarm`).
pub fn map_cool(
    ls: &LayerStack,
    l: &Layer,
    out: &mut [i32],
    x: i32,
    z: i32,
    w: i32,
    h: i32,
) -> i32 {
    let p_w = w as usize + 2;
    let err = ls.get_map(l.p.unwrap(), out, x - 1, z - 1, w + 2, h + 2);
    if err != 0 {
        return err;
    }
    let wu = w as usize;

    for j in 0..h as usize {
        for i in 0..wu {
            let mut v11 = out[i + 1 + (j + 1) * p_w];

            if v11 == WARM {
                let v10 = out[i + 1 + j * p_w];
                let v21 = out[i + 2 + (j + 1) * p_w];
                let v01 = out[i + (j + 1) * p_w];
                let v12 = out[i + 1 + (j + 2) * p_w];

                if is_any4(COLD, v10, v21, v01, v12) || is_any4(FREEZING, v10, v21, v01, v12) {
                    v11 = LUSH;
                }
            }
            out[ix(i, j, wu)] = v11;
        }
    }
    0
}

/// `mapHeat` (old name `mapHeatIce`).
pub fn map_heat(
    ls: &LayerStack,
    l: &Layer,
    out: &mut [i32],
    x: i32,
    z: i32,
    w: i32,
    h: i32,
) -> i32 {
    let p_w = w as usize + 2;
    let err = ls.get_map(l.p.unwrap(), out, x - 1, z - 1, w + 2, h + 2);
    if err != 0 {
        return err;
    }
    let wu = w as usize;

    for j in 0..h as usize {
        for i in 0..wu {
            let mut v11 = out[i + 1 + (j + 1) * p_w];

            if v11 == FREEZING {
                let v10 = out[i + 1 + j * p_w];
                let v21 = out[i + 2 + (j + 1) * p_w];
                let v01 = out[i + (j + 1) * p_w];
                let v12 = out[i + 1 + (j + 2) * p_w];

                if is_any4(WARM, v10, v21, v01, v12) || is_any4(LUSH, v10, v21, v01, v12) {
                    v11 = COLD;
                }
            }
            out[ix(i, j, wu)] = v11;
        }
    }
    0
}

/// `mapSpecial`.
pub fn map_special(
    ls: &LayerStack,
    l: &Layer,
    out: &mut [i32],
    x: i32,
    z: i32,
    w: i32,
    h: i32,
) -> i32 {
    let err = ls.get_map(l.p.unwrap(), out, x, z, w, h);
    if err != 0 {
        return err;
    }

    let st = l.start_salt;
    let ss = l.start_seed;
    let wu = w as usize;

    for j in 0..h as usize {
        for i in 0..wu {
            let mut v = out[ix(i, j, wu)];
            if v == OCEANIC {
                continue;
            }

            let mut cs = chunk_seed(ss, i as i32 + x, j as i32 + z);

            if mc_first_is_zero(cs, 13) {
                cs = mc_step_seed(cs, st);
                v |= (((1 + mc_first_int(cs, 15)) as u32) << 8 & 0xf00) as i32;
                // 1 to 1 mapping so 'out' can be overwritten immediately
                out[ix(i, j, wu)] = v;
            }
        }
    }
    0
}

/// `mapMushroom` (old name `mapAddMushroomIsland`).
pub fn map_mushroom(
    ls: &LayerStack,
    l: &Layer,
    out: &mut [i32],
    x: i32,
    z: i32,
    w: i32,
    h: i32,
) -> i32 {
    let p_w = w as usize + 2;
    let err = ls.get_map(l.p.unwrap(), out, x - 1, z - 1, w + 2, h + 2);
    if err != 0 {
        return err;
    }

    let ss = l.start_seed;
    let wu = w as usize;

    for j in 0..h as usize {
        for i in 0..wu {
            let mut v11 = out[i + 1 + (j + 1) * p_w];

            // surrounded by ocean?
            if v11 == 0
                && out[i + j * p_w] == 0
                && out[i + 2 + j * p_w] == 0
                && out[i + (j + 2) * p_w] == 0
                && out[i + 2 + (j + 2) * p_w] == 0
            {
                let cs = chunk_seed(ss, i as i32 + x, j as i32 + z);
                if mc_first_is_zero(cs, 100) {
                    v11 = MUSHROOM_FIELDS;
                }
            }
            out[ix(i, j, wu)] = v11;
        }
    }
    0
}

/// `mapDeepOcean`.
pub fn map_deep_ocean(
    ls: &LayerStack,
    l: &Layer,
    out: &mut [i32],
    x: i32,
    z: i32,
    w: i32,
    h: i32,
) -> i32 {
    let p_w = w as usize + 2;
    let err = ls.get_map(l.p.unwrap(), out, x - 1, z - 1, w + 2, h + 2);
    if err != 0 {
        return err;
    }
    let wu = w as usize;

    for j in 0..h as usize {
        for i in 0..wu {
            let mut v11 = out[(i + 1) + (j + 1) * p_w];

            if is_shallow_ocean(v11) {
                // count adjacent oceans
                let mut oceans = 0;
                if is_shallow_ocean(out[(i + 1) + j * p_w]) {
                    oceans += 1;
                }
                if is_shallow_ocean(out[(i + 2) + (j + 1) * p_w]) {
                    oceans += 1;
                }
                if is_shallow_ocean(out[i + (j + 1) * p_w]) {
                    oceans += 1;
                }
                if is_shallow_ocean(out[(i + 1) + (j + 2) * p_w]) {
                    oceans += 1;
                }

                if oceans >= 4 {
                    v11 = match v11 {
                        WARM_OCEAN => DEEP_WARM_OCEAN,
                        LUKEWARM_OCEAN => DEEP_LUKEWARM_OCEAN,
                        OCEAN => DEEP_OCEAN,
                        COLD_OCEAN => DEEP_COLD_OCEAN,
                        FROZEN_OCEAN => DEEP_FROZEN_OCEAN,
                        _ => DEEP_OCEAN,
                    };
                }
            }
            out[ix(i, j, wu)] = v11;
        }
    }
    0
}

const WARM_BIOMES: [i32; 6] = [DESERT, DESERT, DESERT, SAVANNA, SAVANNA, PLAINS];
const LUSH_BIOMES: [i32; 6] = [FOREST, DARK_FOREST, MOUNTAINS, PLAINS, BIRCH_FOREST, SWAMP];
const COLD_BIOMES: [i32; 4] = [FOREST, MOUNTAINS, TAIGA, PLAINS];
const SNOW_BIOMES: [i32; 4] = [SNOWY_TUNDRA, SNOWY_TUNDRA, SNOWY_TUNDRA, SNOWY_TAIGA];

const OLD_BIOMES: [i32; 7] = [DESERT, FOREST, MOUNTAINS, SWAMP, PLAINS, TAIGA, JUNGLE];
const OLD_BIOMES11: [i32; 6] = [DESERT, FOREST, MOUNTAINS, SWAMP, PLAINS, TAIGA];

/// `mapBiome`.
pub fn map_biome(
    ls: &LayerStack,
    l: &Layer,
    out: &mut [i32],
    x: i32,
    z: i32,
    w: i32,
    h: i32,
) -> i32 {
    let err = ls.get_map(l.p.unwrap(), out, x, z, w, h);
    if err != 0 {
        return err;
    }

    let mc = l.mc;
    let ss = l.start_seed;
    let wu = w as usize;

    for j in 0..h as usize {
        for i in 0..wu {
            let idx = i + j * wu;
            let mut id = out[idx];
            let has_high_bit = id & 0xf00;
            id &= !0xf00;
            let v;

            if mc <= MC_1_6 {
                if id == OCEAN || id == MUSHROOM_FIELDS {
                    out[idx] = id;
                    continue;
                }

                let cs = chunk_seed(ss, i as i32 + x, j as i32 + z);

                let mut vv = if mc <= MC_1_1 {
                    OLD_BIOMES11[mc_first_int(cs, 6) as usize]
                } else {
                    OLD_BIOMES[mc_first_int(cs, 7) as usize]
                };

                if id != PLAINS && (vv != TAIGA || mc <= MC_1_2) {
                    vv = SNOWY_TUNDRA;
                }
                v = vv;
            } else {
                if is_oceanic(id) || id == MUSHROOM_FIELDS {
                    out[idx] = id;
                    continue;
                }

                let cs = chunk_seed(ss, i as i32 + x, j as i32 + z);

                v = match id {
                    WARM => {
                        if has_high_bit != 0 {
                            if mc_first_is_zero(cs, 3) {
                                BADLANDS_PLATEAU
                            } else {
                                WOODED_BADLANDS_PLATEAU
                            }
                        } else {
                            WARM_BIOMES[mc_first_int(cs, 6) as usize]
                        }
                    }
                    LUSH => {
                        if has_high_bit != 0 {
                            JUNGLE
                        } else {
                            LUSH_BIOMES[mc_first_int(cs, 6) as usize]
                        }
                    }
                    COLD => {
                        if has_high_bit != 0 {
                            GIANT_TREE_TAIGA
                        } else {
                            COLD_BIOMES[mc_first_int(cs, 4) as usize]
                        }
                    }
                    FREEZING => SNOW_BIOMES[mc_first_int(cs, 4) as usize],
                    _ => MUSHROOM_FIELDS,
                };
            }

            out[idx] = v;
        }
    }
    0
}

/// `mapNoise` (old name `mapRiverInit`).
pub fn map_noise(
    ls: &LayerStack,
    l: &Layer,
    out: &mut [i32],
    x: i32,
    z: i32,
    w: i32,
    h: i32,
) -> i32 {
    let err = ls.get_map(l.p.unwrap(), out, x, z, w, h);
    if err != 0 {
        return err;
    }

    let ss = l.start_seed;
    let m = if l.mc <= MC_1_6 { 2 } else { 299999 };
    let wu = w as usize;

    for j in 0..h as usize {
        for i in 0..wu {
            let idx = i + j * wu;
            if out[idx] > 0 {
                let cs = chunk_seed(ss, i as i32 + x, j as i32 + z);
                out[idx] = mc_first_int(cs, m) + 2;
            } else {
                out[idx] = 0;
            }
        }
    }
    0
}

/// `mapBamboo` (old name `mapAddBamboo`).
pub fn map_bamboo(
    ls: &LayerStack,
    l: &Layer,
    out: &mut [i32],
    x: i32,
    z: i32,
    w: i32,
    h: i32,
) -> i32 {
    let err = ls.get_map(l.p.unwrap(), out, x, z, w, h);
    if err != 0 {
        return err;
    }

    let ss = l.start_seed;
    let wu = w as usize;

    for j in 0..h as usize {
        for i in 0..wu {
            let idx = i + j * wu;
            if out[idx] != JUNGLE {
                continue;
            }
            let cs = chunk_seed(ss, i as i32 + x, j as i32 + z);
            if mc_first_is_zero(cs, 10) {
                out[idx] = BAMBOO_JUNGLE;
            }
        }
    }
    0
}

#[allow(clippy::too_many_arguments)]
#[inline]
fn replace_edge(
    out: &mut [i32],
    idx: usize,
    mc: i32,
    v10: i32,
    v21: i32,
    v01: i32,
    v12: i32,
    id: i32,
    base_id: i32,
    edge_id: i32,
) -> bool {
    if id != base_id {
        return false;
    }
    if are_similar(mc, v10, base_id)
        && are_similar(mc, v21, base_id)
        && are_similar(mc, v01, base_id)
        && are_similar(mc, v12, base_id)
    {
        out[idx] = id;
    } else {
        out[idx] = edge_id;
    }
    true
}

/// `mapBiomeEdge`.
pub fn map_biome_edge(
    ls: &LayerStack,
    l: &Layer,
    out: &mut [i32],
    x: i32,
    z: i32,
    w: i32,
    h: i32,
) -> i32 {
    let p_w = w as usize + 2;
    let mc = l.mc;
    let err = ls.get_map(l.p.unwrap(), out, x - 1, z - 1, w + 2, h + 2);
    if err != 0 {
        return err;
    }
    let wu = w as usize;

    for j in 0..h as usize {
        let z0 = j * p_w;
        let z1 = (j + 1) * p_w;
        let z2 = (j + 2) * p_w;

        for i in 0..wu {
            let v11 = out[z1 + i + 1];
            let v10 = out[z0 + i + 1];
            let v21 = out[z1 + i + 2];
            let v01 = out[z1 + i];
            let v12 = out[z2 + i + 1];
            let o = i + j * wu;

            if !replace_edge(
                out,
                o,
                mc,
                v10,
                v21,
                v01,
                v12,
                v11,
                WOODED_BADLANDS_PLATEAU,
                BADLANDS,
            ) && !replace_edge(
                out,
                o,
                mc,
                v10,
                v21,
                v01,
                v12,
                v11,
                BADLANDS_PLATEAU,
                BADLANDS,
            ) && !replace_edge(out, o, mc, v10, v21, v01, v12, v11, GIANT_TREE_TAIGA, TAIGA)
            {
                if v11 == DESERT {
                    if !is_any4(SNOWY_TUNDRA, v10, v21, v01, v12) {
                        out[o] = v11;
                    } else {
                        out[o] = WOODED_MOUNTAINS;
                    }
                } else if v11 == SWAMP {
                    if !is_any4(DESERT, v10, v21, v01, v12)
                        && !is_any4(SNOWY_TAIGA, v10, v21, v01, v12)
                        && !is_any4(SNOWY_TUNDRA, v10, v21, v01, v12)
                    {
                        if !is_any4(JUNGLE, v10, v21, v01, v12)
                            && !is_any4(BAMBOO_JUNGLE, v10, v21, v01, v12)
                        {
                            out[o] = v11;
                        } else {
                            out[o] = JUNGLE_EDGE;
                        }
                    } else {
                        out[o] = PLAINS;
                    }
                } else {
                    out[o] = v11;
                }
            }
        }
    }
    0
}

/// `mapHills`.
pub fn map_hills(
    ls: &LayerStack,
    l: &Layer,
    out: &mut [i32],
    x: i32,
    z: i32,
    w: i32,
    h: i32,
) -> i32 {
    let p_w = w as usize + 2;
    let p_h = h as usize + 2;

    let p2 = l.p2.expect("mapHills() requires two parents");
    let err = ls.get_map(l.p.unwrap(), out, x - 1, z - 1, p_w as i32, p_h as i32);
    if err != 0 {
        return err;
    }

    let riv = p_w * p_h;
    let err = ls.get_map2(p2, out, riv, x - 1, z - 1, p_w as i32, p_h as i32);
    if err != 0 {
        return err;
    }

    let mc = l.mc;
    let st = l.start_salt;
    let ss = l.start_seed;
    let wu = w as usize;

    for j in 0..h as usize {
        for i in 0..wu {
            let a11 = out[i + 1 + (j + 1) * p_w]; // biome branch
            let b11 = out[riv + i + 1 + (j + 1) * p_w]; // river branch
            let idx = i + j * wu;
            let mut bn = -1;

            if mc >= MC_1_7 {
                bn = (b11 - 2) % 29;
            }

            if bn == 1 && b11 >= 2 && !is_shallow_ocean(a11) {
                let m = get_mutated(mc, a11);
                if m > 0 {
                    out[idx] = m;
                } else {
                    out[idx] = a11;
                }
            } else {
                let mut cs = chunk_seed(ss, i as i32 + x, j as i32 + z);
                if bn == 0 || mc_first_is_zero(cs, 3) {
                    let mut hill_id = a11;

                    match a11 {
                        DESERT => hill_id = DESERT_HILLS,
                        FOREST => hill_id = WOODED_HILLS,
                        BIRCH_FOREST => hill_id = BIRCH_FOREST_HILLS,
                        DARK_FOREST => hill_id = PLAINS,
                        TAIGA => hill_id = TAIGA_HILLS,
                        GIANT_TREE_TAIGA => hill_id = GIANT_TREE_TAIGA_HILLS,
                        SNOWY_TAIGA => hill_id = SNOWY_TAIGA_HILLS,
                        PLAINS => {
                            if mc <= MC_1_6 {
                                hill_id = FOREST;
                            } else {
                                cs = mc_step_seed(cs, st);
                                hill_id = if mc_first_is_zero(cs, 3) {
                                    WOODED_HILLS
                                } else {
                                    FOREST
                                };
                            }
                        }
                        SNOWY_TUNDRA => hill_id = SNOWY_MOUNTAINS,
                        JUNGLE => hill_id = JUNGLE_HILLS,
                        BAMBOO_JUNGLE => hill_id = BAMBOO_JUNGLE_HILLS,
                        OCEAN => {
                            if mc >= MC_1_7 {
                                hill_id = DEEP_OCEAN;
                            }
                        }
                        MOUNTAINS => {
                            if mc >= MC_1_7 {
                                hill_id = WOODED_MOUNTAINS;
                            }
                        }
                        SAVANNA => hill_id = SAVANNA_PLATEAU,
                        _ => {
                            if are_similar(mc, a11, WOODED_BADLANDS_PLATEAU) {
                                hill_id = BADLANDS;
                            } else if is_deep_ocean(a11) {
                                cs = mc_step_seed(cs, st);
                                if mc_first_is_zero(cs, 3) {
                                    cs = mc_step_seed(cs, st);
                                    hill_id = if mc_first_is_zero(cs, 2) {
                                        PLAINS
                                    } else {
                                        FOREST
                                    };
                                }
                            }
                        }
                    }

                    if bn == 0 && hill_id != a11 {
                        hill_id = get_mutated(mc, hill_id);
                        if hill_id < 0 {
                            hill_id = a11;
                        }
                    }

                    if hill_id != a11 {
                        let a10 = out[i + 1 + j * p_w];
                        let a21 = out[i + 2 + (j + 1) * p_w];
                        let a01 = out[i + (j + 1) * p_w];
                        let a12 = out[i + 1 + (j + 2) * p_w];
                        let mut equals = 0;

                        if are_similar(mc, a10, a11) {
                            equals += 1;
                        }
                        if are_similar(mc, a21, a11) {
                            equals += 1;
                        }
                        if are_similar(mc, a01, a11) {
                            equals += 1;
                        }
                        if are_similar(mc, a12, a11) {
                            equals += 1;
                        }

                        if equals >= 3 + (mc <= MC_1_6) as i32 {
                            out[idx] = hill_id;
                        } else {
                            out[idx] = a11;
                        }
                    } else {
                        out[idx] = a11;
                    }
                } else {
                    out[idx] = a11;
                }
            }
        }
    }
    0
}

#[inline]
fn reduce_id(id: i32) -> i32 {
    if id >= 2 { 2 + (id & 1) } else { id }
}

/// `mapRiver`.
pub fn map_river(
    ls: &LayerStack,
    l: &Layer,
    out: &mut [i32],
    x: i32,
    z: i32,
    w: i32,
    h: i32,
) -> i32 {
    let p_w = w as usize + 2;
    let err = ls.get_map(l.p.unwrap(), out, x - 1, z - 1, w + 2, h + 2);
    if err != 0 {
        return err;
    }
    let mc = l.mc;
    let wu = w as usize;

    for j in 0..h as usize {
        let z0 = j * p_w;
        let z1 = (j + 1) * p_w;
        let z2 = (j + 2) * p_w;

        for i in 0..wu {
            let mut v01 = out[z1 + i];
            let mut v11 = out[z1 + i + 1];
            let mut v21 = out[z1 + i + 2];
            let mut v10 = out[z0 + i + 1];
            let mut v12 = out[z2 + i + 1];

            if mc >= MC_1_7 {
                v01 = reduce_id(v01);
                v11 = reduce_id(v11);
                v21 = reduce_id(v21);
                v10 = reduce_id(v10);
                v12 = reduce_id(v12);
            } else if v11 == 0 {
                out[i + j * wu] = RIVER;
                continue;
            }

            if v11 == v01 && v11 == v10 && v11 == v12 && v11 == v21 {
                out[i + j * wu] = -1;
            } else {
                out[i + j * wu] = RIVER;
            }
        }
    }
    0
}

/// `mapSmooth`.
pub fn map_smooth(
    ls: &LayerStack,
    l: &Layer,
    out: &mut [i32],
    x: i32,
    z: i32,
    w: i32,
    h: i32,
) -> i32 {
    let p_w = w as usize + 2;
    let err = ls.get_map(l.p.unwrap(), out, x - 1, z - 1, w + 2, h + 2);
    if err != 0 {
        return err;
    }
    let ss = l.start_seed;
    let wu = w as usize;

    for j in 0..h as usize {
        let z0 = j * p_w;
        let z1 = (j + 1) * p_w;
        let z2 = (j + 2) * p_w;

        for i in 0..wu {
            let mut v11 = out[z1 + i + 1];
            let v01 = out[z1 + i];
            let v10 = out[z0 + i + 1];

            if v11 != v01 || v11 != v10 {
                let v21 = out[z1 + i + 2];
                let v12 = out[z2 + i + 1];
                if v01 == v21 && v10 == v12 {
                    let cs = chunk_seed(ss, i as i32 + x, j as i32 + z);
                    if cs & (1u64 << 24) != 0 {
                        v11 = v10;
                    } else {
                        v11 = v01;
                    }
                } else {
                    if v01 == v21 {
                        v11 = v01;
                    }
                    if v10 == v12 {
                        v11 = v10;
                    }
                }
            }
            out[i + j * wu] = v11;
        }
    }
    0
}

/// `mapSunflower` (old name `mapRareBiome`).
pub fn map_sunflower(
    ls: &LayerStack,
    l: &Layer,
    out: &mut [i32],
    x: i32,
    z: i32,
    w: i32,
    h: i32,
) -> i32 {
    let err = ls.get_map(l.p.unwrap(), out, x, z, w, h);
    if err != 0 {
        return err;
    }
    let ss = l.start_seed;
    let wu = w as usize;

    for j in 0..h as usize {
        for i in 0..wu {
            let v = out[i + j * wu];
            if v == PLAINS {
                let cs = chunk_seed(ss, i as i32 + x, j as i32 + z);
                if mc_first_is_zero(cs, 57) {
                    out[i + j * wu] = SUNFLOWER_PLAINS;
                }
            }
        }
    }
    0
}

#[inline]
#[allow(clippy::too_many_arguments)]
fn replace_ocean(
    out: &mut [i32],
    idx: usize,
    v10: i32,
    v21: i32,
    v01: i32,
    v12: i32,
    id: i32,
    replace_id: i32,
) -> bool {
    if is_oceanic(id) {
        return false;
    }
    if is_oceanic(v10) || is_oceanic(v21) || is_oceanic(v01) || is_oceanic(v12) {
        out[idx] = replace_id;
    } else {
        out[idx] = id;
    }
    true
}

#[inline]
fn is_all4_jfto(mc: i32, a: i32, b: i32, c: i32, d: i32) -> bool {
    let ok = |v: i32| get_category(mc, v) == JUNGLE || v == FOREST || v == TAIGA || is_oceanic(v);
    ok(a) && ok(b) && ok(c) && ok(d)
}

#[inline]
fn is_any4_oceanic(a: i32, b: i32, c: i32, d: i32) -> bool {
    is_oceanic(a) || is_oceanic(b) || is_oceanic(c) || is_oceanic(d)
}

/// `mapShore`.
pub fn map_shore(
    ls: &LayerStack,
    l: &Layer,
    out: &mut [i32],
    x: i32,
    z: i32,
    w: i32,
    h: i32,
) -> i32 {
    let p_w = w as usize + 2;
    let err = ls.get_map(l.p.unwrap(), out, x - 1, z - 1, w + 2, h + 2);
    if err != 0 {
        return err;
    }
    let mc = l.mc;
    let wu = w as usize;

    for j in 0..h as usize {
        let z0 = j * p_w;
        let z1 = (j + 1) * p_w;
        let z2 = (j + 2) * p_w;

        for i in 0..wu {
            let mut v11 = out[z1 + i + 1];
            let v10 = out[z0 + i + 1];
            let v21 = out[z1 + i + 2];
            let v01 = out[z1 + i];
            let v12 = out[z2 + i + 1];
            let o = i + j * wu;

            if v11 == MUSHROOM_FIELDS {
                if is_any4(OCEAN, v10, v21, v01, v12) {
                    out[o] = MUSHROOM_FIELD_SHORE;
                } else {
                    out[o] = v11;
                }
                continue;
            }
            if mc <= MC_1_0 {
                out[o] = v11;
                continue;
            }

            if mc <= MC_1_6 {
                if v11 == MOUNTAINS {
                    if v10 != MOUNTAINS || v21 != MOUNTAINS || v01 != MOUNTAINS || v12 != MOUNTAINS
                    {
                        v11 = MOUNTAIN_EDGE;
                    }
                } else if v11 != OCEAN
                    && v11 != RIVER
                    && v11 != SWAMP
                    && is_any4(OCEAN, v10, v21, v01, v12)
                {
                    v11 = BEACH;
                }
                out[o] = v11;
            } else if get_category(mc, v11) == JUNGLE {
                if is_all4_jfto(mc, v10, v21, v01, v12) {
                    if is_any4_oceanic(v10, v21, v01, v12) {
                        out[o] = BEACH;
                    } else {
                        out[o] = v11;
                    }
                } else {
                    out[o] = JUNGLE_EDGE;
                }
            } else if v11 == MOUNTAINS || v11 == WOODED_MOUNTAINS {
                replace_ocean(out, o, v10, v21, v01, v12, v11, STONE_SHORE);
            } else if is_snowy(v11) {
                replace_ocean(out, o, v10, v21, v01, v12, v11, SNOWY_BEACH);
            } else if v11 == BADLANDS || v11 == WOODED_BADLANDS_PLATEAU {
                if !is_any4_oceanic(v10, v21, v01, v12) {
                    if is_mesa(v10) && is_mesa(v21) && is_mesa(v01) && is_mesa(v12) {
                        out[o] = v11;
                    } else {
                        out[o] = DESERT;
                    }
                } else {
                    out[o] = v11;
                }
            } else if v11 != OCEAN && v11 != DEEP_OCEAN && v11 != RIVER && v11 != SWAMP {
                if is_any4_oceanic(v10, v21, v01, v12) {
                    out[o] = BEACH;
                } else {
                    out[o] = v11;
                }
            } else {
                out[o] = v11;
            }
        }
    }
    0
}

/// `mapSwampRiver`.
pub fn map_swamp_river(
    ls: &LayerStack,
    l: &Layer,
    out: &mut [i32],
    x: i32,
    z: i32,
    w: i32,
    h: i32,
) -> i32 {
    let err = ls.get_map(l.p.unwrap(), out, x, z, w, h);
    if err != 0 {
        return err;
    }
    let ss = l.start_seed;
    let wu = w as usize;

    for j in 0..h as usize {
        for i in 0..wu {
            let mut v = out[i + j * wu];
            if v != SWAMP && v != JUNGLE && v != JUNGLE_HILLS {
                continue;
            }
            let cs = chunk_seed(ss, i as i32 + x, j as i32 + z);
            if mc_first_is_zero(cs, if v == SWAMP { 6 } else { 8 }) {
                v = RIVER;
            }
            out[i + j * wu] = v;
        }
    }
    0
}

/// `mapRiverMix`.
pub fn map_river_mix(
    ls: &LayerStack,
    l: &Layer,
    out: &mut [i32],
    x: i32,
    z: i32,
    w: i32,
    h: i32,
) -> i32 {
    let p2 = l.p2.expect("mapRiverMix() requires two parents");
    let err = ls.get_map(l.p.unwrap(), out, x, z, w, h); // biome chain
    if err != 0 {
        return err;
    }

    let len = w as usize * h as usize;
    let mc = l.mc;

    let err = ls.get_map2(p2, out, len, x, z, w, h); // rivers
    if err != 0 {
        return err;
    }

    for idx in 0..len {
        let mut v = out[idx];

        if out[len + idx] == RIVER && v != OCEAN && (mc <= MC_1_6 || !is_oceanic(v)) {
            if v == SNOWY_TUNDRA {
                v = FROZEN_RIVER;
            } else if v == MUSHROOM_FIELDS || v == MUSHROOM_FIELD_SHORE {
                v = MUSHROOM_FIELD_SHORE;
            } else {
                v = RIVER;
            }
        }
        out[idx] = v;
    }
    0
}

/// `mapOceanTemp`.
pub fn map_ocean_temp(
    ls: &LayerStack,
    _l: &Layer,
    out: &mut [i32],
    x: i32,
    z: i32,
    w: i32,
    h: i32,
) -> i32 {
    let rnd = ls
        .ocean_rnd
        .as_ref()
        .expect("the ocean noise needs a seed (set_layer_seed)");
    for j in 0..h {
        for i in 0..w {
            let tmp = rnd.sample((i + x) as f64 / 8.0, (j + z) as f64 / 8.0, 0.0, 0.0, 0.0);
            let o = (i + j * w) as usize;
            out[o] = if tmp > 0.4 {
                WARM_OCEAN
            } else if tmp > 0.2 {
                LUKEWARM_OCEAN
            } else if tmp < -0.4 {
                FROZEN_OCEAN
            } else if tmp < -0.2 {
                COLD_OCEAN
            } else {
                OCEAN
            };
        }
    }
    0
}

/// `mapOceanMix`.
pub fn map_ocean_mix(
    ls: &LayerStack,
    l: &Layer,
    out: &mut [i32],
    x: i32,
    z: i32,
    w: i32,
    h: i32,
) -> i32 {
    let p2 = l.p2.expect("mapOceanMix() requires two parents");
    let err = ls.get_map(p2, out, x, z, w, h);
    if err != 0 {
        return err;
    }

    // determine the minimum required land area: (x+lx0, z+lz0), (lw, lh)
    // (the extra border is only required if there is warm or frozen ocean)
    let (mut lx0, mut lx1) = (0, w);
    let (mut lz0, mut lz1) = (0, h);

    for j in 0..h {
        let jcentre = j - 8 > 0 && j + 9 < h;
        for i in 0..w {
            if jcentre && i - 8 > 0 && i + 9 < w {
                continue;
            }
            let ocean_id = out[(i + j * w) as usize];
            if ocean_id == WARM_OCEAN || ocean_id == FROZEN_OCEAN {
                if i - 8 < lx0 {
                    lx0 = i - 8;
                }
                if i + 9 > lx1 {
                    lx1 = i + 9;
                }
                if j - 8 < lz0 {
                    lz0 = j - 8;
                }
                if j + 9 > lz1 {
                    lz1 = j + 9;
                }
            }
        }
    }

    let land = w as usize * h as usize; // offset of the land area in `out`
    let lw = lx1 - lx0;
    let lh = lz1 - lz0;
    let err = ls.get_map2(l.p.unwrap(), out, land, x + lx0, z + lz0, lw, lh);
    if err != 0 {
        return err;
    }

    for j in 0..h {
        for i in 0..w {
            let land_id = out[land + ((i - lx0) + (j - lz0) * lw) as usize];
            let mut ocean_id = out[(i + j * w) as usize];
            let mut replace_id = 0;

            if !is_oceanic(land_id) {
                out[(i + j * w) as usize] = land_id;
                continue;
            }

            if ocean_id == WARM_OCEAN {
                replace_id = LUKEWARM_OCEAN;
            }
            if ocean_id == FROZEN_OCEAN {
                replace_id = COLD_OCEAN;
            }
            if replace_id != 0 {
                let mut replaced = false;
                'search: for ii in (-8..=8).step_by(4) {
                    for jj in (-8..=8).step_by(4) {
                        let id = out[land + ((i + ii - lx0) + (j + jj - lz0) * lw) as usize];
                        if !is_oceanic(id) {
                            out[(i + j * w) as usize] = replace_id;
                            replaced = true;
                            break 'search;
                        }
                    }
                }
                if replaced {
                    continue;
                }
            }

            if land_id == DEEP_OCEAN {
                match ocean_id {
                    LUKEWARM_OCEAN => ocean_id = DEEP_LUKEWARM_OCEAN,
                    OCEAN => ocean_id = DEEP_OCEAN,
                    COLD_OCEAN => ocean_id = DEEP_COLD_OCEAN,
                    FROZEN_OCEAN => ocean_id = DEEP_FROZEN_OCEAN,
                    _ => {}
                }
            }
            out[(i + j * w) as usize] = ocean_id;
        }
    }
    0
}

/// `mapVoronoi` (old name `mapVoronoiZoom`), 1.15 and later: the layer is
/// seeded with the SHA of the world seed.
pub fn map_voronoi(
    ls: &LayerStack,
    l: &Layer,
    out: &mut [i32],
    x: i32,
    z: i32,
    w: i32,
    h: i32,
) -> i32 {
    let x = x - 2;
    let z = z - 2;
    let px = x >> 2;
    let pz = z >> 2;
    let pw = ((x + w) >> 2) - px + 2;
    let ph = ((z + h) >> 2) - pz + 2;

    if let Some(p) = l.p {
        let err = ls.get_map(p, out, px, pz, pw, ph);
        if err != 0 {
            return err;
        }
    }

    let wh = w as usize * h as usize;
    out.copy_within(0..pw as usize * ph as usize, wh);
    let (dst, src) = out.split_at_mut(wh);
    map_voronoi_plane(l.start_salt, dst, src, x, z, w, h, 0, px, pz, pw, ph);
    0
}

/// `mapVoronoi114`: the planar Voronoi zoom of 1.14 and earlier.
pub fn map_voronoi114(
    ls: &LayerStack,
    l: &Layer,
    out: &mut [i32],
    x: i32,
    z: i32,
    w: i32,
    h: i32,
) -> i32 {
    let x = x - 2;
    let z = z - 2;
    let p_x = x >> 2;
    let p_z = z >> 2;
    let p_w = ((x + w) >> 2) - p_x + 2;
    let p_h = ((z + h) >> 2) - p_z + 2;

    if let Some(p) = l.p {
        let err = ls.get_map(p, out, p_x, p_z, p_w, p_h);
        if err != 0 {
            return err;
        }
    }
    voronoi114_body(
        l.start_salt,
        l.start_seed,
        out,
        x,
        z,
        w,
        h,
        p_x,
        p_z,
        p_w,
        p_h,
    )
}

/// `mapVoronoi114` for a layer without parent: `out` already holds the
/// 1:4 source area (as the End generator of 1.9 to 1.14 does with a
/// `memset` layer whose `startSalt` is `getLayerSalt(10)`). `x` and `z` are
/// the unshifted block coordinates.
pub fn map_voronoi114_end(out: &mut [i32], x: i32, z: i32, w: i32, h: i32) -> i32 {
    let st = layer_salt(10);
    let x = x - 2;
    let z = z - 2;
    let p_x = x >> 2;
    let p_z = z >> 2;
    let p_w = ((x + w) >> 2) - p_x + 2;
    let p_h = ((z + h) >> 2) - p_z + 2;
    voronoi114_body(st, 0, out, x, z, w, h, p_x, p_z, p_w, p_h)
}

#[allow(clippy::too_many_arguments)]
fn voronoi114_body(
    st: u64,
    ss: u64,
    out: &mut [i32],
    x: i32,
    z: i32,
    w: i32,
    h: i32,
    p_x: i32,
    p_z: i32,
    p_w: i32,
    p_h: i32,
) -> i32 {
    let buf = p_w as usize * p_h as usize;
    let pwu = p_w as usize;

    for pj in 0..p_h - 1 {
        let mut v00 = out[(pj as usize) * pwu];
        let mut v01 = out[(pj as usize + 1) * pwu];
        let pjz = p_z + pj;
        let j4 = pjz * 4 - z;

        for pi in 0..p_w - 1 {
            let pix = p_x + pi;
            let i4 = pix * 4 - x;

            let v10 = out[pi as usize + 1 + (pj as usize) * pwu];
            let v11 = out[pi as usize + 1 + (pj as usize + 1) * pwu];

            'cell: {
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
                            out[buf + j as usize * w as usize + i as usize] = v00;
                        }
                    }
                    break 'cell;
                }

                let mut cs = chunk_seed(ss, (pi + p_x) * 4, (pj + p_z) * 4);
                let da1 = (mc_first_int(cs, 1024) as i64 - 512) * 36;
                cs = mc_step_seed(cs, st);
                let da2 = (mc_first_int(cs, 1024) as i64 - 512) * 36;

                cs = chunk_seed(ss, (pi + p_x + 1) * 4, (pj + p_z) * 4);
                let db1 = (mc_first_int(cs, 1024) as i64 - 512) * 36 + 40 * 1024;
                cs = mc_step_seed(cs, st);
                let db2 = (mc_first_int(cs, 1024) as i64 - 512) * 36;

                cs = chunk_seed(ss, (pi + p_x) * 4, (pj + p_z + 1) * 4);
                let dc1 = (mc_first_int(cs, 1024) as i64 - 512) * 36;
                cs = mc_step_seed(cs, st);
                let dc2 = (mc_first_int(cs, 1024) as i64 - 512) * 36 + 40 * 1024;

                cs = chunk_seed(ss, (pi + p_x + 1) * 4, (pj + p_z + 1) * 4);
                let dd1 = (mc_first_int(cs, 1024) as i64 - 512) * 36 + 40 * 1024;
                cs = mc_step_seed(cs, st);
                let dd2 = (mc_first_int(cs, 1024) as i64 - 512) * 36 + 40 * 1024;

                for jj in 0..4 {
                    let j = j4 + jj;
                    if j < 0 || j >= h {
                        continue;
                    }

                    let mj = 10240 * jj as i64;
                    let sja = (mj - da2) * (mj - da2);
                    let sjb = (mj - db2) * (mj - db2);
                    let sjc = (mj - dc2) * (mj - dc2);
                    let sjd = (mj - dd2) * (mj - dd2);

                    for ii in 0..4 {
                        let i = i4 + ii;
                        if i < 0 || i >= w {
                            continue;
                        }

                        let mi = 10240 * ii as i64;
                        let da = (mi - da1) * (mi - da1) + sja;
                        let db = (mi - db1) * (mi - db1) + sjb;
                        let dc = (mi - dc1) * (mi - dc1) + sjc;
                        let dd = (mi - dd1) * (mi - dd1) + sjd;

                        let v = if da < db && da < dc && da < dd {
                            v00
                        } else if db < da && db < dc && db < dd {
                            v10
                        } else if dc < da && dc < db && dc < dd {
                            v01
                        } else {
                            v11
                        };
                        out[buf + j as usize * w as usize + i as usize] = v;
                    }
                }
            }
            v00 = v10;
            v01 = v11;
        }
    }

    let n = w as usize * h as usize;
    out.copy_within(buf..buf + n, 0);
    0
}

impl LayerStack {
    /// Like [`get_map`](Self::get_map), but into the part of `out` that
    /// starts at `offset` (C: `getMap(p, out + offset, ...)`).
    #[inline]
    #[allow(clippy::too_many_arguments)]
    pub fn get_map2(
        &self,
        idx: usize,
        out: &mut [i32],
        offset: usize,
        x: i32,
        z: i32,
        w: i32,
        h: i32,
    ) -> i32 {
        self.get_map(idx, &mut out[offset..], x, z, w, h)
    }
}
