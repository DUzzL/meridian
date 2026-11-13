//! Regions: the unit the map caches, renders and saves.
//!
//! A region is 32 × 32 chunks, 512 × 512 blocks, one map pixel per block
//! column at full zoom.

use meridian_host::{CHUNK_SIZE, ChunkPos};
use serde::{Deserialize, Serialize};

/// Chunks per region side.
pub const REGION_CHUNKS: i32 = 32;
/// Blocks (and pixels) per region side.
pub const REGION_BLOCKS: i32 = REGION_CHUNKS * CHUNK_SIZE;
/// Chunks in a region.
pub const REGION_CHUNK_COUNT: usize = (REGION_CHUNKS * REGION_CHUNKS) as usize;
/// Pixels in a region.
pub const REGION_PIXELS: usize = (REGION_BLOCKS * REGION_BLOCKS) as usize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct RegionPos {
    pub x: i32,
    pub z: i32,
}

impl RegionPos {
    pub const fn new(x: i32, z: i32) -> Self {
        Self { x, z }
    }

    pub const fn of_chunk(chunk: ChunkPos) -> Self {
        Self {
            x: chunk.x.div_euclid(REGION_CHUNKS),
            z: chunk.z.div_euclid(REGION_CHUNKS),
        }
    }

    pub const fn of_block(x: i32, z: i32) -> Self {
        Self {
            x: x.div_euclid(REGION_BLOCKS),
            z: z.div_euclid(REGION_BLOCKS),
        }
    }

    /// The block column at the region's north-west corner.
    pub const fn min_block(self) -> (i32, i32) {
        (self.x * REGION_BLOCKS, self.z * REGION_BLOCKS)
    }

    pub const fn offset(self, dx: i32, dz: i32) -> Self {
        Self {
            x: self.x + dx,
            z: self.z + dz,
        }
    }
}

/// The index of a chunk inside its region (x fastest).
pub const fn chunk_index(chunk: ChunkPos) -> usize {
    let lx = chunk.x.rem_euclid(REGION_CHUNKS);
    let lz = chunk.z.rem_euclid(REGION_CHUNKS);
    (lz * REGION_CHUNKS + lx) as usize
}

/// The chunk at `index` of `region`.
pub const fn chunk_at(region: RegionPos, index: usize) -> ChunkPos {
    let i = index as i32;
    ChunkPos::new(
        region.x * REGION_CHUNKS + i % REGION_CHUNKS,
        region.z * REGION_CHUNKS + i / REGION_CHUNKS,
    )
}

/// A set of chunks of one region, one bit each.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct ChunkMask([u64; REGION_CHUNK_COUNT / 64]);

impl ChunkMask {
    pub const FULL: Self = Self([u64::MAX; REGION_CHUNK_COUNT / 64]);

    pub fn insert(&mut self, index: usize) {
        self.0[index / 64] |= 1 << (index % 64);
    }

    pub fn remove(&mut self, index: usize) {
        self.0[index / 64] &= !(1 << (index % 64));
    }

    pub fn contains(&self, index: usize) -> bool {
        self.0[index / 64] & (1 << (index % 64)) != 0
    }

    pub fn is_empty(&self) -> bool {
        self.0.iter().all(|w| *w == 0)
    }

    pub fn len(&self) -> usize {
        self.0.iter().map(|w| w.count_ones() as usize).sum()
    }

    pub fn clear(&mut self) {
        self.0 = [0; REGION_CHUNK_COUNT / 64];
    }

    /// The set indexes, in ascending order.
    pub fn iter(&self) -> impl Iterator<Item = usize> + '_ {
        self.0.iter().enumerate().flat_map(|(w, bits)| {
            let mut bits = *bits;
            std::iter::from_fn(move || {
                if bits == 0 {
                    return None;
                }
                let b = bits.trailing_zeros() as usize;
                bits &= bits - 1;
                Some(w * 64 + b)
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn regions_of_negative_chunks() {
        assert_eq!(
            RegionPos::of_chunk(ChunkPos::new(-1, 31)),
            RegionPos::new(-1, 0)
        );
        assert_eq!(
            RegionPos::of_chunk(ChunkPos::new(-33, 32)),
            RegionPos::new(-2, 1)
        );
        assert_eq!(RegionPos::of_block(-1, 511), RegionPos::new(-1, 0));
        assert_eq!(RegionPos::new(-1, 2).min_block(), (-512, 1024));
    }

    #[test]
    fn chunk_index_round_trip() {
        for &(x, z) in &[(0, 0), (-1, -1), (31, 0), (-32, 63), (100, -77)] {
            let c = ChunkPos::new(x, z);
            let r = RegionPos::of_chunk(c);
            assert_eq!(chunk_at(r, chunk_index(c)), c);
        }
        assert_eq!(chunk_index(ChunkPos::new(-1, -1)), REGION_CHUNK_COUNT - 1);
    }

    #[test]
    fn mask() {
        let mut m = ChunkMask::default();
        assert!(m.is_empty());
        for i in [0, 63, 64, 1023] {
            m.insert(i);
        }
        assert_eq!(m.len(), 4);
        assert_eq!(m.iter().collect::<Vec<_>>(), [0, 63, 64, 1023]);
        m.remove(63);
        assert!(!m.contains(63));
        assert_eq!(ChunkMask::FULL.len(), REGION_CHUNK_COUNT);
        m.clear();
        assert!(m.is_empty());
    }
}
