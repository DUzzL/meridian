//! Chunk tiles: what the map keeps of a chunk.
//!
//! The game's chunk is read once on the main thread into a
//! [`ChunkSnapshot`]; [`ColorTable::prepare`] attaches how each of its
//! blocks and biomes looks (also main thread, cached by name). A worker
//! then builds the compact [`ChunkTile`]: per column the flat colours in
//! both styles, the height, the water and the light. Shading against
//! neighbours happens later, when a region is drawn.

use std::collections::HashMap;

use meridian_host::{
    BiomeAppearance, BlockAppearance, CHUNK_COLUMNS, ChunkPos, ChunkSnapshot, DimensionId, MapHost,
};

use crate::color::{self, accurate_color, vanilla_color};

/// One column of a tile, 14 bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct TileColumn {
    /// Flat colour in the accurate style (overlays such as glass blended in).
    pub accurate: [u8; 3],
    /// Flat colour in the vanilla map style.
    pub vanilla: [u8; 3],
    /// The water's (or lava's) colour, when `water_depth > 0`.
    pub water: [u8; 3],
    pub water_depth: u8,
    /// Y of the ground the map shows.
    pub height: i16,
    /// Block light (high nibble) and sky light (low nibble).
    pub light: u8,
    /// No block to draw: the column is empty (void, unloaded part).
    pub empty: bool,
}

impl TileColumn {
    pub fn block_light(&self) -> u8 {
        self.light >> 4
    }

    pub fn sky_light(&self) -> u8 {
        self.light & 0x0F
    }
}

/// The map's copy of one chunk.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChunkTile {
    pub pos: ChunkPos,
    /// 256 columns, x fastest.
    pub columns: Box<[TileColumn; CHUNK_COLUMNS]>,
}

impl ChunkTile {
    pub fn empty(pos: ChunkPos) -> Self {
        Self {
            pos,
            columns: Box::new(
                [TileColumn {
                    empty: true,
                    ..Default::default()
                }; CHUNK_COLUMNS],
            ),
        }
    }

    pub fn column(&self, x: usize, z: usize) -> &TileColumn {
        &self.columns[z * 16 + x]
    }
}

/// A snapshot with the looks of its blocks and biomes attached; `Send`, so
/// a worker can turn it into a tile.
#[derive(Clone, Debug)]
pub struct PreparedChunk {
    pub snapshot: ChunkSnapshot,
    pub blocks: Vec<BlockAppearance>,
    pub biomes: Vec<BiomeAppearance>,
    /// Use biome tints in the vanilla style too.
    pub biome_colors_in_vanilla: bool,
}

impl PreparedChunk {
    pub fn dimension(&self) -> &DimensionId {
        &self.snapshot.dimension
    }
}

/// Looks of blocks and biomes by name, asked from the host once each and
/// kept until resources reload.
#[derive(Debug, Default)]
pub struct ColorTable {
    blocks: HashMap<String, BlockAppearance>,
    biomes: HashMap<String, BiomeAppearance>,
}

impl ColorTable {
    pub fn clear(&mut self) {
        self.blocks.clear();
        self.biomes.clear();
    }

    pub fn len(&self) -> usize {
        self.blocks.len() + self.biomes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn block(&mut self, host: &dyn MapHost, name: &str) -> BlockAppearance {
        if let Some(a) = self.blocks.get(name) {
            return *a;
        }
        let a = host.block_appearance(name);
        self.blocks.insert(name.to_owned(), a);
        a
    }

    pub fn biome(&mut self, host: &dyn MapHost, name: &str) -> BiomeAppearance {
        if let Some(a) = self.biomes.get(name) {
            return *a;
        }
        let a = host.biome_appearance(name);
        self.biomes.insert(name.to_owned(), a);
        a
    }

    pub fn prepare(
        &mut self,
        host: &dyn MapHost,
        snapshot: ChunkSnapshot,
        biome_colors_in_vanilla: bool,
    ) -> PreparedChunk {
        let blocks = snapshot
            .blocks
            .iter()
            .map(|b| self.block(host, b))
            .collect();
        let biomes = snapshot
            .biomes
            .iter()
            .map(|b| self.biome(host, b))
            .collect();
        PreparedChunk {
            snapshot,
            blocks,
            biomes,
            biome_colors_in_vanilla,
        }
    }
}

/// Builds a tile; pure and cheap (a few microseconds), run on a worker.
/// Indexes outside the palettes read as air, so a bad snapshot draws holes
/// instead of panicking.
pub fn build_tile(chunk: &PreparedChunk) -> ChunkTile {
    let mut tile = ChunkTile::empty(chunk.snapshot.pos);
    let air = BlockAppearance {
        invisible: true,
        ..Default::default()
    };
    let block = |i: u16| chunk.blocks.get(i as usize).copied().unwrap_or(air);
    let no_biome = BiomeAppearance::default();
    for (i, col) in chunk
        .snapshot
        .columns
        .iter()
        .take(CHUNK_COLUMNS)
        .enumerate()
    {
        let biome = chunk.biomes.get(col.biome as usize).unwrap_or(&no_biome);
        let ground = block(col.block);
        let out = &mut tile.columns[i];
        out.height = col.height;
        out.light = (col.block_light.min(15) << 4) | col.sky_light.min(15);
        if ground.invisible && col.fluid.is_none() {
            out.empty = true;
            continue;
        }
        out.empty = false;
        let mut accurate = accurate_color(&ground, biome);
        let mut vanilla = vanilla_color(&ground, biome, chunk.biome_colors_in_vanilla);
        if let Some(overlay) = col.overlay.map(block).filter(|o| !o.invisible) {
            accurate = color::blend(accurate, accurate_color(&overlay, biome), 0.45);
            vanilla = color::blend(
                vanilla,
                vanilla_color(&overlay, biome, chunk.biome_colors_in_vanilla),
                0.45,
            );
        }
        out.accurate = color::channels(accurate);
        out.vanilla = color::channels(vanilla);
        if let Some((fluid, depth)) = col.fluid {
            let fluid = block(fluid);
            out.water = color::channels(accurate_color(&fluid, biome));
            out.water_depth = depth.max(1);
        }
    }
    tile
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use meridian_host::{ColumnSample, ReadMode, Tint, WorldContext};
    use std::cell::{Cell, RefCell};
    use std::path::PathBuf;

    /// A host whose world is a flat grass plain at y 64 with a lake in
    /// chunk (1, 0); counts appearance lookups.
    #[derive(Default)]
    pub(crate) struct FlatHost {
        pub lookups: Cell<usize>,
        pub data: PathBuf,
        pub requests: RefCell<Vec<ChunkPos>>,
    }

    impl MapHost for FlatHost {
        fn context(&self) -> Option<WorldContext> {
            None
        }
        fn player(&self) -> Option<meridian_host::PlayerState> {
            None
        }
        fn request_chunk(&self, chunk: ChunkPos, _mode: ReadMode) {
            self.requests.borrow_mut().push(chunk);
        }
        fn take_chunks(&self) -> Vec<ChunkSnapshot> {
            self.requests
                .borrow_mut()
                .drain(..)
                .map(flat_chunk)
                .collect()
        }
        fn block_appearance(&self, block: &str) -> BlockAppearance {
            self.lookups.set(self.lookups.get() + 1);
            match block {
                "minecraft:grass_block" => BlockAppearance {
                    map_color: 0x7FB238,
                    texture_color: 0x939393,
                    tint: Tint::Grass,
                    ..Default::default()
                },
                "minecraft:water" => BlockAppearance {
                    map_color: 0x4040FF,
                    texture_color: 0xB0B0B0,
                    tint: Tint::Water,
                    translucent: true,
                    ..Default::default()
                },
                "minecraft:sand" => BlockAppearance {
                    map_color: 0xF7E9A3,
                    texture_color: 0xDBCFA3,
                    ..Default::default()
                },
                _ => BlockAppearance {
                    invisible: true,
                    ..Default::default()
                },
            }
        }
        fn biome_appearance(&self, _biome: &str) -> BiomeAppearance {
            self.lookups.set(self.lookups.get() + 1);
            BiomeAppearance {
                grass: 0x91BD59,
                foliage: 0x77AB2F,
                dry_foliage: 0xA0A69C,
                water: 0x3F76E4,
            }
        }
        fn day_time(&self) -> Option<i64> {
            Some(6_000)
        }
        fn data_dir(&self) -> PathBuf {
            self.data.clone()
        }
        fn config_dir(&self) -> PathBuf {
            self.data.join("config")
        }
    }

    pub(crate) fn flat_chunk(pos: ChunkPos) -> ChunkSnapshot {
        let lake = pos == ChunkPos::new(1, 0);
        let column = |x: usize, z: usize| ColumnSample {
            block: if lake { 2 } else { 0 },
            height: if lake {
                60
            } else {
                64 + (x as i16 + z as i16) / 8
            },
            overlay: None,
            fluid: lake.then_some((1, 4)),
            biome: 0,
            block_light: 0,
            sky_light: 15,
        };
        ChunkSnapshot {
            dimension: DimensionId::new(DimensionId::OVERWORLD),
            pos,
            blocks: vec![
                "minecraft:grass_block".into(),
                "minecraft:water".into(),
                "minecraft:sand".into(),
            ],
            biomes: vec!["minecraft:plains".into()],
            columns: (0..256).map(|i| column(i % 16, i / 16)).collect(),
        }
    }

    #[test]
    fn builds_grass_and_water() {
        let host = FlatHost::default();
        let mut table = ColorTable::default();
        let grass = build_tile(&table.prepare(&host, flat_chunk(ChunkPos::new(0, 0)), false));
        let c = grass.column(3, 5);
        assert!(!c.empty);
        assert_eq!(c.vanilla, color::channels(0x7FB238));
        assert_eq!(
            c.accurate,
            color::channels(color::multiply(0x939393, 0x91BD59))
        );
        assert_eq!(
            (c.water_depth, c.height, c.sky_light(), c.block_light()),
            (0, 65, 15, 0)
        );

        let lake = build_tile(&table.prepare(&host, flat_chunk(ChunkPos::new(1, 0)), false));
        let w = lake.column(0, 0);
        assert_eq!(w.water_depth, 4);
        assert_eq!(
            w.water,
            color::channels(color::multiply(0xB0B0B0, 0x3F76E4))
        );
        assert_eq!(
            w.vanilla,
            color::channels(0xF7E9A3),
            "ground under the water is sand"
        );
    }

    #[test]
    fn appearances_are_asked_once() {
        let host = FlatHost::default();
        let mut table = ColorTable::default();
        for x in 0..10 {
            table.prepare(&host, flat_chunk(ChunkPos::new(x, 0)), false);
        }
        assert_eq!(host.lookups.get(), 4, "three blocks and one biome");
        table.clear();
        table.prepare(&host, flat_chunk(ChunkPos::new(0, 0)), false);
        assert_eq!(host.lookups.get(), 8);
    }

    #[test]
    fn bad_indexes_draw_holes() {
        let host = FlatHost::default();
        let mut snap = flat_chunk(ChunkPos::new(0, 0));
        snap.columns[0].block = 99;
        snap.columns.truncate(10);
        let tile = build_tile(&ColorTable::default().prepare(&host, snap, false));
        assert!(tile.columns[0].empty);
        assert!(!tile.columns[1].empty);
        assert!(tile.columns[200].empty, "missing columns stay empty");
    }
}
