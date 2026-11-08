//! Compiles the Seed Atlas engine and its C ABI for the mods from
//! `../data/seedatlas-c/` (copied by `tools/vendor-seedatlas.sh`) with the engine's own flags:
//! wrapping signed arithmetic and no fused multiply-adds, so every platform
//! computes the same as the Seed Atlas app.

// The C sources sit in the bundle's `engine/data/` folder: a `.hnmod` carries
// only Rust, TOML and data files, and `engine/data/` is a data folder.
const C: &str = "../data/seedatlas-c";

const ENGINE: [&str; 8] = [
    "biomenoise",
    "biomes",
    "finders",
    "generator",
    "layers",
    "noise",
    "quadbase",
    "util",
];

fn main() {
    // The engine uses C11 variable-length arrays, which MSVC's C compiler
    // lacks; Horizon builds Windows with MinGW (x86_64-pc-windows-gnu).
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        panic!(
            "meridian-seedatlas needs a GNU or LLVM C compiler: build for a *-windows-gnu target"
        );
    }
    let mut build = cc::Build::new();
    for f in ENGINE {
        build.file(format!("{C}/seedatlas-engine/{f}.c"));
    }
    build
        .file(format!("{C}/seedatlas-sax/seedatlas_sax.c"))
        .file(format!("{C}/shim.c"))
        .include(format!("{C}/seedatlas-engine"))
        .include(format!("{C}/seedatlas-sax"))
        .define("SAX_STATIC", None)
        .flag("-std=gnu11")
        .flag("-fwrapv")
        .flag("-ffp-contract=off")
        // fast parity tests in debug builds as well
        .opt_level(3)
        .warnings(false);
    build.compile("seedatlas");
    println!("cargo:rerun-if-changed={C}");
}
