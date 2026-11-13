//! Parity with the Fabric version (DUzzL/seed-atlas-xaero main): every
//! fixture was written by its native library for one Minecraft version
//! (`tools/seedatlas-fixtures/`). Each version's file is checked line by line;
//! the test lists every version and kind that differs.
//!
//! `SEEDATLAS_VERSIONS=1.12,26.3` limits the run to some versions.

use std::collections::BTreeMap;
use std::io::Read;

use meridian_seedatlas::atlas::Hit;
use meridian_seedatlas::{Atlas, Dimension, mc};

fn dimension(id: i64) -> Dimension {
    match id {
        -1 => Dimension::Nether,
        1 => Dimension::End,
        _ => Dimension::Overworld,
    }
}

fn rows(v: &[i64]) -> Vec<Hit> {
    let n = v[0].max(0) as usize;
    (0..n)
        .map(|i| {
            let r = &v[1 + i * 6..1 + i * 6 + 6];
            Hit {
                kind: r[0] as i32,
                x: r[1] as i32,
                y: (r[2] != i32::MIN as i64).then_some(r[2] as i32),
                z: r[3] as i32,
                flags: r[4] as i32,
                detail: r[5] as i32,
            }
        })
        .collect()
}

/// Compares one fixture file; returns (lines checked, failures by kind).
fn check_version(version: &str, text: &str) -> (usize, BTreeMap<String, Vec<String>>) {
    let mut fails: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut fail = |kind: &str, msg: String| fails.entry(kind.to_owned()).or_default().push(msg);
    let mc = mc::str2mc(version);
    let mut atlases: Vec<Atlas> = Vec::new();
    let mut checked = 0;
    for line in text.lines() {
        let mut f = line.split(' ');
        let tag = f.next().unwrap_or("");
        if tag == "V" || tag == "K" {
            if tag == "K" {
                let id: i32 = f.next().unwrap().parse().unwrap();
                let name = f.next().unwrap();
                let argb: u32 = f.next().unwrap().parse().unwrap();
                checked += 1;
                let got = (
                    meridian_seedatlas::biomes::name_in(mc, id),
                    meridian_seedatlas::biomes::color_in(mc, id),
                );
                if got != (Some(name), argb) {
                    fail(
                        "K names/colours",
                        format!("id {id}: want {name} {argb:08x}, got {got:?}"),
                    );
                }
            }
            continue;
        }
        let rest: Vec<&str> = f.collect();
        let colon = rest.iter().position(|t| *t == ":");
        let num = |t: &&str| -> i64 {
            t.parse()
                .unwrap_or_else(|_| t.parse::<f64>().map(|v| v as i64).unwrap_or(i64::MIN))
        };
        let seed: i64 = rest[0].parse().unwrap();
        let large = matches!(tag, "P" | "A" | "C" | "T") && rest[1] == "1";
        if !atlases
            .iter()
            .any(|a| a.seed == seed && a.large_biomes == large)
        {
            atlases.push(Atlas::versioned(seed, large, mc));
        }
        let atlas = atlases
            .iter()
            .find(|a| a.seed == seed && a.large_biomes == large)
            .unwrap();
        checked += 1;
        let v: Vec<i64> = rest[..colon.unwrap_or(rest.len())]
            .iter()
            .map(num)
            .collect();
        let i32s = |r: &[i64]| r.iter().map(|&x| x as i32).collect::<Vec<i32>>();
        let b = |i: usize| {
            [
                v[i] as i32,
                v[i + 1] as i32,
                v[i + 2] as i32,
                v[i + 3] as i32,
            ]
        };
        match tag {
            "P" => {
                let got = atlas.biome_at(dimension(v[2]), v[3] as i32, v[4] as i32, v[5] as i32);
                if got != v[6] as i32 {
                    fail(
                        "P biome_at",
                        format!(
                            "seed {seed} large {large} dim {} at {:?}: want {} got {got}",
                            v[2],
                            &v[3..6],
                            v[6]
                        ),
                    );
                }
            }
            "A" => {
                let want = i32s(&v[9..]);
                let got = atlas.biome_area(
                    dimension(v[2]),
                    v[3] as i32,
                    v[4] as i32,
                    v[5] as i32,
                    v[6] as i32,
                    v[7] as i32,
                    v[8] as i32,
                );
                if got.as_ref() != Ok(&want) {
                    let wrong = got.as_ref().map_or(want.len(), |g| {
                        g.iter().zip(&want).filter(|(a, b)| a != b).count()
                    });
                    fail(
                        "A biome_area",
                        format!(
                            "seed {seed} large {large} dim {} step {} at {:?}: {wrong} of {} differ",
                            v[2],
                            v[6],
                            &v[3..6],
                            want.len()
                        ),
                    );
                }
            }
            "C" => {
                let want = i32s(&v[7..]);
                let got = atlas
                    .biome_chunk(
                        dimension(v[2]),
                        v[3] as i32,
                        v[4] as i32,
                        v[5] as i32,
                        v[6] as i32,
                    )
                    .map(|c| c.to_vec());
                if got.as_ref() != Ok(&want) {
                    fail(
                        "C biome_chunk",
                        format!(
                            "seed {seed} large {large} dim {} step {} chunk {:?}",
                            v[2],
                            v[6],
                            &v[3..5]
                        ),
                    );
                }
            }
            "T" => {
                let (rc, want) = (
                    v[8] as i32,
                    rest[9..]
                        .iter()
                        .map(|t| t.parse::<f32>().unwrap())
                        .collect::<Vec<f32>>(),
                );
                let got = atlas.height_area(
                    dimension(v[2]),
                    v[3] as i32,
                    v[4] as i32,
                    v[5] as i32,
                    v[6] as i32,
                    v[7] as i32,
                );
                let ok = match &got {
                    Ok(g) => {
                        rc == 0
                            && g.len() == want.len()
                            && g.iter().zip(&want).all(|(a, b)| {
                                a.to_bits() == b.to_bits()
                                    || (a - b).abs() <= 1e-6 * b.abs().max(1.0)
                            })
                    }
                    Err(_) => rc != 0,
                };
                if !ok {
                    fail(
                        "T heights",
                        format!(
                            "seed {seed} large {large} dim {} step {} at {:?}: want rc {rc} {:?}.. got {:?}",
                            v[2],
                            v[5],
                            &v[3..5],
                            want.first(),
                            got.map(|g| g.first().copied())
                        ),
                    );
                }
            }
            "N" | "R" => {
                let rc = v[7] as i32;
                let (mask, want): (Vec<u64>, Vec<f32>) = if tag == "R" {
                    let c = colon.unwrap();
                    (
                        rest[8..c].iter().map(|t| t.parse().unwrap()).collect(),
                        rest[c + 1..].iter().map(|t| t.parse().unwrap()).collect(),
                    )
                } else {
                    (
                        Vec::new(),
                        rest[8..].iter().map(|t| t.parse().unwrap()).collect(),
                    )
                };
                let got = if tag == "R" {
                    atlas.nether_floor_area_carved(
                        v[1] as i32,
                        v[2] as i32,
                        v[3] as i32,
                        v[4] as i32,
                        v[5] as i32,
                        v[6] as i32,
                        &mask,
                    )
                } else {
                    atlas.nether_floor_area(
                        v[1] as i32,
                        v[2] as i32,
                        v[3] as i32,
                        v[4] as i32,
                        v[5] as i32,
                        v[6] as i32,
                    )
                };
                let ok = match &got {
                    Ok(g) => rc == 0 && g == &want,
                    Err(_) => rc != 0,
                };
                if !ok {
                    fail(
                        if tag == "R" {
                            "R nether floor carved"
                        } else {
                            "N nether floor"
                        },
                        format!("seed {seed} y {} step {} at {:?}", v[3], v[4], &v[1..3]),
                    );
                }
            }
            "S" | "H" | "M" => {
                let (label, want, got) = match tag {
                    "S" => (
                        format!("S kind {}", v[1]),
                        rows(&v[7..]),
                        atlas.scan_structures(v[1] as i32, b(3), v[2] != 0),
                    ),
                    "H" => (
                        "H strongholds".to_owned(),
                        rows(&v[6..]),
                        atlas.scan_strongholds(b(2), v[1] != 0),
                    ),
                    _ => (
                        "M slime".to_owned(),
                        rows(&v[5..]),
                        atlas.scan_slime_chunks(b(1)),
                    ),
                };
                let rc_ok = v[if tag == "S" {
                    7
                } else if tag == "H" {
                    6
                } else {
                    5
                }] >= 0;
                let same = match &got {
                    Ok(g) => rc_ok && g == &want,
                    Err(_) => !rc_ok,
                };
                if !same {
                    let check = if tag == "M" {
                        0
                    } else if tag == "S" {
                        v[2]
                    } else {
                        v[1]
                    };
                    let detail = got.as_ref().map_or("error".to_owned(), |g| {
                        let missing: Vec<_> =
                            want.iter().filter(|h| !g.contains(h)).take(2).collect();
                        let extra: Vec<_> =
                            g.iter().filter(|h| !want.contains(h)).take(2).collect();
                        format!(
                            "want {} got {}; missing {missing:?}; extra {extra:?}",
                            want.len(),
                            g.len()
                        )
                    });
                    fail(&label, format!("seed {seed} check {check}: {detail}"));
                }
            }
            "W" => {
                let got = atlas.spawn(v[1] != 0);
                if got != (v[2] as i32, v[3] as i32) {
                    fail(
                        "W spawn",
                        format!(
                            "seed {seed} detailed {}: want {:?} got {got:?}",
                            v[1],
                            (v[2], v[3])
                        ),
                    );
                }
            }
            _ => panic!("unknown fixture line {tag}"),
        }
    }
    (checked, fails)
}

#[test]
fn every_version_matches_the_fabric_version() {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures");
    let only: Option<Vec<String>> = std::env::var("SEEDATLAS_VERSIONS")
        .ok()
        .map(|v| v.split(',').map(str::to_owned).collect());
    let mut report = Vec::new();
    let mut total = 0;
    let mut files: Vec<_> = std::fs::read_dir(dir)
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .collect();
    files.sort();
    for path in files {
        let version = path
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .trim_end_matches(".txt.gz")
            .to_owned();
        if only.as_ref().is_some_and(|o| !o.contains(&version)) {
            continue;
        }
        let mut text = String::new();
        flate2::read::GzDecoder::new(std::fs::File::open(&path).unwrap())
            .read_to_string(&mut text)
            .unwrap();
        let (checked, fails) = check_version(&version, &text);
        total += checked;
        for (kind, msgs) in fails {
            report.push(format!(
                "{version} {kind}: {} lines, e.g. {}",
                msgs.len(),
                msgs[0]
            ));
        }
    }
    assert!(total > 0, "no fixtures checked");
    assert!(
        report.is_empty(),
        "{} groups differ:\n{}",
        report.len(),
        report.join("\n")
    );
}
