//! Biome ids of the engine (Minecraft's legacy numeric ids, extended for
//! newer biomes), with their names and map colours from the engine itself
//! (`biome2str`, `biomeExists` and `initBiomeColors`).

use std::sync::OnceLock;

use crate::ffi;
use crate::mc::MC_NEWEST;

pub const NONE: i32 = -1;
pub const OCEAN: i32 = 0;
pub const PLAINS: i32 = 1;
pub const DESERT: i32 = 2;
pub const WINDSWEPT_HILLS: i32 = 3;
pub const FOREST: i32 = 4;
pub const TAIGA: i32 = 5;
pub const SWAMP: i32 = 6;
pub const RIVER: i32 = 7;
pub const NETHER_WASTES: i32 = 8;
pub const THE_END: i32 = 9;
pub const FROZEN_OCEAN: i32 = 10;
pub const FROZEN_RIVER: i32 = 11;
pub const SNOWY_PLAINS: i32 = 12;
pub const SNOWY_MOUNTAINS: i32 = 13;
pub const MUSHROOM_FIELDS: i32 = 14;
pub const MUSHROOM_FIELD_SHORE: i32 = 15;
pub const BEACH: i32 = 16;
pub const DESERT_HILLS: i32 = 17;
pub const WOODED_HILLS: i32 = 18;
pub const TAIGA_HILLS: i32 = 19;
pub const MOUNTAIN_EDGE: i32 = 20;
pub const JUNGLE: i32 = 21;
pub const JUNGLE_HILLS: i32 = 22;
pub const SPARSE_JUNGLE: i32 = 23;
pub const DEEP_OCEAN: i32 = 24;
pub const STONY_SHORE: i32 = 25;
pub const SNOWY_BEACH: i32 = 26;
pub const BIRCH_FOREST: i32 = 27;
pub const BIRCH_FOREST_HILLS: i32 = 28;
pub const DARK_FOREST: i32 = 29;
pub const SNOWY_TAIGA: i32 = 30;
pub const SNOWY_TAIGA_HILLS: i32 = 31;
pub const OLD_GROWTH_PINE_TAIGA: i32 = 32;
pub const GIANT_TREE_TAIGA_HILLS: i32 = 33;
pub const WINDSWEPT_FOREST: i32 = 34;
pub const SAVANNA: i32 = 35;
pub const SAVANNA_PLATEAU: i32 = 36;
pub const BADLANDS: i32 = 37;
pub const WOODED_BADLANDS: i32 = 38;
pub const BADLANDS_PLATEAU: i32 = 39;
pub const SMALL_END_ISLANDS: i32 = 40;
pub const END_MIDLANDS: i32 = 41;
pub const END_HIGHLANDS: i32 = 42;
pub const END_BARRENS: i32 = 43;
pub const WARM_OCEAN: i32 = 44;
pub const LUKEWARM_OCEAN: i32 = 45;
pub const COLD_OCEAN: i32 = 46;
pub const DEEP_WARM_OCEAN: i32 = 47;
pub const DEEP_LUKEWARM_OCEAN: i32 = 48;
pub const DEEP_COLD_OCEAN: i32 = 49;
pub const DEEP_FROZEN_OCEAN: i32 = 50;
pub const SEASONAL_FOREST: i32 = 51;
pub const RAINFOREST: i32 = 52;
pub const SHRUBLAND: i32 = 53;
pub const THE_VOID: i32 = 127;
pub const SUNFLOWER_PLAINS: i32 = 129;
pub const DESERT_LAKES: i32 = 130;
pub const WINDSWEPT_GRAVELLY_HILLS: i32 = 131;
pub const FLOWER_FOREST: i32 = 132;
pub const TAIGA_MOUNTAINS: i32 = 133;
pub const SWAMP_HILLS: i32 = 134;
pub const ICE_SPIKES: i32 = 140;
pub const MODIFIED_JUNGLE: i32 = 149;
pub const MODIFIED_JUNGLE_EDGE: i32 = 151;
pub const OLD_GROWTH_BIRCH_FOREST: i32 = 155;
pub const TALL_BIRCH_HILLS: i32 = 156;
pub const DARK_FOREST_HILLS: i32 = 157;
pub const SNOWY_TAIGA_MOUNTAINS: i32 = 158;
pub const OLD_GROWTH_SPRUCE_TAIGA: i32 = 160;
pub const GIANT_SPRUCE_TAIGA_HILLS: i32 = 161;
pub const MODIFIED_GRAVELLY_MOUNTAINS: i32 = 162;
pub const WINDSWEPT_SAVANNA: i32 = 163;
pub const SHATTERED_SAVANNA_PLATEAU: i32 = 164;
pub const ERODED_BADLANDS: i32 = 165;
pub const MODIFIED_WOODED_BADLANDS_PLATEAU: i32 = 166;
pub const MODIFIED_BADLANDS_PLATEAU: i32 = 167;
pub const BAMBOO_JUNGLE: i32 = 168;
pub const BAMBOO_JUNGLE_HILLS: i32 = 169;
pub const SOUL_SAND_VALLEY: i32 = 170;
pub const CRIMSON_FOREST: i32 = 171;
pub const WARPED_FOREST: i32 = 172;
pub const BASALT_DELTAS: i32 = 173;
pub const DRIPSTONE_CAVES: i32 = 174;
pub const LUSH_CAVES: i32 = 175;
pub const MEADOW: i32 = 177;
pub const GROVE: i32 = 178;
pub const SNOWY_SLOPES: i32 = 179;
pub const JAGGED_PEAKS: i32 = 180;
pub const FROZEN_PEAKS: i32 = 181;
pub const STONY_PEAKS: i32 = 182;
pub const DEEP_DARK: i32 = 183;
pub const MANGROVE_SWAMP: i32 = 184;
pub const CHERRY_GROVE: i32 = 185;
pub const PALE_GARDEN: i32 = 186;
pub const SULFUR_CAVES: i32 = 187;
pub const DAPPLED_FOREST: i32 = 188;

// The engine's names before the 1.18 renames (`enum BiomeID` in
// `biomes.h`), kept so the layer code reads like the C.
pub const MOUNTAINS: i32 = WINDSWEPT_HILLS;
pub const SNOWY_TUNDRA: i32 = SNOWY_PLAINS;
pub const JUNGLE_EDGE: i32 = SPARSE_JUNGLE;
pub const STONE_SHORE: i32 = STONY_SHORE;
pub const GIANT_TREE_TAIGA: i32 = OLD_GROWTH_PINE_TAIGA;
pub const WOODED_MOUNTAINS: i32 = WINDSWEPT_FOREST;
pub const WOODED_BADLANDS_PLATEAU: i32 = WOODED_BADLANDS;
pub const GRAVELLY_MOUNTAINS: i32 = WINDSWEPT_GRAVELLY_HILLS;
pub const TALL_BIRCH_FOREST: i32 = OLD_GROWTH_BIRCH_FOREST;
pub const GIANT_SPRUCE_TAIGA: i32 = OLD_GROWTH_SPRUCE_TAIGA;
pub const SHATTERED_SAVANNA: i32 = WINDSWEPT_SAVANNA;

/// The engine's map colours (`initBiomeColors`).
fn colors() -> &'static [[u8; 3]; 256] {
    static COLORS: OnceLock<[[u8; 3]; 256]> = OnceLock::new();
    COLORS.get_or_init(ffi::biome_colors)
}

/// Whether the biome exists in the newest version.
pub fn exists(id: i32) -> bool {
    biome_exists(MC_NEWEST, id)
}

/// The biome's id without namespace (`dark_forest`), if it exists.
pub fn name(id: i32) -> Option<&'static str> {
    name_in(MC_NEWEST, id)
}

/// The biome's map colour as opaque ARGB, or 0 for unknown ids.
pub fn color(id: i32) -> u32 {
    color_in(MC_NEWEST, id)
}

/// The id of a biome by name (`minecraft:` optional).
pub fn by_name(name: &str) -> Option<i32> {
    let name = name.strip_prefix("minecraft:").unwrap_or(name);
    (0..256).find(|&id| self::name(id) == Some(name))
}

/// The biome's name in version `mc` (`sax_context_biome_name`): the
/// engine's `biome2str` where `biomeExists`, e.g. "mountains" before 1.18.
pub fn name_in(mc: i32, id: i32) -> Option<&'static str> {
    if !biome_exists(mc, id) {
        return None;
    }
    ffi::biome_name(mc, id)
}

/// The biome's colour in version `mc` as opaque ARGB
/// (`sax_context_biome_color`): the colour of `initBiomeColors` where
/// `biomeExists`, else 0.
pub fn color_in(mc: i32, id: i32) -> u32 {
    if !biome_exists(mc, id) {
        return 0;
    }
    let [r, g, b] = colors()[id as usize];
    0xFF00_0000 | (r as u32) << 16 | (g as u32) << 8 | b as u32
}

/// `biomeExists`: whether the biome id exists in version `mc`.
pub fn biome_exists(mc: i32, id: i32) -> bool {
    (0..256).contains(&id) && ffi::biome_exists(mc, id)
}
