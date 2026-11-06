//! Seed Atlas for Meridian: the biomes and structures of a world, computed
//! from its seed.
//!
//! A Rust port of the Seed Atlas engine (DUzzL/Seed-Atlas, GPLv3), which
//! derives from cubiomes (Cubitect, MIT), for every Minecraft version the
//! Fabric version offers (1.1 to 26.3).
//! Parity tests compare it with the Fabric version's native library on
//! fixtures (`tools/seedatlas-fixtures/`).
#![forbid(unsafe_code)]

pub mod atlas;
pub mod biome_noise;
mod biome_table;
pub mod biomes;
mod btree18;
mod btree19;
mod btree192;
mod btree20;
mod btree21wd;
pub mod endcity;
pub mod finders;
pub mod generator;
pub mod layers;
pub mod mc;
pub mod noise;
pub mod rng;
pub mod structures;
pub mod surface;
pub mod terrain;
pub mod viability;

pub use atlas::{Atlas, AtlasError};
pub use biome_noise::Range;
pub use generator::{Dimension, Generator};
