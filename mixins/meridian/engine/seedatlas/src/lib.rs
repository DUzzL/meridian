//! Seed Atlas for Meridian: the biomes and structures of a world, computed
//! from its seed.
//!
//! A Rust port of the Seed Atlas engine (DUzzL/Seed-Atlas, GPLv3), which
//! derives from cubiomes (Cubitect, MIT). Only Minecraft 26.3 is supported.
//! Parity tests compare it with the Fabric version's native library on
//! fixtures (`tools/seedatlas-fixtures/`).
#![forbid(unsafe_code)]

pub mod atlas;
pub mod biome_noise;
mod biome_table;
pub mod biomes;
mod btree21wd;
pub mod generator;
pub mod noise;
pub mod rng;

pub use atlas::{Atlas, AtlasError};
pub use biome_noise::Range;
pub use generator::{Dimension, Generator};
