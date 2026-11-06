//! PNG export of a whole map (one dimension's surface or cave layer).
//!
//! Regions are read from disk one row at a time and the image is streamed
//! out row by row, so memory stays at one row of regions however large the
//! map is. Maps wider than `max_side` pixels are scaled down by a power of
//! two (each output pixel the mean of the explored pixels it covers).
//! Unexplored parts are transparent.

use std::collections::BTreeMap;
use std::fs::File;
use std::io::BufWriter;
use std::path::{Path, PathBuf};

use crate::codec;
use crate::color::Shading;
use crate::geom::{REGION_BLOCKS, RegionPos};
use crate::region::{Region, RegionEdges};

#[derive(Debug, thiserror::Error)]
pub enum ExportError {
    #[error("nothing explored to export")]
    Empty,
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("png: {0}")]
    Png(#[from] png::EncodingError),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportReport {
    pub path: PathBuf,
    pub width: u32,
    pub height: u32,
    /// Map pixels per image pixel along each side.
    pub scale_down: u32,
    pub regions: usize,
}

/// The region files of a map folder, by position.
pub fn region_files(regions_dir: &Path) -> BTreeMap<RegionPos, PathBuf> {
    let mut out = BTreeMap::new();
    let Ok(entries) = std::fs::read_dir(regions_dir) else {
        return out;
    };
    for e in entries.flatten() {
        let path = e.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        let Some(stem) = name.strip_suffix(".mrg") else {
            continue;
        };
        let Some((x, z)) = stem.split_once('_') else {
            continue;
        };
        if let (Ok(x), Ok(z)) = (x.parse(), z.parse()) {
            out.insert(RegionPos::new(x, z), path);
        }
    }
    out
}

/// The power of two that brings `side` map pixels to `max_side` or fewer.
pub fn scale_down_for(side: u64, max_side: u32) -> u32 {
    let mut f = 1u32;
    while side / f as u64 > max_side.max(1) as u64 && f < REGION_BLOCKS as u32 {
        f *= 2;
    }
    f
}

/// Writes the map in `regions_dir` to `out` as a PNG.
pub fn export_png(
    regions_dir: &Path,
    out: &Path,
    shading: &Shading,
    max_side: u32,
) -> Result<ExportReport, ExportError> {
    let files = region_files(regions_dir);
    let (Some(first), Some(_)) = (files.keys().next(), files.keys().last()) else {
        return Err(ExportError::Empty);
    };
    let (mut min, mut max) = (*first, *first);
    for p in files.keys() {
        min = RegionPos::new(min.x.min(p.x), min.z.min(p.z));
        max = RegionPos::new(max.x.max(p.x), max.z.max(p.z));
    }
    let side = REGION_BLOCKS as u64;
    let (columns, rows) = ((max.x - min.x + 1) as u64, (max.z - min.z + 1) as u64);
    let f = scale_down_for(columns.max(rows) * side, max_side);
    let tile = REGION_BLOCKS as u32 / f;
    let (width, height) = (columns as u32 * tile, rows as u32 * tile);
    if let Some(dir) = out.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut encoder = png::Encoder::new(BufWriter::new(File::create(out)?), width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header()?;
    let mut stream = writer.stream_writer()?;
    let row_bytes = width as usize * 4;
    let mut previous_south: BTreeMap<i32, Vec<Option<i16>>> = BTreeMap::new();
    let mut regions = 0;
    for z in min.z..=max.z {
        let mut band = vec![0u8; row_bytes * tile as usize];
        let mut south = BTreeMap::new();
        let mut west: Option<Vec<Option<i16>>> = None;
        for x in min.x..=max.x {
            let pos = RegionPos::new(x, z);
            let region = files.get(&pos).and_then(|path| {
                let bytes = std::fs::read(path).ok()?;
                codec::decode(&bytes, pos)
                    .map_err(|e| log::warn!("meridian: export skips {path:?}: {e}"))
                    .ok()
            });
            let Some(mut region) = region else {
                west = None;
                continue;
            };
            regions += 1;
            let edges = RegionEdges {
                north: previous_south.get(&x).cloned(),
                west: west.take(),
            };
            region.render_dirty(&edges, shading, usize::MAX);
            west = Some(region.east_edge());
            south.insert(x, region.south_edge());
            blit(
                &region,
                f,
                &mut band,
                row_bytes,
                ((x - min.x) as u32 * tile) as usize,
            );
        }
        previous_south = south;
        std::io::Write::write_all(&mut stream, &band)?;
    }
    stream.finish()?;
    Ok(ExportReport {
        path: out.to_owned(),
        width,
        height,
        scale_down: f,
        regions,
    })
}

/// Copies a drawn region into a band of the image at column `x0`, scaled
/// down by `f`.
fn blit(region: &Region, f: u32, band: &mut [u8], row_bytes: usize, x0: usize) {
    let pixels = region.pixels();
    if pixels.is_empty() {
        return;
    }
    let side = REGION_BLOCKS as usize;
    let f = f as usize;
    let tile = side / f;
    for ty in 0..tile {
        for tx in 0..tile {
            let (mut r, mut g, mut b, mut n) = (0u32, 0u32, 0u32, 0u32);
            for y in ty * f..(ty + 1) * f {
                for p in &pixels[y * side + tx * f..y * side + (tx + 1) * f] {
                    if p >> 24 != 0 {
                        r += (p >> 16) & 0xFF;
                        g += (p >> 8) & 0xFF;
                        b += p & 0xFF;
                        n += 1;
                    }
                }
            }
            if let (Some(r), Some(g), Some(b)) =
                (r.checked_div(n), g.checked_div(n), b.checked_div(n))
            {
                let o = ty * row_bytes + (x0 + tx) * 4;
                band[o..o + 4].copy_from_slice(&[r as u8, g as u8, b as u8, 255]);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codec::{RegionData, encode};
    use crate::config::TerrainStyle;
    use crate::tile::{ColorTable, build_tile, tests::FlatHost, tests::flat_chunk};
    use meridian_host::ChunkPos;

    #[test]
    fn exports_two_regions_side_by_side() {
        let dir = crate::persist::test_dir("export");
        let regions = dir.join("regions");
        std::fs::create_dir_all(&regions).unwrap();
        let host = FlatHost::default();
        let mut table = ColorTable::default();
        for (rx, chunk) in [(0, ChunkPos::new(0, 0)), (1, ChunkPos::new(32, 0))] {
            let mut r = Region::new(RegionPos::new(rx, 0));
            r.set_tile(build_tile(&table.prepare(&host, flat_chunk(chunk), false)));
            std::fs::write(
                regions.join(format!("{rx}_0.mrg")),
                encode(&RegionData::of(&r)),
            )
            .unwrap();
        }
        let shading = Shading::new(&TerrainStyle::default(), None);
        let out = dir.join("map.png");
        let report = export_png(&regions, &out, &shading, 512).unwrap();
        assert_eq!(
            (
                report.width,
                report.height,
                report.scale_down,
                report.regions
            ),
            (512, 256, 2, 2)
        );
        let decoder = png::Decoder::new(std::io::BufReader::new(File::open(&out).unwrap()));
        let mut reader = decoder.read_info().unwrap();
        let mut buf = vec![0; reader.output_buffer_size().unwrap()];
        reader.next_frame(&mut buf).unwrap();
        assert_eq!(buf[3], 255, "the explored chunk is opaque");
        assert_eq!(
            buf[(100 * 512 + 100) * 4 + 3],
            0,
            "unexplored stays transparent"
        );
        assert!(matches!(
            export_png(&dir.join("none"), &out, &shading, 512),
            Err(ExportError::Empty)
        ));
        assert_eq!(scale_down_for(512 * 40, 16384), 2);
        assert_eq!(scale_down_for(512, 16384), 1);
    }
}
