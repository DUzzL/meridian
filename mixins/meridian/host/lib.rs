//! The boundary between Meridian and the game.
//!
//! Meridian's core never talks to Horizon directly. Everything it needs from
//! the game is either pushed in as a [`HostEvent`] or pulled through the
//! [`MapHost`] trait. The Horizon adapter (`mixins/meridian/engine`) is the
//! only implementation shipped; tests use small fakes.
//!
//! The capabilities that Horizon does not expose yet (see
//! `docs/HORIZON_API.md`, E1-E6) are part of this trait on purpose: the
//! adapter fills them with transitional mixins today and with upstream hooks
//! later, without the core noticing.
#![forbid(unsafe_code)]

use std::fmt;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Blocks per chunk side.
pub const CHUNK_SIZE: i32 = 16;
/// Columns in a chunk.
pub const CHUNK_COLUMNS: usize = (CHUNK_SIZE * CHUNK_SIZE) as usize;

/// A namespaced dimension id such as `minecraft:overworld`.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct DimensionId(pub String);

impl DimensionId {
    pub const OVERWORLD: &'static str = "minecraft:overworld";
    pub const NETHER: &'static str = "minecraft:the_nether";
    pub const END: &'static str = "minecraft:the_end";

    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Horizontal scale relative to the overworld (the Nether is 1:8).
    pub fn coordinate_scale(&self) -> f64 {
        if self.0 == Self::NETHER { 8.0 } else { 1.0 }
    }
}

impl fmt::Display for DimensionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// A chunk position in chunk coordinates.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ChunkPos {
    pub x: i32,
    pub z: i32,
}

impl ChunkPos {
    pub const fn new(x: i32, z: i32) -> Self {
        Self { x, z }
    }

    /// The chunk holding block column (`x`, `z`).
    pub const fn containing(x: i32, z: i32) -> Self {
        Self {
            x: x.div_euclid(CHUNK_SIZE),
            z: z.div_euclid(CHUNK_SIZE),
        }
    }

    /// The block column at the chunk's north-west corner.
    pub const fn min_block(self) -> (i32, i32) {
        (self.x * CHUNK_SIZE, self.z * CHUNK_SIZE)
    }
}

/// Where the player is playing: a singleplayer world or a server.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum WorldSource {
    /// The folder name of a singleplayer world under `saves/`.
    Singleplayer { world: String },
    /// A server, by the address the player connected to.
    Multiplayer { address: String },
}

/// The world and dimension the client currently shows.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct WorldContext {
    pub source: WorldSource,
    pub dimension: DimensionId,
}

/// The local player, as the map needs it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PlayerState {
    pub position: [f64; 3],
    /// Degrees, Minecraft convention: 0 faces south (+Z), 90 faces west.
    pub yaw: f32,
    pub pitch: f32,
    pub dimension: DimensionId,
}

/// How a block is tinted by its biome.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Tint {
    #[default]
    None,
    Grass,
    Foliage,
    DryFoliage,
    Water,
    /// A constant tint (0xRRGGBB), e.g. spruce leaves.
    Fixed(u32),
}

/// What a block looks like from above.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct BlockAppearance {
    /// The vanilla map colour (0xRRGGBB), 0 for none (air, glass…).
    pub map_color: u32,
    /// The mean colour of the block's top texture (0xRRGGBB) in the current
    /// resource packs, before tinting.
    pub texture_color: u32,
    pub tint: Tint,
    /// Lets the block below show through (glass, ice, leaves at some levels).
    pub translucent: bool,
    /// The map shows no colour for it (air, barriers, light blocks).
    pub invisible: bool,
}

/// A biome's tint colours (0xRRGGBB).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct BiomeAppearance {
    pub grass: u32,
    pub foliage: u32,
    pub dry_foliage: u32,
    pub water: u32,
}

/// One column of a chunk, read from the top.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ColumnSample {
    /// Index into [`ChunkSnapshot::blocks`]: the topmost block the map shows.
    pub block: u16,
    /// Y of that block.
    pub height: i16,
    /// A translucent block above `block`, if any (index into `blocks`).
    pub overlay: Option<u16>,
    /// Water or lava above `block`: the fluid's block index and how many
    /// blocks deep it is.
    pub fluid: Option<(u16, u8)>,
    /// Index into [`ChunkSnapshot::biomes`] at the column's surface.
    pub biome: u16,
    /// Block light above the surface, 0-15.
    pub block_light: u8,
    /// Sky light above the surface, 0-15.
    pub sky_light: u8,
}

/// An owned copy of one chunk's top view. It is `Send`, so the core hands it
/// to background workers.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ChunkSnapshot {
    pub dimension: DimensionId,
    pub pos: ChunkPos,
    /// Block state names seen in this chunk (`minecraft:grass_block`).
    pub blocks: Vec<String>,
    /// Biome ids seen in this chunk (`minecraft:plains`).
    pub biomes: Vec<String>,
    /// 256 columns, x fastest, then z.
    pub columns: Vec<ColumnSample>,
}

impl ChunkSnapshot {
    /// The column at local (`x`, `z`), both 0-15.
    pub fn column(&self, x: usize, z: usize) -> &ColumnSample {
        &self.columns[z * CHUNK_SIZE as usize + x]
    }
}

/// How the map reads a chunk: from the surface, or as a cave map from below
/// a ceiling.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ReadMode {
    Surface,
    /// Start scanning down from this Y (cave mode).
    CaveFrom(i32),
}

/// A key binding the mod declares, in Minecraft's key notation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KeyBinding {
    pub id: &'static str,
    pub label: &'static str,
    /// `key.keyboard.b`, or `key.keyboard.unknown` for unbound.
    pub default: &'static str,
}

/// Things the game tells the map, in order.
#[derive(Clone, Debug, PartialEq)]
pub enum HostEvent {
    /// The client now shows this world and dimension.
    LevelLoaded(WorldContext),
    /// The client dropped its level (dimension change or leaving).
    LevelUnloaded,
    /// The connection or singleplayer session ended.
    Disconnected,
    /// A chunk became available on the client (E1).
    ChunkLoaded(ChunkPos),
    /// Blocks of a loaded chunk changed (E1); may arrive batched.
    ChunkChanged(ChunkPos),
    /// A chunk left the client (E1).
    ChunkUnloaded(ChunkPos),
    /// One of the mod's key bindings, by id.
    Key { id: &'static str, pressed: bool },
    /// Resource packs were reloaded: block colours may have changed.
    ResourcesReloaded,
}

/// What the core asks of the game. Implementations live on the game's main
/// thread; nothing here is called from worker threads.
pub trait MapHost {
    /// The current world and dimension, if the client is in one.
    fn context(&self) -> Option<WorldContext>;

    /// The local player, if any.
    fn player(&self) -> Option<PlayerState>;

    /// A top-down copy of a loaded client chunk (E2). `None` when the chunk
    /// is not loaded or the host cannot read chunks yet.
    fn read_chunk(&self, chunk: ChunkPos, mode: ReadMode) -> Option<ChunkSnapshot>;

    /// How a block state looks; asked once per name and resource reload.
    fn block_appearance(&self, block: &str) -> BlockAppearance;

    /// A biome's tints; asked once per id and resource reload.
    fn biome_appearance(&self, biome: &str) -> BiomeAppearance;

    /// Time of day in ticks (0-24000), for map lighting.
    fn day_time(&self) -> Option<i64>;

    /// The folder for the mod's own data (maps, waypoints).
    fn data_dir(&self) -> PathBuf;

    /// The folder for the mod's settings.
    fn config_dir(&self) -> PathBuf;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunk_of_negative_blocks_rounds_down() {
        assert_eq!(ChunkPos::containing(-1, -16), ChunkPos::new(-1, -1));
        assert_eq!(ChunkPos::containing(-17, 15), ChunkPos::new(-2, 0));
        assert_eq!(ChunkPos::new(-2, 3).min_block(), (-32, 48));
    }

    #[test]
    fn nether_scales_eight() {
        assert_eq!(
            DimensionId::new(DimensionId::NETHER).coordinate_scale(),
            8.0
        );
        assert_eq!(
            DimensionId::new(DimensionId::OVERWORLD).coordinate_scale(),
            1.0
        );
    }
}
