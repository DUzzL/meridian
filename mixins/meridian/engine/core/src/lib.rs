//! Meridian's map engine.
//!
//! Game-independent: everything it needs from the game comes through
//! [`meridian_host::MapHost`] and [`meridian_host::HostEvent`], so the whole
//! crate is tested without Minecraft resources.
//!
//! - [`config`]: settings, profiles and their file
//! - [`keybinds`]: key bindings and their defaults
//! - [`persist`]: atomic writes and the data folder layout
//! - [`geom`], [`tile`], [`region`]: the data model, chunk tile to region
//! - [`color`]: colour and shading maths
//! - [`codec`]: the region file format
//! - [`worker`]: background threads
//! - [`engine`]: ties it together under a per-frame time budget
//! - [`seedatlas`], [`seedatlas_state`], [`seedatlas_commands`], [`completed`],
//!   [`nearest`]: the built-in Seed Atlas
#![forbid(unsafe_code)]

pub mod cave;
pub mod codec;
pub mod color;
pub mod completed;
pub mod config;
pub mod engine;
pub mod export;
pub mod geom;
pub mod info;
pub mod keybinds;
pub mod minimap;
pub mod nearest;
pub mod persist;
pub mod radar;
pub mod region;
pub mod screens;
pub mod settings_screen;
pub mod screens_seedatlas;
pub mod seedatlas;
pub mod seedatlas_commands;
pub mod seedatlas_look;
pub mod seedatlas_state;
pub mod subworld;
pub mod texture;
pub mod tile;
pub mod ui;
pub mod waypoint;
pub mod waypoint_io;
pub mod worker;
pub mod worldmap;

pub use engine::{MapEngine, TickReport};
pub use meridian_host as host;
