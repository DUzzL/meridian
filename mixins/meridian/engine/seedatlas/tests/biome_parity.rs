//! Biome parity with the Fabric version: every fixture line was written by
//! its native library (`tools/seedatlas-fixtures/biome_fixtures.c`).

use std::io::Read;

use meridian_seedatlas::{Atlas, Dimension};

fn dimension(id: i32) -> Dimension {
    match id {
        -1 => Dimension::Nether,
        1 => Dimension::End,
        _ => Dimension::Overworld,
    }
}

fn fixtures() -> String {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/biomes.txt.gz");
    let mut s = String::new();
    flate2::read::GzDecoder::new(std::fs::File::open(path).unwrap())
        .read_to_string(&mut s)
        .unwrap();
    s
}

#[test]
fn biomes_match_the_fabric_version() {
    let mut atlases: Vec<Atlas> = Vec::new();
    let (mut checked, mut failures) = (0usize, Vec::new());
    for line in fixtures().lines() {
        let mut f = line.split(' ');
        let kind = f.next().unwrap();
        let v: Vec<i64> = f.map(|t| t.parse().unwrap()).collect();
        let (seed, large, dim) = (v[0], v[1] != 0, dimension(v[2] as i32));
        if !atlases
            .iter()
            .any(|a| a.seed == seed && a.large_biomes == large)
        {
            atlases.push(Atlas::new(seed, large));
        }
        let atlas = atlases
            .iter()
            .find(|a| a.seed == seed && a.large_biomes == large)
            .unwrap();
        let p: Vec<i32> = v[3..].iter().map(|&x| x as i32).collect();
        let (want, got): (Vec<i32>, Vec<i32>) = match kind {
            "P" => (vec![p[3]], vec![atlas.biome_at(dim, p[0], p[1], p[2])]),
            "A" => (
                p[6..].to_vec(),
                atlas
                    .biome_area(dim, p[0], p[1], p[2], p[3], p[4], p[5])
                    .unwrap(),
            ),
            "C" => (
                p[4..].to_vec(),
                atlas
                    .biome_chunk(dim, p[0], p[1], p[2], p[3])
                    .unwrap()
                    .to_vec(),
            ),
            _ => unreachable!(),
        };
        checked += want.len();
        let wrong = want.iter().zip(&got).filter(|(a, b)| a != b).count();
        if wrong > 0 || want.len() != got.len() {
            failures.push(format!(
                "{kind} seed {seed} large {large} {dim:?} {:?}: {wrong} of {} differ",
                &p[..p.len().min(6)],
                want.len()
            ));
        }
    }
    assert!(checked > 50_000, "only {checked} samples");
    assert!(
        failures.is_empty(),
        "{} of the fixture lines differ:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
