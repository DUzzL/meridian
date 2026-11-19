//! The Nether's carved air from the seed alone (`NetherCaveMask` of the
//! Fabric version): the geometry of vanilla's Nether cave carver
//! (`worldgen/carvers.json`, `minecraft:nether_cave`), followed step by step
//! as Horizon ports vanilla's cave carver (`worldgen/carver.rs`, MIT). No
//! world and no game data are read, so the Seed Atlas layer shows the Nether's
//! caves for any seed.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::{Arc, Condvar, Mutex, OnceLock};

/// A chunk's position, and its carved blocks (two words per column).
type ChunkPos = (i32, i32);
type Mask = Arc<[u64; 512]>;

/// The largest output side, as in the Java.
const MAX_DIMENSION: usize = 262;

fn valid_step(step: i32) -> bool {
    matches!(step, 1 | 2 | 4 | 8 | 16)
}

fn valid_area(origin_x: i32, origin_z: i32, step: i32, width: usize, height: usize) -> bool {
    valid_step(step)
        && (1..=MAX_DIMENSION).contains(&width)
        && (1..=MAX_DIMENSION).contains(&height)
        && i64::from(origin_x) + width as i64 * i64::from(step) - 1 <= i64::from(i32::MAX)
        && i64::from(origin_z) + height as i64 * i64::from(step) - 1 <= i64::from(i32::MAX)
}

/// A map with a least-recently-used eviction (`LinkedHashMap` in access order).
struct Lru<K, V> {
    entries: HashMap<K, (V, u64)>,
    order: BTreeMap<u64, K>,
    tick: u64,
    capacity: usize,
}

impl<K: Clone + Eq + std::hash::Hash, V: Clone> Lru<K, V> {
    fn new(capacity: usize) -> Self {
        Self {
            entries: HashMap::new(),
            order: BTreeMap::new(),
            tick: 0,
            capacity,
        }
    }

    fn get(&mut self, key: &K) -> Option<V> {
        let tick = self.tick + 1;
        let (value, used) = self.entries.get_mut(key)?;
        self.order.remove(used);
        *used = tick;
        self.order.insert(tick, key.clone());
        self.tick = tick;
        Some(value.clone())
    }

    fn clear(&mut self) {
        self.entries = HashMap::new();
        self.order.clear();
    }

    fn insert(&mut self, key: K, value: V) {
        if let Some((_, used)) = self.entries.remove(&key) {
            self.order.remove(&used);
        }
        if self.entries.len() >= self.capacity
            && let Some((_, eldest)) = self.order.pop_first()
        {
            self.entries.remove(&eldest);
        }
        self.tick += 1;
        self.order.insert(self.tick, key.clone());
        self.entries.insert(key, (value, self.tick));
    }
}

// ---------------------------------------------------------------------------
// Nether cave mask
// ---------------------------------------------------------------------------

/// Two words per row-major sample (bit `y` of the 128-bit pair is set where a
/// Nether cave carver reaches that block), the geometry of the vanilla
/// carvers from the seed alone: `NetherCaveMask.area`. Steps 1/2/4 use the
/// lower block at the native fractional centre; steps 8/16 use the nearest
/// 4-block cell. An out-of-range argument gives an empty vector.
pub fn nether_cave_mask(
    seed: i64,
    origin_x: i32,
    origin_z: i32,
    step: i32,
    width: usize,
    height: usize,
) -> Vec<u64> {
    if !valid_area(origin_x, origin_z, step, width, height) {
        return Vec::new();
    }
    let mut result = vec![0u64; width * height * 2];
    let mut last: Option<(ChunkPos, Mask)> = None;
    for z in 0..height {
        let block_z = sample_coordinate(origin_z, z, step);
        for x in 0..width {
            let block_x = sample_coordinate(origin_x, x, step);
            let chunk = (block_x >> 4, block_z >> 4);
            let mask = match &last {
                Some((c, mask)) if *c == chunk => mask.clone(),
                _ => {
                    let mask = chunk_mask(seed, chunk.0, chunk.1);
                    last = Some((chunk, mask.clone()));
                    mask
                }
            };
            let source = (((block_z & 15) * 16 + (block_x & 15)) * 2) as usize;
            let target = (z * width + x) * 2;
            result[target] = mask[source];
            result[target + 1] = mask[source + 1];
        }
    }
    result
}

fn sample_coordinate(origin: i32, index: usize, step: i32) -> i32 {
    let block = i64::from(origin) + index as i64 * i64::from(step);
    (if step >= 8 {
        (block + i64::from(step / 2) + 2).div_euclid(4) * 4
    } else {
        block + i64::from((step - 1) / 2)
    }) as i32
}

const MAX_CACHED_CHUNKS: usize = 512;

type MaskKey = (i64, i32, i32);

struct MaskState {
    cache: Lru<MaskKey, Arc<[u64; 512]>>,
    /// Chunks some thread is carving: others wait for them.
    carving: HashSet<MaskKey>,
}

fn masks() -> &'static (Mutex<MaskState>, Condvar) {
    static MASKS: OnceLock<(Mutex<MaskState>, Condvar)> = OnceLock::new();
    MASKS.get_or_init(|| {
        (
            Mutex::new(MaskState {
                cache: Lru::new(MAX_CACHED_CHUNKS),
                carving: HashSet::new(),
            }),
            Condvar::new(),
        )
    })
}

/// Gives the chunk back to the waiters when carving ends, however it ends.
struct Carving(MaskKey);

impl Drop for Carving {
    fn drop(&mut self) {
        let (lock, wake) = masks();
        lock.lock()
            .unwrap_or_else(|e| e.into_inner())
            .carving
            .remove(&self.0);
        wake.notify_all();
    }
}

/// A chunk's carved blocks, cached. Carved outside the lock, so workers
/// refine different Nether tiles at once; the same chunk is still never
/// carved twice at once (the Java's `synchronized`).
fn chunk_mask(seed: i64, chunk_x: i32, chunk_z: i32) -> Arc<[u64; 512]> {
    let key = (seed, chunk_x, chunk_z);
    let (lock, wake) = masks();
    let mut state = lock.lock().unwrap_or_else(|e| e.into_inner());
    loop {
        if let Some(mask) = state.cache.get(&key) {
            return mask;
        }
        if state.carving.insert(key) {
            break;
        }
        state = wake.wait(state).unwrap_or_else(|e| e.into_inner());
    }
    drop(state);
    let guard = Carving(key);
    let mask = Arc::new(carve_chunk(seed, chunk_x, chunk_z));
    lock.lock()
        .unwrap_or_else(|e| e.into_inner())
        .cache
        .insert(key, mask.clone());
    drop(guard);
    mask
}

/// The union of the Nether cave carver from the 17x17 chunks around the target.
fn carve_chunk(seed: i64, chunk_x: i32, chunk_z: i32) -> [u64; 512] {
    let mut out = ChunkMask { words: [0; 512] };
    for dx in -8..=8i32 {
        for dz in -8..=8i32 {
            let origin = (chunk_x.wrapping_add(dx), chunk_z.wrapping_add(dz));
            // The Nether's biomes all carry exactly one carver, at index 0.
            let mut random = large_feature_random(seed, origin);
            if random.next_float() <= nether_cave::PROBABILITY {
                nether_cave::carve(&mut random, (chunk_x, chunk_z), origin, &mut out);
            }
        }
    }
    out.words
}

/// Carved blocks of one chunk: two words per column, bit `y & 63` of word `y >> 6`.
struct ChunkMask {
    words: [u64; 512],
}

impl ChunkMask {
    /// The lowest and highest block a carver may reach (`CarverOutput`).
    const MIN_Y: i32 = 1;
    const MAX_Y: i32 = 120;

    fn carve(&mut self, local_x: i32, y: i32, local_z: i32) {
        let index = (((local_z & 15) * 16 + (local_x & 15)) * 2 + (y >> 6)) as usize;
        self.words[index] |= 1u64 << (y & 63);
    }
}

/// `WorldgenRandom.setLargeFeatureSeed` over a legacy random source.
fn large_feature_random(seed: i64, origin: (i32, i32)) -> JavaRandom {
    let mut random = JavaRandom::new(seed);
    let a = random.next_long();
    let b = random.next_long();
    random
        .set_seed(i64::from(origin.0).wrapping_mul(a) ^ i64::from(origin.1).wrapping_mul(b) ^ seed);
    random
}

/// `java.util.Random` as vanilla's `LegacyRandomSource`.
#[derive(Clone, Debug)]
struct JavaRandom {
    state: u64,
}

impl JavaRandom {
    const MULTIPLIER: u64 = 0x5_DEEC_E66D;
    const MASK: u64 = (1 << 48) - 1;

    fn new(seed: i64) -> Self {
        Self {
            state: (seed as u64 ^ Self::MULTIPLIER) & Self::MASK,
        }
    }

    fn set_seed(&mut self, seed: i64) {
        *self = Self::new(seed);
    }

    fn bits(&mut self, count: u32) -> i32 {
        self.state = self.state.wrapping_mul(Self::MULTIPLIER).wrapping_add(11) & Self::MASK;
        (self.state >> (48 - count)) as i32
    }

    /// A bound that is not positive never happens with the carver's providers.
    fn next_int_bound(&mut self, bound: i32) -> i32 {
        if bound <= 0 {
            return 0;
        }
        if bound & (bound - 1) == 0 {
            return ((i64::from(bound) * i64::from(self.bits(31))) >> 31) as i32;
        }
        loop {
            let bits = self.bits(31);
            let value = bits % bound;
            // Java's signed overflow is part of the rejection rule.
            if bits.wrapping_sub(value).wrapping_add(bound - 1) >= 0 {
                return value;
            }
        }
    }

    fn next_long(&mut self) -> i64 {
        let high = i64::from(self.bits(32)) << 32;
        // Java sign-extends the low int before adding.
        high.wrapping_add(i64::from(self.bits(32)))
    }

    fn next_float(&mut self) -> f32 {
        self.bits(24) as f32 * (1.0 / 16_777_216.0)
    }

    fn next_int_between_inclusive(&mut self, min: i32, max: i32) -> i32 {
        self.next_int_bound(max.wrapping_sub(min).wrapping_add(1))
            .wrapping_add(min)
    }
}

/// `Mth.sin` / `Mth.cos`: vanilla's quantized sine table.
mod trig {
    use std::sync::OnceLock;

    const SCALE: f64 = 10430.378350470453;

    fn table() -> &'static [f32] {
        static SINE: OnceLock<Box<[f32]>> = OnceLock::new();
        SINE.get_or_init(|| {
            (0..65536)
                .map(|i| (f64::from(i) / SCALE).sin() as f32)
                .collect()
        })
    }

    pub fn sin(angle: f64) -> f32 {
        table()[((angle * SCALE) as i64 & 65535) as usize]
    }

    pub fn cos(angle: f64) -> f32 {
        table()[((angle * SCALE + 16384.0) as i64 & 65535) as usize]
    }
}

/// `minecraft:nether_cave` (`worldgen/carvers.json`): a `minecraft:cave`
/// carver. Its parameters are inlined; the generation context is the Nether's
/// (min Y 0, height 128, so `y` is uniform in 0..=126).
mod nether_cave {
    use super::{ChunkMask, JavaRandom, trig};
    use std::f32::consts::{FRAC_PI_2, PI, TAU};

    pub const PROBABILITY: f32 = 0.2;
    /// `very_biased_to_bottom`, 0..=9.
    const COUNT_MAX: i32 = 9;
    /// Uniform from `absolute 0` to `below_top 1` in a 128-high dimension.
    const Y_MAX: i32 = 126;
    /// `trapezoid`: min 0, max 6, plateau 2.
    const THICKNESS: (f32, f32, f32) = (0.0, 6.0, 2.0);
    const ROOM_VERTICAL_RADIUS_MULTIPLIER: f32 = 0.5;
    const HORIZONTAL_RADIUS_MULTIPLIER: f32 = 1.0;
    const VERTICAL_RADIUS_MULTIPLIER: f32 = 1.0;
    const START_VERTICAL_RADIUS_MULTIPLIER: f32 = 5.0;
    const FLOOR_LEVEL: f32 = -0.7;
    /// Candidate cells one carve may test (Horizon's guard against runaway paths).
    const BUDGET: usize = 1 << 24;

    pub fn carve(
        random: &mut JavaRandom,
        target: (i32, i32),
        origin: (i32, i32),
        out: &mut ChunkMask,
    ) {
        let mut value = random.next_int_bound(COUNT_MAX + 1);
        value = random.next_int_bound(value.wrapping_add(1));
        value = random.next_int_bound(value.wrapping_add(1));
        let count = value;
        let mut budget = BUDGET;
        for _ in 0..count {
            let x = f64::from(
                origin
                    .0
                    .wrapping_shl(4)
                    .wrapping_add(random.next_int_bound(16)),
            );
            let y = f64::from(random.next_int_between_inclusive(0, Y_MAX));
            let z = f64::from(
                origin
                    .1
                    .wrapping_shl(4)
                    .wrapping_add(random.next_int_bound(16)),
            );
            let mut run = Run {
                target,
                out: &mut *out,
                budget: &mut budget,
                horizontal: f64::from(HORIZONTAL_RADIUS_MULTIPLIER),
                vertical: f64::from(VERTICAL_RADIUS_MULTIPLIER),
                floor: f64::from(FLOOR_LEVEL),
            };
            let mut tunnels = 1;
            if random.next_int_bound(4) == 0 {
                let multiplier = f64::from(ROOM_VERTICAL_RADIUS_MULTIPLIER);
                let thickness = 1.0 + random.next_float() * 6.0;
                let radius = 1.5 + f64::from(trig::sin(f64::from(FRAC_PI_2)) * thickness);
                run.ellipsoid([x + 1.0, y, z], radius, radius * multiplier);
                tunnels += random.next_int_bound(4);
            }
            for _ in 0..tunnels {
                let yaw = random.next_float() * TAU;
                let pitch = (random.next_float() - 0.5) / 4.0;
                let (min, max, plateau) = THICKNESS;
                let range = max - min;
                let slope = (range - plateau) / 2.0;
                let middle = range - slope;
                let thickness = (min + random.next_float() * middle) + random.next_float() * slope;
                let end = 112 - random.next_int_bound(28);
                run.tunnel(Tunnel {
                    seed: random.next_long(),
                    position: [x, y, z],
                    thickness,
                    yaw,
                    pitch,
                    start: 0,
                    end,
                    vertical_scale: f64::from(START_VERTICAL_RADIUS_MULTIPLIER),
                });
            }
        }
    }

    struct Run<'a> {
        target: (i32, i32),
        out: &'a mut ChunkMask,
        budget: &'a mut usize,
        horizontal: f64,
        vertical: f64,
        floor: f64,
    }

    #[derive(Clone, Copy)]
    struct Tunnel {
        seed: i64,
        position: [f64; 3],
        thickness: f32,
        yaw: f32,
        pitch: f32,
        start: i32,
        end: i32,
        vertical_scale: f64,
    }

    /// `Mth.floor`.
    fn floor(value: f64) -> i32 {
        let i = value as i32;
        if value < f64::from(i) {
            i.wrapping_sub(1)
        } else {
            i
        }
    }

    /// Whether a path at (`x`, `z`) can still reach the target chunk.
    fn can_reach(chunk: (i32, i32), x: f64, z: f64, step: i32, end: i32, thickness: f32) -> bool {
        let dx = x - f64::from(chunk.0.wrapping_shl(4).wrapping_add(8));
        let dz = z - f64::from(chunk.1.wrapping_shl(4).wrapping_add(8));
        let remaining = f64::from(end.wrapping_sub(step));
        let radius = f64::from((thickness + 2.0) + 16.0);
        dx * dx + dz * dz - remaining * remaining <= radius * radius
    }

    impl Run<'_> {
        /// The ellipsoid's cells in the target chunk, X, then Z, then Y
        /// descending, less the cave's floor and corners.
        fn ellipsoid(&mut self, [x, y, z]: [f64; 3], horizontal: f64, vertical: f64) {
            let cx = self.target.0.wrapping_shl(4);
            let cz = self.target.1.wrapping_shl(4);
            let reach = 16.0 + horizontal * 2.0;
            if (x - f64::from(cx.wrapping_add(8))).abs() > reach
                || (z - f64::from(cz.wrapping_add(8))).abs() > reach
            {
                return;
            }
            let min_x = floor(x - horizontal)
                .wrapping_sub(cx)
                .wrapping_sub(1)
                .max(0);
            let max_x = floor(x + horizontal).wrapping_sub(cx).min(15);
            let min_y = floor(y - vertical).wrapping_sub(1).max(ChunkMask::MIN_Y);
            let max_y = floor(y + vertical).wrapping_add(1).min(ChunkMask::MAX_Y);
            let min_z = floor(z - horizontal)
                .wrapping_sub(cz)
                .wrapping_sub(1)
                .max(0);
            let max_z = floor(z + horizontal).wrapping_sub(cz).min(15);
            for lx in min_x..=max_x {
                let nx = (f64::from(cx.wrapping_add(lx)) + 0.5 - x) / horizontal;
                for lz in min_z..=max_z {
                    let nz = (f64::from(cz.wrapping_add(lz)) + 0.5 - z) / horizontal;
                    if nx * nx + nz * nz >= 1.0 {
                        continue;
                    }
                    let mut by = max_y;
                    while by > min_y {
                        if *self.budget == 0 {
                            return;
                        }
                        *self.budget -= 1;
                        let ny = (f64::from(by) - 0.5 - y) / vertical;
                        if !(ny <= self.floor || nx * nx + ny * ny + nz * nz >= 1.0) {
                            self.out.carve(lx, by, lz);
                        }
                        by -= 1;
                    }
                }
            }
        }

        fn tunnel(&mut self, mut path: Tunnel) {
            let mut random = JavaRandom::new(path.seed);
            let split = random.next_int_bound(path.end / 2) + path.end / 4;
            let gentle = random.next_int_bound(6) == 0;
            let (mut yaw_change, mut pitch_change) = (0f32, 0f32);
            for step in path.start..path.end {
                let radius = 1.5
                    + f64::from(
                        trig::sin(f64::from(PI * step as f32 / path.end as f32)) * path.thickness,
                    );
                let vertical = radius * path.vertical_scale;
                let cos_pitch = trig::cos(f64::from(path.pitch));
                path.position[0] += f64::from(trig::cos(f64::from(path.yaw)) * cos_pitch);
                path.position[1] += f64::from(trig::sin(f64::from(path.pitch)));
                path.position[2] += f64::from(trig::sin(f64::from(path.yaw)) * cos_pitch);
                path.pitch *= if gentle { 0.92 } else { 0.7 };
                path.pitch += pitch_change * 0.1;
                path.yaw += yaw_change * 0.1;
                pitch_change *= 0.9;
                yaw_change *= 0.75;
                pitch_change +=
                    (random.next_float() - random.next_float()) * random.next_float() * 2.0;
                yaw_change +=
                    (random.next_float() - random.next_float()) * random.next_float() * 4.0;
                if step == split && path.thickness > 1.0 {
                    let seed = random.next_long();
                    let thickness = random.next_float() * 0.5 + 0.5;
                    self.tunnel(Tunnel {
                        seed,
                        thickness,
                        yaw: path.yaw - FRAC_PI_2,
                        pitch: path.pitch / 3.0,
                        start: step,
                        vertical_scale: 1.0,
                        ..path
                    });
                    let seed = random.next_long();
                    let thickness = random.next_float() * 0.5 + 0.5;
                    self.tunnel(Tunnel {
                        seed,
                        thickness,
                        yaw: path.yaw + FRAC_PI_2,
                        pitch: path.pitch / 3.0,
                        start: step,
                        vertical_scale: 1.0,
                        ..path
                    });
                    return;
                }
                if random.next_int_bound(4) == 0 {
                    continue;
                }
                if !can_reach(
                    self.target,
                    path.position[0],
                    path.position[2],
                    step,
                    path.end,
                    path.thickness,
                ) {
                    return;
                }
                let (horizontal, vertical_radius) =
                    (radius * self.horizontal, vertical * self.vertical);
                self.ellipsoid(path.position, horizontal, vertical_radius);
            }
        }
    }
}

/// Frees the carved chunks kept for later tiles.
pub fn release_caches() {
    masks()
        .0
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .cache
        .clear();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_random_matches_java() {
        let mut random = JavaRandom::new(0);
        assert_eq!(random.bits(32), -1_155_484_576);
        assert_eq!(random.bits(32), -723_955_400);
        assert_eq!(JavaRandom::new(0).next_long(), -4_962_768_465_676_381_896);
    }

    #[test]
    fn nether_sample_coordinates() {
        // Steps below 8 use the lower block at the native centre.
        assert_eq!(sample_coordinate(100, 3, 1), 103);
        assert_eq!(sample_coordinate(100, 3, 2), 106);
        assert_eq!(sample_coordinate(100, 3, 4), 113);
        // Steps 8 and 16 use the nearest 4-block cell.
        assert_eq!(sample_coordinate(0, 1, 8), 12);
        assert_eq!(sample_coordinate(-16, 0, 16), -8);
        assert_eq!(sample_coordinate(-3, 0, 8), 0);
        assert_eq!(sample_coordinate(-3, 1, 8), 8);
    }

    #[test]
    fn nether_mask_is_stable_and_bounded() {
        let a = nether_cave_mask(1234, -40, 25, 4, 6, 5);
        let b = nether_cave_mask(1234, -40, 25, 4, 6, 5);
        assert_eq!(a.len(), 6 * 5 * 2);
        assert_eq!(a, b);
        // Nothing is carved below block 2 or above 120 (word 1 holds 64..127).
        for pair in a.chunks(2) {
            assert_eq!(pair[0] & 0b11, 0);
            assert_eq!(pair[1] >> (121 - 64), 0);
        }
        assert!(nether_cave_mask(1, 0, 0, 3, 4, 4).is_empty());
        assert!(nether_cave_mask(1, 0, 0, 4, 0, 4).is_empty());
    }

    #[test]
    fn nether_masks_carve_at_once_and_survive_a_release() {
        let serial = nether_cave_mask(77, -300, 140, 2, 96, 96);
        release_caches();
        let threads: Vec<_> = (0..4)
            .map(|_| std::thread::spawn(|| nether_cave_mask(77, -300, 140, 2, 96, 96)))
            .collect();
        for t in threads {
            assert_eq!(t.join().unwrap(), serial);
        }
        release_caches();
        assert_eq!(nether_cave_mask(77, -300, 140, 2, 96, 96), serial);
    }

    #[test]
    fn nether_mask_has_caves_and_follows_the_seed() {
        // A 64x64 area at step 1 holds carved cells for some seed in a handful.
        let carved = |seed: i64| {
            nether_cave_mask(seed, 0, 0, 1, 64, 64)
                .iter()
                .map(|w| w.count_ones())
                .sum::<u32>()
        };
        let counts: Vec<u32> = (0..4).map(carved).collect();
        assert!(counts.iter().any(|&n| n > 0), "{counts:?}");
        assert_ne!(
            nether_cave_mask(1, 0, 0, 1, 32, 32),
            nether_cave_mask(2, 0, 0, 1, 32, 32)
        );
    }

    #[test]
    #[ignore = "timing"]
    fn nether_chunk_cost() {
        let started = std::time::Instant::now();
        let mask = nether_cave_mask(1, 0, 0, 1, 16, 16);
        eprintln!("one nether chunk mask: {:?}", started.elapsed());
        let started = std::time::Instant::now();
        let big = nether_cave_mask(1, 256, 256, 4, 64, 64);
        eprintln!("64x64 at step 4 (16 chunks): {:?}", started.elapsed());
        let carved = |m: &[u64]| m.iter().map(|w| w.count_ones()).sum::<u32>();
        eprintln!("carved cells: {} / {}", carved(&mask), carved(&big));
    }
}
