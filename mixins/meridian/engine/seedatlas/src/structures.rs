//! Structure kinds and their placement (`getStructureConfig` of the
//! engine's `finders.c`), and slime chunks.

use crate::Dimension;
use crate::ffi;

/// The engine's structure numbers (also the Fabric interface's, 1..25).
pub mod kind {
    /// Pre-1.13 temple generation attempts.
    pub const FEATURE: i32 = 0;
    pub const DESERT_PYRAMID: i32 = 1;
    pub const JUNGLE_TEMPLE: i32 = 2;
    pub const SWAMP_HUT: i32 = 3;
    pub const IGLOO: i32 = 4;
    pub const VILLAGE: i32 = 5;
    pub const OCEAN_RUIN: i32 = 6;
    pub const SHIPWRECK: i32 = 7;
    pub const MONUMENT: i32 = 8;
    pub const MANSION: i32 = 9;
    pub const OUTPOST: i32 = 10;
    pub const RUINED_PORTAL: i32 = 11;
    pub const RUINED_PORTAL_NETHER: i32 = 12;
    pub const ANCIENT_CITY: i32 = 13;
    pub const BURIED_TREASURE: i32 = 14;
    pub const MINESHAFT: i32 = 15;
    pub const DESERT_WELL: i32 = 16;
    pub const GEODE: i32 = 17;
    pub const FORTRESS: i32 = 18;
    pub const BASTION: i32 = 19;
    pub const END_CITY: i32 = 20;
    pub const END_GATEWAY: i32 = 21;
    pub const END_ISLAND: i32 = 22;
    pub const TRAIL_RUINS: i32 = 23;
    pub const TRIAL_CHAMBERS: i32 = 24;
    pub const ABANDONED_CAMP: i32 = 25;
    /// One past the last engine structure.
    pub const COUNT: i32 = 26;
}

/// `StructureConfig`: how a structure kind is placed in a version.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StructureConfig {
    pub salt: i32,
    pub region_size: i8,
    pub chunk_range: i8,
    pub kind: i32,
    pub dimension: Dimension,
    pub rarity: f32,
}

/// `getStructureConfig`: the placement of a structure kind in version `mc`,
/// or `None` when the version does not generate it.
pub fn config(kind: i32, mc: i32) -> Option<StructureConfig> {
    let c = ffi::structure_config(kind, mc)?;
    Some(StructureConfig {
        salt: c.salt,
        region_size: c.region_size,
        chunk_range: c.chunk_range,
        kind: c.struct_type as i32,
        dimension: Dimension::from_id(c.dim as i32),
        rarity: c.rarity,
    })
}

/// `isSlimeChunk`: whether a chunk spawns slimes (also in other biomes than
/// swamps).
pub fn is_slime_chunk(seed: u64, cx: i32, cz: i32) -> bool {
    ffi::is_slime_chunk(seed, cx, cz)
}
