//! Seed Atlas for Meridian: the biomes and structures of a world, computed
//! from its seed.
//!
//! The engine is the Seed Atlas engine itself (DUzzL/Seed-Atlas, GPLv3,
//! derived from cubiomes by Cubitect, MIT), with its C ABI for the mods
//! (`seedatlas-sax`), which the Fabric version uses as well: `build.rs`
//! compiles the copy in `c/` (see `tools/vendor-seedatlas.sh`), and this
//! crate calls it through a small FFI layer (`ffi`). So Meridian computes
//! exactly what the Seed Atlas app and the Fabric version do, for every
//! Minecraft version they offer (1.1 to 26.3).

pub mod atlas;
pub mod biomes;
mod ffi;
pub mod mc;
pub mod structures;

pub use atlas::{Atlas, AtlasError};

/// The version of the engine's C ABI compiled in (`SAX_ABI_VERSION`).
pub fn abi_version() -> i32 {
    ffi::abi_version()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Dimension {
    Overworld,
    Nether,
    End,
}

impl Dimension {
    /// The engine's number for the dimension (-1, 0, 1).
    pub fn id(self) -> i32 {
        match self {
            Dimension::Nether => -1,
            Dimension::Overworld => 0,
            Dimension::End => 1,
        }
    }

    /// The dimension of an engine number (the Overworld for unknown ones).
    pub fn from_id(id: i32) -> Self {
        match id {
            -1 => Dimension::Nether,
            1 => Dimension::End,
            _ => Dimension::Overworld,
        }
    }
}
