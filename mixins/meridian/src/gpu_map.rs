//! The world map's terrain on the GPU.
//!
//! The map screen hands over the regions it shows, each with its rectangle
//! on screen and (when the region is in memory) its pixels. The render
//! thread keeps one texture per region and level of detail (512, 128 or 32
//! texels a side, picked by how big the region is on screen), uploads a few
//! per frame when they are missing or redrawn, and draws them as quads over
//! a black background. Textures stay cached after their regions leave
//! memory, so zooming out shows what was seen without loading it again;
//! they are dropped when the world or the server's map changes.

use std::sync::Arc;

use parking_lot::Mutex;

/// One region on screen.
#[derive(Debug, Clone)]
pub struct RegionQuad {
    pub pos: [i32; 2],
    /// Left, top, right, bottom in GUI pixels.
    pub rect: [f32; 4],
    /// `size` x `size` ARGB when the region is in memory and drawn.
    pub pixels: Option<Arc<Vec<u32>>>,
    pub revision: u64,
    /// Texels a side of `pixels`: 512 for terrain, which is drawn at a level
    /// of detail; other sizes (Seed Atlas tiles) are drawn as they are.
    pub size: u32,
    /// Opacity of the texels, 0..=255.
    pub alpha: u8,
    /// Which set the position belongs to (Seed Atlas tiles: their step), so
    /// equal positions of different sets keep their own textures.
    pub layer: u32,
}

/// A marker icon on screen.
#[derive(Debug, Clone, Copy)]
pub struct IconQuad {
    pub icon: usize,
    pub rect: [f32; 4],
    /// Opacity 0..=1 (completed structures fade).
    pub alpha: f32,
}

/// Icons the map can draw, RGBA, uploaded once.
pub static ICONS: std::sync::OnceLock<Vec<(u32, u32, Vec<u8>)>> = std::sync::OnceLock::new();

#[derive(Debug, Clone)]
pub struct WorldMapFrame {
    pub gui: [f32; 2],
    /// Which map the regions belong to (world, dimension, cave layer).
    pub world: u64,
    pub regions: Vec<RegionQuad>,
    /// The Seed Atlas biome layer: drawn first, under the terrain, or over
    /// it when `seed_above` (biome highlighting, "Over Explored Terrain").
    pub background: Vec<RegionQuad>,
    pub seed_above: bool,
    /// Drawn last, over the terrain.
    pub icons: Vec<IconQuad>,
    pub at: std::time::Instant,
}

pub static FRAME: Mutex<Option<WorldMapFrame>> = Mutex::new(None);

pub fn set_frame(frame: Option<WorldMapFrame>) {
    *FRAME.lock() = frame;
}

/// Set when the region textures belong to a world no longer shown.
pub static FORGET_REGIONS: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// Drops every region texture at the next draw (the world or the server's
/// map changed: nothing cached is shown again soon).
pub fn forget_regions() {
    FORGET_REGIONS.store(true, std::sync::atomic::Ordering::Relaxed);
}
