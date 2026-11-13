//! Seed terrain callbacks implemented by the pinned game-side support module.
use std::sync::OnceLock;
#[derive(Clone, Debug, PartialEq)]
pub struct SurfaceGrid {
    pub heights: Vec<f32>,
    pub floors: Vec<f32>,
    pub colors: Vec<i32>,
}
pub type SurfaceSampler =
    fn(i64, bool, i32, i32, i32, usize, usize, &dyn Fn() -> bool) -> Option<SurfaceGrid>;
pub type CaveSampler = fn(i64, i32, i32, i32, usize, usize) -> Vec<u64>;
pub static SAMPLERS: OnceLock<(SurfaceSampler, CaveSampler, fn())> = OnceLock::new();
#[allow(clippy::too_many_arguments)]
pub fn sample_overworld(
    seed: i64,
    large: bool,
    x: i32,
    z: i32,
    step: i32,
    w: usize,
    h: usize,
    current: &dyn Fn() -> bool,
) -> Option<SurfaceGrid> {
    (SAMPLERS.get()?.0)(seed, large, x, z, step, w, h, current)
}
pub fn nether_cave_mask(seed: i64, x: i32, z: i32, step: i32, w: usize, h: usize) -> Vec<u64> {
    SAMPLERS
        .get()
        .map_or_else(Vec::new, |s| (s.1)(seed, x, z, step, w, h))
}
pub fn release_caches() {
    if let Some(s) = SAMPLERS.get() {
        (s.2)();
    }
}
