//! Biome ids of the engine (Minecraft's legacy numeric ids, extended for
//! newer biomes), with their names and map colours for 26.3.

use crate::biome_table::BIOMES;

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
pub const MUSHROOM_FIELDS: i32 = 14;
pub const BEACH: i32 = 16;
pub const JUNGLE: i32 = 21;
pub const SPARSE_JUNGLE: i32 = 23;
pub const DEEP_OCEAN: i32 = 24;
pub const STONY_SHORE: i32 = 25;
pub const SNOWY_BEACH: i32 = 26;
pub const BIRCH_FOREST: i32 = 27;
pub const DARK_FOREST: i32 = 29;
pub const SNOWY_TAIGA: i32 = 30;
pub const OLD_GROWTH_PINE_TAIGA: i32 = 32;
pub const WINDSWEPT_FOREST: i32 = 34;
pub const SAVANNA: i32 = 35;
pub const SAVANNA_PLATEAU: i32 = 36;
pub const BADLANDS: i32 = 37;
pub const WOODED_BADLANDS: i32 = 38;
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
pub const SUNFLOWER_PLAINS: i32 = 129;
pub const WINDSWEPT_GRAVELLY_HILLS: i32 = 131;
pub const FLOWER_FOREST: i32 = 132;
pub const ICE_SPIKES: i32 = 140;
pub const OLD_GROWTH_BIRCH_FOREST: i32 = 155;
pub const OLD_GROWTH_SPRUCE_TAIGA: i32 = 160;
pub const WINDSWEPT_SAVANNA: i32 = 163;
pub const ERODED_BADLANDS: i32 = 165;
pub const BAMBOO_JUNGLE: i32 = 168;
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

/// Whether the biome exists in 26.3.
pub fn exists(id: i32) -> bool {
    (0..256).contains(&id) && BIOMES[id as usize].0
}

/// The biome's id without namespace (`dark_forest`), if it exists.
pub fn name(id: i32) -> Option<&'static str> {
    exists(id).then(|| BIOMES[id as usize].1)
}

/// The biome's map colour as opaque ARGB, or 0 for unknown ids.
pub fn color(id: i32) -> u32 {
    if exists(id) {
        0xFF00_0000 | BIOMES[id as usize].2
    } else {
        0
    }
}

/// The id of a biome by name (`minecraft:` optional).
pub fn by_name(name: &str) -> Option<i32> {
    let name = name.strip_prefix("minecraft:").unwrap_or(name);
    (0..256).find(|&id| exists(id) && BIOMES[id as usize].1 == name)
}
