//! Performance measurements for large explored maps. Ignored by default;
//! run with
//!
//! ```text
//! cargo test -p meridian-core --release --test perf -- --ignored --nocapture
//! ```
//!
//! Every region is fully explored with varied terrain, the worst case for
//! file size, decoding and drawing.

use std::time::Instant;

use meridian_core::codec::{self, RegionData};
use meridian_core::color::Shading;
use meridian_core::config::TerrainStyle;
use meridian_core::export;
use meridian_core::geom::{REGION_CHUNK_COUNT, RegionPos, chunk_at};
use meridian_core::region::{Region, RegionEdges};
use meridian_core::tile::{ChunkTile, TileColumn};

fn explored(pos: RegionPos) -> Region {
    let mut region = Region::new(pos);
    for i in 0..REGION_CHUNK_COUNT {
        let chunk = chunk_at(pos, i);
        let mut tile = ChunkTile::empty(chunk);
        tile.biomes = vec!["minecraft:plains".into(), "minecraft:forest".into()];
        for (c, col) in tile.columns.iter_mut().enumerate() {
            let n = (i * 256 + c) as u32;
            let h = 64 + ((n.wrapping_mul(2_654_435_761) >> 27) as i16);
            *col = TileColumn {
                accurate: [(n % 200) as u8, 120, (n % 90) as u8],
                vanilla: [100, (n % 160) as u8, 60],
                water: [40, 60, 200],
                water_depth: if n.is_multiple_of(7) { 3 } else { 0 },
                height: h,
                light: 0x0F,
                empty: false,
                biome: (n % 2) as u8,
            };
        }
        region.set_tile(tile);
    }
    region
}

fn ms(t: Instant) -> f64 {
    t.elapsed().as_secs_f64() * 1000.0
}

#[test]
#[ignore = "measurement, run by hand"]
fn large_map() {
    let shading = Shading::new(&TerrainStyle::default(), Some(6000));
    let dir = std::env::temp_dir().join(format!("meridian-perf-{}", std::process::id()));
    let regions_dir = dir.join("regions");
    std::fs::create_dir_all(&regions_dir).unwrap();

    // One region: build, render, encode, decode.
    let t = Instant::now();
    let mut region = explored(RegionPos::new(0, 0));
    let build = ms(t);
    let t = Instant::now();
    region.mark_all_dirty();
    region.render_dirty(&RegionEdges::default(), &shading, usize::MAX);
    let render = ms(t);
    let t = Instant::now();
    let bytes = codec::encode(&RegionData::of(&region));
    let encode = ms(t);
    let t = Instant::now();
    let decoded = codec::decode(&bytes, RegionPos::new(0, 0)).unwrap();
    let decode = ms(t);
    assert_eq!(decoded.tile_count(), REGION_CHUNK_COUNT);
    let t = Instant::now();
    let overview = meridian_core::engine::overview_pixels(region.pixels());
    println!("overview of one region: {:.2} ms, {} KiB", ms(t), overview.len() * 4 / 1024);
    println!(
        "one region (1024 chunks): build {build:.1} ms, render {render:.1} ms, encode {encode:.1} ms, decode {decode:.1} ms, file {} KiB",
        bytes.len() / 1024
    );

    // A 20 x 20 region map (10 240 x 10 240 blocks) on disk.
    let side = 20;
    let t = Instant::now();
    for x in 0..side {
        for z in 0..side {
            let pos = RegionPos::new(x - side / 2, z - side / 2);
            let data = RegionData {
                pos,
                tiles: region
                    .tiles()
                    .cloned()
                    .map(|mut t| {
                        let i = meridian_core::geom::chunk_index(t.pos);
                        t.pos = chunk_at(pos, i);
                        t
                    })
                    .collect(),
            };
            let path = regions_dir.join(format!("{}_{}.mrg", pos.x, pos.z));
            std::fs::write(path, codec::encode(&data)).unwrap();
        }
    }
    println!("write {} regions: {:.0} ms", side * side, ms(t));

    // Loading the 300 regions the world map keeps in memory.
    let t = Instant::now();
    let mut loaded = Vec::new();
    for (pos, path) in export::region_files(&regions_dir).into_iter().take(300) {
        let mut r = codec::decode(&std::fs::read(path).unwrap(), pos).unwrap();
        r.mark_all_dirty();
        r.render_dirty(&RegionEdges::default(), &shading, usize::MAX);
        loaded.push(r);
    }
    let load = ms(t);
    let memory: usize = loaded.len() * (512 * 512 * 4 + REGION_CHUNK_COUNT * (256 * 15 + 64));
    println!(
        "load and draw 300 regions: {load:.0} ms ({:.1} ms each), about {} MiB in memory",
        load / 300.0,
        memory / (1024 * 1024)
    );

    // Exporting all of it as a PNG (scaled to 8192 px).
    let t = Instant::now();
    let report = export::export_png(&regions_dir, &dir.join("map.png"), &shading, 8192).unwrap();
    println!(
        "export {side}x{side} regions to PNG: {:.0} ms ({report:?})",
        ms(t)
    );

    std::fs::remove_dir_all(&dir).ok();
}
