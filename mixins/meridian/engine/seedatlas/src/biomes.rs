//! Biome ids of the engine (Minecraft's legacy numeric ids, extended for
//! newer biomes), with their names and map colours, and the helpers of
//! `biomes.c` for every version.

use crate::biome_table::BIOMES;
use crate::mc::*;

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

/// The biome's name in version `mc` (`sax_context_biome_name`): `biome2str`
/// where `biomeExists`.
pub fn name_in(mc: i32, id: i32) -> Option<&'static str> {
    if !(0..256).contains(&id) || !biome_exists(mc, id) {
        return None;
    }
    biome2str(mc, id)
}

/// The biome's colour in version `mc` as opaque ARGB
/// (`sax_context_biome_color`): the colour of `initBiomeColors` where
/// `biomeExists`, else 0.
pub fn color_in(mc: i32, id: i32) -> u32 {
    if !(0..256).contains(&id) || !biome_exists(mc, id) {
        return 0;
    }
    0xFF00_0000 | BIOMES[id as usize].2
}

/// `biomeExists`: whether the biome id exists in version `mc`.
pub fn biome_exists(mc: i32, id: i32) -> bool {
    if mc >= MC_1_18 {
        if (SOUL_SAND_VALLEY..=BASALT_DELTAS).contains(&id) {
            return true;
        }
        if (SMALL_END_ISLANDS..=END_BARRENS).contains(&id) {
            return true;
        }
        if id == DAPPLED_FOREST {
            return mc >= MC_26_3;
        }
        if id == SULFUR_CAVES {
            return mc >= MC_26_2;
        }
        if id == PALE_GARDEN {
            return mc >= MC_1_21_4;
        }
        if id == CHERRY_GROVE {
            return mc >= MC_1_20;
        }
        if id == DEEP_DARK || id == MANGROVE_SWAMP {
            return mc >= MC_1_19_2;
        }
        return matches!(
            id,
            OCEAN
                | PLAINS
                | DESERT
                | MOUNTAINS
                | FOREST
                | TAIGA
                | SWAMP
                | RIVER
                | NETHER_WASTES
                | THE_END
                | FROZEN_OCEAN
                | FROZEN_RIVER
                | SNOWY_TUNDRA
                | MUSHROOM_FIELDS
                | BEACH
                | JUNGLE
                | JUNGLE_EDGE
                | DEEP_OCEAN
                | STONE_SHORE
                | SNOWY_BEACH
                | BIRCH_FOREST
                | DARK_FOREST
                | SNOWY_TAIGA
                | GIANT_TREE_TAIGA
                | WOODED_MOUNTAINS
                | SAVANNA
                | SAVANNA_PLATEAU
                | BADLANDS
                | WOODED_BADLANDS_PLATEAU
                | WARM_OCEAN
                | LUKEWARM_OCEAN
                | COLD_OCEAN
                | DEEP_WARM_OCEAN
                | DEEP_LUKEWARM_OCEAN
                | DEEP_COLD_OCEAN
                | DEEP_FROZEN_OCEAN
                | SUNFLOWER_PLAINS
                | GRAVELLY_MOUNTAINS
                | FLOWER_FOREST
                | ICE_SPIKES
                | TALL_BIRCH_FOREST
                | GIANT_SPRUCE_TAIGA
                | SHATTERED_SAVANNA
                | ERODED_BADLANDS
                | BAMBOO_JUNGLE
                | DRIPSTONE_CAVES
                | LUSH_CAVES
                | MEADOW
                | GROVE
                | SNOWY_SLOPES
                | STONY_PEAKS
                | JAGGED_PEAKS
                | FROZEN_PEAKS
        );
    }

    if mc <= MC_B1_7 {
        // areas below the sea level are treated as oceans
        return matches!(
            id,
            PLAINS
                | DESERT
                | FOREST
                | TAIGA
                | SWAMP
                | SNOWY_TUNDRA
                | SAVANNA
                | SEASONAL_FOREST
                | RAINFOREST
                | SHRUBLAND
                | OCEAN
                | FROZEN_OCEAN
        );
    }

    if mc <= MC_B1_8
        && matches!(
            id,
            FROZEN_OCEAN
                | FROZEN_RIVER
                | SNOWY_TUNDRA
                | MUSHROOM_FIELDS
                | MUSHROOM_FIELD_SHORE
                | THE_END
        )
    {
        return false;
    }
    if mc <= MC_1_0
        && matches!(
            id,
            SNOWY_MOUNTAINS | BEACH | DESERT_HILLS | WOODED_HILLS | TAIGA_HILLS | MOUNTAIN_EDGE
        )
    {
        return false;
    }

    if (OCEAN..=MOUNTAIN_EDGE).contains(&id) {
        return true;
    }
    if (JUNGLE..=JUNGLE_HILLS).contains(&id) {
        return mc >= MC_1_2;
    }
    if (JUNGLE_EDGE..=BADLANDS_PLATEAU).contains(&id) {
        return mc >= MC_1_7;
    }
    if (SMALL_END_ISLANDS..=END_BARRENS).contains(&id) {
        return mc >= MC_1_9;
    }
    if (WARM_OCEAN..=DEEP_FROZEN_OCEAN).contains(&id) {
        return mc >= MC_1_13;
    }

    match id {
        THE_VOID => mc >= MC_1_9,
        SUNFLOWER_PLAINS
        | DESERT_LAKES
        | GRAVELLY_MOUNTAINS
        | FLOWER_FOREST
        | TAIGA_MOUNTAINS
        | SWAMP_HILLS
        | ICE_SPIKES
        | MODIFIED_JUNGLE
        | MODIFIED_JUNGLE_EDGE
        | TALL_BIRCH_FOREST
        | TALL_BIRCH_HILLS
        | DARK_FOREST_HILLS
        | SNOWY_TAIGA_MOUNTAINS
        | GIANT_SPRUCE_TAIGA
        | GIANT_SPRUCE_TAIGA_HILLS
        | MODIFIED_GRAVELLY_MOUNTAINS
        | SHATTERED_SAVANNA
        | SHATTERED_SAVANNA_PLATEAU
        | ERODED_BADLANDS
        | MODIFIED_WOODED_BADLANDS_PLATEAU
        | MODIFIED_BADLANDS_PLATEAU => mc >= MC_1_7,
        BAMBOO_JUNGLE | BAMBOO_JUNGLE_HILLS => mc >= MC_1_14,
        SOUL_SAND_VALLEY | CRIMSON_FOREST | WARPED_FOREST | BASALT_DELTAS => mc >= MC_1_16_1,
        DRIPSTONE_CAVES | LUSH_CAVES => mc >= MC_1_17,
        _ => false,
    }
}

/// `isOverworld`: false for biomes that don't generate in the overworld.
pub fn is_overworld(mc: i32, id: i32) -> bool {
    if !biome_exists(mc, id) {
        return false;
    }
    if (SMALL_END_ISLANDS..=END_BARRENS).contains(&id) {
        return false;
    }
    if (SOUL_SAND_VALLEY..=BASALT_DELTAS).contains(&id) {
        return false;
    }
    match id {
        NETHER_WASTES | THE_END => false,
        FROZEN_OCEAN => mc <= MC_1_6 || mc >= MC_1_13,
        MOUNTAIN_EDGE => mc <= MC_1_6,
        DEEP_WARM_OCEAN | THE_VOID => false,
        TALL_BIRCH_FOREST => mc <= MC_1_8 || mc >= MC_1_11,
        DRIPSTONE_CAVES | LUSH_CAVES => mc >= MC_1_18,
        _ => true,
    }
}

/// `getDimension`: -1 (Nether), 0 (overworld) or 1 (End).
pub fn get_dimension(id: i32) -> i32 {
    if (SMALL_END_ISLANDS..=END_BARRENS).contains(&id) || id == THE_END {
        1
    } else if (SOUL_SAND_VALLEY..=BASALT_DELTAS).contains(&id) || id == NETHER_WASTES {
        -1
    } else {
        0
    }
}

/// `getMutated`: the mutated variant of a biome, or `NONE`.
pub fn get_mutated(mc: i32, id: i32) -> i32 {
    match id {
        PLAINS => SUNFLOWER_PLAINS,
        DESERT => DESERT_LAKES,
        MOUNTAINS => GRAVELLY_MOUNTAINS,
        FOREST => FLOWER_FOREST,
        TAIGA => TAIGA_MOUNTAINS,
        SWAMP => SWAMP_HILLS,
        SNOWY_TUNDRA => ICE_SPIKES,
        JUNGLE => MODIFIED_JUNGLE,
        JUNGLE_EDGE => MODIFIED_JUNGLE_EDGE,
        // emulates MC-98995
        BIRCH_FOREST => {
            if (MC_1_9..=MC_1_10).contains(&mc) {
                TALL_BIRCH_HILLS
            } else {
                TALL_BIRCH_FOREST
            }
        }
        BIRCH_FOREST_HILLS => {
            if (MC_1_9..=MC_1_10).contains(&mc) {
                NONE
            } else {
                TALL_BIRCH_HILLS
            }
        }
        DARK_FOREST => DARK_FOREST_HILLS,
        SNOWY_TAIGA => SNOWY_TAIGA_MOUNTAINS,
        GIANT_TREE_TAIGA => GIANT_SPRUCE_TAIGA,
        GIANT_TREE_TAIGA_HILLS => GIANT_SPRUCE_TAIGA_HILLS,
        WOODED_MOUNTAINS => MODIFIED_GRAVELLY_MOUNTAINS,
        SAVANNA => SHATTERED_SAVANNA,
        SAVANNA_PLATEAU => SHATTERED_SAVANNA_PLATEAU,
        BADLANDS => ERODED_BADLANDS,
        WOODED_BADLANDS_PLATEAU => MODIFIED_WOODED_BADLANDS_PLATEAU,
        BADLANDS_PLATEAU => MODIFIED_BADLANDS_PLATEAU,
        _ => NONE,
    }
}

/// `getCategory`: the biome's category (the id of the category's main
/// biome), or `NONE`.
pub fn get_category(mc: i32, id: i32) -> i32 {
    match id {
        BEACH | SNOWY_BEACH => BEACH,
        DESERT | DESERT_HILLS | DESERT_LAKES => DESERT,
        MOUNTAINS
        | MOUNTAIN_EDGE
        | WOODED_MOUNTAINS
        | GRAVELLY_MOUNTAINS
        | MODIFIED_GRAVELLY_MOUNTAINS => MOUNTAINS,
        FOREST | WOODED_HILLS | BIRCH_FOREST | BIRCH_FOREST_HILLS | DARK_FOREST | FLOWER_FOREST
        | TALL_BIRCH_FOREST | TALL_BIRCH_HILLS | DARK_FOREST_HILLS | DAPPLED_FOREST => FOREST,
        SNOWY_TUNDRA | SNOWY_MOUNTAINS | ICE_SPIKES => SNOWY_TUNDRA,
        JUNGLE | JUNGLE_HILLS | JUNGLE_EDGE | MODIFIED_JUNGLE | MODIFIED_JUNGLE_EDGE
        | BAMBOO_JUNGLE | BAMBOO_JUNGLE_HILLS => JUNGLE,
        BADLANDS
        | ERODED_BADLANDS
        | MODIFIED_WOODED_BADLANDS_PLATEAU
        | MODIFIED_BADLANDS_PLATEAU => BADLANDS,
        WOODED_BADLANDS_PLATEAU | BADLANDS_PLATEAU => {
            if mc <= MC_1_15 {
                BADLANDS
            } else {
                BADLANDS_PLATEAU
            }
        }
        MUSHROOM_FIELDS | MUSHROOM_FIELD_SHORE => MUSHROOM_FIELDS,
        STONE_SHORE => STONE_SHORE,
        OCEAN | FROZEN_OCEAN | DEEP_OCEAN | WARM_OCEAN | LUKEWARM_OCEAN | COLD_OCEAN
        | DEEP_WARM_OCEAN | DEEP_LUKEWARM_OCEAN | DEEP_COLD_OCEAN | DEEP_FROZEN_OCEAN => OCEAN,
        PLAINS | SUNFLOWER_PLAINS => PLAINS,
        RIVER | FROZEN_RIVER => RIVER,
        SAVANNA | SAVANNA_PLATEAU | SHATTERED_SAVANNA | SHATTERED_SAVANNA_PLATEAU => SAVANNA,
        SWAMP | SWAMP_HILLS => SWAMP,
        TAIGA
        | TAIGA_HILLS
        | SNOWY_TAIGA
        | SNOWY_TAIGA_HILLS
        | GIANT_TREE_TAIGA
        | GIANT_TREE_TAIGA_HILLS
        | TAIGA_MOUNTAINS
        | SNOWY_TAIGA_MOUNTAINS
        | GIANT_SPRUCE_TAIGA
        | GIANT_SPRUCE_TAIGA_HILLS => TAIGA,
        NETHER_WASTES | SOUL_SAND_VALLEY | CRIMSON_FOREST | WARPED_FOREST | BASALT_DELTAS => {
            NETHER_WASTES
        }
        _ => NONE,
    }
}

/// `areSimilar`.
pub fn are_similar(mc: i32, id1: i32, id2: i32) -> bool {
    if id1 == id2 {
        return true;
    }
    if mc <= MC_1_15 && (id1 == WOODED_BADLANDS_PLATEAU || id1 == BADLANDS_PLATEAU) {
        return id2 == WOODED_BADLANDS_PLATEAU || id2 == BADLANDS_PLATEAU;
    }
    get_category(mc, id1) == get_category(mc, id2)
}

/// `isMesa`.
pub fn is_mesa(id: i32) -> bool {
    matches!(
        id,
        BADLANDS
            | ERODED_BADLANDS
            | MODIFIED_WOODED_BADLANDS_PLATEAU
            | MODIFIED_BADLANDS_PLATEAU
            | WOODED_BADLANDS_PLATEAU
            | BADLANDS_PLATEAU
    )
}

/// Whether `id` (below 64) is in the bit set built from `ids`.
fn in_bits(id: i32, ids: &[i32]) -> bool {
    let bits = ids.iter().fold(0u64, |b, &i| b | (1u64 << i));
    (id as u32) < 64 && (bits >> id) & 1 != 0
}

/// `isShallowOcean`.
pub fn is_shallow_ocean(id: i32) -> bool {
    in_bits(
        id,
        &[OCEAN, FROZEN_OCEAN, WARM_OCEAN, LUKEWARM_OCEAN, COLD_OCEAN],
    )
}

/// `isDeepOcean`.
pub fn is_deep_ocean(id: i32) -> bool {
    in_bits(
        id,
        &[
            DEEP_OCEAN,
            DEEP_WARM_OCEAN,
            DEEP_LUKEWARM_OCEAN,
            DEEP_COLD_OCEAN,
            DEEP_FROZEN_OCEAN,
        ],
    )
}

/// `isOceanic`.
pub fn is_oceanic(id: i32) -> bool {
    is_shallow_ocean(id) || is_deep_ocean(id)
}

/// `isSnowy`.
pub fn is_snowy(id: i32) -> bool {
    matches!(
        id,
        FROZEN_OCEAN
            | FROZEN_RIVER
            | SNOWY_TUNDRA
            | SNOWY_MOUNTAINS
            | SNOWY_BEACH
            | SNOWY_TAIGA
            | SNOWY_TAIGA_HILLS
            | ICE_SPIKES
            | SNOWY_TAIGA_MOUNTAINS
    )
}

/// `biome2str` (`util.c`): the biome's name in version `mc`.
pub fn biome2str(mc: i32, id: i32) -> Option<&'static str> {
    if mc >= MC_1_18 {
        // a number of 'new' biomes in 1.18 are renamed ones
        match id {
            OLD_GROWTH_BIRCH_FOREST => return Some("old_growth_birch_forest"),
            OLD_GROWTH_PINE_TAIGA => return Some("old_growth_pine_taiga"),
            OLD_GROWTH_SPRUCE_TAIGA => return Some("old_growth_spruce_taiga"),
            SNOWY_PLAINS => return Some("snowy_plains"),
            SPARSE_JUNGLE => return Some("sparse_jungle"),
            STONY_SHORE => return Some("stony_shore"),
            WINDSWEPT_HILLS => return Some("windswept_hills"),
            WINDSWEPT_FOREST => return Some("windswept_forest"),
            WINDSWEPT_GRAVELLY_HILLS => return Some("windswept_gravelly_hills"),
            WINDSWEPT_SAVANNA => return Some("windswept_savanna"),
            WOODED_BADLANDS => return Some("wooded_badlands"),
            _ => {}
        }
    }
    Some(match id {
        OCEAN => "ocean",
        PLAINS => "plains",
        DESERT => "desert",
        MOUNTAINS => "mountains",
        FOREST => "forest",
        TAIGA => "taiga",
        SWAMP => "swamp",
        RIVER => "river",
        NETHER_WASTES => "nether_wastes",
        THE_END => "the_end",
        FROZEN_OCEAN => "frozen_ocean",
        FROZEN_RIVER => "frozen_river",
        SNOWY_TUNDRA => "snowy_tundra",
        SNOWY_MOUNTAINS => "snowy_mountains",
        MUSHROOM_FIELDS => "mushroom_fields",
        MUSHROOM_FIELD_SHORE => "mushroom_field_shore",
        BEACH => "beach",
        DESERT_HILLS => "desert_hills",
        WOODED_HILLS => "wooded_hills",
        TAIGA_HILLS => "taiga_hills",
        MOUNTAIN_EDGE => "mountain_edge",
        JUNGLE => "jungle",
        JUNGLE_HILLS => "jungle_hills",
        JUNGLE_EDGE => "jungle_edge",
        DEEP_OCEAN => "deep_ocean",
        STONE_SHORE => "stone_shore",
        SNOWY_BEACH => "snowy_beach",
        BIRCH_FOREST => "birch_forest",
        BIRCH_FOREST_HILLS => "birch_forest_hills",
        DARK_FOREST => "dark_forest",
        SNOWY_TAIGA => "snowy_taiga",
        SNOWY_TAIGA_HILLS => "snowy_taiga_hills",
        GIANT_TREE_TAIGA => "giant_tree_taiga",
        GIANT_TREE_TAIGA_HILLS => "giant_tree_taiga_hills",
        WOODED_MOUNTAINS => "wooded_mountains",
        SAVANNA => "savanna",
        SAVANNA_PLATEAU => "savanna_plateau",
        BADLANDS => "badlands",
        WOODED_BADLANDS_PLATEAU => "wooded_badlands_plateau",
        BADLANDS_PLATEAU => "badlands_plateau",
        SMALL_END_ISLANDS => "small_end_islands",
        END_MIDLANDS => "end_midlands",
        END_HIGHLANDS => "end_highlands",
        END_BARRENS => "end_barrens",
        WARM_OCEAN => "warm_ocean",
        LUKEWARM_OCEAN => "lukewarm_ocean",
        COLD_OCEAN => "cold_ocean",
        DEEP_WARM_OCEAN => "deep_warm_ocean",
        DEEP_LUKEWARM_OCEAN => "deep_lukewarm_ocean",
        DEEP_COLD_OCEAN => "deep_cold_ocean",
        DEEP_FROZEN_OCEAN => "deep_frozen_ocean",
        SEASONAL_FOREST => "seasonal_forest",
        SHRUBLAND => "shrubland",
        RAINFOREST => "rainforest",
        THE_VOID => "the_void",
        SUNFLOWER_PLAINS => "sunflower_plains",
        DESERT_LAKES => "desert_lakes",
        GRAVELLY_MOUNTAINS => "gravelly_mountains",
        FLOWER_FOREST => "flower_forest",
        TAIGA_MOUNTAINS => "taiga_mountains",
        SWAMP_HILLS => "swamp_hills",
        ICE_SPIKES => "ice_spikes",
        MODIFIED_JUNGLE => "modified_jungle",
        MODIFIED_JUNGLE_EDGE => "modified_jungle_edge",
        TALL_BIRCH_FOREST => "tall_birch_forest",
        TALL_BIRCH_HILLS => "tall_birch_hills",
        DARK_FOREST_HILLS => "dark_forest_hills",
        SNOWY_TAIGA_MOUNTAINS => "snowy_taiga_mountains",
        GIANT_SPRUCE_TAIGA => "giant_spruce_taiga",
        GIANT_SPRUCE_TAIGA_HILLS => "giant_spruce_taiga_hills",
        MODIFIED_GRAVELLY_MOUNTAINS => "modified_gravelly_mountains",
        SHATTERED_SAVANNA => "shattered_savanna",
        SHATTERED_SAVANNA_PLATEAU => "shattered_savanna_plateau",
        ERODED_BADLANDS => "eroded_badlands",
        MODIFIED_WOODED_BADLANDS_PLATEAU => "modified_wooded_badlands_plateau",
        MODIFIED_BADLANDS_PLATEAU => "modified_badlands_plateau",
        BAMBOO_JUNGLE => "bamboo_jungle",
        BAMBOO_JUNGLE_HILLS => "bamboo_jungle_hills",
        SOUL_SAND_VALLEY => "soul_sand_valley",
        CRIMSON_FOREST => "crimson_forest",
        WARPED_FOREST => "warped_forest",
        BASALT_DELTAS => "basalt_deltas",
        DRIPSTONE_CAVES => "dripstone_caves",
        LUSH_CAVES => "lush_caves",
        MEADOW => "meadow",
        GROVE => "grove",
        SNOWY_SLOPES => "snowy_slopes",
        STONY_PEAKS => "stony_peaks",
        JAGGED_PEAKS => "jagged_peaks",
        FROZEN_PEAKS => "frozen_peaks",
        DEEP_DARK => "deep_dark",
        MANGROVE_SWAMP => "mangrove_swamp",
        CHERRY_GROVE => "cherry_grove",
        PALE_GARDEN => "pale_garden",
        SULFUR_CAVES => "sulfur_caves",
        DAPPLED_FOREST => "dappled_forest",
        _ => return None,
    })
}
