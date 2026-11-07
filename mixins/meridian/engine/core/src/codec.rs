//! The region file format (`.mrg`).
//!
//! ```text
//! "MRDN"           magic
//! u16 LE           format version (2; 1 is still read)
//! i32 LE, i32 LE   region x, z
//! 128 bytes        chunk mask: bit i set = chunk i has a tile
//! zlib stream      for each set chunk, in index order:
//!                  (version 2) u8 biome count, each biome as u8 length + UTF-8
//!                  256 columns of 15 bytes (14 in version 1):
//!                  accurate rgb, vanilla rgb, water rgb, water depth,
//!                  height (i16 LE), light, flags (bit 0 = empty),
//!                  (version 2) biome index (255 = unknown)
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
use crate::tile::{ChunkTile, NO_BIOME, TileColumn};

const MAGIC: &[u8; 4] = b"MRDN";
const VERSION: u16 = 2;
const COLUMN_BYTES: usize = 15;
const V1_COLUMN_BYTES: usize = 14;
/// The most a tile's biome palette can take: 255 names of 255 bytes.
const MAX_PALETTE_BYTES: usize = 1 + 255 * 256;
#[cfg(test)]
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

impl From<CodecError> for crate::persist::LoadError {
    /// A file from a newer Meridian is kept; anything else is broken.
    fn from(e: CodecError) -> Self {
        match e {
            CodecError::UnsupportedVersion(v) if v > VERSION => Self::Newer(e.to_string()),
            e => Self::Broken(e.to_string()),
        }
    }
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
    let mut out = Vec::with_capacity(HEADER_BYTES + tiles.len() * 256 * COLUMN_BYTES / 4);
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
        let names: Vec<&[u8]> = t
            .biomes
            .iter()
            .take(255)
            .map(|b| &b.as_bytes()[..b.len().min(255)])
            .collect();
        let mut palette = vec![names.len() as u8];
        for n in names {
            palette.push(n.len() as u8);
            palette.extend_from_slice(n);
        }
        z.write_all(&palette).expect("writing to a Vec cannot fail");
        for c in t.columns.iter() {
            buf[0..3].copy_from_slice(&c.accurate);
            buf[3..6].copy_from_slice(&c.vanilla);
            buf[6..9].copy_from_slice(&c.water);
            buf[9] = c.water_depth;
            buf[10..12].copy_from_slice(&c.height.to_le_bytes());
            buf[12] = c.light;
            buf[13] = c.empty as u8;
            buf[14] = c.biome;
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
    if version == 0 || version > VERSION {
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
    let (column_bytes, palette_bytes) = if version == 1 {
        (V1_COLUMN_BYTES, 0)
    } else {
        (COLUMN_BYTES, MAX_PALETTE_BYTES)
    };
    let mut body = Vec::with_capacity(mask.len() * 256 * column_bytes);
    ZlibDecoder::new(&bytes[HEADER_BYTES..])
        // Never inflate more than the mask can explain, plus one byte to
        // notice trailing garbage.
        .take((mask.len() * (256 * column_bytes + palette_bytes) + 1) as u64)
        .read_to_end(&mut body)
        .map_err(|e| CodecError::Corrupt(e.to_string()))?;
    let corrupt = |what: &str| {
        CodecError::Corrupt(format!(
            "{what} ({} bytes for {} chunks)",
            body.len(),
            mask.len()
        ))
    };
    let mut region = Region::new(expected);
    let mut at = 0usize;
    for index in mask.iter() {
        let mut tile = ChunkTile::empty(chunk_at(expected, index));
        if version >= 2 {
            let count = *body.get(at).ok_or_else(|| corrupt("missing palette"))? as usize;
            at += 1;
            for _ in 0..count {
                let len = *body.get(at).ok_or_else(|| corrupt("short palette"))? as usize;
                let name = body
                    .get(at + 1..at + 1 + len)
                    .ok_or_else(|| corrupt("short palette"))?;
                tile.biomes.push(String::from_utf8_lossy(name).into_owned());
                at += 1 + len;
            }
        }
        let raw = body
            .get(at..at + 256 * column_bytes)
            .ok_or_else(|| corrupt("short tile"))?;
        at += 256 * column_bytes;
        for (c, b) in tile.columns.iter_mut().zip(raw.chunks_exact(column_bytes)) {
            *c = TileColumn {
                accurate: [b[0], b[1], b[2]],
                vanilla: [b[3], b[4], b[5]],
                water: [b[6], b[7], b[8]],
                water_depth: b[9],
                height: i16::from_le_bytes([b[10], b[11]]),
                light: b[12],
                empty: b[13] & 1 != 0,
                biome: b.get(14).copied().unwrap_or(NO_BIOME),
            };
        }
        region.set_tile(tile);
    }
    if at != body.len() {
        return Err(corrupt("trailing data"));
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
    fn biomes_round_trip_and_version_one_still_reads() {
        let r = region_with(&[(0, 0)]);
        let t = r.tile(ChunkPos::new(0, 0)).unwrap();
        assert_eq!(t.biome(3, 3), Some("minecraft:plains"));
        let back = decode(&encode(&RegionData::of(&r)), r.pos).unwrap();
        assert_eq!(
            back.tile(ChunkPos::new(0, 0)).unwrap().biome(3, 3),
            Some("minecraft:plains")
        );
        // A version 1 file: no palettes, 14-byte columns.
        let mut v1 = encode(&RegionData {
            pos: r.pos,
            tiles: Vec::new(),
        });
        v1.truncate(HEADER_BYTES);
        v1[4] = 1;
        v1[HEADER_BYTES - 128] |= 1;
        let mut z = ZlibEncoder::new(v1, Compression::fast());
        for c in t.columns.iter() {
            let mut b = [0u8; 14];
            b[10..12].copy_from_slice(&c.height.to_le_bytes());
            z.write_all(&b).unwrap();
        }
        let old = decode(&z.finish().unwrap(), r.pos).unwrap();
        let tile = old.tile(ChunkPos::new(0, 0)).unwrap();
        assert_eq!(tile.biome(3, 3), None);
        assert_eq!(tile.column(3, 3).height, t.column(3, 3).height);
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
