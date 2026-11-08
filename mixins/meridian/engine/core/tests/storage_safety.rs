//! Real-file and cross-process checks of every Meridian store.
use meridian_core::{
    codec,
    completed::{CompletedStore, StructureKey},
    config::ConfigStore,
    geom::RegionPos,
    persist,
    seedatlas::{SeedBook, SeedEntry},
    subworld::ServerMaps,
    waypoint::{Waypoint, WaypointBook},
};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

fn folder(label: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!(
        "meridian-safety-{label}-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&p).unwrap();
    p
}

fn files(root: &Path) -> Vec<PathBuf> {
    [
        "config/meridian.toml",
        "data/seeds.toml",
        "data/completed.json",
        "data/server/maps.toml",
        "data/server/default/waypoints.json",
        "data/server/default/overworld/regions/0_0.mrg",
    ]
    .into_iter()
    .map(|p| root.join(p))
    .collect()
}

/// Use the actual loaders and mutation entry points, not just write_atomic.
fn exercise(root: &Path) {
    let store = ConfigStore::new(&root.join("config"));
    let mut config = store.load();
    config.current_mut().minimap.zoom = 3;
    let _ = store.save(&config);
    SeedBook::load(&root.join("data")).set(
        "world",
        SeedEntry {
            input: "42".into(),
            seed: 42,
            large_biomes: false,
        },
    );
    let key =
        StructureKey::new("world", 42, false, "minecraft:overworld", "village", 1, 2).unwrap();
    CompletedStore::load(&root.join("data")).mark(&key);
    let mut maps = ServerMaps::load(&root.join("data/server"));
    maps.select("test-map", true);
    let _ = maps.save();
    let mut book = WaypointBook::load(&root.join("data/server/default"));
    book.add(
        "minecraft:overworld",
        Waypoint::new("test", [1, 2, 3], 12),
        false,
        1,
    );
    let _ = book.save();
    let path = files(root).pop().unwrap();
    let loaded = persist::load_or_set_aside(&path, "region", |bytes| {
        codec::decode(bytes, RegionPos::new(0, 0)).map_err(Into::into)
    });
    if loaded.split().1 {
        let _ = persist::write_atomic(
            &path,
            &codec::encode(&codec::RegionData {
                pos: RegionPos::new(0, 0),
                tiles: Vec::new(),
            }),
        );
    }
}

#[test]
fn every_store_preserves_bad_newer_and_unreadable_files() {
    for kind in ["missing", "broken", "newer", "unreadable"] {
        let root = folder(kind);
        let paths = files(&root);
        for path in &paths {
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            match kind {
                "missing" => {}
                "unreadable" => fs::create_dir(path).unwrap(),
                "broken" => fs::write(path, b"broken storage fixture").unwrap(),
                "newer" => {
                    let bytes = match path.extension().unwrap().to_str().unwrap() {
                        "json" => br#"{"version":99,"future":"unrecognized"}"#.to_vec(),
                        "toml" => b"version = 99\nfuture = 'unrecognized'\n".to_vec(),
                        "mrg" => {
                            let mut b = vec![0; 142];
                            b[..4].copy_from_slice(b"MRDN");
                            b[4..6].copy_from_slice(&99u16.to_le_bytes());
                            b
                        }
                        _ => unreachable!(),
                    };
                    fs::write(path, bytes).unwrap();
                }
                _ => unreachable!(),
            }
        }
        let before: Vec<_> = paths.iter().map(|p| fs::read(p).ok()).collect();
        exercise(&root);
        // A second load/save catches read-only state accidentally lost on reload.
        exercise(&root);
        for (path, before) in paths.iter().zip(&before) {
            match kind {
                "missing" => assert!(path.is_file(), "{} was not created", path.display()),
                "unreadable" => assert!(path.is_dir(), "{} was replaced", path.display()),
                "newer" => assert_eq!(
                    fs::read(path).unwrap(),
                    *before.as_ref().unwrap(),
                    "{}",
                    path.display()
                ),
                "broken" => {
                    let prefix = format!("{}.broken.", path.file_name().unwrap().to_str().unwrap());
                    let backups: Vec<_> = fs::read_dir(path.parent().unwrap())
                        .unwrap()
                        .map(Result::unwrap)
                        .filter(|e| e.file_name().to_str().unwrap().starts_with(&prefix))
                        .collect();
                    assert_eq!(backups.len(), 1, "{}", path.display());
                    assert_eq!(
                        fs::read(backups[0].path()).unwrap(),
                        *before.as_ref().unwrap()
                    );
                    assert!(path.is_file());
                }
                _ => unreachable!(),
            }
        }
        fs::remove_dir_all(root).unwrap();
    }
}

#[test]
fn another_process_cannot_change_any_store_or_quarantine_files() {
    let root = folder("process");
    exercise(&root);
    assert!(persist::claim_data_dir(&root));
    // A corrupt file must also stay untouched in the second process.
    fs::write(files(&root)[1].clone(), b"broken locked fixture").unwrap();
    let before: Vec<_> = files(&root).iter().map(|p| fs::read(p).unwrap()).collect();
    let child = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "child_storage_probe", "--ignored", "--nocapture"])
        .env("MERIDIAN_STORAGE_PROBE", &root)
        .output()
        .unwrap();
    assert!(
        child.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&child.stdout),
        String::from_utf8_lossy(&child.stderr)
    );
    for (path, bytes) in files(&root).iter().zip(before) {
        assert_eq!(fs::read(path).unwrap(), bytes, "{}", path.display());
    }
    assert_eq!(fs::read_dir(root.join("data")).unwrap().count(), 3);
    fs::remove_dir_all(root).unwrap();
}

#[test]
#[ignore = "launched by the cross-process test"]
fn child_storage_probe() {
    let root = PathBuf::from(std::env::var_os("MERIDIAN_STORAGE_PROBE").expect("probe root"));
    assert!(!persist::claim_data_dir(&root));
    #[cfg(unix)]
    {
        let alias = root.with_extension("alias");
        std::os::unix::fs::symlink(&root, &alias).unwrap();
        assert!(!persist::claim_data_dir(&alias));
        assert!(!persist::path_writable(&alias.join("data/seeds.toml")));
        fs::remove_file(alias).unwrap();
    }
    for path in files(&root) {
        assert!(!persist::path_writable(&path));
        assert!(persist::write_atomic(&path, b"must not write").is_err());
    }
    exercise(&root);
    assert!(persist::require_writable(&root.join("data/server/default")).is_err());
}
