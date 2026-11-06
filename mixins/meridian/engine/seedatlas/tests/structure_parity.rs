//! Structure parity with the Fabric version: every fixture line was written
//! by its native library (`tools/seedatlas-fixtures/structure_fixtures.c`).

use std::collections::HashMap;
use std::io::Read;

use meridian_seedatlas::Atlas;
use meridian_seedatlas::atlas::Hit;

fn fixtures() -> String {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/structures.txt.gz"
    );
    let mut s = String::new();
    flate2::read::GzDecoder::new(std::fs::File::open(path).unwrap())
        .read_to_string(&mut s)
        .unwrap();
    s
}

fn rows(v: &[i64]) -> Vec<Hit> {
    let n = v[0] as usize;
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

#[test]
fn structures_match_the_fabric_version() {
    let mut atlases: HashMap<i64, Atlas> = HashMap::new();
    let mut failures = Vec::new();
    let mut compared = 0;
    for line in fixtures().lines() {
        let mut f = line.split(' ');
        let tag = f.next().unwrap();
        let v: Vec<i64> = f.map(|t| t.parse().unwrap()).collect();
        let atlas = atlases
            .entry(v[0])
            .or_insert_with(|| Atlas::new(v[0], false));
        let b = |i: usize| {
            [
                v[i] as i32,
                v[i + 1] as i32,
                v[i + 2] as i32,
                v[i + 3] as i32,
            ]
        };
        let (label, want, got) = match tag {
            "S" => (
                format!("seed {} kind {} check {}", v[0], v[1], v[2]),
                rows(&v[7..]),
                atlas.scan_structures(v[1] as i32, b(3), v[2] != 0).unwrap(),
            ),
            "H" => (
                format!("seed {} strongholds check {}", v[0], v[1]),
                rows(&v[6..]),
                atlas.scan_strongholds(b(2), v[1] != 0).unwrap(),
            ),
            "M" => (
                format!("seed {} slime", v[0]),
                rows(&v[5..]),
                atlas.scan_slime_chunks(b(1)).unwrap(),
            ),
            "W" => {
                let got = atlas.spawn(v[1] != 0);
                compared += 1;
                if got != (v[2] as i32, v[3] as i32) {
                    failures.push(format!(
                        "seed {} spawn detailed {}: want {:?} got {:?}",
                        v[0],
                        v[1],
                        (v[2], v[3]),
                        got
                    ));
                }
                continue;
            }
            _ => unreachable!(),
        };
        compared += want.len();
        if want != got {
            let missing: Vec<_> = want.iter().filter(|h| !got.contains(h)).take(3).collect();
            let extra: Vec<_> = got.iter().filter(|h| !want.contains(h)).take(3).collect();
            failures.push(format!(
                "{label}: want {} got {}; missing {missing:?}; extra {extra:?}",
                want.len(),
                got.len()
            ));
        }
    }
    assert!(compared > 10_000, "only {compared} results");
    assert!(
        failures.is_empty(),
        "{} differ:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
