//! The minimap's terrain image, composed from the regions' pixels for the
//! GPU, which turns it and cuts it to the minimap.
//!
//! The image is a window of `size` × `size` texels around the player, each
//! texel `scale` blocks wide: one block up close, about one physical pixel
//! of the minimap further out (a power of two, so texels line up with
//! chunks and regions). Its texture wraps around: absolute texel (x, z) is
//! stored at (x mod size, z mod size). The window moves in whole cells of
//! [`CELL`] texels, so walking composes and uploads only the cells that
//! scroll in, and a redrawn region only its own cells, of which only those
//! whose texels changed are uploaded.

use std::collections::{HashMap, HashSet};

use meridian_host::ChunkPos;

use crate::color;
use crate::geom::{REGION_BLOCKS, RegionPos};
use crate::region::Region;

/// Texels per cell side: what is composed and uploaded at once.
pub const CELL: u32 = 32;
/// The largest image side, texels.
pub const MAX_SIZE: u32 = 2048;
/// The coarsest texel, blocks (a cell then still lies in one region).
pub const MAX_SCALE: i32 = 16;

/// What the image shows besides the regions' pixels; a change composes it
/// anew.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Style {
    /// Ground whose block light lies in this range is tinted (spawnable).
    pub light_overlay: Option<[u8; 2]>,
    /// Slime chunks of this seed are tinted green.
    pub slime: Option<i64>,
    /// Changes when earlier pixels no longer belong to this map (another
    /// world, dimension or cave layer).
    pub epoch: u64,
}

/// What the minimap shows this frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct View {
    /// The player's position (x, z), blocks.
    pub center: [f64; 2],
    /// Blocks across the minimap's side.
    pub side_blocks: f32,
    /// Blocks per physical pixel of the minimap.
    pub blocks_per_px: f32,
    pub style: Style,
}

/// Blocks per texel and texels per side for a map `side_blocks` wide that
/// shows `blocks_per_px` blocks per physical pixel: the largest power of
/// two not above that (so a texel is about a pixel when zoomed out), and
/// the map's diagonal plus a cell either way for the snapped origin. A
/// window that would exceed [`MAX_SIZE`] takes coarser texels.
pub fn fit(side_blocks: f32, blocks_per_px: f32) -> (i32, u32) {
    let mut scale = 1;
    while scale < MAX_SCALE && (scale * 2) as f32 <= blocks_per_px {
        scale *= 2;
    }
    loop {
        let diagonal =
            (side_blocks.max(1.0) * std::f32::consts::SQRT_2 / scale as f32).ceil() as u32;
        let size = (diagonal + 2 + 2 * CELL).div_ceil(CELL) * CELL;
        if size <= MAX_SIZE || scale == MAX_SCALE {
            return (scale, size.min(MAX_SIZE));
        }
        scale *= 2;
    }
}

/// A region as the image last saw it: its revision and its pixel buffer
/// (a region loaded anew has another).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Stamp {
    revision: u64,
    pixels: usize,
}

/// What changed since the last [`MinimapImage::take_changes`].
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Changes {
    /// The image is new (size, scale or style changed): upload all of it.
    pub reset: bool,
    /// Otherwise the cells to upload, in cells from the texture's corner.
    pub cells: Vec<[u32; 2]>,
}

/// The composed image and what it was composed from.
#[derive(Debug, Default)]
pub struct MinimapImage {
    scale: i32,
    size: u32,
    style: Style,
    /// The window's north-west cell, in absolute cells.
    origin: Option<[i32; 2]>,
    /// The absolute cell each slot of the texture holds (row-major).
    slots: Vec<Option<[i32; 2]>>,
    /// ARGB texels in texture order (absolute texel mod size).
    pixels: Vec<u32>,
    /// The regions under the window as last composed.
    regions: HashMap<RegionPos, Option<Stamp>>,
    changed: Vec<[u32; 2]>,
    reset: bool,
    /// Cells composed so far (for tests and measurements).
    composed: usize,
}

impl MinimapImage {
    /// Brings the image up to date for `view`: composes the cells that
    /// scrolled in and those of regions redrawn since, from `region`.
    pub fn update<'a>(&mut self, view: &View, region: impl Fn(RegionPos) -> Option<&'a Region>) {
        let (scale, size) = fit(view.side_blocks, view.blocks_per_px);
        if (scale, size, view.style) != (self.scale, self.size, self.style) {
            let n = (size / CELL) as usize;
            *self = Self {
                scale,
                size,
                style: view.style,
                slots: vec![None; n * n],
                pixels: vec![0; (size * size) as usize],
                reset: true,
                composed: self.composed,
                ..Self::default()
            };
        }
        let n = (size / CELL) as i32;
        let cell_blocks = CELL as i32 * scale;
        let origin = view
            .center
            .map(|b| ((b / scale as f64 - size as f64 / 2.0) / CELL as f64).floor() as i32);
        // The regions under the window, and which were redrawn (or came or
        // went) since.
        let r0 = RegionPos::of_block(origin[0] * cell_blocks, origin[1] * cell_blocks);
        let r1 = RegionPos::of_block(
            (origin[0] + n) * cell_blocks - 1,
            (origin[1] + n) * cell_blocks - 1,
        );
        let mut redrawn = HashSet::new();
        for x in r0.x..=r1.x {
            for z in r0.z..=r1.z {
                let pos = RegionPos::new(x, z);
                let stamp = region(pos)
                    .filter(|r| !r.pixels().is_empty())
                    .map(|r| Stamp {
                        revision: r.revision,
                        pixels: r.pixels().as_ptr() as usize,
                    });
                if self.regions.insert(pos, stamp) != Some(stamp) {
                    redrawn.insert(pos);
                }
            }
        }
        self.regions
            .retain(|p, _| (r0.x..=r1.x).contains(&p.x) && (r0.z..=r1.z).contains(&p.z));
        if self.origin == Some(origin) && redrawn.is_empty() {
            return;
        }
        self.origin = Some(origin);
        let mut cell_pixels = [0u32; (CELL * CELL) as usize];
        for sz in 0..n {
            for sx in 0..n {
                let cell = [
                    origin[0] + (sx - origin[0]).rem_euclid(n),
                    origin[1] + (sz - origin[1]).rem_euclid(n),
                ];
                let pos = RegionPos::of_block(cell[0] * cell_blocks, cell[1] * cell_blocks);
                let slot = (sz * n + sx) as usize;
                if self.slots[slot] == Some(cell) && !redrawn.contains(&pos) {
                    continue;
                }
                self.slots[slot] = Some(cell);
                self.compose(cell, region(pos), &mut cell_pixels);
                let at = [sx as u32, sz as u32];
                if self.store(at, &cell_pixels) && !self.reset {
                    self.changed.push(at);
                }
            }
        }
    }

    /// One cell's texels from its region's pixels, with the overlays.
    fn compose(&mut self, cell: [i32; 2], region: Option<&Region>, out: &mut [u32]) {
        self.composed += 1;
        out.fill(0);
        let (k, c) = (self.scale, CELL as i32);
        let (bx0, bz0) = (cell[0] * c * k, cell[1] * c * k);
        if let Some(r) = region
            && !r.pixels().is_empty()
        {
            let (rx, rz) = r.pos.min_block();
            let pixels = r.pixels();
            for row in 0..c {
                let start = ((bz0 - rz + row * k) * REGION_BLOCKS + bx0 - rx) as usize;
                let dst = &mut out[(row * c) as usize..][..CELL as usize];
                if k == 1 {
                    dst.copy_from_slice(&pixels[start..start + CELL as usize]);
                } else {
                    for (i, d) in dst.iter_mut().enumerate() {
                        *d = pixels[start + i * k as usize];
                    }
                }
            }
            if let Some([lo, hi]) = self.style.light_overlay {
                for (i, p) in out.iter_mut().enumerate() {
                    let (bx, bz) = (bx0 + (i as i32 % c) * k, bz0 + (i as i32 / c) * k);
                    if *p >> 24 == 0 {
                        continue;
                    }
                    let Some(tile) = r.tile(ChunkPos::containing(bx, bz)) else {
                        continue;
                    };
                    let col = tile.column(bx.rem_euclid(16) as usize, bz.rem_euclid(16) as usize);
                    if !col.empty && col.water_depth == 0 && (lo..=hi).contains(&col.block_light())
                    {
                        // Spawnable ground: tinted light purple, as the reference's overlay.
                        *p = 0xFF00_0000 | color::blend(*p & 0x00FF_FFFF, 0xFF55FF, 0.45);
                    }
                }
            }
        }
        if let Some(seed) = self.style.slime {
            // One test per chunk; a texel never spans two (scale <= 16).
            let per_chunk = (16 / k).max(1) as usize;
            let chunks = (CELL as usize).div_ceil(per_chunk);
            for cz in 0..chunks {
                for cx in 0..chunks {
                    let chunk = ((bx0 >> 4) + cx as i32, (bz0 >> 4) + cz as i32);
                    if !meridian_seedatlas::structures::is_slime_chunk(
                        seed as u64,
                        chunk.0,
                        chunk.1,
                    ) {
                        continue;
                    }
                    for z in cz * per_chunk..(cz + 1) * per_chunk {
                        for x in cx * per_chunk..(cx + 1) * per_chunk {
                            let p = &mut out[z * CELL as usize + x];
                            // Slime chunks: tinted green; unexplored ones half see-through.
                            *p = if *p >> 24 == 0 {
                                0x8022_DD22
                            } else {
                                0xFF00_0000 | color::blend(*p & 0x00FF_FFFF, 0x22DD22, 0.4)
                            };
                        }
                    }
                }
            }
        }
    }

    /// Writes a cell into its slot; whether any texel changed.
    fn store(&mut self, slot: [u32; 2], cell: &[u32]) -> bool {
        let (size, c) = (self.size as usize, CELL as usize);
        let (x0, z0) = (slot[0] as usize * c, slot[1] as usize * c);
        let mut changed = false;
        for row in 0..c {
            let dst = &mut self.pixels[(z0 + row) * size + x0..][..c];
            let src = &cell[row * c..][..c];
            if dst != src {
                dst.copy_from_slice(src);
                changed = true;
            }
        }
        changed
    }

    /// What to upload since the last call.
    pub fn take_changes(&mut self) -> Changes {
        let reset = std::mem::take(&mut self.reset);
        let cells = std::mem::take(&mut self.changed);
        Changes {
            reset,
            cells: if reset { Vec::new() } else { cells },
        }
    }

    /// One cell's texels (row-major, `CELL` × `CELL`).
    pub fn cell(&self, slot: [u32; 2]) -> Vec<u32> {
        let (size, c) = (self.size as usize, CELL as usize);
        let (x0, z0) = (slot[0] as usize * c, slot[1] as usize * c);
        (0..c)
            .flat_map(|row| self.pixels[(z0 + row) * size + x0..][..c].iter().copied())
            .collect()
    }

    /// Texels per side.
    pub fn size(&self) -> u32 {
        self.size
    }

    /// Blocks per texel.
    pub fn scale(&self) -> i32 {
        self.scale
    }

    /// All texels in texture order, ARGB.
    pub fn pixels(&self) -> &[u32] {
        &self.pixels
    }

    /// The window's north-west texel, absolute.
    fn origin_texel(&self) -> [i64; 2] {
        self.origin
            .unwrap_or_default()
            .map(|o| o as i64 * CELL as i64)
    }

    /// Where `pos` (x, z, blocks) lies in the window, texels from its
    /// north-west corner.
    pub fn center(&self, pos: [f64; 2]) -> [f32; 2] {
        let o = self.origin_texel();
        [0, 1].map(|i| (pos[i] / self.scale.max(1) as f64 - o[i] as f64) as f32)
    }

    /// Where the window's north-west texel is stored in the texture.
    pub fn wrap(&self) -> [u32; 2] {
        self.origin_texel()
            .map(|o| o.rem_euclid(self.size.max(1) as i64) as u32)
    }

    /// The block at the window's north-west corner (the chunk grid's phase).
    pub fn origin_block(&self) -> [i32; 2] {
        self.origin_texel().map(|o| (o * self.scale as i64) as i32)
    }

    /// Cells composed so far.
    pub fn composed(&self) -> usize {
        self.composed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::Shading;
    use crate::config::TerrainStyle;
    use crate::region::RegionEdges;
    use crate::tile::tests::{FlatHost, flat_chunk};
    use crate::tile::{ColorTable, build_tile};

    /// Regions whose chunks in `chunks` (inclusive ranges) are explored.
    fn world(chunks: [std::ops::RangeInclusive<i32>; 2]) -> HashMap<RegionPos, Region> {
        let host = FlatHost::default();
        let mut table = ColorTable::default();
        let mut regions: HashMap<RegionPos, Region> = HashMap::new();
        for x in chunks[0].clone() {
            for z in chunks[1].clone() {
                let pos = ChunkPos::new(x, z);
                let tile = build_tile(&table.prepare(&host, flat_chunk(pos), false));
                let r = RegionPos::of_chunk(pos);
                regions
                    .entry(r)
                    .or_insert_with(|| Region::new(r))
                    .set_tile(tile);
            }
        }
        for r in regions.values_mut() {
            draw(r);
        }
        regions
    }

    fn draw(r: &mut Region) {
        let shading = Shading::new(&TerrainStyle::default(), Some(6000));
        r.render_dirty(&RegionEdges::default(), &shading, usize::MAX);
    }

    fn view(center: [f64; 2]) -> View {
        View {
            center,
            side_blocks: 100.0,
            blocks_per_px: 1.0,
            style: Style::default(),
        }
    }

    /// The texel of block (`x`, `z`) as the shader finds it.
    fn texel(img: &MinimapImage, x: i32, z: i32) -> u32 {
        let (k, size) = (img.scale(), img.size() as i64);
        let t = [x.div_euclid(k) as i64, z.div_euclid(k) as i64];
        let [tx, tz] = t.map(|v| v.rem_euclid(size) as usize);
        img.pixels()[tz * size as usize + tx]
    }

    fn pixel(regions: &HashMap<RegionPos, Region>, x: i32, z: i32) -> u32 {
        let pos = RegionPos::of_block(x, z);
        let (rx, rz) = pos.min_block();
        regions.get(&pos).map_or(0, |r| {
            r.pixels()[((z - rz) * REGION_BLOCKS + x - rx) as usize]
        })
    }

    #[test]
    fn the_window_holds_the_regions_pixels_around_the_player() {
        let regions = world([0..=7, 0..=7]);
        let mut img = MinimapImage::default();
        img.update(&view([60.5, 70.5]), |p| regions.get(&p));
        assert_eq!(
            img.take_changes(),
            Changes {
                reset: true,
                cells: Vec::new()
            }
        );
        for z in 20..120 {
            for x in 10..110 {
                assert_eq!(texel(&img, x, z), pixel(&regions, x, z), "block {x} {z}");
            }
        }
        // The map's diagonal fits around the player whatever the snapping.
        let [cx, cz] = img.center([60.5, 70.5]);
        let radius = 100.0 * std::f32::consts::SQRT_2 / 2.0;
        let size = img.size() as f32;
        assert!(
            cx >= radius && cz >= radius && cx + radius <= size && cz + radius <= size,
            "{cx} {cz} in {size}"
        );
    }

    #[test]
    fn walking_composes_only_the_cells_scrolling_in() {
        let regions = world([0..=20, 0..=20]);
        let mut img = MinimapImage::default();
        img.update(&view([160.0, 160.0]), |p| regions.get(&p));
        img.take_changes();
        let n = (img.size() / CELL) as usize;
        let before = img.composed();
        // Within a cell: nothing to do.
        img.update(&view([161.0, 160.5]), |p| regions.get(&p));
        assert_eq!(img.composed(), before);
        assert_eq!(img.take_changes(), Changes::default());
        // A cell east: one column of cells.
        img.update(&view([160.0 + CELL as f64, 160.0]), |p| regions.get(&p));
        assert_eq!(img.composed(), before + n);
        let changes = img.take_changes();
        assert!(!changes.reset && changes.cells.len() <= n);
        assert!(
            changes.cells.iter().all(|c| c[0] == changes.cells[0][0]),
            "one column: {changes:?}"
        );
        for z in 120..200 {
            for x in 140..230 {
                assert_eq!(texel(&img, x, z), pixel(&regions, x, z), "block {x} {z}");
            }
        }
    }

    #[test]
    fn a_redrawn_region_recomposes_only_its_cells() {
        // Explored on both sides of the border between regions 0 and 1.
        let mut regions = world([28..=35, 0..=6]);
        let mut img = MinimapImage::default();
        img.update(&view([512.0, 50.0]), |p| regions.get(&p));
        img.take_changes();
        let n = (img.size() / CELL) as usize;
        let before = img.composed();
        // A lake appears in region 1.
        let host = FlatHost::default();
        let mut lake = flat_chunk(ChunkPos::new(33, 2));
        lake.columns.iter_mut().for_each(|c| c.fluid = Some((1, 4)));
        let tile = build_tile(&ColorTable::default().prepare(&host, lake, false));
        let east = regions.get_mut(&RegionPos::new(1, 0)).unwrap();
        east.set_tile(tile);
        draw(east);
        img.update(&view([512.0, 50.0]), |p| regions.get(&p));
        let recomposed = img.composed() - before;
        assert!(
            recomposed > 0 && recomposed < n * n,
            "{recomposed} of {}",
            n * n
        );
        let changes = img.take_changes();
        assert!(!changes.reset);
        assert!(
            changes.cells.len() <= 2,
            "only the lake's cells changed: {changes:?}"
        );
        for x in 528..544 {
            assert_eq!(texel(&img, x, 40), pixel(&regions, x, 40));
        }
    }

    #[test]
    fn zoomed_out_one_texel_per_pixel() {
        assert_eq!(fit(100.0, 0.5), (1, 224));
        assert_eq!(fit(1000.0, 4.0).0, 4);
        assert_eq!(fit(1000.0, 3.9).0, 2);
        // Too wide for the largest texture: coarser texels.
        let (scale, size) = fit(10_000.0, 1.0);
        assert!(scale >= 8 && size <= MAX_SIZE, "{scale} {size}");
        let regions = world([0..=31, 0..=31]);
        let mut img = MinimapImage::default();
        img.update(
            &View {
                center: [256.0, 256.0],
                side_blocks: 400.0,
                blocks_per_px: 4.0,
                style: Style::default(),
            },
            |p| regions.get(&p),
        );
        assert_eq!(img.scale(), 4);
        for z in (100..400).step_by(4) {
            for x in (100..400).step_by(4) {
                assert_eq!(texel(&img, x, z), pixel(&regions, x, z), "block {x} {z}");
            }
        }
    }

    #[test]
    fn slime_chunks_are_tested_per_chunk() {
        let regions: HashMap<RegionPos, Region> = HashMap::new();
        for blocks_per_px in [1.0, 4.0, 16.0] {
            let style = Style {
                slime: Some(12345),
                ..Style::default()
            };
            let mut img = MinimapImage::default();
            let v = View {
                center: [-300.0, 700.0],
                side_blocks: 300.0 * blocks_per_px,
                blocks_per_px,
                style,
            };
            img.update(&v, |p| regions.get(&p));
            let k = img.scale();
            for z in (500..900).step_by(k as usize) {
                for x in (-500..-100).step_by(k as usize) {
                    let slime =
                        meridian_seedatlas::structures::is_slime_chunk(12345, x >> 4, z >> 4);
                    assert_eq!(
                        texel(&img, x, z) == 0x8022_DD22,
                        slime,
                        "block {x} {z} at scale {k}"
                    );
                }
            }
        }
    }

    /// `cargo test -p meridian-core --release --lib minimap_image -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn walking_cost() {
        let mut regions = world([-48..=47, -48..=47]);
        // The enlarged minimap at about a block per pixel, 1000 blocks across.
        let v = |x: f64| View {
            center: [x, 0.0],
            side_blocks: 1000.0,
            blocks_per_px: 1.0,
            style: Style::default(),
        };
        let mut img = MinimapImage::default();
        let t = std::time::Instant::now();
        img.update(&v(0.0), |p| regions.get(&p));
        let size = img.size();
        let full = t.elapsed();
        // Sprinting: 0.05 blocks a frame at 120 fps; here a block a frame.
        let mut worst = std::time::Duration::ZERO;
        let t = std::time::Instant::now();
        for i in 1..=320 {
            let f = std::time::Instant::now();
            img.update(&v(i as f64), |p| regions.get(&p));
            img.take_changes();
            worst = worst.max(f.elapsed());
        }
        let walk = t.elapsed() / 320;
        // A region under the map redrawn.
        let r = regions.get_mut(&RegionPos::new(0, 0)).unwrap();
        r.mark_all_dirty();
        draw(r);
        let t = std::time::Instant::now();
        img.update(&v(320.0), |p| regions.get(&p));
        let redraw = t.elapsed();
        // What every block step cost before: the whole window, a region
        // lookup per texel (as `Sampler::pixel`).
        let t = std::time::Instant::now();
        let mut old = Vec::with_capacity((size * size) as usize);
        let mut last: Option<(RegionPos, Option<&Region>)> = None;
        for z in 0..size as i32 {
            for x in 0..size as i32 {
                let (bx, bz) = (x - size as i32 / 2, z - size as i32 / 2);
                let pos = RegionPos::of_block(bx, bz);
                let r = match last {
                    Some((p, r)) if p == pos => r,
                    _ => {
                        let r = regions.get(&pos);
                        last = Some((pos, r));
                        r
                    }
                };
                old.push(r.map_or(0, |r| {
                    let (rx, rz) = pos.min_block();
                    r.pixels()[((bz - rz) * REGION_BLOCKS + bx - rx) as usize]
                }));
            }
        }
        let bytes: Vec<u8> = old
            .iter()
            .flat_map(|p| [(p >> 16) as u8, (p >> 8) as u8, *p as u8, (p >> 24) as u8])
            .collect();
        let before = t.elapsed();
        assert_eq!(bytes.len(), old.len() * 4);
        println!(
            "size {size}: full {full:?}, walking {walk:?} a frame (worst {worst:?}), region redrawn {redraw:?}; \
             before: {before:?} per block step"
        );
    }

    #[test]
    fn another_style_or_size_starts_over() {
        let regions = world([0..=3, 0..=3]);
        let mut img = MinimapImage::default();
        img.update(&view([30.0, 30.0]), |p| regions.get(&p));
        img.take_changes();
        let style = Style {
            epoch: 1,
            ..Style::default()
        };
        img.update(
            &View {
                style,
                ..view([30.0, 30.0])
            },
            |p| regions.get(&p),
        );
        assert!(img.take_changes().reset);
        img.update(
            &View {
                style,
                side_blocks: 300.0,
                ..view([30.0, 30.0])
            },
            |p| regions.get(&p),
        );
        assert!(img.take_changes().reset);
        assert!(img.size() > 224);
    }
}
