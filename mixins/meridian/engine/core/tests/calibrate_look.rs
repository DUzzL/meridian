//! Measures the seed layer's End islands and Nether floors per biome, for
//! the coarse previews' colours (run by hand, `--ignored --nocapture`).

use meridian_seedatlas::Dimension;
use meridian_seedatlas::atlas::Atlas;
use meridian_seedatlas::biomes;
use std::collections::BTreeMap;
use std::time::Instant;

#[test]
#[ignore]
fn end_land_per_biome() {
    for seed in [42i64, 1234567, -987654321] {
        let atlas = Atlas::versioned(seed, false, meridian_seedatlas::mc::MC_NEWEST);
        let (step, n) = (8, 512);
        let mut stats: BTreeMap<i32, (u64, u64)> = BTreeMap::new();
        for (ox, oz) in [(2000, 2000), (-9000, 4000), (5000, -12000)] {
            let ids = atlas
                .biome_area(Dimension::End, ox, oz, 64, step, n, n)
                .unwrap();
            let h = atlas
                .height_area(Dimension::End, ox, oz, step, n, n)
                .unwrap();
            for (id, h) in ids.iter().zip(&h) {
                let e = stats.entry(*id).or_default();
                e.0 += 1;
                e.1 += (*h > 0.0) as u64;
            }
        }
        for (id, (all, land)) in stats {
            println!(
                "seed {seed} biome {id}: {all} samples, land {:.3}",
                land as f64 / all as f64
            );
        }
    }
    let atlas = Atlas::versioned(42, false, meridian_seedatlas::mc::MC_NEWEST);
    for step in [16, 32, 64, 128] {
        let t = Instant::now();
        let r = atlas.biome_area(
            Dimension::End,
            128 * step * 3,
            128 * step * 2,
            64,
            step,
            128,
            128,
        );
        assert!(r.is_ok(), "{r:?}");
        println!("End biome tile at step {step}: {:?}", t.elapsed());
    }
}

#[test]
#[ignore]
fn nether_floor_per_biome() {
    for y in [64, 100, 127] {
        let mut stats: BTreeMap<i32, [u64; 4]> = BTreeMap::new();
        for seed in [42i64, 1234567] {
            let atlas = Atlas::versioned(seed, false, meridian_seedatlas::mc::MC_NEWEST);
            for (ox, oz) in [(0, 0), (4000, -3000), (-6000, 2000)] {
                let (step, n) = (8, 256);
                let ids = atlas
                    .biome_area(Dimension::Nether, ox, oz, y, step, n, n)
                    .unwrap();
                let f = atlas.nether_floor_area(ox, oz, y, step, n, n).unwrap();
                for (id, f) in ids.iter().zip(&f) {
                    let e = stats.entry(*id).or_default();
                    e[0] += 1;
                    let f = *f as i32;
                    if f <= 0 || f < y - 30 && f >= 31 || f < 31 && 31 < y - 30 {
                        e[1] += 1; // black
                    } else if f < 31 {
                        e[2] += 1; // lava
                    } else {
                        e[3] += 1; // ground
                    }
                }
            }
        }
        for (id, [all, black, lava, ground]) in stats {
            let p = |v: u64| v as f64 / all as f64;
            println!(
                "y {y} biome {} ({id}): black {:.2} lava {:.2} ground {:.2}",
                biomes::name_in(meridian_seedatlas::mc::MC_NEWEST, id).unwrap_or("?"),
                p(black),
                p(lava),
                p(ground)
            );
        }
    }
}

#[test]
#[ignore]
fn end_land_mean_colour() {
    use meridian_core::seedatlas_look::{END_VOID, apply_end};
    let (mut sum, mut count) = ([0f64; 3], 0u64);
    let (mut void_px, mut all_px) = (0u64, 0u64);
    for seed in [42i64, 1234567, -987654321] {
        let atlas = Atlas::versioned(seed, false, meridian_seedatlas::mc::MC_NEWEST);
        for step in [1, 4, 8] {
            for (ox, oz) in [(2048, 2048), (-8192, 4096), (4096, -12288)] {
                let n = 128usize;
                let hw = n + 2;
                let heights = atlas
                    .height_area(
                        Dimension::End,
                        ox - step,
                        oz - step,
                        step,
                        hw as i32,
                        hw as i32,
                    )
                    .unwrap();
                let ids = vec![meridian_seedatlas::biomes::END_HIGHLANDS; n * n];
                let mut px = vec![0xFF00_0000u32; n * n];
                apply_end(&mut px, &ids, ox, oz, step, n, &heights);
                for p in px {
                    all_px += 1;
                    if p & 0xFF_FFFF == END_VOID {
                        void_px += 1;
                        continue;
                    }
                    for (i, s) in sum.iter_mut().enumerate() {
                        *s += ((p >> (16 - 8 * i)) & 0xFF) as f64;
                    }
                    count += 1;
                }
            }
        }
    }
    let m = sum.map(|s| (s / count as f64).round() as u32);
    println!(
        "End land mean #{:02X}{:02X}{:02X} over {count} samples, void share {:.3}",
        m[0],
        m[1],
        m[2],
        void_px as f64 / all_px as f64
    );
}

#[test]
#[ignore]
fn end_fine_preview_cost() {
    let atlas = Atlas::versioned(42, false, meridian_seedatlas::mc::MC_NEWEST);
    for (step, k) in [(16, 2), (32, 4), (64, 8), (128, 8), (512, 8)] {
        let sub = step / k;
        let n = 128 * k;
        let t = Instant::now();
        let r = atlas.biome_area(
            Dimension::End,
            128 * step * 3,
            128 * step * 2,
            64,
            sub,
            n,
            n,
        );
        assert!(r.is_ok(), "{r:?}");
        println!("End tile step {step} with {k}x{k}: {:?}", t.elapsed());
    }
}
