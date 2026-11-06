//! The region file format (`.mrg`).
//!
//! ```text
//! "MRDN"           magic
//! u16 LE           format version (1)
//! i32 LE, i32 LE   region x, z
//! 128 bytes        chunk mask: bit i set = chunk i has a tile
//! zlib stream      for each set chunk, in index order, 256 columns of 14 bytes:
//!                  accurate rgb, vanilla rgb, water rgb, water depth,
//!                  height (i16 LE), light, flags (bit 0 = empty)
//! ```
//!
//! Only tiles are stored; pixels are redrawn after loading, so a change of
//! style never needs a file conversion.

use std::io::{Read, Write};

use flate2::Compression;
use flate2::read::ZlibDecoder;
use flate2::write::ZlibEncoder;

use crate::geom::{ChunkMask, REGION_CHUNK_COUNT, RegionPos, chunk_at};
use crate::region::Region;
use crate::tile::{ChunkTile, TileColumn};

const MAGIC: &[u8; 4] = b"MRDN";
const VERSION: u16 = 1;
const COLUMN_BYTES: usize = 14;
const TILE_BYTES: usize = 256 * COLUMN_BYTES;
const HEADER_BYTES: usize = 4 + 2 + 8 + REGION_CHUNK_COUNT / 8;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum CodecError {
    #[error("not a Meridian region file")]
    BadMagic,
    #[error("region file version {0} is newer than this Meridian")]
    UnsupportedVersion(u16),
    #[error("file is for region {found:?}, expected {expected:?}")]
    WrongRegion {
        expected: RegionPos,
        found: RegionPos,
    },
    #[error("file is truncated or corrupt: {0}")]
    Corrupt(String),
}

/// The data a save needs, copied out of a region so a worker can encode it
/// while the region stays in use.
#[derive(Clone, Debug)]
pub struct RegionData {
    pub pos: RegionPos,
    pub tiles: Vec<ChunkTile>,
}

impl RegionData {
    pub fn of(region: &Region) -> Self {
        Self {
            pos: region.pos,
            tiles: region.tiles().cloned().collect(),
        }
    }
}

pub fn encode(data: &RegionData) -> Vec<u8> {
    let mut mask = ChunkMask::default();
    let mut tiles: Vec<&ChunkTile> = data.tiles.iter().collect();
    tiles.sort_by_key(|t| crate::geom::chunk_index(t.pos));
    tiles.dedup_by_key(|t| crate::geom::chunk_index(t.pos));
    for t in &tiles {
        mask.insert(crate::geom::chunk_index(t.pos));
    }
    let mut out = Vec::with_capacity(HEADER_BYTES + tiles.len() * TILE_BYTES / 4);
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&VERSION.to_le_bytes());
    out.extend_from_slice(&data.pos.x.to_le_bytes());
    out.extend_from_slice(&data.pos.z.to_le_bytes());
    for i in 0..REGION_CHUNK_COUNT / 8 {
        let byte = (0..8).fold(0u8, |b, bit| {
            b | ((mask.contains(i * 8 + bit) as u8) << bit)
        });
        out.push(byte);
    }
    let mut z = ZlibEncoder::new(out, Compression::fast());
    let mut buf = [0u8; COLUMN_BYTES];
    for t in tiles {
        for c in t.columns.iter() {
            buf[0..3].copy_from_slice(&c.accurate);
            buf[3..6].copy_from_slice(&c.vanilla);
            buf[6..9].copy_from_slice(&c.water);
            buf[9] = c.water_depth;
            buf[10..12].copy_from_slice(&c.height.to_le_bytes());
            buf[12] = c.light;
            buf[13] = c.empty as u8;
            z.write_all(&buf).expect("writing to a Vec cannot fail");
        }
    }
    z.finish().expect("writing to a Vec cannot fail")
}

pub fn decode(bytes: &[u8], expected: RegionPos) -> Result<Region, CodecError> {
    if bytes.len() < HEADER_BYTES || &bytes[0..4] != MAGIC {
        return Err(CodecError::BadMagic);
    }
    let version = u16::from_le_bytes([bytes[4], bytes[5]]);
    if version != VERSION {
        return Err(CodecError::UnsupportedVersion(version));
    }
    let i32_at = |o: usize| i32::from_le_bytes(bytes[o..o + 4].try_into().expect("4 bytes"));
    let found = RegionPos::new(i32_at(6), i32_at(10));
    if found != expected {
        return Err(CodecError::WrongRegion { expected, found });
    }
    let mut mask = ChunkMask::default();
    for (i, byte) in bytes[14..HEADER_BYTES].iter().enumerate() {
        for bit in 0..8 {
            if byte & (1 << bit) != 0 {
                mask.insert(i * 8 + bit);
            }
        }
    }
    let mut body = Vec::with_capacity(mask.len() * TILE_BYTES);
    ZlibDecoder::new(&bytes[HEADER_BYTES..])
        // Never inflate more than the mask can explain, plus one byte to
        // notice trailing garbage.
        .take((mask.len() * TILE_BYTES + 1) as u64)
        .read_to_end(&mut body)
        .map_err(|e| CodecError::Corrupt(e.to_string()))?;
    if body.len() != mask.len() * TILE_BYTES {
        return Err(CodecError::Corrupt(format!(
            "{} tile bytes for {} chunks",
            body.len(),
            mask.len()
        )));
    }
    let mut region = Region::new(expected);
    for (n, index) in mask.iter().enumerate() {
        let mut tile = ChunkTile::empty(chunk_at(expected, index));
        let raw = &body[n * TILE_BYTES..(n + 1) * TILE_BYTES];
        for (c, b) in tile.columns.iter_mut().zip(raw.chunks_exact(COLUMN_BYTES)) {
            *c = TileColumn {
                accurate: [b[0], b[1], b[2]],
                vanilla: [b[3], b[4], b[5]],
                water: [b[6], b[7], b[8]],
                water_depth: b[9],
                height: i16::from_le_bytes([b[10], b[11]]),
                light: b[12],
                empty: b[13] & 1 != 0,
            };
        }
        region.set_tile(tile);
    }
    // Freshly loaded from disk: nothing to save yet.
    region.unsaved = false;
    Ok(region)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tile::{ColorTable, build_tile, tests::FlatHost, tests::flat_chunk};
    use meridian_host::ChunkPos;

    fn region_with(chunks: &[(i32, i32)]) -> Region {
        let host = FlatHost::default();
        let mut table = ColorTable::default();
        let pos = RegionPos::of_chunk(ChunkPos::new(chunks[0].0, chunks[0].1));
        let mut r = Region::new(pos);
        for &(x, z) in chunks {
            r.set_tile(build_tile(&table.prepare(
                &host,
                flat_chunk(ChunkPos::new(x, z)),
                false,
            )));
        }
        r
    }

    #[test]
    fn round_trip() {
        let r = region_with(&[(-32, -32), (-1, -1), (-5, -20), (-31, -2)]);
        let bytes = encode(&RegionData::of(&r));
        let back = decode(&bytes, r.pos).unwrap();
        assert_eq!(back.tile_count(), 4);
        for t in r.tiles() {
            assert_eq!(back.tile(t.pos), Some(t));
        }
        assert!(!back.unsaved);
        assert!(back.has_dirty(), "loaded tiles still need drawing");
    }

    #[test]
    fn empty_region_round_trips() {
        let data = RegionData {
            pos: RegionPos::new(3, -4),
            tiles: Vec::new(),
        };
        let back = decode(&encode(&data), data.pos).unwrap();
        assert_eq!(back.tile_count(), 0);
    }

    #[test]
    fn compresses_flat_terrain() {
        let chunks: Vec<_> = (0..32).flat_map(|x| (0..32).map(move |z| (x, z))).collect();
        let bytes = encode(&RegionData::of(&region_with(&chunks)));
        assert!(
            bytes.len() < 1024 * TILE_BYTES / 10,
            "{} bytes",
            bytes.len()
        );
    }

    #[test]
    fn rejects_bad_files() {
        let r = region_with(&[(0, 0), (1, 0)]);
        let bytes = encode(&RegionData::of(&r));
        assert_eq!(decode(b"nope", r.pos).unwrap_err(), CodecError::BadMagic);
        assert!(matches!(
            decode(&bytes, RegionPos::new(1, 0)),
            Err(CodecError::WrongRegion { .. })
        ));
        let mut newer = bytes.clone();
        newer[4] = 9;
        assert_eq!(
            decode(&newer, r.pos).unwrap_err(),
            CodecError::UnsupportedVersion(9)
        );
        let truncated = &bytes[..bytes.len() - 20];
        assert!(matches!(
            decode(truncated, r.pos),
            Err(CodecError::Corrupt(_))
        ));
        let mut more_chunks = bytes.clone();
        more_chunks[14] |= 0b100; // claims a third chunk the body lacks
        assert!(matches!(
            decode(&more_chunks, r.pos),
            Err(CodecError::Corrupt(_))
        ));
    }
}
