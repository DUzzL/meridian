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
//! dimension change mid-flight never paints the wrong map.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use meridian_host::{ChunkPos, HostEvent, MapHost, ReadMode, WorldContext};

use crate::codec::{self, RegionData};
use crate::color::Shading;
use crate::config::Settings;
use crate::geom::{REGION_CHUNKS, RegionPos};
use crate::persist::{self, MapPaths};
use crate::region::{Region, RegionEdges};
use crate::tile::{ChunkTile, ColorTable, build_tile};
use crate::worker::WorkerPool;

/// The map used until map selection (sub-worlds) arrives.
pub const DEFAULT_MAP: &str = "default";
/// Chunks redrawn per batch between deadline checks.
const RENDER_BATCH: usize = 32;

#[derive(Debug)]
enum WorkResult {
    TileBuilt {
        generation: u64,
        tile: ChunkTile,
    },
    RegionLoaded {
        generation: u64,
        pos: RegionPos,
        region: Result<Option<Region>, String>,
    },
    RegionSaved {
        pos: RegionPos,
        result: Result<(), String>,
    },
}

#[derive(Debug)]
struct ActiveMap {
    context: WorldContext,
    paths: MapPaths,
}

/// What one tick did; for tests and the debug overlay.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TickReport {
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
    loaded_chunks: HashSet<ChunkPos>,
    colors: ColorTable,
    shading: Option<Shading>,
    workers: WorkerPool<WorkResult>,
    last_save: Instant,
    save_errors: usize,
}

impl MapEngine {
    pub fn new(settings: Settings, data_dir: PathBuf) -> Self {
        let workers = WorkerPool::new(settings.performance.effective_workers(), "meridian-worker");
        Self {
            settings,
            data_dir,
            active: None,
            generation: 0,
            regions: HashMap::new(),
            clock: 0,
            loading: HashSet::new(),
            waiting_tiles: HashMap::new(),
            pending_reads: HashSet::new(),
            loaded_chunks: HashSet::new(),
            colors: ColorTable::default(),
            shading: None,
            workers,
            last_save: Instant::now(),
            save_errors: 0,
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
            HostEvent::LevelUnloaded | HostEvent::Disconnected => self.close(),
            HostEvent::ChunkLoaded(chunk) => {
                if self.active.is_some() {
                    self.loaded_chunks.insert(chunk);
                    if self.settings.world_map.load_new_chunks {
                        self.pending_reads.insert(chunk);
                    }
                }
            }
            HostEvent::ChunkChanged(chunk) => {
                if self.active.is_some() && self.settings.world_map.update_chunks {
                    self.pending_reads.insert(chunk);
                }
            }
            HostEvent::ChunkUnloaded(chunk) => {
                self.loaded_chunks.remove(&chunk);
                self.pending_reads.remove(&chunk);
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
            DEFAULT_MAP,
            &context.dimension,
        );
        self.active = Some(ActiveMap { context, paths });
    }

    /// Saves what changed and forgets the current world.
    fn close(&mut self) {
        self.flush();
        self.generation += 1;
        self.active = None;
        self.regions.clear();
        self.loading.clear();
        self.waiting_tiles.clear();
        self.pending_reads.clear();
        self.loaded_chunks.clear();
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
        region.unsaved = false;
        let data = RegionData::of(region);
        let path = active.paths.region_file(pos);
        self.workers.submit(move || {
            let bytes = codec::encode(&data);
            let result = persist::write_atomic(&path, &bytes).map_err(|e| e.to_string());
            WorkResult::RegionSaved { pos, result }
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
        self.workers.submit(move || {
            let region = match std::fs::read(&path) {
                Ok(bytes) => codec::decode(&bytes, pos)
                    .map(Some)
                    .map_err(|e| format!("{}: {e}", path.display())),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
                Err(e) => Err(format!("{}: {e}", path.display())),
            };
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

    /// Does this tick's share of the work, stopping after `budget`.
    pub fn tick(&mut self, host: &dyn MapHost, budget: Duration) -> TickReport {
        let deadline = Instant::now() + budget;
        let mut report = TickReport::default();
        if self.active.is_none() {
            while self.workers.poll().is_some() {}
            return report;
        }
        let focus = host
            .player()
            .map(|p| ChunkPos::containing(p.position[0] as i32, p.position[2] as i32));

        // 1. Finished background work.
        while let Some(result) = self.workers.poll() {
            match result {
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
                    let region = region.unwrap_or_else(|e| {
                        log::warn!("meridian: region unreadable, starting it fresh: {e}");
                        None
                    });
                    self.insert_region(region.unwrap_or_else(|| Region::new(pos)));
                    report.regions_loaded += 1;
                }
                WorkResult::RegionSaved {
                    pos,
                    result: Err(e),
                } => {
                    self.save_errors += 1;
                    log::warn!("meridian: saving region {pos:?} failed: {e}");
                    if let Some(r) = self.regions.get_mut(&pos) {
                        r.unsaved = true;
                    }
                }
                _ => {}
            }
        }

        // 2. Read the nearest changed chunks.
        let reads = self.next_reads(focus, self.settings.performance.chunk_reads_per_tick);
        let generation = self.generation;
        let vanilla_biomes = self.settings.world_map.terrain.biome_colors_in_vanilla;
        for chunk in reads {
            self.pending_reads.remove(&chunk);
            let Some(snapshot) = host.read_chunk(chunk, ReadMode::Surface) else {
                continue;
            };
            if snapshot.pos != chunk {
                log::warn!(
                    "meridian: host answered chunk {:?} for {chunk:?}",
                    snapshot.pos
                );
                continue;
            }
            let prepared = self.colors.prepare(host, snapshot, vanilla_biomes);
            self.ensure_region(RegionPos::of_chunk(chunk));
            self.workers.submit(move || WorkResult::TileBuilt {
                generation,
                tile: build_tile(&prepared),
            });
            report.chunks_read += 1;
            if Instant::now() >= deadline {
                break;
            }
        }

        // 3. Redraw, nearest regions first.
        let shading = Shading::new(&self.settings.world_map.terrain, host.day_time());
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

    fn evict(&mut self, focus: RegionPos) -> usize {
        let limit = self.settings.world_map.max_loaded_regions.max(9);
        if self.regions.len() <= limit {
            return 0;
        }
        let mut by_age: Vec<(u64, RegionPos)> = self
            .regions
            .values()
            // Never drop the player's own neighbourhood.
            .filter(|r| (r.pos.x - focus.x).abs() > 1 || (r.pos.z - focus.z).abs() > 1)
            .map(|r| (r.last_used, r.pos))
            .collect();
        by_age.sort();
        let excess = self.regions.len() - limit;
        let mut evicted = 0;
        for (_, pos) in by_age.into_iter().take(excess) {
            if self.regions[&pos].unsaved {
                self.save(pos);
            }
            self.regions.remove(&pos);
            evicted += 1;
        }
        evicted
    }

    /// No reads, background jobs, loads or redraws left.
    pub fn is_idle(&self) -> bool {
        self.pending_reads.is_empty()
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
                unreadable: RefCell::default(),
            }
        }
    }

    impl MapHost for World {
        fn context(&self) -> Option<WorldContext> {
            Some(overworld())
        }
        fn player(&self) -> Option<PlayerState> {
            Some(PlayerState {
                position: *self.player.borrow(),
                yaw: 0.0,
                pitch: 0.0,
                dimension: DimensionId::new(DimensionId::OVERWORLD),
            })
        }
        fn read_chunk(&self, chunk: ChunkPos, mode: ReadMode) -> Option<ChunkSnapshot> {
            if self.unreadable.borrow().contains(&chunk) {
                return None;
            }
            self.reads.set(self.reads.get() + 1);
            self.flat.read_chunk(chunk, mode)
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
        MapEngine::new(s, world.data_dir())
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
        let mut e = MapEngine::new(s, world.data_dir());
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
        assert_eq!(r.chunks_read, 5);
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
        let nether = WorldContext {
            dimension: DimensionId::new(DimensionId::NETHER),
            ..overworld()
        };
        e.handle(&world, HostEvent::LevelLoaded(nether.clone()));
        settle(&mut e, &world);
        assert_eq!(e.context(), Some(&nether));
        // Overworld tiles still in flight when it closed are dropped: the
        // nether map stays empty and writes no file.
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
        let mut e = MapEngine::new(s, world.data_dir());
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
    fn evicts_least_recently_used_and_saves_them() {
        let world = World::new("engine_evict");
        let mut s = Settings::default();
        s.world_map.max_loaded_regions = 9;
        let mut e = MapEngine::new(s, world.data_dir());
        e.handle(&world, HostEvent::LevelLoaded(overworld()));
        for x in 0..12 {
            e.handle(&world, HostEvent::ChunkLoaded(ChunkPos::new(x * 32, 0)));
        }
        settle(&mut e, &world);
        assert!(e.loaded_region_count() <= 9);
        let paths = MapPaths::new(
            &world.data_dir(),
            &overworld().source,
            DEFAULT_MAP,
            &overworld().dimension,
        );
        assert!(
            paths.region_file(RegionPos::new(11, 0)).exists()
                || e.region(RegionPos::new(11, 0)).is_some()
        );
        let files = std::fs::read_dir(paths.regions_dir()).map_or(0, |d| d.count());
        assert!(
            files >= 12 - 9,
            "evicted regions were saved ({files} files)"
        );
    }
}
