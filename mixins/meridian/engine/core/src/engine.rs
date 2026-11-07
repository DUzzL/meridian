//! The map engine: turns the game's chunks into cached, saved, drawn
//! regions without stalling a frame.
//!
//! Per tick, on the main thread, within a time budget:
//!
//! 1. take finished background work (tiles, loaded regions, saves);
//! 2. read the nearest few changed chunks from the game and hand them to
//!    workers to become tiles;
//! 3. redraw dirty chunks of regions near the player first;
//! 4. every few seconds, save changed regions on a worker; drop the least
//!    recently used regions over the limit (saving them first).
//!
//! Results from an earlier world are dropped by generation number, so a
//! dimension change mid-flight never paints the wrong map. Loads and saves
//! of one file keep their order ([`persist::FileOrder`]); a file that cannot
//! be read is set aside or left alone, never written over
//! ([`persist::load_or_set_aside`]).

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use meridian_host::{ChunkPos, HostEvent, MapHost, ReadMode, WorldContext};

use crate::codec::{self, RegionData};
use crate::color::Shading;
use crate::config::Settings;
use crate::geom::{REGION_BLOCKS, REGION_CHUNKS, RegionPos};
use crate::persist::{self, FileOrder, Loaded, MapPaths};
use crate::region::{Region, RegionEdges};
use crate::tile::{ChunkTile, ColorTable, build_tile};
use crate::worker::WorkerPool;

/// The map used until map selection (sub-worlds) arrives.
pub const DEFAULT_MAP: &str = "default";
/// Explored chunks kept in memory at most (a full region is 1024 and takes
/// about 5 MB with its pixels): about 500 MB, whatever the region limit.
pub const MAX_CACHED_CHUNKS: usize = 96 * 1024;
/// Side of a region's overview image (a quarter of its 512 pixels).
pub const OVERVIEW_SIDE: usize = 128;
/// Overviews kept (64 KiB each): far more regions than fit in memory whole.
pub const MAX_OVERVIEWS: usize = 4096;
/// Regions loaded at once only to make their overview.
const OVERVIEW_LOADS: usize = 4;
/// Region files looked up per call when asking for overviews.
const OVERVIEW_PROBES: usize = 32;
/// Chunks redrawn per batch between deadline checks.
const RENDER_BATCH: usize = 32;
/// How long a chunk request may stay unanswered before it is asked again
/// (200 game ticks).
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);
/// One game tick: `chunk_reads_per_tick` counts these, whatever the frame
/// rate.
const TICK: Duration = Duration::from_millis(50);
/// Unanswered requests before a chunk waits for its next load or change.
const MAX_ATTEMPTS: u8 = 3;

#[derive(Debug)]
enum WorkResult {
    TileBuilt {
        generation: u64,
        tile: ChunkTile,
    },
    RegionLoaded {
        generation: u64,
        pos: RegionPos,
        region: Loaded<Region>,
    },
    RegionSaved {
        generation: u64,
        pos: RegionPos,
        path: PathBuf,
        result: Result<(), String>,
    },
}

#[derive(Debug)]
struct ActiveMap {
    context: WorldContext,
    paths: MapPaths,
}

/// See [`MapEngine::sampler`].
#[derive(Debug)]
pub struct Sampler<'a> {
    regions: &'a HashMap<RegionPos, Region>,
    last: std::cell::Cell<Option<(RegionPos, Option<&'a Region>)>>,
}

impl Sampler<'_> {
    /// The map's data of block column (`x`, `z`), if explored.
    pub fn column(&self, x: i32, z: i32) -> Option<crate::tile::TileColumn> {
        let region = self.regions.get(&RegionPos::of_block(x, z))?;
        let tile = region.tile(ChunkPos::containing(x, z))?;
        let col = *tile.column(x.rem_euclid(16) as usize, z.rem_euclid(16) as usize);
        (!col.empty).then_some(col)
    }

    /// The biome id of block column (`x`, `z`), if explored and known.
    pub fn biome(&self, x: i32, z: i32) -> Option<String> {
        let region = self.regions.get(&RegionPos::of_block(x, z))?;
        let tile = region.tile(ChunkPos::containing(x, z))?;
        tile.biome(x.rem_euclid(16) as usize, z.rem_euclid(16) as usize)
            .map(str::to_owned)
    }

    /// The ARGB pixel of block column (`x`, `z`).
    pub fn pixel(&self, x: i32, z: i32) -> u32 {
        let pos = RegionPos::of_block(x, z);
        let region = match self.last.get() {
            Some((p, r)) if p == pos => r,
            _ => {
                let r = self.regions.get(&pos);
                self.last.set(Some((pos, r)));
                r
            }
        };
        let Some(region) = region else { return 0 };
        let pixels = region.pixels();
        if pixels.is_empty() {
            return 0;
        }
        let (rx, rz) = pos.min_block();
        pixels[((z - rz) * crate::geom::REGION_BLOCKS + (x - rx)) as usize]
    }
}

/// What one tick did; for tests and the debug overlay.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TickReport {
    pub chunks_requested: usize,
    pub chunks_read: usize,
    pub tiles_applied: usize,
    pub regions_loaded: usize,
    pub chunks_drawn: usize,
    pub saves_started: usize,
    pub regions_evicted: usize,
}

#[derive(Debug)]
pub struct MapEngine {
    settings: Settings,
    data_dir: PathBuf,
    active: Option<ActiveMap>,
    generation: u64,
    regions: HashMap<RegionPos, Region>,
    clock: u64,
    loading: HashSet<RegionPos>,
    waiting_tiles: HashMap<RegionPos, Vec<ChunkTile>>,
    pending_reads: HashSet<ChunkPos>,
    /// Asked of the host and not answered yet, with when they were asked.
    in_flight: HashMap<ChunkPos, Instant>,
    request_timeout: Duration,
    /// Chunk reads that may be asked now: `chunk_reads_per_tick` per game
    /// tick gone by, at most one tick's worth.
    read_credit: f64,
    last_tick: Option<Instant>,
    /// Nothing is saved: it is not known yet which map of a server the
    /// chunks read belong to.
    hold_saves: bool,
    /// Requests that went unanswered, per chunk.
    failures: HashMap<ChunkPos, u8>,
    /// Chunks the client holds, whatever map is open.
    loaded_chunks: HashSet<ChunkPos>,
    /// The cave layer shown, `None` for the surface.
    layer: Option<i32>,
    colors: ColorTable,
    shading: Option<Shading>,
    workers: WorkerPool<WorkResult>,
    last_save: Instant,
    save_errors: usize,
    /// Saves handed to workers and not answered yet.
    saves_in_flight: usize,
    /// Another process writes this data folder: nothing is saved.
    read_only: bool,
    /// The only regions to keep whole at the next eviction (besides the
    /// player's), when the world map is zoomed out.
    keep_whole: Option<(RegionPos, RegionPos)>,
    /// The cache clock when the last tick ended: regions used since are
    /// shown now and are not evicted.
    frame_start: u64,
    /// Small images of regions not held whole (the zoomed-out world map).
    overviews: HashMap<RegionPos, Overview>,
    /// Regions loaded only to make their overview, dropped once drawn,
    /// with their cache clock when they arrived.
    transient: HashMap<RegionPos, Option<u64>>,
    /// Regions without a file (nothing to make an overview of).
    no_file: HashSet<RegionPos>,
    overview_clock: u64,
    /// The map of the world or server shown (`subworld`).
    map: String,
}

/// A region's map at a quarter of the resolution.
#[derive(Clone, Debug)]
pub struct Overview {
    /// `OVERVIEW_SIDE`² ARGB pixels.
    pub pixels: Arc<Vec<u32>>,
    /// Changes whenever the image does.
    pub revision: u64,
    shading: Shading,
    last_used: u64,
}

/// A quarter-size copy of a region's pixels: each pixel the mean of four
/// by four, transparent where most of them are unexplored.
pub fn overview_pixels(pixels: &[u32]) -> Vec<u32> {
    let side = REGION_BLOCKS as usize;
    let k = side / OVERVIEW_SIDE;
    let mut out = vec![0u32; OVERVIEW_SIDE * OVERVIEW_SIDE];
    if pixels.len() != side * side {
        return out;
    }
    for oy in 0..OVERVIEW_SIDE {
        for ox in 0..OVERVIEW_SIDE {
            let (mut r, mut g, mut b, mut n) = (0u32, 0u32, 0u32, 0u32);
            for dy in 0..k {
                let row = (oy * k + dy) * side + ox * k;
                for &p in &pixels[row..row + k] {
                    if p >> 24 != 0 {
                        r += (p >> 16) & 0xFF;
                        g += (p >> 8) & 0xFF;
                        b += p & 0xFF;
                        n += 1;
                    }
                }
            }
            if n * 2 >= (k * k) as u32 {
                out[oy * OVERVIEW_SIDE + ox] = 0xFF00_0000 | (r / n) << 16 | (g / n) << 8 | (b / n);
            }
        }
    }
    out
}

impl MapEngine {
    pub fn new(settings: Settings, data_dir: PathBuf) -> Self {
        let workers = WorkerPool::new(settings.performance.effective_workers(), "meridian-worker");
        let read_only = !persist::claim_data_dir(&data_dir);
        if read_only {
            log::warn!(
                "meridian: another game uses {}; map changes are not saved",
                data_dir.display()
            );
        }
        Self {
            settings,
            data_dir,
            active: None,
            generation: 0,
            regions: HashMap::new(),
            clock: 0,
            frame_start: 0,
            loading: HashSet::new(),
            waiting_tiles: HashMap::new(),
            pending_reads: HashSet::new(),
            in_flight: HashMap::new(),
            request_timeout: REQUEST_TIMEOUT,
            read_credit: 0.0,
            last_tick: None,
            hold_saves: false,
            failures: HashMap::new(),
            loaded_chunks: HashSet::new(),
            layer: None,
            colors: ColorTable::default(),
            shading: None,
            workers,
            last_save: Instant::now(),
            save_errors: 0,
            saves_in_flight: 0,
            read_only,
            keep_whole: None,
            overviews: HashMap::new(),
            transient: HashMap::new(),
            no_file: HashSet::new(),
            overview_clock: 0,
            map: DEFAULT_MAP.to_owned(),
        }
    }

    /// The map of the world or server shown.
    pub fn map(&self) -> &str {
        &self.map
    }

    /// Names the map the next level opens with (the open one is saved to
    /// its own files when that happens).
    pub fn prepare_map(&mut self, map: &str) {
        self.map = map.to_owned();
    }

    /// Shows another map of the same world or server: saves this one and
    /// opens that one for the current dimension. While saves are held
    /// ([`MapEngine::hold_saves`]) what this one got is dropped instead; the
    /// loaded chunks are read again into the other.
    pub fn set_map(&mut self, map: &str) {
        if self.map == map {
            return;
        }
        self.map = map.to_owned();
        if let Some(active) = self.active.as_ref() {
            let context = active.context.clone();
            let layer = self.layer;
            self.open(context);
            self.set_layer(layer);
        }
    }

    pub fn settings(&self) -> &Settings {
        &self.settings
    }

    /// Applies new settings; redraws or rereads what they affect.
    pub fn set_settings(&mut self, settings: Settings) {
        let old = std::mem::replace(&mut self.settings, settings);
        if old.world_map.terrain.biome_colors_in_vanilla
            != self.settings.world_map.terrain.biome_colors_in_vanilla
        {
            // Tiles hold resolved colours: read loaded chunks again.
            self.pending_reads
                .extend(self.loaded_chunks.iter().copied());
        }
        if old.world_map.terrain != self.settings.world_map.terrain {
            // Redrawn on the next tick, when the new shading is computed.
            self.shading = None;
        }
    }

    /// The folder of the region files the map shows now (its cave layer's
    /// when one is shown).
    pub fn regions_dir(&self) -> Option<std::path::PathBuf> {
        self.active
            .as_ref()
            .map(|a| a.paths.with_layer(self.layer).regions_dir())
    }

    /// The cave layer the map shows (its ceiling Y), or `None` for the
    /// surface.
    pub fn layer(&self) -> Option<i32> {
        self.layer
    }

    /// Switches between the surface and a cave layer: the current one is
    /// saved, and the loaded chunks are read again for the new one (which
    /// keeps its own files).
    pub fn set_layer(&mut self, layer: Option<i32>) {
        if self.layer == layer {
            return;
        }
        self.flush();
        self.layer = layer;
        self.generation += 1;
        self.regions.clear();
        self.loading.clear();
        self.waiting_tiles.clear();
        self.in_flight.clear();
        self.failures.clear();
        self.shading = None;
        // Overviews belong to the layer they were drawn from.
        self.overviews.clear();
        self.transient.clear();
        self.no_file.clear();
        if let Some(active) = &mut self.active {
            active.paths = active.paths.with_layer(layer);
            self.pending_reads = self.loaded_chunks.clone();
        }
    }

    /// The world the map shows, if any.
    pub fn context(&self) -> Option<&WorldContext> {
        self.active.as_ref().map(|a| &a.context)
    }

    /// A cached region, for drawing.
    pub fn region(&self, pos: RegionPos) -> Option<&Region> {
        self.regions.get(&pos)
    }

    pub fn loaded_region_count(&self) -> usize {
        self.regions.len()
    }

    /// A region's overview, when it isn't held whole.
    pub fn overview(&self, pos: RegionPos) -> Option<&Overview> {
        self.overviews.get(&pos)
    }

    pub fn overview_count(&self) -> usize {
        self.overviews.len()
    }

    /// Asks for overviews of the regions in a rectangle (inclusive), outside
    /// `near` (which [`MapEngine::request_area`] keeps whole): regions with a
    /// file and no current overview are loaded a few at a time, drawn,
    /// reduced and dropped again.
    pub fn request_overviews(
        &mut self,
        min: RegionPos,
        max: RegionPos,
        near: (RegionPos, RegionPos),
    ) {
        let Some(active) = &self.active else { return };
        let Some(shading) = self.shading else { return };
        self.overview_clock += 1;
        let (center_x, center_z) = ((min.x + max.x) / 2, (min.z + max.z) / 2);
        let mut wanted = Vec::new();
        for x in min.x..=max.x {
            for z in min.z..=max.z {
                let pos = RegionPos::new(x, z);
                if (near.0.x..=near.1.x).contains(&x) && (near.0.z..=near.1.z).contains(&z) {
                    continue;
                }
                if let Some(o) = self.overviews.get_mut(&pos) {
                    o.last_used = self.overview_clock;
                    if !o.shading.differs(&shading) {
                        continue;
                    }
                }
                if self.regions.contains_key(&pos)
                    || self.loading.contains(&pos)
                    || self.no_file.contains(&pos)
                {
                    continue;
                }
                wanted.push(pos);
            }
        }
        // Nearest the centre first; missing overviews before stale ones.
        wanted.sort_by_key(|p| {
            (
                self.overviews.contains_key(p),
                (p.x - center_x).abs().max((p.z - center_z).abs()),
            )
        });
        let paths = active.paths.clone();
        for pos in wanted.into_iter().take(OVERVIEW_PROBES) {
            if self.transient.len() >= OVERVIEW_LOADS {
                break;
            }
            if !paths.region_file(pos).exists() {
                self.no_file.insert(pos);
                continue;
            }
            self.transient.insert(pos, None);
            self.ensure_region(pos);
        }
    }

    /// Makes or refreshes a region's overview from its drawn pixels.
    fn keep_overview(&mut self, pos: RegionPos) {
        let (Some(region), Some(shading)) = (self.regions.get(&pos), self.shading) else {
            return;
        };
        if region.has_dirty() || region.pixels().is_empty() || region.tile_count() == 0 {
            return;
        }
        self.overview_clock += 1;
        let pixels = Arc::new(overview_pixels(region.pixels()));
        self.overviews.insert(
            pos,
            // From the counter region revisions use: they share the GPU's
            // texture keys.
            Overview {
                pixels,
                revision: crate::region::next_revision(),
                shading,
                last_used: self.overview_clock,
            },
        );
        if self.overviews.len() > MAX_OVERVIEWS {
            let mut by_age: Vec<(u64, RegionPos)> = self
                .overviews
                .iter()
                .map(|(p, o)| (o.last_used, *p))
                .collect();
            by_age.sort();
            for (_, p) in by_age
                .into_iter()
                .take(self.overviews.len() - MAX_OVERVIEWS)
            {
                self.overviews.remove(&p);
            }
        }
    }

    /// Transient regions that are drawn become overviews and leave.
    fn settle_transient(&mut self) {
        let done: Vec<(RegionPos, Option<u64>)> = self
            .transient
            .iter()
            .filter(|(p, _)| self.regions.get(p).is_some_and(|r| !r.has_dirty()))
            .map(|(p, c)| (*p, *c))
            .collect();
        for (pos, arrived) in done {
            self.transient.remove(&pos);
            self.keep_overview(pos);
            let region = &self.regions[&pos];
            // Asked for whole meanwhile (shown up close) or written to: keep it.
            if arrived.is_some_and(|c| region.last_used > c) || region.unsaved {
                continue;
            }
            if region.tile_count() == 0 {
                self.no_file.insert(pos);
            }
            self.regions.remove(&pos);
        }
        // A transient region that failed to load or stays undrawn must not
        // block the others forever.
        self.transient
            .retain(|p, _| self.regions.contains_key(p) || self.loading.contains(p));
    }

    /// Reads drawn map pixels by block column; transparent where nothing is
    /// explored or drawn yet. Remembers the last region, so sampling an area
    /// row by row costs one hash lookup per region, not per pixel.
    pub fn sampler(&self) -> Sampler<'_> {
        Sampler {
            regions: &self.regions,
            last: std::cell::Cell::new(None),
        }
    }

    /// Asks for the regions in a rectangle (inclusive) to be loaded and kept;
    /// the renderer calls this each frame for what it shows.
    pub fn request_area(&mut self, min: RegionPos, max: RegionPos) {
        if self.active.is_none() {
            return;
        }
        for x in min.x..=max.x {
            for z in min.z..=max.z {
                self.ensure_region(RegionPos::new(x, z));
            }
        }
    }

    pub fn handle(&mut self, host: &dyn MapHost, event: HostEvent) {
        match event {
            HostEvent::LevelLoaded(context) => self.open(context),
            HostEvent::LevelUnloaded => self.close(),
            HostEvent::Disconnected => {
                self.close();
                self.loaded_chunks.clear();
            }
            HostEvent::ChunkLoaded(chunk) => {
                // Kept even before a map opens: the level hook and the first
                // chunks may arrive in either order.
                self.loaded_chunks.insert(chunk);
                self.failures.remove(&chunk);
                if self.active.is_some() && self.settings.world_map.load_new_chunks {
                    self.pending_reads.insert(chunk);
                }
            }
            HostEvent::ChunkChanged(chunk) => {
                if self.active.is_some() && self.settings.world_map.update_chunks {
                    self.failures.remove(&chunk);
                    self.pending_reads.insert(chunk);
                }
            }
            HostEvent::ChunkUnloaded(chunk) => {
                self.loaded_chunks.remove(&chunk);
                self.pending_reads.remove(&chunk);
                self.in_flight.remove(&chunk);
                self.failures.remove(&chunk);
            }
            HostEvent::ChunksReset => {
                self.loaded_chunks.clear();
                self.pending_reads.clear();
                self.in_flight.clear();
                self.failures.clear();
            }
            HostEvent::ResourcesReloaded => {
                self.colors.clear();
                self.pending_reads
                    .extend(self.loaded_chunks.iter().copied());
            }
            HostEvent::Key { .. } => {}
        }
        let _ = host;
    }

    fn open(&mut self, context: WorldContext) {
        self.close();
        let paths = MapPaths::new(
            &self.data_dir,
            &context.source,
            &self.map,
            &context.dimension,
        );
        self.active = Some(ActiveMap { context, paths });
        self.layer = None;
        if self.settings.world_map.load_new_chunks {
            self.pending_reads
                .extend(self.loaded_chunks.iter().copied());
        }
    }

    /// Saves what changed and forgets the current world.
    fn close(&mut self) {
        self.flush();
        self.generation += 1;
        self.active = None;
        self.regions.clear();
        self.overviews.clear();
        self.transient.clear();
        self.no_file.clear();
        self.loading.clear();
        self.waiting_tiles.clear();
        self.pending_reads.clear();
        self.in_flight.clear();
    }

    /// Starts saving every changed region.
    pub fn flush(&mut self) -> usize {
        let unsaved: Vec<RegionPos> = self
            .regions
            .values()
            .filter(|r| r.unsaved)
            .map(|r| r.pos)
            .collect();
        for pos in &unsaved {
            self.save(*pos);
        }
        self.last_save = Instant::now();
        unsaved.len()
    }

    fn save(&mut self, pos: RegionPos) {
        let (Some(active), Some(region)) = (&self.active, self.regions.get_mut(&pos)) else {
            return;
        };
        if self.hold_saves {
            return;
        }
        region.unsaved = false;
        if region.read_only || self.read_only {
            return;
        }
        let data = RegionData::of(region);
        let path = active.paths.region_file(pos);
        let order = FileOrder::global();
        let turn = order.save_started(&path);
        let generation = self.generation;
        self.saves_in_flight += 1;
        self.workers.submit(move || {
            /// Counts the save as finished even when encoding panics, so
            /// later loads and saves of the file never wait for it.
            struct Finished<'a>(&'a Path);
            impl Drop for Finished<'_> {
                fn drop(&mut self) {
                    FileOrder::global().save_finished(self.0);
                }
            }
            order.wait_finished(&path, turn);
            let finished = Finished(&path);
            let bytes = codec::encode(&data);
            let result = persist::write_atomic(&path, &bytes).map_err(|e| e.to_string());
            drop(finished);
            WorkResult::RegionSaved {
                generation,
                pos,
                path,
                result,
            }
        });
    }

    fn ensure_region(&mut self, pos: RegionPos) {
        self.clock += 1;
        if let Some(r) = self.regions.get_mut(&pos) {
            r.last_used = self.clock;
            return;
        }
        let Some(active) = &self.active else { return };
        if !self.loading.insert(pos) {
            return;
        }
        let path = active.paths.region_file(pos);
        let generation = self.generation;
        let order = FileOrder::global();
        let saves = order.saves_started(&path);
        self.workers.submit(move || {
            // Saves of this file started before are on disk first.
            order.wait_finished(&path, saves);
            let region = persist::load_or_set_aside(&path, "region", |bytes| {
                codec::decode(bytes, pos).map_err(Into::into)
            });
            WorkResult::RegionLoaded {
                generation,
                pos,
                region,
            }
        });
    }

    fn apply_tile(&mut self, tile: ChunkTile) {
        let pos = RegionPos::of_chunk(tile.pos);
        let Some(region) = self.regions.get_mut(&pos) else {
            self.waiting_tiles.entry(pos).or_default().push(tile);
            self.ensure_region(pos);
            return;
        };
        region.last_used = self.clock;
        for chunk in region.set_tile(tile) {
            if let Some(other) = self.regions.get_mut(&RegionPos::of_chunk(chunk)) {
                other.mark_dirty(chunk);
            }
        }
    }

    fn insert_region(&mut self, mut region: Region) {
        let pos = region.pos;
        self.clock += 1;
        region.last_used = self.clock;
        self.regions.insert(pos, region);
        // Neighbours to the east and south shade their edge against this one.
        for i in 0..REGION_CHUNKS {
            let (east, south) = (pos.offset(1, 0), pos.offset(0, 1));
            for (rp, chunk) in [
                (
                    east,
                    ChunkPos::new(east.x * REGION_CHUNKS, east.z * REGION_CHUNKS + i),
                ),
                (
                    south,
                    ChunkPos::new(south.x * REGION_CHUNKS + i, south.z * REGION_CHUNKS),
                ),
            ] {
                if let Some(r) = self.regions.get_mut(&rp)
                    && r.tile(chunk).is_some()
                {
                    r.mark_dirty(chunk);
                }
            }
        }
        for tile in self.waiting_tiles.remove(&pos).unwrap_or_default() {
            self.apply_tile(tile);
        }
    }

    fn edges(&self, pos: RegionPos) -> RegionEdges {
        RegionEdges {
            north: self.regions.get(&pos.offset(0, -1)).map(Region::south_edge),
            west: self.regions.get(&pos.offset(-1, 0)).map(Region::east_edge),
        }
    }

    /// Takes one finished background job's result.
    fn take_result(&mut self, result: WorkResult, report: &mut TickReport) {
        match result {
            // Counted and reported whatever map is open now.
            WorkResult::RegionSaved {
                generation,
                pos,
                path,
                result,
            } => {
                self.saves_in_flight = self.saves_in_flight.saturating_sub(1);
                if let Err(e) = result {
                    self.save_errors += 1;
                    log::warn!("meridian: saving region {} failed: {e}", path.display());
                    if generation == self.generation
                        && let Some(r) = self.regions.get_mut(&pos)
                    {
                        r.unsaved = true;
                    }
                }
            }
            WorkResult::TileBuilt { generation, tile } if generation == self.generation => {
                self.apply_tile(tile);
                report.tiles_applied += 1;
            }
            WorkResult::RegionLoaded {
                generation,
                pos,
                region,
            } if generation == self.generation => {
                self.loading.remove(&pos);
                let region = match region {
                    Loaded::Read(region) => region,
                    Loaded::Missing | Loaded::SetAside => Region::new(pos),
                    // Shown and drawn into this session, never saved.
                    Loaded::ReadOnly => {
                        let mut region = Region::new(pos);
                        region.read_only = true;
                        region
                    }
                };
                self.insert_region(region);
                if let Some(arrived) = self.transient.get_mut(&pos) {
                    *arrived = self.regions.get(&pos).map(|r| r.last_used);
                }
                report.regions_loaded += 1;
            }
            _ => {}
        }
    }

    /// How chunks are read for the layer shown.
    fn read_mode(&self) -> ReadMode {
        self.layer.map_or(ReadMode::Surface, ReadMode::CaveFrom)
    }

    /// Holds (or resumes) saving, while it is not known yet which map of a
    /// server the chunks read belong to: switch maps with
    /// [`MapEngine::set_map`] before resuming, so nothing read meanwhile
    /// ends up in the wrong one.
    pub fn hold_saves(&mut self, hold: bool) {
        self.hold_saves = hold;
    }

    /// Keeps only the regions in a rectangle (inclusive) and around the
    /// player whole at the next tick; the others become overviews and leave
    /// memory (D-027). The world map calls this every frame it is zoomed
    /// far out.
    pub fn keep_whole_only(&mut self, min: RegionPos, max: RegionPos) {
        self.keep_whole = Some((min, max));
    }

    /// Whether another game writes this data folder (nothing is saved).
    pub fn read_only(&self) -> bool {
        self.read_only
    }

    /// Saves every changed region and waits up to `timeout` for the saves
    /// to reach the disk (quitting, disconnecting); whether they all did.
    pub fn flush_and_wait(&mut self, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;
        let mut report = TickReport::default();
        loop {
            // Tiles that arrive meanwhile change regions again.
            if self.regions.values().any(|r| r.unsaved) {
                self.flush();
            }
            if self.saves_in_flight == 0 {
                return true;
            }
            if self.workers.outstanding() == 0 {
                // A save job panicked and never answered.
                self.saves_in_flight = 0;
                return true;
            }
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                log::warn!(
                    "meridian: {} region saves still running after {timeout:?}",
                    self.saves_in_flight
                );
                return false;
            }
            if let Some(result) = self
                .workers
                .wait_timeout(left.min(Duration::from_millis(20)))
            {
                self.take_result(result, &mut report);
            }
        }
    }

    /// Does this tick's share of the work, stopping after `budget`.
    pub fn tick(&mut self, host: &dyn MapHost, budget: Duration) -> TickReport {
        let deadline = Instant::now() + budget;
        let mut report = TickReport::default();
        // 1. Finished background work.
        while let Some(result) = self.workers.poll() {
            self.take_result(result, &mut report);
        }
        if self.active.is_none() {
            return report;
        }
        let focus = host.player().map(|p| {
            ChunkPos::containing(p.position[0].floor() as i32, p.position[2].floor() as i32)
        });

        // 2. Chunks the host read since last tick become tiles on workers;
        //    then ask for the nearest changed ones.
        let generation = self.generation;
        let vanilla_biomes = self.settings.world_map.terrain.biome_colors_in_vanilla;
        let dimension = self.active.as_ref().map(|a| a.context.dimension.clone());
        let mode = self.read_mode();
        for snapshot in host.take_chunks() {
            if Some(&snapshot.dimension) != dimension.as_ref() || snapshot.mode != mode {
                // Read from a level or layer the map no longer shows: the
                // chunk is asked again (or times out) for this one.
                continue;
            }
            self.in_flight.remove(&snapshot.pos);
            self.failures.remove(&snapshot.pos);
            let chunk = snapshot.pos;
            let prepared = self.colors.prepare(host, snapshot, vanilla_biomes);
            self.ensure_region(RegionPos::of_chunk(chunk));
            self.workers.submit(move || WorkResult::TileBuilt {
                generation,
                tile: build_tile(&prepared),
            });
            report.chunks_read += 1;
        }
        let now = Instant::now();
        let expired: Vec<ChunkPos> = self
            .in_flight
            .iter()
            .filter(|(_, asked)| now.duration_since(**asked) > self.request_timeout)
            .map(|(c, _)| *c)
            .collect();
        for chunk in expired {
            self.in_flight.remove(&chunk);
            let failures = self.failures.entry(chunk).or_insert(0);
            *failures += 1;
            if *failures < MAX_ATTEMPTS && self.loaded_chunks.contains(&chunk) {
                self.pending_reads.insert(chunk);
            }
        }
        let per_tick = self.settings.performance.chunk_reads_per_tick;
        let ticks = self.last_tick.map_or(1.0, |t| {
            now.duration_since(t).as_secs_f64() / TICK.as_secs_f64()
        });
        self.last_tick = Some(now);
        self.read_credit = (self.read_credit + ticks * per_tick as f64).min(per_tick as f64);
        let budget = (self.read_credit as usize).min(per_tick.saturating_sub(self.in_flight.len()));
        for chunk in self.next_reads(focus, budget) {
            self.pending_reads.remove(&chunk);
            host.request_chunk(chunk, mode);
            self.in_flight.insert(chunk, now);
            self.read_credit -= 1.0;
            report.chunks_requested += 1;
        }

        // 3. Redraw, nearest regions first.
        let shading = Shading::new(&self.settings.world_map.terrain, host.day_time()).for_cave(
            self.layer.is_some(),
            self.settings.minimap.cave.legible_caves,
        );
        if self.shading.is_none_or(|s| s.differs(&shading)) {
            self.shading = Some(shading);
            for r in self.regions.values_mut() {
                r.mark_all_dirty();
            }
        }
        let shading = self.shading.expect("set above");
        let focus_region = focus
            .map(RegionPos::of_chunk)
            .unwrap_or(RegionPos::new(0, 0));
        let mut dirty: Vec<RegionPos> = self
            .regions
            .values()
            .filter(|r| r.has_dirty())
            .map(|r| r.pos)
            .collect();
        dirty.sort_by_key(|p| {
            (p.x - focus_region.x)
                .abs()
                .max((p.z - focus_region.z).abs())
        });
        'draw: for pos in dirty {
            let edges = self.edges(pos);
            let region = self.regions.get_mut(&pos).expect("listed above");
            while region.has_dirty() {
                report.chunks_drawn += region.render_dirty(&edges, &shading, RENDER_BATCH);
                if Instant::now() >= deadline {
                    break 'draw;
                }
            }
        }

        // 4. Save and evict.
        let interval = Duration::from_secs(self.settings.performance.save_interval_s.max(1) as u64);
        if self.last_save.elapsed() >= interval {
            report.saves_started += self.flush();
        }
        self.settle_transient();
        report.regions_evicted = self.evict(focus_region);
        report
    }

    fn next_reads(&self, focus: Option<ChunkPos>, n: usize) -> Vec<ChunkPos> {
        let mut all: Vec<ChunkPos> = self.pending_reads.iter().copied().collect();
        if let Some(f) = focus {
            let dist = |c: &ChunkPos| (c.x - f.x).abs().max((c.z - f.z).abs());
            if all.len() > n && n > 0 {
                all.select_nth_unstable_by_key(n, dist);
                all.truncate(n);
            }
            all.sort_by_key(dist);
        }
        all.truncate(n);
        all
    }

    /// Drops least recently used regions (saving them and keeping their
    /// overviews) while over the region, chunk or memory limit, and those
    /// outside the area to keep whole when the map is zoomed out.
    fn evict(&mut self, focus: RegionPos) -> usize {
        let limit = self.settings.world_map.max_loaded_regions.max(9);
        let byte_limit = self.settings.world_map.max_region_mib.max(64) << 20;
        let keep = self.keep_whole.take();
        let chunks: usize = self.regions.values().map(Region::tile_count).sum();
        let bytes: usize = self.regions.values().map(Region::approx_bytes).sum();
        let over = |count: usize, chunks: usize, bytes: usize| {
            count > limit || chunks > MAX_CACHED_CHUNKS || bytes > byte_limit
        };
        if keep.is_none() && !over(self.regions.len(), chunks, bytes) {
            self.frame_start = self.clock;
            return 0;
        }
        let outside = |p: RegionPos| {
            keep.is_some_and(|(min, max)| {
                !(min.x..=max.x).contains(&p.x) || !(min.z..=max.z).contains(&p.z)
            })
        };
        // Outside the area to keep first (`false` sorts first), then oldest.
        let mut by_age: Vec<(bool, u64, RegionPos)> = self
            .regions
            .values()
            // Never drop the player's own neighbourhood, nor what is shown
            // now, nor what is on its way to becoming an overview.
            .filter(|r| (r.pos.x - focus.x).abs() > 1 || (r.pos.z - focus.z).abs() > 1)
            .filter(|r| r.last_used <= self.frame_start)
            .filter(|r| !self.transient.contains_key(&r.pos))
            // Changes waiting for the map to be decided stay.
            .filter(|r| !(self.hold_saves && r.unsaved))
            .map(|r| (!outside(r.pos), r.last_used, r.pos))
            .collect();
        by_age.sort();
        let (mut count, mut chunks, mut bytes) = (self.regions.len(), chunks, bytes);
        self.frame_start = self.clock;
        let mut evicted = 0;
        for (inside, _, pos) in by_age {
            if inside && !over(count, chunks, bytes) {
                break;
            }
            if self.regions[&pos].unsaved {
                self.save(pos);
            }
            self.keep_overview(pos);
            if let Some(r) = self.regions.remove(&pos) {
                chunks -= r.tile_count();
                bytes -= r.approx_bytes();
            }
            count -= 1;
            evicted += 1;
        }
        evicted
    }

    /// No reads, background jobs, loads or redraws left.
    pub fn is_idle(&self) -> bool {
        self.pending_reads.is_empty()
            && self.in_flight.is_empty()
            && self.workers.outstanding() == 0
            && self.loading.is_empty()
            && self.regions.values().all(|r| !r.has_dirty())
    }

    /// Failed saves since start.
    pub fn save_errors(&self) -> usize {
        self.save_errors
    }
}

impl Drop for MapEngine {
    fn drop(&mut self) {
        // The pool's own drop then finishes these saves before joining.
        self.flush();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persist::test_dir;
    use crate::tile::tests::{FlatHost, flat_chunk};
    use meridian_host::{
        BiomeAppearance, BlockAppearance, ChunkSnapshot, DimensionId, PlayerState, WorldSource,
    };
    use std::cell::{Cell, RefCell};

    struct World {
        flat: FlatHost,
        player: RefCell<[f64; 3]>,
        reads: Cell<usize>,
        requested: RefCell<Vec<(ChunkPos, ReadMode)>>,
        /// The level the host reads from (and labels its snapshots with).
        dimension: RefCell<DimensionId>,
        /// Chunks the host refuses to read.
        unreadable: RefCell<HashSet<ChunkPos>>,
    }

    impl World {
        fn new(name: &str) -> Self {
            Self {
                flat: FlatHost {
                    data: test_dir(name),
                    ..Default::default()
                },
                player: RefCell::new([8.0, 64.0, 8.0]),
                reads: Cell::new(0),
                requested: RefCell::default(),
                dimension: RefCell::new(DimensionId::new(DimensionId::OVERWORLD)),
                unreadable: RefCell::default(),
            }
        }
    }

    impl MapHost for World {
        fn context(&self) -> Option<WorldContext> {
            Some(WorldContext {
                dimension: self.dimension.borrow().clone(),
                ..overworld()
            })
        }
        fn player(&self) -> Option<PlayerState> {
            Some(PlayerState {
                position: *self.player.borrow(),
                yaw: 0.0,
                pitch: 0.0,
                dimension: self.dimension.borrow().clone(),
            })
        }
        fn request_chunk(&self, chunk: ChunkPos, mode: ReadMode) {
            self.requested.borrow_mut().push((chunk, mode));
        }
        fn take_chunks(&self) -> Vec<ChunkSnapshot> {
            let asked: Vec<(ChunkPos, ReadMode)> = self.requested.borrow_mut().drain(..).collect();
            asked
                .into_iter()
                .filter(|(c, _)| !self.unreadable.borrow().contains(c))
                .map(|(c, mode)| {
                    self.reads.set(self.reads.get() + 1);
                    ChunkSnapshot {
                        dimension: self.dimension.borrow().clone(),
                        ..read_chunk(c, mode)
                    }
                })
                .collect()
        }
        fn block_appearance(&self, block: &str) -> BlockAppearance {
            self.flat.block_appearance(block)
        }
        fn biome_appearance(&self, biome: &str) -> BiomeAppearance {
            self.flat.biome_appearance(biome)
        }
        fn day_time(&self) -> Option<i64> {
            Some(6_000)
        }
        fn data_dir(&self) -> PathBuf {
            self.flat.data.clone()
        }
        fn config_dir(&self) -> PathBuf {
            self.flat.data.join("config")
        }
    }

    /// The test world's chunk as read in `mode`: a cave read finds the
    /// floor of a cave at Y 20.
    fn read_chunk(pos: ChunkPos, mode: ReadMode) -> ChunkSnapshot {
        let mut chunk = flat_chunk(pos);
        chunk.mode = mode;
        if let ReadMode::CaveFrom(_) = mode {
            for c in &mut chunk.columns {
                c.height = 20;
                c.fluid = None;
            }
        }
        chunk
    }

    /// The tile a chunk read in `mode` becomes.
    fn tile_of(pos: ChunkPos, mode: ReadMode) -> ChunkTile {
        build_tile(&ColorTable::default().prepare(
            &FlatHost::default(),
            read_chunk(pos, mode),
            false,
        ))
    }

    fn paths(world: &World, map: &str) -> MapPaths {
        MapPaths::new(
            &world.data_dir(),
            &overworld().source,
            map,
            &overworld().dimension,
        )
    }

    /// A region's file, decoded.
    fn saved(world: &World, layer: Option<i32>, pos: RegionPos) -> Option<Region> {
        let file = paths(world, DEFAULT_MAP).with_layer(layer).region_file(pos);
        Some(codec::decode(&std::fs::read(file).ok()?, pos).unwrap())
    }

    fn overworld() -> WorldContext {
        WorldContext {
            source: WorldSource::Singleplayer {
                world: "Test".into(),
            },
            dimension: DimensionId::new(DimensionId::OVERWORLD),
        }
    }

    fn engine(world: &World) -> MapEngine {
        let mut s = Settings::default();
        s.performance.worker_threads = 2;
        engine_with(world, s)
    }

    /// An engine whose unanswered requests time out quickly.
    fn engine_with(world: &World, settings: Settings) -> MapEngine {
        let mut e = MapEngine::new(settings, world.data_dir());
        e.request_timeout = Duration::from_millis(30);
        e
    }

    fn settle(engine: &mut MapEngine, world: &World) {
        for _ in 0..10_000 {
            engine.tick(world, Duration::from_millis(50));
            if engine.is_idle() {
                return;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        panic!("engine never settled");
    }

    fn load_square(engine: &mut MapEngine, world: &World, r: i32) {
        for x in -r..=r {
            for z in -r..=r {
                engine.handle(world, HostEvent::ChunkLoaded(ChunkPos::new(x, z)));
            }
        }
    }

    #[test]
    fn chunks_become_drawn_regions() {
        let world = World::new("engine_draw");
        let mut e = engine(&world);
        e.handle(&world, HostEvent::LevelLoaded(overworld()));
        load_square(&mut e, &world, 3);
        settle(&mut e, &world);
        assert_eq!(world.reads.get(), 49);
        let regions: Vec<_> = (-1..=0)
            .flat_map(|x| (-1..=0).map(move |z| RegionPos::new(x, z)))
            .collect();
        let tiles: usize = regions
            .iter()
            .map(|p| e.region(*p).map_or(0, Region::tile_count))
            .sum();
        assert_eq!(tiles, 49);
        let home = e.region(RegionPos::new(0, 0)).unwrap();
        assert!(home.revision > 0);
        assert_eq!(home.pixels()[0] >> 24, 0xFF, "block 0,0 is drawn");
        assert_eq!(home.pixels()[100 * 512 + 100], 0, "far away is unexplored");
    }

    #[test]
    fn reads_nearest_first_within_budget() {
        let world = World::new("engine_nearest");
        let mut s = Settings::default();
        s.performance.chunk_reads_per_tick = 5;
        let mut e = engine_with(&world, s);
        e.handle(&world, HostEvent::LevelLoaded(overworld()));
        for x in [40, -30, 1, 0, 2, 25, -1] {
            e.handle(&world, HostEvent::ChunkLoaded(ChunkPos::new(x, 0)));
        }
        let first = e.next_reads(Some(ChunkPos::new(0, 0)), 3);
        assert_eq!(first[0], ChunkPos::new(0, 0));
        let mut ring: Vec<_> = first[1..].to_vec();
        ring.sort();
        assert_eq!(ring, [ChunkPos::new(-1, 0), ChunkPos::new(1, 0)]);
        let r = e.tick(&world, Duration::from_secs(1));
        assert_eq!(r.chunks_requested, 5);
        assert_eq!(world.requested.borrow().len(), 5);
        let r = e.tick(&world, Duration::from_secs(1));
        assert_eq!(r.chunks_read, 5, "answers arrive a tick later");
        assert_eq!(world.reads.get(), 5);
        settle(&mut e, &world);
        assert_eq!(world.reads.get(), 7);
    }

    #[test]
    fn map_survives_restart() {
        let world = World::new("engine_restart");
        {
            let mut e = engine(&world);
            e.handle(&world, HostEvent::LevelLoaded(overworld()));
            load_square(&mut e, &world, 2);
            settle(&mut e, &world);
            e.handle(&world, HostEvent::Disconnected);
        }
        let mut e = engine(&world);
        e.handle(&world, HostEvent::LevelLoaded(overworld()));
        e.request_area(RegionPos::new(-1, -1), RegionPos::new(0, 0));
        settle(&mut e, &world);
        assert_eq!(world.reads.get(), 25, "nothing read again");
        let tile = e
            .region(RegionPos::new(0, 0))
            .and_then(|r| r.tile(ChunkPos::new(1, 0)))
            .cloned();
        let host = FlatHost::default();
        let expected = build_tile(&ColorTable::default().prepare(
            &host,
            flat_chunk(ChunkPos::new(1, 0)),
            false,
        ));
        assert_eq!(tile, Some(expected));
        assert!(
            e.region(RegionPos::new(0, 0)).unwrap().pixels()[16] >> 24 == 0xFF,
            "drawn after loading"
        );
    }

    #[test]
    fn dimension_change_drops_stale_work() {
        let world = World::new("engine_dimension");
        let mut e = engine(&world);
        e.handle(&world, HostEvent::LevelLoaded(overworld()));
        load_square(&mut e, &world, 1);
        e.tick(&world, Duration::from_secs(1));
        // The answers become tile jobs; their results are not taken yet.
        e.tick(&world, Duration::from_secs(1));
        assert_eq!(world.reads.get(), 9);
        // Nothing is read for the nether: whatever reaches its map could only
        // be overworld work still in flight.
        let mut s = e.settings().clone();
        s.world_map.load_new_chunks = false;
        e.set_settings(s);
        let nether = WorldContext {
            dimension: DimensionId::new(DimensionId::NETHER),
            ..overworld()
        };
        *world.dimension.borrow_mut() = nether.dimension.clone();
        e.handle(&world, HostEvent::LevelLoaded(nether.clone()));
        settle(&mut e, &world);
        assert_eq!(e.context(), Some(&nether));
        assert_eq!(world.reads.get(), 9, "nothing read for the nether");
        assert_eq!(
            e.loaded_region_count(),
            0,
            "overworld tiles never reach the nether map"
        );
        e.flush();
        settle(&mut e, &world);
        let nether_paths = MapPaths::new(
            &world.data_dir(),
            &nether.source,
            DEFAULT_MAP,
            &nether.dimension,
        );
        assert!(!nether_paths.regions_dir().exists());
    }

    #[test]
    fn reads_for_another_level_or_layer_are_dropped() {
        let world = World::new("engine_stale_reads");
        let mut e = engine(&world);
        e.handle(&world, HostEvent::LevelLoaded(overworld()));
        load_square(&mut e, &world, 1);
        assert_eq!(e.tick(&world, Duration::from_secs(1)).chunks_requested, 9);
        // The cave layer opens before the surface reads come back.
        e.set_layer(Some(32));
        let r = e.tick(&world, Duration::from_secs(1));
        assert_eq!(r.chunks_read, 0, "surface reads never reach a cave layer");
        settle(&mut e, &world);
        let cave = ReadMode::CaveFrom(32);
        let tile = e
            .region(RegionPos::new(0, 0))
            .and_then(|r| r.tile(ChunkPos::new(0, 0)))
            .cloned();
        assert_eq!(tile, Some(tile_of(ChunkPos::new(0, 0), cave)));

        // Reads answered from another level (the host switched first).
        *world.dimension.borrow_mut() = DimensionId::new(DimensionId::NETHER);
        e.handle(&world, HostEvent::ChunkChanged(ChunkPos::new(0, 0)));
        e.tick(&world, Duration::from_secs(1));
        let r = e.tick(&world, Duration::from_secs(1));
        assert_eq!(
            r.chunks_read, 0,
            "nether reads never reach the overworld map"
        );
    }

    #[test]
    fn unloaded_and_unreadable_chunks_are_skipped() {
        let world = World::new("engine_unload");
        let mut e = engine(&world);
        e.handle(&world, HostEvent::LevelLoaded(overworld()));
        world.unreadable.borrow_mut().insert(ChunkPos::new(0, 0));
        e.handle(&world, HostEvent::ChunkLoaded(ChunkPos::new(0, 0)));
        e.handle(&world, HostEvent::ChunkLoaded(ChunkPos::new(1, 0)));
        e.handle(&world, HostEvent::ChunkUnloaded(ChunkPos::new(1, 0)));
        settle(&mut e, &world);
        assert_eq!(world.reads.get(), 0);
        assert_eq!(
            e.region(RegionPos::new(0, 0)).map_or(0, Region::tile_count),
            0
        );
    }

    #[test]
    fn settings_switch_off_writing() {
        let world = World::new("engine_settings");
        let mut s = Settings::default();
        s.world_map.load_new_chunks = false;
        let mut e = engine_with(&world, s);
        e.handle(&world, HostEvent::LevelLoaded(overworld()));
        load_square(&mut e, &world, 1);
        settle(&mut e, &world);
        assert_eq!(world.reads.get(), 0);
        e.handle(&world, HostEvent::ChunkChanged(ChunkPos::new(0, 0)));
        settle(&mut e, &world);
        assert_eq!(world.reads.get(), 1, "changes still update by default");
    }

    #[test]
    fn style_change_redraws_everything() {
        let world = World::new("engine_style");
        let mut e = engine(&world);
        e.handle(&world, HostEvent::LevelLoaded(overworld()));
        load_square(&mut e, &world, 1);
        settle(&mut e, &world);
        let before = e.region(RegionPos::new(0, 0)).unwrap().pixels()[0];
        let mut s = e.settings().clone();
        s.world_map.terrain.block_colors = crate::config::BlockColors::Vanilla;
        e.set_settings(s);
        settle(&mut e, &world);
        assert_ne!(e.region(RegionPos::new(0, 0)).unwrap().pixels()[0], before);
        assert_eq!(
            world.reads.get(),
            9,
            "a style change redraws without rereading"
        );
    }

    #[test]
    fn cave_layers_read_again_and_keep_their_own_files() {
        let world = World::new("engine_caves");
        let mut e = engine(&world);
        e.handle(&world, HostEvent::LevelLoaded(overworld()));
        load_square(&mut e, &world, 1);
        settle(&mut e, &world);
        assert_eq!(world.reads.get(), 9);
        e.set_layer(Some(32));
        assert_eq!(e.layer(), Some(32));
        settle(&mut e, &world);
        assert_eq!(
            world.reads.get(),
            18,
            "every loaded chunk read again for the cave layer"
        );
        e.set_layer(None);
        settle(&mut e, &world);
        let chunk = ChunkPos::new(0, 0);
        let surface = saved(&world, None, RegionPos::new(0, 0)).expect("surface file");
        let cave = saved(&world, Some(32), RegionPos::new(0, 0)).expect("cave file");
        assert_eq!(surface.tile_count(), 4);
        assert_eq!(cave.tile_count(), 4);
        assert_eq!(
            surface.tile(chunk),
            Some(&tile_of(chunk, ReadMode::Surface))
        );
        assert_eq!(
            cave.tile(chunk),
            Some(&tile_of(chunk, ReadMode::CaveFrom(32))),
            "the cave layer keeps what was read for it"
        );
        assert_ne!(surface.tile(chunk), cave.tile(chunk));
    }

    #[test]
    fn evicts_least_recently_used_and_saves_them() {
        let world = World::new("engine_evict");
        let mut s = Settings::default();
        s.world_map.max_loaded_regions = 9;
        let mut e = engine_with(&world, s);
        e.handle(&world, HostEvent::LevelLoaded(overworld()));
        for x in 0..12 {
            e.handle(&world, HostEvent::ChunkLoaded(ChunkPos::new(x * 32, 0)));
        }
        settle(&mut e, &world);
        // Regions shown since the last tick stay one more tick.
        e.tick(&world, Duration::from_millis(50));
        settle(&mut e, &world);
        assert_eq!(e.loaded_region_count(), 9);
        let evicted: Vec<i32> = (0..12)
            .filter(|&x| e.region(RegionPos::new(x, 0)).is_none())
            .collect();
        assert_eq!(evicted.len(), 3);
        assert!(
            evicted.iter().all(|&x| x > 1),
            "the player's neighbourhood stays: {evicted:?}"
        );
        for x in evicted {
            let pos = RegionPos::new(x, 0);
            let file = saved(&world, None, pos).expect("an evicted region was saved");
            let chunk = ChunkPos::new(x * 32, 0);
            assert_eq!(file.tile(chunk), Some(&tile_of(chunk, ReadMode::Surface)));
            assert!(e.overview(pos).is_some(), "region {x} left an overview");
        }
    }

    #[test]
    fn memory_budget_evicts_whatever_the_region_count() {
        let world = World::new("engine_budget");
        let mut s = Settings::default();
        s.world_map.max_region_mib = 64;
        let mut e = engine_with(&world, s);
        e.handle(&world, HostEvent::LevelLoaded(overworld()));
        for x in 0..80 {
            e.handle(&world, HostEvent::ChunkLoaded(ChunkPos::new(x * 32, 0)));
        }
        settle(&mut e, &world);
        e.tick(&world, Duration::from_millis(50));
        settle(&mut e, &world);
        let bytes: usize = (0..80)
            .filter_map(|x| e.region(RegionPos::new(x, 0)))
            .map(Region::approx_bytes)
            .sum();
        assert!(bytes <= 64 << 20, "{} MiB held", bytes >> 20);
        assert!(e.loaded_region_count() > 40, "only what is over goes");
    }

    #[test]
    fn zoomed_out_map_keeps_only_the_middle_whole() {
        let world = World::new("engine_far");
        let mut e = engine(&world);
        e.handle(&world, HostEvent::LevelLoaded(overworld()));
        for x in 0..10 {
            e.handle(&world, HostEvent::ChunkLoaded(ChunkPos::new(x * 32, 0)));
        }
        settle(&mut e, &world);
        assert_eq!(e.loaded_region_count(), 10);
        let middle = (RegionPos::new(5, 0), RegionPos::new(7, 0));
        e.request_area(middle.0, middle.1);
        e.keep_whole_only(middle.0, middle.1);
        e.tick(&world, Duration::from_millis(50));
        settle(&mut e, &world);
        let whole: Vec<i32> = (0..10)
            .filter(|&x| e.region(RegionPos::new(x, 0)).is_some())
            .collect();
        assert_eq!(whole, [0, 1, 5, 6, 7], "the player's and the middle");
        for x in [2, 3, 4, 8, 9] {
            assert!(
                e.overview(RegionPos::new(x, 0)).is_some(),
                "overview of {x}"
            );
        }
        // Up close again: nothing more leaves.
        e.request_area(RegionPos::new(0, 0), RegionPos::new(9, 0));
        settle(&mut e, &world);
        e.tick(&world, Duration::from_millis(50));
        assert_eq!(e.loaded_region_count(), 10);
    }

    #[test]
    fn unreadable_region_files_are_set_aside() {
        let world = World::new("engine_broken");
        let file = paths(&world, DEFAULT_MAP).region_file(RegionPos::new(0, 0));
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(&file, b"MRDN this is not a region").unwrap();
        let mut e = engine(&world);
        e.handle(&world, HostEvent::LevelLoaded(overworld()));
        e.handle(&world, HostEvent::ChunkLoaded(ChunkPos::new(0, 0)));
        settle(&mut e, &world);
        assert!(e.flush_and_wait(Duration::from_secs(5)));
        let aside: Vec<_> = std::fs::read_dir(file.parent().unwrap())
            .unwrap()
            .flatten()
            .filter(|f| {
                f.file_name()
                    .to_string_lossy()
                    .starts_with("0_0.mrg.broken.")
            })
            .collect();
        assert_eq!(aside.len(), 1, "the broken file is kept beside");
        assert_eq!(
            std::fs::read(aside[0].path()).unwrap(),
            b"MRDN this is not a region"
        );
        assert_eq!(
            saved(&world, None, RegionPos::new(0, 0))
                .unwrap()
                .tile_count(),
            1
        );
    }

    #[test]
    fn newer_or_unreadable_region_files_are_never_written() {
        let world = World::new("engine_newer");
        let regions = paths(&world, DEFAULT_MAP);
        // A file from a newer Meridian: version 9.
        let newer = regions.region_file(RegionPos::new(0, 0));
        let mut bytes = codec::encode(&RegionData {
            pos: RegionPos::new(0, 0),
            tiles: vec![tile_of(ChunkPos::new(5, 5), ReadMode::Surface)],
        });
        bytes[4] = 9;
        std::fs::create_dir_all(newer.parent().unwrap()).unwrap();
        std::fs::write(&newer, &bytes).unwrap();
        // A file that cannot be read (a folder in its place).
        let blocked = regions.region_file(RegionPos::new(-1, 0));
        std::fs::create_dir_all(&blocked).unwrap();

        let mut e = engine(&world);
        e.handle(&world, HostEvent::LevelLoaded(overworld()));
        for chunk in [ChunkPos::new(0, 0), ChunkPos::new(-1, 0)] {
            e.handle(&world, HostEvent::ChunkLoaded(chunk));
        }
        settle(&mut e, &world);
        // Shown this session...
        assert!(e.region(RegionPos::new(0, 0)).unwrap().read_only);
        assert_eq!(e.region(RegionPos::new(-1, 0)).unwrap().tile_count(), 1);
        // ...but never saved over the files.
        assert!(e.flush_and_wait(Duration::from_secs(5)));
        assert_eq!(std::fs::read(&newer).unwrap(), bytes);
        assert!(blocked.is_dir());
        assert_eq!(e.save_errors(), 0);
    }

    #[test]
    fn flush_and_wait_puts_the_saves_on_disk() {
        let world = World::new("engine_flush_wait");
        let mut e = engine(&world);
        e.handle(&world, HostEvent::LevelLoaded(overworld()));
        load_square(&mut e, &world, 1);
        settle(&mut e, &world);
        assert!(
            saved(&world, None, RegionPos::new(0, 0)).is_none(),
            "not saved yet"
        );
        assert!(e.flush_and_wait(Duration::from_secs(5)));
        assert_eq!(
            saved(&world, None, RegionPos::new(0, 0))
                .unwrap()
                .tile_count(),
            4
        );
        // After the world closed, too.
        e.handle(&world, HostEvent::ChunkChanged(ChunkPos::new(-1, -1)));
        let mut s = e.settings().clone();
        s.world_map.terrain.biome_colors_in_vanilla = true;
        e.set_settings(s);
        settle(&mut e, &world);
        e.handle(&world, HostEvent::Disconnected);
        assert!(e.flush_and_wait(Duration::from_secs(5)));
        assert!(e.is_idle());
    }

    #[test]
    fn a_load_waits_for_the_save_before_it() {
        let world = World::new("engine_order");
        let mut e = engine(&world);
        for _ in 0..5 {
            e.handle(&world, HostEvent::LevelLoaded(overworld()));
            load_square(&mut e, &world, 2);
            settle(&mut e, &world);
            // Closing queues the saves; opening again loads the same files
            // at once.
            e.handle(&world, HostEvent::Disconnected);
            e.handle(&world, HostEvent::LevelLoaded(overworld()));
            e.request_area(RegionPos::new(-1, -1), RegionPos::new(0, 0));
            settle(&mut e, &world);
            let tiles: usize = (-1..=0)
                .flat_map(|x| (-1..=0).map(move |z| RegionPos::new(x, z)))
                .map(|p| e.region(p).map_or(0, Region::tile_count))
                .sum();
            assert_eq!(tiles, 25, "the load saw the saved file");
            e.handle(&world, HostEvent::Disconnected);
        }
    }

    #[test]
    fn held_saves_wait_for_the_map() {
        let world = World::new("engine_hold");
        let mut e = engine(&world);
        e.hold_saves(true);
        e.handle(&world, HostEvent::LevelLoaded(overworld()));
        load_square(&mut e, &world, 1);
        settle(&mut e, &world);
        assert_eq!(world.reads.get(), 9, "reading goes on");
        assert!(e.flush_and_wait(Duration::from_secs(5)));
        assert!(!paths(&world, DEFAULT_MAP).regions_dir().exists());
        // The spawn says it is another map: what the shown one got is dropped
        // and read again into that one.
        e.set_map("map-2");
        e.hold_saves(false);
        settle(&mut e, &world);
        assert!(e.flush_and_wait(Duration::from_secs(5)));
        assert!(!paths(&world, DEFAULT_MAP).regions_dir().exists());
        assert_eq!(world.reads.get(), 18);
        assert!(
            paths(&world, "map-2")
                .region_file(RegionPos::new(0, 0))
                .exists()
        );
        // Released without a switch, the held changes are saved.
        e.hold_saves(true);
        e.handle(&world, HostEvent::ChunkLoaded(ChunkPos::new(40, 40)));
        settle(&mut e, &world);
        e.hold_saves(false);
        assert!(e.flush_and_wait(Duration::from_secs(5)));
        assert!(
            paths(&world, "map-2")
                .region_file(RegionPos::new(1, 1))
                .exists()
        );
    }

    #[test]
    fn reads_follow_game_ticks_not_frames() {
        let world = World::new("engine_rate");
        let mut s = Settings::default();
        s.performance.chunk_reads_per_tick = 4;
        let mut e = engine_with(&world, s);
        e.handle(&world, HostEvent::LevelLoaded(overworld()));
        load_square(&mut e, &world, 4);
        // Many frames within much less than a tick: one tick's reads.
        let start = Instant::now();
        let mut asked = 0;
        for _ in 0..20 {
            asked += e.tick(&world, Duration::from_millis(5)).chunks_requested;
        }
        let ticks = start.elapsed().as_secs_f64() / TICK.as_secs_f64();
        assert!(
            asked as f64 <= 4.0 + 4.0 * ticks + 1.0,
            "{asked} reads in {ticks:.2} ticks"
        );
        std::thread::sleep(TICK * 2);
        assert_eq!(e.tick(&world, Duration::from_millis(5)).chunks_requested, 4);
    }

    #[test]
    fn maps_of_one_server_keep_their_own_files() {
        let world = World::new("engine_maps");
        let mut e = engine_with(&world, Settings::default());
        e.handle(&world, HostEvent::LevelLoaded(overworld()));
        e.handle(&world, HostEvent::ChunkLoaded(ChunkPos::new(0, 0)));
        settle(&mut e, &world);
        assert!(e.sampler().column(1, 1).is_some());
        e.set_map("map-2");
        assert_eq!(e.map(), "map-2");
        settle(&mut e, &world);
        let paths = |map: &str| paths(&world, map);
        assert!(
            paths(DEFAULT_MAP)
                .region_file(RegionPos::new(0, 0))
                .exists(),
            "the first map was saved"
        );
        // The loaded chunk is read again into the new map.
        e.flush();
        settle(&mut e, &world);
        assert!(paths("map-2").region_file(RegionPos::new(0, 0)).exists());
    }

    #[test]
    fn overview_is_a_quarter_size_mean() {
        let side = REGION_BLOCKS as usize;
        let mut px = vec![0u32; side * side];
        // The top left 4x4: two shades; the next block: only a quarter explored.
        for y in 0..4 {
            for x in 0..4 {
                px[y * side + x] = if x < 2 { 0xFF00_0010 } else { 0xFF00_0030 };
            }
        }
        for y in 0..2 {
            for x in 4..6 {
                px[y * side + x] = 0xFFFF_FFFF;
            }
        }
        let o = overview_pixels(&px);
        assert_eq!(o.len(), OVERVIEW_SIDE * OVERVIEW_SIDE);
        assert_eq!(o[0], 0xFF00_0020);
        assert_eq!(o[1], 0, "mostly unexplored stays transparent");
    }

    #[test]
    fn far_regions_become_overviews_and_leave() {
        let world = World::new("engine_overview");
        let mut e = engine_with(&world, Settings::default());
        e.handle(&world, HostEvent::LevelLoaded(overworld()));
        for x in 0..6 {
            e.handle(&world, HostEvent::ChunkLoaded(ChunkPos::new(x * 32, 0)));
        }
        settle(&mut e, &world);
        e.flush();
        settle(&mut e, &world);
        // A fresh engine: nothing in memory, the files on disk.
        let mut e = engine_with(&world, Settings::default());
        e.handle(&world, HostEvent::LevelLoaded(overworld()));
        e.tick(&world, std::time::Duration::from_millis(50));
        let near = (RegionPos::new(0, 0), RegionPos::new(0, 0));
        for _ in 0..200 {
            e.request_overviews(RegionPos::new(0, -1), RegionPos::new(6, 1), near);
            e.tick(&world, std::time::Duration::from_millis(50));
            if e.overview_count() >= 5 && e.is_idle() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        for x in 1..6 {
            assert!(
                e.overview(RegionPos::new(x, 0)).is_some(),
                "overview of region {x}"
            );
        }
        assert!(
            e.overview(RegionPos::new(6, 0)).is_none(),
            "no file, no overview"
        );
        e.tick(&world, std::time::Duration::from_millis(50));
        e.tick(&world, std::time::Duration::from_millis(50));
        assert!(
            e.loaded_region_count() <= 2,
            "{} regions still whole",
            e.loaded_region_count()
        );
    }
}
