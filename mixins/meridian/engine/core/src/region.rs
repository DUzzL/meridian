//! A region's tiles and its rendered pixels.
//!
//! Pixels are redrawn per chunk, only for chunks marked dirty: a chunk is
//! dirty when its tile changed or a neighbour it is shaded against (north
//! or west) changed. Each redraw gives [`Region::revision`] a new number
//! from one counter of the process, so the renderer knows to upload the
//! texture again and never takes one region's image for another's.

use meridian_host::{CHUNK_COLUMNS, CHUNK_SIZE, ChunkPos};

use crate::color::{self, Shading};
use crate::config::BlockColors;
use crate::geom::{
    ChunkMask, REGION_BLOCKS, REGION_CHUNK_COUNT, REGION_CHUNKS, REGION_PIXELS, RegionPos,
    chunk_at, chunk_index,
};
use crate::tile::{ChunkTile, TileColumn};

/// A transparent pixel: nothing explored there.
pub const UNEXPLORED: u32 = 0;

/// A revision no image had before: every region and overview of every map
/// draws from this one counter (texture caches key by it).
pub fn next_revision() -> u64 {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

/// Heights just outside a region, which its north row and west column are
/// shaded against. `None` entries (or `None` edges) are unknown.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RegionEdges {
    /// The row north of the region, x = 0..512.
    pub north: Option<Vec<Option<i16>>>,
    /// The column west of the region, z = 0..512.
    pub west: Option<Vec<Option<i16>>>,
}

#[derive(Clone, Debug)]
pub struct Region {
    pub pos: RegionPos,
    tiles: Vec<Option<Box<ChunkTile>>>,
    /// ARGB, row-major, 512 × 512; empty until first drawn.
    pixels: std::sync::Arc<Vec<u32>>,
    dirty: ChunkMask,
    /// Tiles changed since the region was last saved.
    pub unsaved: bool,
    /// New whenever pixels change ([`next_revision`]); 0 before the first
    /// draw.
    pub revision: u64,
    /// Its file must not be written (from a newer Meridian, or unreadable).
    pub read_only: bool,
    /// The cache's clock when last used, for eviction.
    pub last_used: u64,
    /// Chunks with a tile.
    count: usize,
}

/// The height neighbours are shaded against: the water surface where there
/// is water.
fn surface(col: &TileColumn) -> Option<i16> {
    (!col.empty).then(|| col.height.saturating_add(col.water_depth as i16))
}

impl Region {
    pub fn new(pos: RegionPos) -> Self {
        Self {
            pos,
            tiles: vec![None; REGION_CHUNK_COUNT],
            pixels: std::sync::Arc::new(Vec::new()),
            dirty: ChunkMask::default(),
            unsaved: false,
            revision: 0,
            read_only: false,
            last_used: 0,
            count: 0,
        }
    }

    pub fn tile(&self, chunk: ChunkPos) -> Option<&ChunkTile> {
        self.tiles[chunk_index(chunk)].as_deref()
    }

    pub fn tiles(&self) -> impl Iterator<Item = &ChunkTile> {
        self.tiles.iter().filter_map(|t| t.as_deref())
    }

    pub fn tile_count(&self) -> usize {
        self.count
    }

    /// About the memory the region takes: its tiles (with a typical biome
    /// list each) and its pixels.
    pub fn approx_bytes(&self) -> usize {
        const TILE: usize = std::mem::size_of::<ChunkTile>()
            + std::mem::size_of::<[TileColumn; CHUNK_COLUMNS]>()
            + 96;
        std::mem::size_of::<Self>()
            + REGION_CHUNK_COUNT * std::mem::size_of::<Option<Box<ChunkTile>>>()
            + self.count * TILE
            + self.pixels.len() * 4
    }

    /// Stores a tile and marks it and its south and east neighbours inside
    /// this region dirty. Returns the neighbours that lie in other regions
    /// (the caller marks those).
    pub fn set_tile(&mut self, tile: ChunkTile) -> Vec<ChunkPos> {
        debug_assert_eq!(RegionPos::of_chunk(tile.pos), self.pos);
        let pos = tile.pos;
        if self.tiles[chunk_index(pos)].as_deref() == Some(&tile) {
            return Vec::new();
        }
        if self.tiles[chunk_index(pos)].is_none() {
            self.count += 1;
        }
        self.tiles[chunk_index(pos)] = Some(Box::new(tile));
        self.unsaved = true;
        let mut outside = Vec::new();
        for chunk in [
            pos,
            ChunkPos::new(pos.x + 1, pos.z),
            ChunkPos::new(pos.x, pos.z + 1),
        ] {
            if RegionPos::of_chunk(chunk) == self.pos {
                self.dirty.insert(chunk_index(chunk));
            } else {
                outside.push(chunk);
            }
        }
        outside
    }

    /// Marks one chunk of this region for redrawing.
    pub fn mark_dirty(&mut self, chunk: ChunkPos) {
        self.dirty.insert(chunk_index(chunk));
    }

    /// Marks every chunk with a tile for redrawing (style or light changed).
    pub fn mark_all_dirty(&mut self) {
        for (i, t) in self.tiles.iter().enumerate() {
            if t.is_some() {
                self.dirty.insert(i);
            }
        }
    }

    pub fn has_dirty(&self) -> bool {
        !self.dirty.is_empty()
    }

    pub fn dirty_count(&self) -> usize {
        self.dirty.len()
    }

    /// The region's pixels (ARGB, row-major, 512 per row); empty before the
    /// first draw.
    pub fn pixels(&self) -> &[u32] {
        &self.pixels
    }

    /// The pixels as a shared buffer, for a renderer on another thread;
    /// `None` before the first draw. Drawing again copies the buffer only
    /// while someone still holds it.
    pub fn shared_pixels(&self) -> Option<std::sync::Arc<Vec<u32>>> {
        (!self.pixels.is_empty()).then(|| self.pixels.clone())
    }

    /// Surface height at a column of this region (local 0..512).
    pub fn height_at(&self, x: i32, z: i32) -> Option<i16> {
        let chunk = ChunkPos::new(
            self.pos.x * REGION_CHUNKS + x.div_euclid(CHUNK_SIZE),
            self.pos.z * REGION_CHUNKS + z.div_euclid(CHUNK_SIZE),
        );
        let tile = self.tile(chunk)?;
        surface(tile.column(
            x.rem_euclid(CHUNK_SIZE) as usize,
            z.rem_euclid(CHUNK_SIZE) as usize,
        ))
    }

    /// The south row's heights, for the region below to shade against.
    pub fn south_edge(&self) -> Vec<Option<i16>> {
        (0..REGION_BLOCKS)
            .map(|x| self.height_at(x, REGION_BLOCKS - 1))
            .collect()
    }

    /// The east column's heights, for the region to the right.
    pub fn east_edge(&self) -> Vec<Option<i16>> {
        (0..REGION_BLOCKS)
            .map(|z| self.height_at(REGION_BLOCKS - 1, z))
            .collect()
    }

    /// Redraws up to `max_chunks` dirty chunks; returns how many it drew.
    pub fn render_dirty(
        &mut self,
        edges: &RegionEdges,
        shading: &Shading,
        max_chunks: usize,
    ) -> usize {
        if self.dirty.is_empty() || max_chunks == 0 {
            return 0;
        }
        if self.pixels.is_empty() {
            self.pixels = std::sync::Arc::new(vec![UNEXPLORED; REGION_PIXELS]);
        }
        let todo: Vec<usize> = self.dirty.iter().take(max_chunks).collect();
        for &index in &todo {
            self.render_chunk(index, edges, shading);
            self.dirty.remove(index);
        }
        self.revision = next_revision();
        todo.len()
    }

    fn render_chunk(&mut self, index: usize, edges: &RegionEdges, shading: &Shading) {
        let chunk = chunk_at(self.pos, index);
        let (bx, bz) = chunk.min_block();
        let (rx, rz) = self.pos.min_block();
        let (ox, oz) = (bx - rx, bz - rz);
        let mut out = [UNEXPLORED; 256];
        for z in 0..CHUNK_SIZE {
            for x in 0..CHUNK_SIZE {
                let (lx, lz) = (ox + x, oz + z);
                let pixel = match self.tiles[index].as_deref() {
                    None => UNEXPLORED,
                    Some(tile) => {
                        let col = tile.column(x as usize, z as usize);
                        let north = if lz > 0 {
                            self.height_at(lx, lz - 1)
                        } else {
                            edges.north.as_ref().and_then(|e| e[lx as usize])
                        };
                        let west = if lx > 0 {
                            self.height_at(lx - 1, lz)
                        } else {
                            edges.west.as_ref().and_then(|e| e[lz as usize])
                        };
                        shade(col, north, west, shading)
                    }
                };
                out[(z * CHUNK_SIZE + x) as usize] = pixel;
            }
        }
        let pixels = std::sync::Arc::make_mut(&mut self.pixels);
        for z in 0..CHUNK_SIZE {
            let row = ((oz + z) * REGION_BLOCKS + ox) as usize;
            let src = (z * CHUNK_SIZE) as usize;
            pixels[row..row + CHUNK_SIZE as usize]
                .copy_from_slice(&out[src..src + CHUNK_SIZE as usize]);
        }
    }
}

/// One map pixel from a column and its neighbours' heights.
pub fn shade(col: &TileColumn, north: Option<i16>, west: Option<i16>, sh: &Shading) -> u32 {
    if col.empty || sh.floor.is_some_and(|f| i32::from(col.height) < f) {
        return UNEXPLORED;
    }
    let base = match sh.colors {
        BlockColors::Accurate => col.accurate,
        BlockColors::Vanilla => col.vanilla,
    };
    let base = color::rgb(base[0], base[1], base[2]);
    let mut c = if col.water_depth > 0 {
        let [r, g, b] = col.water;
        color::water_over(base, color::rgb(r, g, b), col.water_depth, sh.transparency)
    } else {
        let mut f = color::slope_factor(sh.slopes, col.height, north, west);
        if sh.depth {
            f *= color::depth_factor(col.height);
        }
        color::scale(base, f)
    };
    if sh.lighting {
        c = color::scale(
            c,
            if sh.cave && sh.legible {
                // Legible: caves are lit by the map, torches still brighten.
                0.7 + 0.3 * col.block_light() as f32 / 15.0
            } else if sh.cave {
                // Dark caves: what torches light stands out.
                0.35 + 0.65 * col.block_light() as f32 / 15.0
            } else {
                color::light_factor(col.block_light(), col.sky_light(), sh.daylight)
            },
        );
    }
    color::opaque(c)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::TerrainStyle;
    use crate::tile::{ColorTable, build_tile, tests::FlatHost, tests::flat_chunk};

    fn tile(x: i32, z: i32) -> ChunkTile {
        let host = FlatHost::default();
        build_tile(&ColorTable::default().prepare(&host, flat_chunk(ChunkPos::new(x, z)), false))
    }

    fn noon() -> Shading {
        Shading::new(&TerrainStyle::default(), Some(6_000))
    }

    #[test]
    fn set_tile_marks_neighbours() {
        let mut r = Region::new(RegionPos::new(0, 0));
        assert!(r.set_tile(tile(0, 0)).is_empty());
        assert_eq!(r.dirty_count(), 3, "itself, east and south");
        assert!(r.unsaved);
        let outside = r.set_tile(tile(31, 31));
        assert_eq!(outside, [ChunkPos::new(32, 31), ChunkPos::new(31, 32)]);
    }

    #[test]
    fn identical_tile_changes_nothing() {
        let mut r = Region::new(RegionPos::new(0, 0));
        r.set_tile(tile(2, 2));
        r.render_dirty(&RegionEdges::default(), &noon(), usize::MAX);
        r.unsaved = false;
        let rev = r.revision;
        r.set_tile(tile(2, 2));
        assert!(!r.has_dirty());
        assert!(!r.unsaved);
        assert_eq!(r.revision, rev);
    }

    #[test]
    fn renders_only_dirty_chunks_within_budget() {
        let mut r = Region::new(RegionPos::new(-1, -1));
        for x in -4..0 {
            r.set_tile(tile(x, -1));
        }
        let dirty = r.dirty_count();
        assert_eq!(r.render_dirty(&RegionEdges::default(), &noon(), 2), 2);
        assert_eq!(r.dirty_count(), dirty - 2);
        r.render_dirty(&RegionEdges::default(), &noon(), usize::MAX);
        assert!(!r.has_dirty());
        let mut other = Region::new(RegionPos::new(-1, -1));
        other.set_tile(tile(-1, -1));
        other.render_dirty(&RegionEdges::default(), &noon(), usize::MAX);
        assert!(
            other.revision > r.revision,
            "revisions never repeat across regions"
        );
        let (rx, rz) = r.pos.min_block();
        let (bx, bz) = ChunkPos::new(-1, -1).min_block();
        let p = r.pixels()[((bz - rz) * REGION_BLOCKS + (bx - rx)) as usize];
        assert_eq!(p >> 24, 0xFF, "explored pixels are opaque");
        assert_eq!(r.pixels()[0], UNEXPLORED);
    }

    #[test]
    fn slope_against_north_neighbour() {
        let col = TileColumn {
            accurate: [100, 100, 100],
            height: 70,
            light: 0x0F,
            ..Default::default()
        };
        let mut sh = noon();
        sh.slopes = crate::config::Slopes::Vanilla;
        sh.depth = false;
        let up = shade(&col, Some(69), None, &sh);
        let flat = shade(&col, Some(70), None, &sh);
        let down = shade(&col, Some(71), None, &sh);
        assert!(up > flat && flat > down);
    }

    #[test]
    fn edges_and_heights() {
        let mut r = Region::new(RegionPos::new(0, 0));
        r.set_tile(tile(0, 31));
        r.set_tile(tile(31, 0));
        let south = r.south_edge();
        assert_eq!(south[0], Some(64 + 15 / 8));
        assert_eq!(south[100], None);
        let east = r.east_edge();
        assert_eq!(east[0], Some(64 + 15 / 8));
        assert_eq!(r.height_at(5, 5), None);
    }

    #[test]
    fn water_is_drawn_over_ground() {
        let mut r = Region::new(RegionPos::new(0, 0));
        r.set_tile(tile(1, 0));
        r.render_dirty(&RegionEdges::default(), &noon(), usize::MAX);
        let p = r.pixels()[16];
        let [_, _, b] = color::channels(p);
        let [red, _, _] = color::channels(p);
        assert!(b > red, "lake pixels are blue");
        assert_eq!(
            r.height_at(16, 0),
            Some(64),
            "neighbours see the water surface"
        );
    }

    #[test]
    fn dark_caves_are_darker_than_legible_ones() {
        let col = crate::tile::TileColumn {
            accurate: [200, 200, 200],
            vanilla: [200, 200, 200],
            height: 20,
            ..Default::default()
        };
        let base = Shading::new(&TerrainStyle::default(), Some(6000));
        let lum = |c: u32| ((c >> 16) & 0xFF) + ((c >> 8) & 0xFF) + (c & 0xFF);
        let legible = shade(&col, Some(20), Some(20), &base.for_cave(true, true));
        let dark = shade(&col, Some(20), Some(20), &base.for_cave(true, false));
        assert!(lum(dark) < lum(legible));
        let lit = crate::tile::TileColumn { light: 0xF0, ..col };
        assert!(
            lum(shade(&lit, Some(20), Some(20), &base.for_cave(true, false))) > lum(dark),
            "torches light dark caves"
        );
        // Cave depth: ground deeper below the layer is left out.
        let deep = base.for_cave(true, true).with_floor(Some(25));
        assert_eq!(shade(&col, Some(20), Some(20), &deep), UNEXPLORED);
        assert_ne!(
            shade(&col, Some(20), Some(20), &deep.with_floor(Some(20))),
            UNEXPLORED
        );
    }
}
